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

//! Compatibility metadata for guests predating generic stat.
//! Keep the old kind codes and 48-byte timestamp layout separate from native
//! queries. TODO: remove this module when the corresponding compatibility imports retire.

use std::ffi::OsStr;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use crate::async_host::AsyncHostResult;
use crate::async_sys::ported_fns;
use crate::filesystem::os::{
    BlockingIo, RawFile,
    metadata::{self, FileInfo},
};
use crate::resource::Resource;

#[cfg(unix)]
use super::FILE_KIND_BLOCK_DEVICE;
use super::{
    FILE_KIND_CHAR_DEVICE, FILE_KIND_DIRECTORY, FILE_KIND_PIPE, FILE_KIND_REGULAR,
    FILE_KIND_SOCKET, FILE_KIND_SYMLINK, FILE_KIND_UNKNOWN,
};

#[derive(Clone, Copy)]
pub(in crate::filesystem) struct FileTimeResult(FileInfo);

impl FileTimeResult {
    pub(in crate::filesystem) fn new(file_time: FileInfo) -> Self {
        Self(file_time)
    }

    pub(in crate::filesystem) fn encode(&self) -> [u8; 48] {
        let file_time = &self.0;
        let mut record = [0; 48];
        record[0..8].copy_from_slice(&get_atime_sec(file_time).to_le_bytes());
        record[8..12].copy_from_slice(&get_atime_nsec(file_time).to_le_bytes());
        record[16..24].copy_from_slice(&get_mtime_sec(file_time).to_le_bytes());
        record[24..28].copy_from_slice(&get_mtime_nsec(file_time).to_le_bytes());
        record[32..40].copy_from_slice(&get_ctime_sec(file_time).to_le_bytes());
        record[40..44].copy_from_slice(&get_ctime_nsec(file_time).to_le_bytes());
        record
    }
}

impl std::fmt::Debug for FileTimeResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("FileTimeResult").finish_non_exhaustive()
    }
}

#[cfg(windows)]
const WINDOWS_TICKS_PER_SECOND: i64 = 10_000_000;
#[cfg(windows)]
const WINDOWS_TO_UNIX_EPOCH_SECONDS: i64 = 11_644_473_600;

#[cfg(windows)]
fn windows_filetime_to_unix_seconds(ticks: i64) -> i64 {
    ticks / WINDOWS_TICKS_PER_SECOND - WINDOWS_TO_UNIX_EPOCH_SECONDS
}

#[cfg(windows)]
fn windows_filetime_to_nanoseconds(ticks: i64) -> i32 {
    ((ticks % WINDOWS_TICKS_PER_SECOND) * 100) as i32
}

pub(in crate::filesystem) fn resource_kind(file: &Resource) -> AsyncHostResult<i32> {
    #[cfg(unix)]
    let raw = file.as_file()?.as_raw_fd();
    #[cfg(windows)]
    let raw = {
        if file.resource_class().is_socket() {
            file.as_socket()?;
            return Ok(FILE_KIND_SOCKET);
        }
        file.as_file()?.as_raw_handle()
    };
    kind_of_fd(raw)
}

pub(in crate::filesystem) fn kind_by_path(
    parent: Option<RawFile>,
    path: &OsStr,
    follow_symlink: bool,
    _blocking: &impl BlockingIo,
) -> AsyncHostResult<i32> {
    #[cfg(unix)]
    {
        Ok(kind_from_stat(&metadata::file_info_at(
            parent,
            path,
            follow_symlink,
        )?))
    }
    #[cfg(windows)]
    {
        use crate::filesystem::os::openat::{
            self, Access, CreateMode, OpenOptions, Sync, TypeConstraint,
        };
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
            _blocking,
        )?;
        kind_of_fd(file.as_raw_handle())
    }
}

#[cfg(unix)]
fn kind_from_stat(stat: &FileInfo) -> i32 {
    match stat.st_mode & libc::S_IFMT {
        libc::S_IFREG => FILE_KIND_REGULAR,
        libc::S_IFDIR => FILE_KIND_DIRECTORY,
        libc::S_IFLNK => FILE_KIND_SYMLINK,
        libc::S_IFSOCK => FILE_KIND_SOCKET,
        libc::S_IFIFO => FILE_KIND_PIPE,
        libc::S_IFBLK => FILE_KIND_BLOCK_DEVICE,
        libc::S_IFCHR => FILE_KIND_CHAR_DEVICE,
        _ => FILE_KIND_UNKNOWN,
    }
}

