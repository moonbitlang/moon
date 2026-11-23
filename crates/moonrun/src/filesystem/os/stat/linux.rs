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

use std::ffi::{CString, OsStr};
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt;

use super::*;

const AT_EMPTY_PATH: i32 = 0x1000;
const STATX_TYPE: u32 = 0x0000_0001;
const STATX_ATIME: u32 = 0x0000_0020;
const STATX_MTIME: u32 = 0x0000_0040;
const STATX_CTIME: u32 = 0x0000_0080;
const STATX_INO: u32 = 0x0000_0100;
const STATX_SIZE: u32 = 0x0000_0200;
const STATX_BTIME: u32 = 0x0000_0800;

#[repr(C)]
#[derive(Clone, Copy)]
struct LinuxStatxTimestamp {
    tv_sec: i64,
    tv_nsec: u32,
    _reserved: i32,
}

#[repr(C)]
struct LinuxStatx {
    stx_mask: u32,
    stx_blksize: u32,
    stx_attributes: u64,
    stx_nlink: u32,
    stx_uid: u32,
    stx_gid: u32,
    stx_mode: u16,
    _pad1: u16,
    stx_ino: u64,
    stx_size: u64,
    stx_blocks: u64,
    stx_attributes_mask: u64,
    stx_atime: LinuxStatxTimestamp,
    stx_btime: LinuxStatxTimestamp,
    stx_ctime: LinuxStatxTimestamp,
    stx_mtime: LinuxStatxTimestamp,
    stx_rdev_major: u32,
    stx_rdev_minor: u32,
    stx_dev_major: u32,
    stx_dev_minor: u32,
    _spare: [u64; 14],
}

pub(super) fn query_handle(fd: i32, fields: u32) -> io::Result<StatValues> {
    query(fd, c"", AT_EMPTY_PATH, fields)
}

pub(super) fn query_path(
    parent: Option<i32>,
    path: &OsStr,
    follow_symlink: bool,
    fields: u32,
) -> io::Result<StatValues> {
    let path =
        CString::new(path.as_bytes()).map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;
    let flags = if follow_symlink {
        0
    } else {
        libc::AT_SYMLINK_NOFOLLOW
    };
    query(parent.unwrap_or(libc::AT_FDCWD), &path, flags, fields)
}

fn query(dirfd: i32, path: &std::ffi::CStr, flags: i32, fields: u32) -> io::Result<StatValues> {
    let requested_statx_mask = (if fields & KIND != 0 { STATX_TYPE } else { 0 })
        | (if fields & SIZE != 0 { STATX_SIZE } else { 0 })
        | (if fields & FILE_ID != 0 { STATX_INO } else { 0 })
        | (if fields & ACCESS_TIME != 0 {
            STATX_ATIME
        } else {
            0
        })
        | (if fields & MODIFY_TIME != 0 {
            STATX_MTIME
        } else {
            0
        })
        | (if fields & CHANGE_TIME != 0 {
            STATX_CTIME
        } else {
            0
        })
        | (if fields & CREATE_TIME != 0 {
            STATX_BTIME
        } else {
            0
        });

    let mut info = MaybeUninit::<LinuxStatx>::zeroed();
    if unsafe {
        libc::syscall(
            libc::SYS_statx,
            dirfd,
            path.as_ptr(),
            flags,
            requested_statx_mask,
            info.as_mut_ptr(),
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    let info = unsafe { info.assume_init() };
    let supported = |property, statx_mask| {
        fields & property != 0 && (statx_mask == 0 || info.stx_mask & statx_mask != 0)
    };
    let timestamp = |value: LinuxStatxTimestamp| StatTimestamp {
        seconds: value.tv_sec,
        nanoseconds: i64::from(value.tv_nsec),
    };

    Ok(StatValues {
        kind: supported(KIND, STATX_TYPE).then(|| file_kind(info.stx_mode)),
        size: supported(SIZE, STATX_SIZE).then_some(info.stx_size as i64),
        device_id: supported(DEVICE_ID, 0)
            .then(|| libc::makedev(info.stx_dev_major, info.stx_dev_minor)),
        file_id: supported(FILE_ID, STATX_INO).then_some(info.stx_ino),
        access_time: supported(ACCESS_TIME, STATX_ATIME).then(|| timestamp(info.stx_atime)),
        modify_time: supported(MODIFY_TIME, STATX_MTIME).then(|| timestamp(info.stx_mtime)),
        change_time: supported(CHANGE_TIME, STATX_CTIME).then(|| timestamp(info.stx_ctime)),
        create_time: supported(CREATE_TIME, STATX_BTIME).then(|| timestamp(info.stx_btime)),
    })
}

fn file_kind(mode: u16) -> FileKind {
    match u32::from(mode) & libc::S_IFMT {
        libc::S_IFREG => FileKind::Regular,
        libc::S_IFDIR => FileKind::Directory,
        libc::S_IFLNK => FileKind::Symlink,
        libc::S_IFSOCK => FileKind::Socket,
        libc::S_IFIFO => FileKind::Pipe,
        libc::S_IFBLK => FileKind::BlockDevice,
        libc::S_IFCHR => FileKind::CharDevice,
        _ => FileKind::Unknown,
    }
}
