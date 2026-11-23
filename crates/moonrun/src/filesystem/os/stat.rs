// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Selective native stat queries, following async's fs.c at
//! d53eadfc623d343edaf01f0b11dad28badcb1274. Callers select properties independently
//! of guest buffer capacity and encode the returned optional values themselves.

#[cfg(target_os = "linux")]
#[path = "stat/linux.rs"]
mod platform;
#[cfg(target_os = "macos")]
#[path = "stat/macos.rs"]
mod platform;
#[cfg(windows)]
#[path = "stat/windows.rs"]
mod platform;

use super::{BlockingIo, RawFile};
use std::{ffi::OsStr, io};

pub(in crate::filesystem) const KIND: u32 = 1 << 0;
pub(in crate::filesystem) const SIZE: u32 = 1 << 1;
pub(in crate::filesystem) const DEVICE_ID: u32 = 1 << 2;
pub(in crate::filesystem) const FILE_ID: u32 = 1 << 3;
pub(in crate::filesystem) const ACCESS_TIME: u32 = 1 << 4;
pub(in crate::filesystem) const MODIFY_TIME: u32 = 1 << 5;
pub(in crate::filesystem) const CHANGE_TIME: u32 = 1 << 6;
pub(in crate::filesystem) const CREATE_TIME: u32 = 1 << 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::filesystem) enum FileKind {
    Unknown,
    Regular,
    Directory,
    Symlink,
    Socket,
    Pipe,
    #[cfg(unix)]
    BlockDevice,
    CharDevice,
    Console,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::filesystem) struct StatTimestamp {
    pub(in crate::filesystem) seconds: i64,
    pub(in crate::filesystem) nanoseconds: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::filesystem) struct StatValues {
    pub(in crate::filesystem) kind: Option<FileKind>,
    pub(in crate::filesystem) size: Option<i64>,
    pub(in crate::filesystem) device_id: Option<u64>,
    pub(in crate::filesystem) file_id: Option<u64>,
    pub(in crate::filesystem) access_time: Option<StatTimestamp>,
    pub(in crate::filesystem) modify_time: Option<StatTimestamp>,
    pub(in crate::filesystem) change_time: Option<StatTimestamp>,
    pub(in crate::filesystem) create_time: Option<StatTimestamp>,
}

pub(in crate::filesystem) fn query_handle(handle: RawFile, fields: u32) -> io::Result<StatValues> {
    let values = platform::query_handle(handle, fields)?;
    #[cfg(unix)]
    if values.kind == Some(FileKind::CharDevice) && unsafe { libc::isatty(handle) } == 1 {
        return Ok(StatValues {
            kind: Some(FileKind::Console),
            ..values
        });
    }
    Ok(values)
}

pub(in crate::filesystem) fn query_path(
    parent: Option<RawFile>,
    path: &OsStr,
    follow_symlink: bool,
    fields: u32,
    blocking: &impl BlockingIo,
) -> io::Result<StatValues> {
    #[cfg(unix)]
    {
        let _ = blocking;
        platform::query_path(parent, path, follow_symlink, fields)
    }

    #[cfg(windows)]
    {
        use super::openat::{self, Access, CreateMode, OpenOptions, Sync, TypeConstraint};
        use std::os::windows::io::AsRawHandle;
        let file = openat::open(
            parent,
            path,
            &OpenOptions {
                access: Access::Attributes,
                type_constraint: TypeConstraint::Any,
                follow_symlink,
                is_async: false,
                create: CreateMode::Open,
                append: false,
                sync: Sync::None,
                permission: 0,
            },
            blocking,
        )?;
        platform::query(file.as_raw_handle(), fields)
    }
}

#[cfg(any(windows, test))]
const WINDOWS_EPOCH_OFFSET_SECONDS: i64 = 11_644_473_600;

#[cfg(any(windows, test))]
fn windows_timestamp(ticks: i64) -> StatTimestamp {
    StatTimestamp {
        seconds: ticks.div_euclid(10_000_000) - WINDOWS_EPOCH_OFFSET_SECONDS,
        nanoseconds: ticks.rem_euclid(10_000_000) * 100,
    }
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
mod platform {
    use std::ffi::OsStr;

    use super::*;

    pub(super) fn query_handle(_fd: i32, _request: u32) -> io::Result<StatValues> {
        Err(io::Error::from_raw_os_error(libc::ENOSYS))
    }

    pub(super) fn query_path(
        _parent: Option<i32>,
        _path: &OsStr,
        _follow_symlink: bool,
        _request: u32,
    ) -> io::Result<StatValues> {
        Err(io::Error::from_raw_os_error(libc::ENOSYS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn platform_adapter_distinguishes_a_symlink_from_its_target() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        let link = temp.path().join("link");
        std::fs::write(&target, b"target").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let fields = KIND;

        let link_values = query_path(
            None,
            link.as_os_str(),
            false,
            fields,
            &super::super::DirectIo,
        )
        .unwrap();
        let target_values = query_path(
            None,
            target.as_os_str(),
            true,
            fields,
            &super::super::DirectIo,
        )
        .unwrap();

        assert_eq!(link_values.kind, Some(FileKind::Symlink));
        assert_eq!(target_values.kind, Some(FileKind::Regular));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_non_vnode_fallback_returns_all_fstat_metadata() {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

        let mut pipe = [0; 2];
        assert_eq!(unsafe { libc::pipe(pipe.as_mut_ptr()) }, 0);
        let read_end = unsafe { OwnedFd::from_raw_fd(pipe[0]) };
        let _write_end = unsafe { OwnedFd::from_raw_fd(pipe[1]) };
        let fields = KIND
            | SIZE
            | DEVICE_ID
            | FILE_ID
            | ACCESS_TIME
            | MODIFY_TIME
            | CHANGE_TIME
            | CREATE_TIME;

        let values = query_handle(read_end.as_raw_fd(), fields).unwrap();

        assert_eq!(values.kind, Some(FileKind::Pipe));
        assert!(values.size.is_some());
        assert!(values.device_id.is_some());
        assert!(values.file_id.is_some());
        assert!(values.access_time.is_some());
        assert!(values.modify_time.is_some());
        assert!(values.change_time.is_some());
        assert!(values.create_time.is_some());
    }

    #[cfg(windows)]
    #[test]
    fn windows_empty_stat_request_still_validates_the_handle() {
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

        let fields = 0;

        assert!(query_handle(INVALID_HANDLE_VALUE, fields).is_err());
    }

    #[test]
    fn windows_filetime_is_reported_relative_to_the_unix_epoch() {
        let epoch = WINDOWS_EPOCH_OFFSET_SECONDS * 10_000_000;

        assert_eq!(
            windows_timestamp(epoch),
            StatTimestamp {
                seconds: 0,
                nanoseconds: 0,
            }
        );
        assert_eq!(
            windows_timestamp(epoch + 12_345_678),
            StatTimestamp {
                seconds: 1,
                nanoseconds: 234_567_800,
            }
        );
    }
}
