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
use std::mem::size_of;
use std::os::unix::ffi::OsStrExt;

use super::*;

type FsObjType = u32;
const VREG: FsObjType = 1;
const VDIR: FsObjType = 2;
const VBLK: FsObjType = 3;
const VCHR: FsObjType = 4;
const VLNK: FsObjType = 5;
const VSOCK: FsObjType = 6;
const VFIFO: FsObjType = 7;

#[derive(Clone, Copy)]
struct Attribute {
    group: usize,
    bit: libc::attrgroup_t,
    size: usize,
}

const ATTRIBUTES: [Attribute; 8] = [
    Attribute {
        group: 0,
        bit: libc::ATTR_CMN_OBJTYPE,
        size: size_of::<FsObjType>(),
    },
    Attribute {
        group: 3,
        bit: libc::ATTR_FILE_DATALENGTH,
        size: size_of::<i64>(),
    },
    Attribute {
        group: 0,
        bit: libc::ATTR_CMN_DEVID,
        size: size_of::<libc::dev_t>(),
    },
    Attribute {
        group: 0,
        bit: libc::ATTR_CMN_FILEID,
        size: size_of::<u64>(),
    },
    Attribute {
        group: 0,
        bit: libc::ATTR_CMN_ACCTIME,
        size: size_of::<libc::timespec>(),
    },
    Attribute {
        group: 0,
        bit: libc::ATTR_CMN_MODTIME,
        size: size_of::<libc::timespec>(),
    },
    Attribute {
        group: 0,
        bit: libc::ATTR_CMN_CHGTIME,
        size: size_of::<libc::timespec>(),
    },
    Attribute {
        group: 0,
        bit: libc::ATTR_CMN_CRTIME,
        size: size_of::<libc::timespec>(),
    },
];

pub(super) fn query_handle(fd: i32, fields: u32) -> io::Result<StatValues> {
    query(QueryTarget::Handle(fd), fields)
}

pub(super) fn query_path(
    parent: Option<i32>,
    path: &OsStr,
    follow_symlink: bool,
    fields: u32,
) -> io::Result<StatValues> {
    let path =
        CString::new(path.as_bytes()).map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;
    query(
        QueryTarget::Path {
            parent,
            path: &path,
            follow_symlink,
        },
        fields,
    )
}

enum QueryTarget<'a> {
    Handle(i32),
    Path {
        parent: Option<i32>,
        path: &'a std::ffi::CStr,
        follow_symlink: bool,
    },
}

