// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Stable generic-stat result layout shared by the platform adapters.

use std::ffi::OsString;

#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, AsRawSocket};

use crate::async_host::{AsyncHostError, AsyncHostResult};

use super::Resource;

pub(super) const STAT_FILE_KIND: u32 = 0x0001;
pub(super) const STAT_FILE_SIZE: u32 = 0x0002;
pub(super) const STAT_DEVICE_ID: u32 = 0x0004;
pub(super) const STAT_FILE_ID: u32 = 0x0008;
pub(super) const STAT_ACCESS_TIME: u32 = 0x0010;
pub(super) const STAT_MODIFY_TIME: u32 = 0x0020;
pub(super) const STAT_CHANGE_TIME: u32 = 0x0040;
pub(super) const STAT_CREATE_TIME: u32 = 0x0080;

pub(crate) const STAT_OPEN_IDENTITY: u32 = STAT_FILE_KIND | STAT_DEVICE_ID | STAT_FILE_ID;

const STAT_SUPPORTED_PROPERTY_MASK: u32 = 0x00ff;
const STAT_HEADER_LEN: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StatRequest {
    mask: u32,
    encoded_len: usize,
}

impl StatRequest {
    pub(super) const fn open_identity() -> Self {
        Self {
            mask: STAT_OPEN_IDENTITY,
            encoded_len: 32,
        }
    }

    pub(super) fn new(mask: u32, capacity: i32) -> AsyncHostResult<Self> {
        if mask & !STAT_SUPPORTED_PROPERTY_MASK != 0 || capacity < STAT_HEADER_LEN as i32 {
            return Err(AsyncHostError::Inval);
        }
        let property_words = (0..8)
            .filter(|bit| mask & (1 << bit) != 0)
            .map(|bit| if bit < 4 { 1 } else { 2 })
            .sum::<usize>();
        let encoded_len = STAT_HEADER_LEN + property_words * 8;
        if encoded_len > capacity as usize {
            return Err(AsyncHostError::Inval);
        }
        Ok(Self { mask, encoded_len })
    }

    pub(crate) fn mask(self) -> u32 {
        self.mask
    }

    pub(super) fn encoded_len(self) -> usize {
        self.encoded_len
    }
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
mod platform {
    use std::ffi::OsString;

    use super::*;

    pub(super) fn query_handle(_fd: i32, _request: StatRequest) -> AsyncHostResult<StatValues> {
        Err(AsyncHostError::Native(libc::ENOSYS))
    }