ported_fns! {
    #[compat(
        source = "src/internal/event_loop/detect_file_kind.c",
        original = "moonbitlang_async_kind_of_fd",
        upstream_pr = 527,
        replacement = "fstatx with STAT_FILE_KIND"
    )]
    fn kind_of_fd(file: RawFile) -> AsyncHostResult<i32> {
        #[cfg(unix)]
        {
            Ok(kind_from_stat(&metadata::file_info(file)?))
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_DIRECTORY,
                FILE_TYPE_DISK, FILE_TYPE_CHAR, FILE_TYPE_PIPE, FILE_TYPE_UNKNOWN,
            };
            let (file_type, is_socket) = metadata::file_type(file)?;
            Ok(match file_type {
                FILE_TYPE_DISK => {
                    let attrs = metadata::file_info(file)?.FileAttributes;
                    if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                        FILE_KIND_SYMLINK
                    } else if attrs & FILE_ATTRIBUTE_DIRECTORY != 0 {
                        FILE_KIND_DIRECTORY
                    } else {
                        FILE_KIND_REGULAR
                    }
                }
                FILE_TYPE_CHAR => FILE_KIND_CHAR_DEVICE,
                FILE_TYPE_PIPE | FILE_TYPE_UNKNOWN if is_socket => FILE_KIND_SOCKET,
                FILE_TYPE_PIPE => FILE_KIND_PIPE,
                _ => FILE_KIND_UNKNOWN,
            })
        }
    }

    #[compat(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_get_atime_sec",
        upstream_pr = 660,
        replacement = "generic stat timestamps in src/internal/event_loop/fs.c"
    )]
    #[allow(clippy::unnecessary_cast)]
    fn get_atime_sec(file_time: &FileInfo) -> i64 {
        #[cfg(windows)]
        {
            windows_filetime_to_unix_seconds(file_time.LastAccessTime)
        }
        #[cfg(unix)]
        {
            file_time.st_atime as i64
        }
    }

    #[compat(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_get_atime_nsec",
        upstream_pr = 660,
        replacement = "generic stat timestamps in src/internal/event_loop/fs.c"
    )]
    fn get_atime_nsec(file_time: &FileInfo) -> i32 {
        #[cfg(windows)]
        {
            windows_filetime_to_nanoseconds(file_time.LastAccessTime)
        }
        #[cfg(unix)]
        {
            file_time.st_atime_nsec as i32
        }
    }

    #[compat(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_get_mtime_sec",
        upstream_pr = 660,
        replacement = "generic stat timestamps in src/internal/event_loop/fs.c"
    )]
    #[allow(clippy::unnecessary_cast)]
    fn get_mtime_sec(file_time: &FileInfo) -> i64 {
        #[cfg(windows)]
        {
            windows_filetime_to_unix_seconds(file_time.LastWriteTime)
        }
        #[cfg(unix)]
        {
            file_time.st_mtime as i64
        }
    }

    #[compat(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_get_mtime_nsec",
        upstream_pr = 660,
        replacement = "generic stat timestamps in src/internal/event_loop/fs.c"
    )]
    fn get_mtime_nsec(file_time: &FileInfo) -> i32 {
        #[cfg(windows)]
        {
            windows_filetime_to_nanoseconds(file_time.LastWriteTime)
        }
        #[cfg(unix)]
        {
            file_time.st_mtime_nsec as i32
        }
    }

    #[compat(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_get_ctime_sec",
        upstream_pr = 660,
        replacement = "generic stat timestamps in src/internal/event_loop/fs.c"
    )]
    #[allow(clippy::unnecessary_cast)]
    fn get_ctime_sec(file_time: &FileInfo) -> i64 {
        #[cfg(windows)]
        {
            windows_filetime_to_unix_seconds(file_time.ChangeTime)
        }
        #[cfg(unix)]
        {
            file_time.st_ctime as i64
        }
    }

    #[compat(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_get_ctime_nsec",
        upstream_pr = 660,
        replacement = "generic stat timestamps in src/internal/event_loop/fs.c"
    )]
    fn get_ctime_nsec(file_time: &FileInfo) -> i32 {
        #[cfg(windows)]
        {
            windows_filetime_to_nanoseconds(file_time.ChangeTime)
        }
        #[cfg(unix)]
        {
            file_time.st_ctime_nsec as i32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_time_record_preserves_the_legacy_layout() {
        use crate::filesystem::os::raw_file;
        use std::fs::FileTimes;
        use std::time::{Duration, UNIX_EPOCH};

        let file = tempfile::NamedTempFile::new().unwrap();
        let accessed = UNIX_EPOCH + Duration::new(1_700_000_000, 123_400_000);
        let modified = UNIX_EPOCH + Duration::new(1_700_000_001, 567_800_000);
        file.as_file()
            .set_times(
                FileTimes::new()
                    .set_accessed(accessed)
                    .set_modified(modified),
            )
            .unwrap();

        let record =
            FileTimeResult::new(metadata::file_info(raw_file(file.as_file())).unwrap()).encode();
        let by_path = FileTimeResult::new(
            metadata::file_info_by_path(file.path().as_os_str(), true).unwrap(),
        )
        .encode();
        assert_eq!(record, by_path);
        for (offset, expected) in [(0, accessed), (16, modified)] {
            let expected = expected.duration_since(UNIX_EPOCH).unwrap();
            assert_eq!(
                i64::from_le_bytes(record[offset..offset + 8].try_into().unwrap()),
                expected.as_secs() as i64
            );
            assert_eq!(
                i32::from_le_bytes(record[offset + 8..offset + 12].try_into().unwrap()),
                expected.subsec_nanos() as i32
            );
        }
        for padding in [12..16, 28..32, 44..48] {
            assert_eq!(&record[padding], &[0; 4]);
        }
    }

    #[cfg(unix)]
    #[test]
    fn path_kind_uses_the_directory_handle_and_preserves_symlink_mode() {
        use crate::filesystem::os::DirectIo;
        use crate::filesystem::os::raw_file;
        use std::fs::File;

        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original");
        let moved = root.path().join("moved");
        std::fs::create_dir(&original).unwrap();
        std::fs::write(original.join("file"), b"abc").unwrap();
        std::os::unix::fs::symlink("file", original.join("link")).unwrap();
        let directory = File::open(&original).unwrap();
        std::fs::rename(&original, &moved).unwrap();
        std::fs::create_dir(&original).unwrap();
        // A stale pathname would now refer to a different kind of entry.
        std::fs::create_dir(original.join("file")).unwrap();

        let parent = Some(raw_file(&directory));
        assert_eq!(
            kind_by_path(parent, OsStr::new("file"), true, &DirectIo).unwrap(),
            FILE_KIND_REGULAR
        );
        assert_eq!(
            kind_by_path(parent, OsStr::new("link"), true, &DirectIo).unwrap(),
            FILE_KIND_REGULAR
        );
        assert_eq!(
            kind_by_path(parent, OsStr::new("link"), false, &DirectIo).unwrap(),
            FILE_KIND_SYMLINK
        );
        let file = File::open(moved.join("file")).unwrap();
        assert_eq!(metadata::file_size(raw_file(&file)).unwrap(), 3);
        assert_eq!(kind_of_fd(raw_file(&file)).unwrap(), FILE_KIND_REGULAR);
    }

    #[cfg(windows)]
    #[test]
    fn windows_filetime_seconds_use_unix_epoch() {
        let unix_epoch = WINDOWS_TO_UNIX_EPOCH_SECONDS * WINDOWS_TICKS_PER_SECOND;

        assert_eq!(windows_filetime_to_unix_seconds(unix_epoch), 0);
        assert_eq!(
            windows_filetime_to_unix_seconds(unix_epoch + WINDOWS_TICKS_PER_SECOND),
            1
        );
        assert_eq!(windows_filetime_to_nanoseconds(unix_epoch + 123), 12_300);
    }
}