fn query(target: QueryTarget<'_>, fields: u32) -> io::Result<StatValues> {
    // getattrlist packs attributes in a platform-defined order rather than request-bit order.
    const ATTRIBUTE_ORDER: [usize; 8] = [2, 0, 7, 5, 6, 4, 3, 1];
    let mut list = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let mut offsets = [0_usize; 8];
    let mut offset = size_of::<u32>() + size_of::<libc::attribute_set_t>();
    for index in ATTRIBUTE_ORDER {
        if fields & (1 << index) == 0 {
            continue;
        }
        let attribute = ATTRIBUTES[index];
        match attribute.group {
            0 => list.commonattr |= attribute.bit,
            3 => list.fileattr |= attribute.bit,
            _ => unreachable!(),
        }
        offsets[index] = offset;
        offset += attribute.size;
    }

    let mut buffer = [0_u8; 256];
    let ret = unsafe {
        match target {
            QueryTarget::Handle(fd) => libc::fgetattrlist(
                fd,
                std::ptr::from_mut(&mut list).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                libc::FSOPT_PACK_INVAL_ATTRS,
            ),
            QueryTarget::Path {
                parent: None,
                path,
                follow_symlink,
            } => libc::getattrlist(
                path.as_ptr(),
                std::ptr::from_mut(&mut list).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                libc::FSOPT_PACK_INVAL_ATTRS
                    | if follow_symlink {
                        0
                    } else {
                        libc::FSOPT_NOFOLLOW
                    },
            ),
            QueryTarget::Path {
                parent: Some(parent),
                path,
                follow_symlink,
            } => libc::getattrlistat(
                parent,
                path.as_ptr(),
                std::ptr::from_mut(&mut list).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                (libc::FSOPT_PACK_INVAL_ATTRS
                    | if follow_symlink {
                        0
                    } else {
                        libc::FSOPT_NOFOLLOW
                    }) as libc::c_ulong,
            ),
        }
    };
    if ret < 0 {
        let error = io::Error::last_os_error();
        if matches!(target, QueryTarget::Handle(_)) && error.raw_os_error() == Some(libc::EINVAL) {
            return fallback_non_vnode(target, fields);
        }
        return Err(error);
    }

    let result_len = read_unaligned::<u32>(&buffer, 0).unwrap_or(0) as usize;
    let returned = read_unaligned::<libc::attribute_set_t>(&buffer, size_of::<u32>())
        .ok_or(io::Error::from_raw_os_error(libc::EIO))?;
    let is_returned = |index: usize| {
        let attribute = ATTRIBUTES[index];
        let group = match attribute.group {
            0 => returned.commonattr,
            3 => returned.fileattr,
            _ => 0,
        };
        group & attribute.bit != 0 && offsets[index] + attribute.size <= result_len
    };
    let read_time = |index| {
        is_returned(index)
            .then(|| read_unaligned::<libc::timespec>(&buffer, offsets[index]))
            .flatten()
            .map(|value| StatTimestamp {
                seconds: value.tv_sec,
                nanoseconds: value.tv_nsec,
            })
    };

    Ok(StatValues {
        kind: is_returned(0)
            .then(|| read_unaligned::<FsObjType>(&buffer, offsets[0]))
            .flatten()
            .map(file_kind_from_vnode),
        size: is_returned(1)
            .then(|| read_unaligned::<i64>(&buffer, offsets[1]))
            .flatten(),
        device_id: is_returned(2)
            .then(|| read_unaligned::<libc::dev_t>(&buffer, offsets[2]))
            .flatten()
            .map(|value| value as u64),
        file_id: is_returned(3)
            .then(|| read_unaligned::<u64>(&buffer, offsets[3]))
            .flatten(),
        access_time: read_time(4),
        modify_time: read_time(5),
        change_time: read_time(6),
        create_time: read_time(7),
    })
}

fn fallback_non_vnode(target: QueryTarget<'_>, fields: u32) -> io::Result<StatValues> {
    let QueryTarget::Handle(fd) = target else {
        unreachable!();
    };
    let stat = super::super::metadata::file_info(fd)?;
    let requested_time = |property, seconds, nanoseconds| {
        (fields & property != 0).then_some(StatTimestamp {
            seconds,
            nanoseconds,
        })
    };
    Ok(StatValues {
        kind: (fields & KIND != 0).then(|| file_kind_from_mode(stat.st_mode)),
        size: (fields & SIZE != 0).then_some(stat.st_size),
        device_id: (fields & DEVICE_ID != 0).then_some(stat.st_dev as u64),
        file_id: (fields & FILE_ID != 0).then_some(stat.st_ino),
        access_time: requested_time(ACCESS_TIME, stat.st_atime, stat.st_atime_nsec),
        modify_time: requested_time(MODIFY_TIME, stat.st_mtime, stat.st_mtime_nsec),
        change_time: requested_time(CHANGE_TIME, stat.st_ctime, stat.st_ctime_nsec),
        create_time: requested_time(CREATE_TIME, stat.st_birthtime, stat.st_birthtime_nsec),
    })
}

fn read_unaligned<T: Copy>(buffer: &[u8], offset: usize) -> Option<T> {
    buffer
        .get(offset..offset.checked_add(size_of::<T>())?)
        .map(|bytes| unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<T>()) })
}

fn file_kind_from_vnode(kind: FsObjType) -> FileKind {
    match kind {
        VREG => FileKind::Regular,
        VDIR => FileKind::Directory,
        VLNK => FileKind::Symlink,
        VSOCK => FileKind::Socket,
        VFIFO => FileKind::Pipe,
        VBLK => FileKind::BlockDevice,
        VCHR => FileKind::CharDevice,
        _ => FileKind::Unknown,
    }
}

fn file_kind_from_mode(mode: libc::mode_t) -> FileKind {
    match mode & libc::S_IFMT {
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