    pub(super) fn query_path(
        _parent: Option<i32>,
        _path: OsString,
        _follow_symlink: bool,
        _request: StatRequest,
    ) -> AsyncHostResult<StatValues> {
        Err(AsyncHostError::Native(libc::ENOSYS))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StatTimestamp {
    pub(super) seconds: i64,
    pub(super) nanoseconds: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct StatValues {
    pub(super) kind: Option<i32>,
    pub(super) size: Option<i64>,
    pub(super) device_id: Option<u64>,
    pub(super) file_id: Option<u64>,
    pub(super) access_time: Option<StatTimestamp>,
    pub(super) modify_time: Option<StatTimestamp>,
    pub(super) change_time: Option<StatTimestamp>,
    pub(super) create_time: Option<StatTimestamp>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PackedStat {
    request: StatRequest,
    bytes: Box<[u8]>,
}

impl PackedStat {
    pub(super) fn new(request: StatRequest, values: StatValues) -> Self {
        let mut bytes = vec![0; request.encoded_len()];
        let mut returned_mask = 0_u32;
        let mut offset = STAT_HEADER_LEN;

        for bit in 0..8 {
            let property = 1_u32 << bit;
            if request.mask() & property == 0 {
                continue;
            }
            let mut write_word = |value: [u8; 8]| {
                bytes[offset..offset + 8].copy_from_slice(&value);
                offset += 8;
            };
            match property {
                STAT_FILE_KIND => {
                    if let Some(value) = values.kind {
                        returned_mask |= property;
                        write_word(u64::from(value as u32).to_le_bytes());
                    } else {
                        offset += 8;
                    }
                }
                STAT_FILE_SIZE => {
                    if let Some(value) = values.size {
                        returned_mask |= property;
                        write_word(value.to_le_bytes());
                    } else {
                        offset += 8;
                    }
                }
                STAT_DEVICE_ID => {
                    if let Some(value) = values.device_id {
                        returned_mask |= property;
                        write_word(value.to_le_bytes());
                    } else {
                        offset += 8;
                    }
                }
                STAT_FILE_ID => {
                    if let Some(value) = values.file_id {
                        returned_mask |= property;
                        write_word(value.to_le_bytes());
                    } else {
                        offset += 8;
                    }
                }
                STAT_ACCESS_TIME | STAT_MODIFY_TIME | STAT_CHANGE_TIME | STAT_CREATE_TIME => {
                    let value = match property {
                        STAT_ACCESS_TIME => values.access_time,
                        STAT_MODIFY_TIME => values.modify_time,
                        STAT_CHANGE_TIME => values.change_time,
                        STAT_CREATE_TIME => values.create_time,
                        _ => unreachable!(),
                    };
                    if let Some(value) = value {
                        returned_mask |= property;
                        write_word(value.seconds.to_le_bytes());
                        write_word(value.nanoseconds.to_le_bytes());
                    } else {
                        offset += 16;
                    }
                }
                _ => unreachable!(),
            }
        }

        bytes[0..4].copy_from_slice(&(request.encoded_len() as u32).to_le_bytes());
        bytes[4..8].copy_from_slice(&returned_mask.to_le_bytes());
        Self {
            request,
            bytes: bytes.into_boxed_slice(),
        }
    }

    pub(super) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn scalar(&self, property: u32) -> Option<u64> {
        if property.count_ones() != 1 || self.request.mask() & property == 0 {
            return None;
        }
        // Read the fixed slot, not the returned-property mask: legacy open
        // getters exposed an unavailable identity value as zero.
        let property_index = property.trailing_zeros();
        let preceding_words = (0..property_index)
            .filter(|bit| self.request.mask() & (1 << bit) != 0)
            .map(|bit| if bit < 4 { 1 } else { 2 })
            .sum::<usize>();
        self.bytes
            .get(STAT_HEADER_LEN + preceding_words * 8..STAT_HEADER_LEN + (preceding_words + 1) * 8)
            .and_then(|bytes| bytes.try_into().ok())
            .map(u64::from_le_bytes)
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

pub(super) fn run_fstatx_job(
    file: &Resource,
    request: StatRequest,
    result: &mut Option<PackedStat>,
) -> AsyncHostResult<i64> {
    #[cfg(unix)]
    let raw = file.as_file()?.as_raw_fd();
    #[cfg(windows)]
    let raw = if file.resource_class().is_socket() {
        file.as_socket()?.as_raw_socket() as usize as windows_sys::Win32::Foundation::HANDLE
    } else {
        file.as_file()?.as_raw_handle()
    };

    *result = Some(PackedStat::new(
        request,
        platform::query_handle(raw, request)?,
    ));
    Ok(0)
}

pub(super) fn run_statx_job(
    parent: Option<&Resource>,
    path: OsString,
    request: StatRequest,
    follow_symlink: bool,
    result: &mut Option<PackedStat>,
) -> AsyncHostResult<i64> {
    #[cfg(unix)]
    let parent = parent
        .map(|resource| resource.as_file().map(|file| file.as_raw_fd()))
        .transpose()?;
    #[cfg(windows)]
    let parent = parent
        .map(|resource| resource.as_file().map(|file| file.as_raw_handle()))
        .transpose()?;

    *result = Some(PackedStat::new(
        request,
        platform::query_path(parent, path, follow_symlink, request)?,
    ));
    Ok(0)
}

#[cfg(target_os = "linux")]
mod platform {
    use std::ffi::{CString, OsString};
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

    pub(super) fn query_handle(fd: i32, request: StatRequest) -> AsyncHostResult<StatValues> {
        query(fd, c"", AT_EMPTY_PATH, request)
    }

    pub(super) fn query_path(
        parent: Option<i32>,
        path: OsString,
        follow_symlink: bool,
        request: StatRequest,
    ) -> AsyncHostResult<StatValues> {
        let path = CString::new(path.as_bytes()).map_err(|_| AsyncHostError::Inval)?;
        let flags = if follow_symlink {
            0
        } else {
            libc::AT_SYMLINK_NOFOLLOW
        };
        query(parent.unwrap_or(libc::AT_FDCWD), &path, flags, request)
    }

    fn query(
        dirfd: i32,
        path: &std::ffi::CStr,
        flags: i32,
        request: StatRequest,
    ) -> AsyncHostResult<StatValues> {
        let requested_statx_mask = (if request.mask() & STAT_FILE_KIND != 0 {
            STATX_TYPE
        } else {
            0
        }) | (if request.mask() & STAT_FILE_SIZE != 0 {
            STATX_SIZE
        } else {
            0
        }) | (if request.mask() & STAT_FILE_ID != 0 {
            STATX_INO
        } else {
            0
        }) | (if request.mask() & STAT_ACCESS_TIME != 0 {
            STATX_ATIME
        } else {
            0
        }) | (if request.mask() & STAT_MODIFY_TIME != 0 {
            STATX_MTIME
        } else {
            0
        }) | (if request.mask() & STAT_CHANGE_TIME != 0 {
            STATX_CTIME
        } else {
            0
        }) | (if request.mask() & STAT_CREATE_TIME != 0 {
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
            return Err(last_native_error());
        }
        let info = unsafe { info.assume_init() };
        let supported = |property, statx_mask| {
            request.mask() & property != 0 && (statx_mask == 0 || info.stx_mask & statx_mask != 0)
        };
        let timestamp = |value: LinuxStatxTimestamp| StatTimestamp {
            seconds: value.tv_sec,
            nanoseconds: i64::from(value.tv_nsec),
        };

        Ok(StatValues {
            kind: supported(STAT_FILE_KIND, STATX_TYPE).then(|| file_kind(info.stx_mode)),
            size: supported(STAT_FILE_SIZE, STATX_SIZE).then_some(info.stx_size as i64),
            device_id: supported(STAT_DEVICE_ID, 0)
                .then(|| libc::makedev(info.stx_dev_major, info.stx_dev_minor)),
            file_id: supported(STAT_FILE_ID, STATX_INO).then_some(info.stx_ino),
            access_time: supported(STAT_ACCESS_TIME, STATX_ATIME)
                .then(|| timestamp(info.stx_atime)),
            modify_time: supported(STAT_MODIFY_TIME, STATX_MTIME)
                .then(|| timestamp(info.stx_mtime)),
            change_time: supported(STAT_CHANGE_TIME, STATX_CTIME)
                .then(|| timestamp(info.stx_ctime)),
            create_time: supported(STAT_CREATE_TIME, STATX_BTIME)
                .then(|| timestamp(info.stx_btime)),
        })
    }

    fn file_kind(mode: u16) -> i32 {
        match u32::from(mode) & libc::S_IFMT {
            libc::S_IFREG => 1,
            libc::S_IFDIR => 2,
            libc::S_IFLNK => 3,
            libc::S_IFSOCK => 4,
            libc::S_IFIFO => 5,
            libc::S_IFBLK => 6,
            libc::S_IFCHR => 7,
            _ => 0,
        }
    }

    fn last_native_error() -> AsyncHostError {
        AsyncHostError::Native(
            std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EINVAL),
        )
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::{CString, OsString};
    use std::mem::{MaybeUninit, size_of};
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

    pub(super) fn query_handle(fd: i32, request: StatRequest) -> AsyncHostResult<StatValues> {
        query(QueryTarget::Handle(fd), request)
    }

    pub(super) fn query_path(
        parent: Option<i32>,
        path: OsString,
        follow_symlink: bool,
        request: StatRequest,
    ) -> AsyncHostResult<StatValues> {
        let path = CString::new(path.as_bytes()).map_err(|_| AsyncHostError::Inval)?;
        query(
            QueryTarget::Path {
                parent,
                path: &path,
                follow_symlink,
            },
            request,
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

    fn query(target: QueryTarget<'_>, request: StatRequest) -> AsyncHostResult<StatValues> {
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
            if request.mask() & (1 << index) == 0 {
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
            let error = last_native_error();
            if matches!(target, QueryTarget::Handle(_)) && error.errno() == libc::EINVAL {
                return fallback_non_vnode(target, request);
            }
            return Err(error);
        }

        let result_len = read_unaligned::<u32>(&buffer, 0).unwrap_or(0) as usize;
        let returned = read_unaligned::<libc::attribute_set_t>(&buffer, size_of::<u32>())
            .ok_or(AsyncHostError::Io)?;
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

    fn fallback_non_vnode(
        target: QueryTarget<'_>,
        request: StatRequest,
    ) -> AsyncHostResult<StatValues> {
        let QueryTarget::Handle(fd) = target else {
            unreachable!();
        };
        let mut stat = MaybeUninit::<libc::stat>::uninit();
        if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } < 0 {
            return Err(last_native_error());
        }
        let stat = unsafe { stat.assume_init() };
        let requested_time = |property, seconds, nanoseconds| {
            (request.mask() & property != 0).then_some(StatTimestamp {
                seconds,
                nanoseconds,
            })
        };
        Ok(StatValues {
            kind: (request.mask() & STAT_FILE_KIND != 0).then(|| file_kind_from_mode(stat.st_mode)),
            size: (request.mask() & STAT_FILE_SIZE != 0).then_some(stat.st_size),
            device_id: (request.mask() & STAT_DEVICE_ID != 0).then_some(stat.st_dev as u64),
            file_id: (request.mask() & STAT_FILE_ID != 0).then_some(stat.st_ino),
            access_time: requested_time(STAT_ACCESS_TIME, stat.st_atime, stat.st_atime_nsec),
            modify_time: requested_time(STAT_MODIFY_TIME, stat.st_mtime, stat.st_mtime_nsec),
            change_time: requested_time(STAT_CHANGE_TIME, stat.st_ctime, stat.st_ctime_nsec),
            create_time: requested_time(
                STAT_CREATE_TIME,
                stat.st_birthtime,
                stat.st_birthtime_nsec,
            ),
        })
    }

    fn read_unaligned<T: Copy>(buffer: &[u8], offset: usize) -> Option<T> {
        buffer
            .get(offset..offset.checked_add(size_of::<T>())?)
            .map(|bytes| unsafe { std::ptr::read_unaligned(bytes.as_ptr().cast::<T>()) })
    }

    fn file_kind_from_vnode(kind: FsObjType) -> i32 {
        match kind {
            VREG => 1,
            VDIR => 2,
            VLNK => 3,
            VSOCK => 4,
            VFIFO => 5,
            VBLK => 6,
            VCHR => 7,
            _ => 0,
        }
    }

    fn file_kind_from_mode(mode: libc::mode_t) -> i32 {
        match mode & libc::S_IFMT {
            libc::S_IFREG => 1,
            libc::S_IFDIR => 2,
            libc::S_IFLNK => 3,
            libc::S_IFSOCK => 4,
            libc::S_IFIFO => 5,
            libc::S_IFBLK => 6,
            libc::S_IFCHR => 7,
            _ => 0,
        }
    }

    fn last_native_error() -> AsyncHostError {
        AsyncHostError::Native(
            std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EINVAL),
        )
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::OsString;
    use std::mem::MaybeUninit;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE, SetLastError,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO, FILE_BASIC_INFO,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_INFO_BY_HANDLE_CLASS,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_CHAR,
        FILE_TYPE_DISK, FILE_TYPE_PIPE, FILE_TYPE_UNKNOWN, FileAttributeTagInfo, FileBasicInfo,
        GetFileInformationByHandle, GetFileInformationByHandleEx, GetFileSize, GetFileType,
        INVALID_FILE_SIZE, OPEN_EXISTING,
    };

    use super::super::fs::{handle_is_socket, resolve_windows_path_for_parent};
    use super::*;

    #[derive(Default)]
    struct WindowsStat {
        file_type: Option<u32>,
        is_socket: bool,
        attributes: Option<u32>,
        size: Option<i64>,
        device_id: Option<u64>,
        file_id: Option<u64>,
        access_time: Option<i64>,
        modify_time: Option<i64>,
        change_time: Option<i64>,
        create_time: Option<i64>,
    }

    pub(super) fn query_handle(
        handle: HANDLE,
        request: StatRequest,
    ) -> AsyncHostResult<StatValues> {
        let values = query(handle, request)?;
        if values.returned_mask() == 0 {
            get_file_type(handle)?;
        }
        Ok(values)
    }

    pub(super) fn query_path(
        parent: Option<HANDLE>,
        path: OsString,
        follow_symlink: bool,
        request: StatRequest,
    ) -> AsyncHostResult<StatValues> {
        let path = resolve_windows_path_for_parent(parent, path)?;
        let path: Vec<u16> = path.encode_wide().chain(std::iter::once(0)).collect();
        let flags = FILE_ATTRIBUTE_NORMAL
            | FILE_FLAG_BACKUP_SEMANTICS
            | if follow_symlink {
                0
            } else {
                FILE_FLAG_OPEN_REPARSE_POINT
            };
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                flags,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(last_native_error());
        }
        let result = query(handle, request);
        unsafe {
            CloseHandle(handle);
        }
        result
    }

    fn query(handle: HANDLE, request: StatRequest) -> AsyncHostResult<StatValues> {
        let mut stat = WindowsStat::default();
        let mut remaining = request.mask();

        if request.mask() & STAT_FILE_KIND != 0 {
            let (file_type, is_socket) = get_file_type(handle)?;
            stat.file_type = Some(file_type);
            stat.is_socket = is_socket;
            if file_type != FILE_TYPE_DISK {
                return Ok(to_values(request, stat));
            }
        }

        const BASIC_PROPERTIES: u32 = STAT_FILE_KIND
            | STAT_ACCESS_TIME
            | STAT_MODIFY_TIME
            | STAT_CHANGE_TIME
            | STAT_CREATE_TIME;
        if remaining & !STAT_FILE_KIND == 0 {
            if let Some(info) = query_info::<FILE_ATTRIBUTE_TAG_INFO>(handle, FileAttributeTagInfo)
            {
                stat.attributes = Some(info.FileAttributes);
                remaining &= !STAT_FILE_KIND;
            }
        } else if (remaining & STAT_CHANGE_TIME != 0 || remaining & !BASIC_PROPERTIES == 0)
            && let Some(info) = query_info::<FILE_BASIC_INFO>(handle, FileBasicInfo)
        {
            stat.attributes = Some(info.FileAttributes);
            stat.access_time = Some(info.LastAccessTime);
            stat.modify_time = Some(info.LastWriteTime);
            stat.change_time = Some(info.ChangeTime);
            stat.create_time = Some(info.CreationTime);
            remaining &= !BASIC_PROPERTIES;
        }

        if remaining == STAT_FILE_SIZE {
            unsafe { SetLastError(0) };
            let mut high = 0;
            let low = unsafe { GetFileSize(handle, &mut high) };
            if low != INVALID_FILE_SIZE || unsafe { GetLastError() } == 0 {
                stat.size = Some(i64::from(high) << 32 | i64::from(low));
            }
        } else if remaining != 0 {
            let mut info = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
            if unsafe { GetFileInformationByHandle(handle, info.as_mut_ptr()) } != 0 {
                let info = unsafe { info.assume_init() };
                stat.attributes = Some(info.dwFileAttributes);
                stat.size =
                    Some(i64::from(info.nFileSizeHigh) << 32 | i64::from(info.nFileSizeLow));
                stat.device_id = Some(u64::from(info.dwVolumeSerialNumber));
                stat.file_id =
                    Some(u64::from(info.nFileIndexHigh) << 32 | u64::from(info.nFileIndexLow));
                stat.access_time = Some(filetime_ticks(info.ftLastAccessTime));
                stat.modify_time = Some(filetime_ticks(info.ftLastWriteTime));
                stat.create_time = Some(filetime_ticks(info.ftCreationTime));
            }
        }

        Ok(to_values(request, stat))
    }

    fn query_info<T: Copy>(handle: HANDLE, class: FILE_INFO_BY_HANDLE_CLASS) -> Option<T> {
        let mut info = MaybeUninit::<T>::uninit();
        (unsafe {
            GetFileInformationByHandleEx(
                handle,
                class,
                info.as_mut_ptr().cast(),
                std::mem::size_of::<T>() as u32,
            )
        } != 0)
            .then(|| unsafe { info.assume_init() })
    }

    fn to_values(request: StatRequest, stat: WindowsStat) -> StatValues {
        StatValues {
            kind: (request.mask() & STAT_FILE_KIND != 0)
                .then(|| windows_file_kind(stat.file_type?, stat.is_socket, stat.attributes))
                .flatten(),
            size: (request.mask() & STAT_FILE_SIZE != 0)
                .then_some(stat.size)
                .flatten(),
            device_id: (request.mask() & STAT_DEVICE_ID != 0)
                .then_some(stat.device_id)
                .flatten(),
            file_id: (request.mask() & STAT_FILE_ID != 0)
                .then_some(stat.file_id)
                .flatten(),
            access_time: requested_timestamp(request, STAT_ACCESS_TIME, stat.access_time),
            modify_time: requested_timestamp(request, STAT_MODIFY_TIME, stat.modify_time),
            change_time: requested_timestamp(request, STAT_CHANGE_TIME, stat.change_time),
            create_time: requested_timestamp(request, STAT_CREATE_TIME, stat.create_time),
        }
    }

    fn requested_timestamp(
        request: StatRequest,
        property: u32,
        ticks: Option<i64>,
    ) -> Option<StatTimestamp> {
        (request.mask() & property != 0)
            .then_some(ticks)
            .flatten()
            .map(windows_timestamp)
    }

    fn windows_file_kind(file_type: u32, is_socket: bool, attributes: Option<u32>) -> Option<i32> {
        match file_type {
            FILE_TYPE_DISK => attributes.map(|attributes| {
                if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    3
                } else if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                    2
                } else {
                    1
                }
            }),
            FILE_TYPE_CHAR => Some(7),
            FILE_TYPE_PIPE | FILE_TYPE_UNKNOWN => {
                if is_socket {
                    Some(4)
                } else if file_type == FILE_TYPE_PIPE {
                    Some(5)
                } else {
                    Some(0)
                }
            }
            _ => Some(0),
        }
    }

    fn get_file_type(handle: HANDLE) -> AsyncHostResult<(u32, bool)> {
        unsafe { SetLastError(0) };
        let file_type = unsafe { GetFileType(handle) };
        let get_file_type_error = unsafe { GetLastError() };
        let is_socket =
            matches!(file_type, FILE_TYPE_PIPE | FILE_TYPE_UNKNOWN) && handle_is_socket(handle);
        if file_type == FILE_TYPE_UNKNOWN && get_file_type_error != 0 && !is_socket {
            // A Winsock SOCKET is not a kernel HANDLE. Give getsockopt a chance
            // before propagating GetFileType's ERROR_INVALID_HANDLE.
            unsafe { SetLastError(get_file_type_error) };
            Err(last_native_error())
        } else {
            Ok((file_type, is_socket))
        }
    }

    impl StatValues {
        fn returned_mask(&self) -> u32 {
            self.kind.map_or(0, |_| STAT_FILE_KIND)
                | self.size.map_or(0, |_| STAT_FILE_SIZE)
                | self.device_id.map_or(0, |_| STAT_DEVICE_ID)
                | self.file_id.map_or(0, |_| STAT_FILE_ID)
                | self.access_time.map_or(0, |_| STAT_ACCESS_TIME)
                | self.modify_time.map_or(0, |_| STAT_MODIFY_TIME)
                | self.change_time.map_or(0, |_| STAT_CHANGE_TIME)
                | self.create_time.map_or(0, |_| STAT_CREATE_TIME)
        }
    }

    fn filetime_ticks(value: windows_sys::Win32::Foundation::FILETIME) -> i64 {
        i64::from(value.dwHighDateTime) << 32 | i64::from(value.dwLowDateTime)
    }

    fn last_native_error() -> AsyncHostError {
        AsyncHostError::Native(unsafe { GetLastError() } as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::async_sys::internal::event_loop::thread_pool::{
        JobPayload, Resource, get_stat_result, make_fstatx_job,
    };
    use crate::async_sys::internal::event_loop::thread_pool::{make_open_stat_job, run_host_job};

    #[test]
    fn request_layout_is_stable_for_every_supported_mask() {
        for mask in 0..=STAT_SUPPORTED_PROPERTY_MASK {
            let expected_words = (0..8)
                .filter(|bit| mask & (1 << bit) != 0)
                .map(|bit| if bit < 4 { 1 } else { 2 })
                .sum::<usize>();
            let expected_len = STAT_HEADER_LEN + expected_words * 8;

            let request = StatRequest::new(mask, expected_len as i32).unwrap();

            assert_eq!(request.mask(), mask);
            assert_eq!(request.encoded_len(), expected_len);
        }
    }

    #[test]
    fn request_rejects_unknown_properties_and_short_buffers() {
        assert_eq!(StatRequest::new(0x0100, 8), Err(AsyncHostError::Inval));
        assert_eq!(StatRequest::new(0, 7), Err(AsyncHostError::Inval));
        assert_eq!(
            StatRequest::new(STAT_CREATE_TIME, 23),
            Err(AsyncHostError::Inval)
        );
    }

    #[test]
    fn packed_result_uses_fixed_little_endian_slots_and_returned_mask() {
        let mask = STAT_FILE_KIND | STAT_FILE_SIZE | STAT_MODIFY_TIME | STAT_CREATE_TIME;
        let request = StatRequest::new(mask, 56).unwrap();
        let packed = PackedStat::new(
            request,
            StatValues {
                kind: Some(2),
                size: None,
                modify_time: Some(StatTimestamp {
                    seconds: -2,
                    nanoseconds: 123,
                }),
                create_time: None,
                ..StatValues::default()
            },
        );

        assert_eq!(packed.as_bytes().len(), 56);
        assert_eq!(&packed.as_bytes()[0..4], &56_u32.to_le_bytes());
        assert_eq!(
            &packed.as_bytes()[4..8],
            &(STAT_FILE_KIND | STAT_MODIFY_TIME).to_le_bytes()
        );
        assert_eq!(&packed.as_bytes()[8..16], &2_u64.to_le_bytes());
        assert_eq!(&packed.as_bytes()[16..24], &[0; 8]);
        assert_eq!(&packed.as_bytes()[24..32], &(-2_i64).to_le_bytes());
        assert_eq!(&packed.as_bytes()[32..40], &123_i64.to_le_bytes());
        assert_eq!(&packed.as_bytes()[40..56], &[0; 16]);
        assert_eq!(packed.scalar(STAT_FILE_KIND), Some(2));
        assert_eq!(packed.scalar(STAT_FILE_SIZE), Some(0));
        assert_eq!(packed.scalar(STAT_DEVICE_ID), None);
    }

    #[cfg(unix)]
    #[test]
    fn completed_job_keeps_stat_in_host_memory_until_copy_out() {
        use std::os::fd::AsRawFd;

        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), b"moonrun stat").unwrap();
        let raw = unsafe { libc::dup(temp.as_file().as_raw_fd()) };
        assert!(raw >= 0);
        let request = STAT_FILE_KIND | STAT_FILE_SIZE | STAT_DEVICE_ID | STAT_FILE_ID;
        let mut job = make_fstatx_job(
            std::sync::Arc::new(Resource::new(raw)),
            request,
            StatRequest::new(request, 40).unwrap().encoded_len() as i32,
        );

        run_host_job(&mut job);

        assert_eq!(job.err(), 0);
        let JobPayload::Fstatx {
            result: Some(result),
            ..
        } = job.payload()
        else {
            panic!("expected a host-owned packed result");
        };
        assert_eq!(result.as_bytes()[0..4], 40_u32.to_le_bytes());

        let mut guest = [0_u8; 48];
        get_stat_result(&job, &mut guest[..], 4, 40).unwrap();
        assert_eq!(guest[0..4], [0; 4]);
        assert_eq!(guest[4..8], 40_u32.to_le_bytes());
        assert_eq!(guest[8..12], request.to_le_bytes());
        assert_eq!(i64::from_le_bytes(guest[20..28].try_into().unwrap()), 12);
    }

    #[test]
    fn invalid_open_stat_request_fails_before_opening_the_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("must-not-be-created");
        let mut job = make_open_stat_job(
            path.clone().into_os_string(),
            1,
            3,
            false,
            0,
            0o600,
            0x0100,
            8,
        );

        run_host_job(&mut job);

        assert_eq!(job.ret(), -1);
        assert_eq!(job.err(), AsyncHostError::Inval.errno());
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn platform_adapter_distinguishes_a_symlink_from_its_target() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        let link = temp.path().join("link");
        std::fs::write(&target, b"target").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let request = StatRequest::new(STAT_FILE_KIND, 16).unwrap();

        let link_values =
            platform::query_path(None, link.into_os_string(), false, request).unwrap();
        let target_values =
            platform::query_path(None, target.into_os_string(), true, request).unwrap();

        assert_eq!(link_values.kind, Some(3));
        assert_eq!(target_values.kind, Some(1));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_non_vnode_fallback_returns_all_fstat_metadata() {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

        let mut pipe = [0; 2];
        assert_eq!(unsafe { libc::pipe(pipe.as_mut_ptr()) }, 0);
        let read_end = unsafe { OwnedFd::from_raw_fd(pipe[0]) };
        let _write_end = unsafe { OwnedFd::from_raw_fd(pipe[1]) };
        let request = StatRequest::new(STAT_SUPPORTED_PROPERTY_MASK, 104).unwrap();

        let values = platform::query_handle(read_end.as_raw_fd(), request).unwrap();

        assert_eq!(values.kind, Some(5));
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
    fn windows_parent_relative_query_resolves_from_directory_handle() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        };

        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("child"), b"child").unwrap();
        let parent_path: Vec<u16> = temp
            .path()
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let parent = unsafe {
            CreateFileW(
                parent_path.as_ptr(),
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(parent, INVALID_HANDLE_VALUE);
        let request = StatRequest::new(STAT_FILE_KIND | STAT_FILE_SIZE, 24).unwrap();

        let values = platform::query_path(Some(parent), OsString::from("child"), true, request);
        unsafe {
            CloseHandle(parent);
        }
        let values = values.unwrap();

        assert_eq!(values.kind, Some(1));
        assert_eq!(values.size, Some(5));
    }

    #[cfg(windows)]
    #[test]
    fn windows_fstatx_reports_socket_kind_before_get_file_type_error() {
        use crate::async_sys::internal::event_loop::thread_pool::ResourceClass;

        assert_eq!(crate::async_sys::internal::event_loop::io::init_wsa(), 0);
        let raw_socket = crate::async_sys::socket::make_tcp_socket(4).unwrap();
        let resource = Resource::new_socket(raw_socket, ResourceClass::TcpSocket, 4);
        let request = StatRequest::new(STAT_FILE_KIND, 16).unwrap();
        let mut result = None;

        run_fstatx_job(&resource, request, &mut result).unwrap();

        assert_eq!(result.unwrap().scalar(STAT_FILE_KIND), Some(4));

        let request = StatRequest::new(STAT_FILE_SIZE, 16).unwrap();
        let mut result = None;
        run_fstatx_job(&resource, request, &mut result).unwrap();
        assert_eq!(&result.unwrap().as_bytes()[4..8], &0_u32.to_le_bytes());

        drop(resource);
        assert_eq!(crate::async_sys::internal::event_loop::io::cleanup_wsa(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn windows_empty_stat_request_still_validates_the_handle() {
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

        let request = StatRequest::new(0, 8).unwrap();

        assert!(platform::query_handle(INVALID_HANDLE_VALUE, request).is_err());
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
