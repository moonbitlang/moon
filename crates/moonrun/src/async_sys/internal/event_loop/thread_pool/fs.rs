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

use std::ffi::OsString;
use std::fs::File;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::fs::dir::EntryRecord;
use crate::async_sys::internal::fd_util;

use super::{GuestBuffer, HostFile, HostFileTable, HostHandle, OpenJobResult};

#[allow(clippy::too_many_arguments)]
pub(super) fn run_open_job(
    files: &mut impl HostFileTable,
    result: &mut Option<OpenJobResult>,
    filename: OsString,
    access: i32,
    create_mode: i32,
    append: bool,
    sync: i32,
    mode: i32,
) -> AsyncHostResult<i64> {
    let OpenedFile {
        file,
        kind,
        dev_id,
        file_id,
    } = open_native_file(filename, access, create_mode, append, sync, mode)?;
    let fd = files.insert_file(file)?;
    *result = Some(OpenJobResult {
        fd,
        kind,
        dev_id,
        file_id,
    });
    Ok(0)
}

pub(super) fn run_read_job(
    files: &mut impl HostFileTable,
    fd: HostHandle,
    dst: GuestBuffer,
    position: i64,
    result: &mut Option<Vec<u8>>,
) -> AsyncHostResult<i64> {
    files.with_file_mut(fd, |file| {
        let mut buf = vec![0; usize::try_from(dst.len).map_err(|_| AsyncHostError::Fault)?];
        let n = read_from_native_file(file, &mut buf, position)?;
        buf.truncate(n);
        *result = Some(buf);
        Ok(n as i64)
    })
}

pub(super) fn run_write_job(
    files: &mut impl HostFileTable,
    fd: HostHandle,
    data: &[u8],
    position: i64,
) -> AsyncHostResult<i64> {
    files.with_file_mut(fd, |file| {
        let n = write_to_native_file(file, data, position)?;
        Ok(n as i64)
    })
}

pub(super) fn run_file_kind_by_path_job(
    files: &mut impl HostFileTable,
    parent: HostHandle,
    path: OsString,
    follow_symlink: bool,
) -> AsyncHostResult<i64> {
    file_kind_by_path(files, parent, path, follow_symlink).map(i64::from)
}

pub(super) fn run_file_size_job(
    files: &mut impl HostFileTable,
    fd: HostHandle,
    result: &mut i64,
) -> AsyncHostResult<i64> {
    files.with_file_mut(fd, |file| {
        *result = file_size(file)?;
        Ok(0)
    })
}

pub(super) fn run_file_time_job(
    files: &mut impl HostFileTable,
    fd: HostHandle,
    result: &mut Option<Vec<u8>>,
) -> AsyncHostResult<i64> {
    files.with_file_mut(fd, |file| {
        *result = Some(file_time(file)?);
        Ok(0)
    })
}

pub(super) fn run_file_time_by_path_job(
    path: OsString,
    follow_symlink: bool,
    result: &mut Option<Vec<u8>>,
) -> AsyncHostResult<i64> {
    *result = Some(file_time_by_path(path, follow_symlink)?);
    Ok(0)
}

pub(super) fn run_access_job(path: OsString, access: i32) -> AsyncHostResult<i64> {
    access_native_path(path, access)?;
    Ok(0)
}

pub(super) fn run_chmod_job(path: OsString, mode: i32) -> AsyncHostResult<i64> {
    chmod_native_path(path, mode)?;
    Ok(0)
}

pub(super) fn run_fsync_job(
    files: &mut impl HostFileTable,
    fd: HostHandle,
    only_data: bool,
) -> AsyncHostResult<i64> {
    files.with_file_mut(fd, |file| {
        sync_native_file(file, only_data)?;
        Ok(0)
    })
}

pub(super) fn run_flock_job(
    files: &mut impl HostFileTable,
    fd: HostHandle,
    exclusive: bool,
) -> AsyncHostResult<i64> {
    #[cfg(windows)]
    {
        let lock_file = files.with_file_mut(fd, |file| {
            let lock_file = file.try_clone().map_err(native_io_error)?;
            lock_native_file(&lock_file, exclusive)?;
            Ok(lock_file)
        })?;
        files.with_host_file_mut(fd, |file| {
            file.set_lock_file(lock_file);
            Ok(0)
        })
    }

    #[cfg(not(windows))]
    files.with_file_mut(fd, |file| {
        lock_native_file(file, exclusive)?;
        Ok(0)
    })
}

pub(super) fn run_remove_job(path: OsString) -> AsyncHostResult<i64> {
    remove_native_path(path)?;
    Ok(0)
}

pub(super) fn run_rename_job(
    old_path: OsString,
    new_path: OsString,
    replace: bool,
) -> AsyncHostResult<i64> {
    rename_native_path(old_path, new_path, replace)?;
    Ok(0)
}

pub(super) fn run_symlink_job(target: OsString, path: OsString) -> AsyncHostResult<i64> {
    symlink_native_path(target, path)?;
    Ok(0)
}

pub(super) fn run_mkdir_job(path: OsString, mode: i32) -> AsyncHostResult<i64> {
    mkdir_native_path(path, mode)?;
    Ok(0)
}

pub(super) fn run_rmdir_job(path: OsString) -> AsyncHostResult<i64> {
    rmdir_native_path(path)?;
    Ok(0)
}

pub(super) fn run_readdir_job(
    files: &mut impl HostFileTable,
    dir: HostHandle,
    dst: GuestBuffer,
    restart: bool,
    result: &mut Option<Vec<u8>>,
) -> AsyncHostResult<i64> {
    files.with_host_file_mut(dir, |file| {
        let len = usize::try_from(dst.len).map_err(|_| AsyncHostError::Fault)?;
        let records = read_native_dir(file, len, restart)?;
        let ret = i64::try_from(records.len()).map_err(|_| AsyncHostError::Fault)?;
        *result = Some(records);
        Ok(ret)
    })
}

#[cfg(unix)]
#[allow(clippy::unnecessary_cast)]
fn open_native_file(
    filename: OsString,
    access: i32,
    create_mode: i32,
    append: bool,
    sync: i32,
    mode: i32,
) -> AsyncHostResult<OpenedFile> {
    use std::ffi::CString;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStringExt;

    let access_flag = match access {
        0 | 3 => libc::O_RDONLY,
        1 => libc::O_WRONLY,
        2 => libc::O_RDWR,
        _ => return Err(AsyncHostError::Inval),
    };
    let create_flag = match create_mode {
        0 => 0,
        1 => libc::O_TRUNC,
        2 => libc::O_CREAT,
        3 => libc::O_CREAT | libc::O_TRUNC,
        4 => libc::O_CREAT | libc::O_EXCL,
        _ => return Err(AsyncHostError::Inval),
    };
    let sync_flag = match sync {
        0 => 0,
        1 => libc::O_DSYNC,
        2 => libc::O_SYNC,
        _ => return Err(AsyncHostError::Inval),
    };
    let append_flag = if append { libc::O_APPEND } else { 0 };
    let filename = CString::new(filename.into_vec()).map_err(|_| AsyncHostError::Inval)?;
    let fd = unsafe {
        libc::open(
            filename.as_ptr(),
            access_flag | sync_flag | create_flag | append_flag | libc::O_CLOEXEC,
            mode as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(last_native_error());
    }

    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } < 0 {
        let error = last_native_error();
        unsafe {
            libc::close(fd);
        }
        return Err(error);
    }
    let stat = unsafe { stat.assume_init() };
    let file = unsafe { File::from_raw_fd(fd) };
    Ok(OpenedFile {
        file,
        kind: file_kind_from_stat(&stat),
        dev_id: stat.st_dev as u64,
        file_id: stat.st_ino as u64,
    })
}

#[cfg(unix)]
fn file_kind_from_stat(stat: &libc::stat) -> i32 {
    match stat.st_mode & libc::S_IFMT {
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

#[cfg(windows)]
fn open_native_file(
    filename: OsString,
    access: i32,
    create_mode: i32,
    append: bool,
    sync: i32,
    _mode: i32,
) -> AsyncHostResult<OpenedFile> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_PIPE_BUSY, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CREATE_ALWAYS, CREATE_NEW, CreateFileW, FILE_APPEND_DATA,
        FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED,
        FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        GetFileInformationByHandle, OPEN_ALWAYS, OPEN_EXISTING, TRUNCATE_EXISTING,
    };
    use windows_sys::Win32::System::Pipes::{NMPWAIT_WAIT_FOREVER, WaitNamedPipeW};

    if !(0..=2).contains(&sync) {
        return Err(AsyncHostError::Inval);
    }
    let mut access_flag = match access {
        0 => GENERIC_READ,
        1 => GENERIC_WRITE,
        2 => GENERIC_READ | GENERIC_WRITE,
        3 => FILE_LIST_DIRECTORY,
        _ => return Err(AsyncHostError::Inval),
    };
    let create_mode = match create_mode {
        0 => OPEN_EXISTING,
        1 => TRUNCATE_EXISTING,
        2 => OPEN_ALWAYS,
        3 => CREATE_ALWAYS,
        4 => CREATE_NEW,
        _ => return Err(AsyncHostError::Inval),
    };
    if append {
        access_flag = (access_flag ^ GENERIC_WRITE) | FILE_APPEND_DATA;
    }
    let mut flags = FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS;
    if access == 3 {
        flags |= FILE_FLAG_OVERLAPPED;
    }

    let filename = filename
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle: HANDLE = loop {
        let handle = unsafe {
            CreateFileW(
                filename.as_ptr(),
                access_flag,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                create_mode,
                flags,
                std::ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            break handle;
        }
        let error = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno());
        if error != ERROR_PIPE_BUSY as i32 {
            return Err(AsyncHostError::Native(error));
        }
        if unsafe { WaitNamedPipeW(filename.as_ptr(), NMPWAIT_WAIT_FOREVER) } == 0 {
            return Err(last_native_error());
        }
    };

    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    if unsafe { GetFileInformationByHandle(handle, info.as_mut_ptr()) } == 0 {
        let error = last_native_error();
        unsafe {
            CloseHandle(handle);
        }
        return Err(error);
    }
    let info = unsafe { info.assume_init() };
    let file = unsafe { File::from_raw_handle(handle as _) };
    Ok(OpenedFile {
        file,
        kind: file_kind_from_attr(info.dwFileAttributes),
        dev_id: u64::from(info.dwVolumeSerialNumber),
        file_id: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    })
}

#[cfg(windows)]
fn file_kind_from_attr(attrs: u32) -> i32 {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    };

    if (attrs & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
        3
    } else if (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0 {
        2
    } else {
        1
    }
}

struct OpenedFile {
    file: File,
    kind: i32,
    dev_id: u64,
    file_id: u64,
}

#[cfg(unix)]
fn read_from_native_file(file: &File, buf: &mut [u8], position: i64) -> AsyncHostResult<usize> {
    use std::os::fd::AsRawFd;

    let ret = if position < 0 {
        unsafe { libc::read(file.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) }
    } else {
        unsafe {
            libc::pread(
                file.as_raw_fd(),
                buf.as_mut_ptr().cast(),
                buf.len(),
                position as libc::off_t,
            )
        }
    };
    native_io_result(ret)
}

#[cfg(unix)]
fn write_to_native_file(file: &File, data: &[u8], position: i64) -> AsyncHostResult<usize> {
    use std::os::fd::AsRawFd;

    let ret = if position < 0 {
        unsafe { libc::write(file.as_raw_fd(), data.as_ptr().cast(), data.len()) }
    } else {
        unsafe {
            libc::pwrite(
                file.as_raw_fd(),
                data.as_ptr().cast(),
                data.len(),
                position as libc::off_t,
            )
        }
    };
    native_io_result(ret)
}

#[cfg(unix)]
fn file_kind_by_path(
    files: &mut impl HostFileTable,
    parent: HostHandle,
    path: OsString,
    follow_symlink: bool,
) -> AsyncHostResult<i32> {
    use std::os::fd::AsRawFd;

    let path = path_to_cstring(path)?;
    let flags = if follow_symlink {
        0
    } else {
        libc::AT_SYMLINK_NOFOLLOW
    };
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    let ret = if parent < 0 {
        unsafe { libc::fstatat(libc::AT_FDCWD, path.as_ptr(), stat.as_mut_ptr(), flags) }
    } else {
        files.with_file_mut(parent, |file| {
            Ok(unsafe { libc::fstatat(file.as_raw_fd(), path.as_ptr(), stat.as_mut_ptr(), flags) })
        })?
    };
    if ret < 0 {
        Err(last_native_error())
    } else {
        Ok(file_kind_from_stat(&unsafe { stat.assume_init() }))
    }
}

#[cfg(unix)]
#[allow(clippy::unnecessary_cast)]
fn file_size(file: &File) -> AsyncHostResult<i64> {
    use std::os::fd::AsRawFd;

    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(file.as_raw_fd(), stat.as_mut_ptr()) } < 0 {
        return Err(last_native_error());
    }
    Ok(unsafe { stat.assume_init() }.st_size as i64)
}

#[cfg(unix)]
fn file_time(file: &File) -> AsyncHostResult<Vec<u8>> {
    use std::os::fd::AsRawFd;

    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(file.as_raw_fd(), stat.as_mut_ptr()) } < 0 {
        return Err(last_native_error());
    }
    Ok(encode_file_time(&unsafe { stat.assume_init() }))
}

#[cfg(unix)]
fn file_time_by_path(path: OsString, follow_symlink: bool) -> AsyncHostResult<Vec<u8>> {
    let path = path_to_cstring(path)?;
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    let ret = if follow_symlink {
        unsafe { libc::stat(path.as_ptr(), stat.as_mut_ptr()) }
    } else {
        unsafe { libc::lstat(path.as_ptr(), stat.as_mut_ptr()) }
    };
    if ret < 0 {
        return Err(last_native_error());
    }
    Ok(encode_file_time(&unsafe { stat.assume_init() }))
}

fn encode_file_time(file_time: &fd_util::stub::FileTime) -> Vec<u8> {
    let mut record = Vec::with_capacity(48);
    record.extend_from_slice(&fd_util::stub::get_atime_sec(file_time).to_le_bytes());
    record.extend_from_slice(&fd_util::stub::get_atime_nsec(file_time).to_le_bytes());
    record.resize(16, 0);
    record.extend_from_slice(&fd_util::stub::get_mtime_sec(file_time).to_le_bytes());
    record.extend_from_slice(&fd_util::stub::get_mtime_nsec(file_time).to_le_bytes());
    record.resize(32, 0);
    record.extend_from_slice(&fd_util::stub::get_ctime_sec(file_time).to_le_bytes());
    record.extend_from_slice(&fd_util::stub::get_ctime_nsec(file_time).to_le_bytes());
    record.resize(48, 0);
    record
}

#[cfg(unix)]
fn access_native_path(path: OsString, access: i32) -> AsyncHostResult<()> {
    let path = path_to_cstring(path)?;
    let mode = match access {
        0 => libc::F_OK,
        1 => libc::R_OK,
        2 => libc::W_OK,
        3 => libc::X_OK,
        _ => return Err(AsyncHostError::Inval),
    };
    if unsafe { libc::access(path.as_ptr(), mode) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn chmod_native_path(path: OsString, mode: i32) -> AsyncHostResult<()> {
    let path = path_to_cstring(path)?;
    if unsafe { libc::chmod(path.as_ptr(), mode as libc::mode_t) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_native_file(file: &File, only_data: bool) -> AsyncHostResult<()> {
    use std::os::fd::AsRawFd;

    #[cfg(target_os = "macos")]
    let ret = {
        let _ = only_data;
        unsafe { libc::fsync(file.as_raw_fd()) }
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let ret = unsafe {
        if only_data {
            libc::fdatasync(file.as_raw_fd())
        } else {
            libc::fsync(file.as_raw_fd())
        }
    };

    if ret < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn lock_native_file(file: &File, exclusive: bool) -> AsyncHostResult<()> {
    use std::os::fd::AsRawFd;

    let operation = if exclusive {
        libc::LOCK_EX
    } else {
        libc::LOCK_SH
    };
    if unsafe { libc::flock(file.as_raw_fd(), operation) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn remove_native_path(path: OsString) -> AsyncHostResult<()> {
    let path = path_to_cstring(path)?;
    if unsafe { libc::remove(path.as_ptr()) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn symlink_native_path(target: OsString, path: OsString) -> AsyncHostResult<()> {
    let target = path_to_cstring(target)?;
    let path = path_to_cstring(path)?;
    if unsafe { libc::symlink(target.as_ptr(), path.as_ptr()) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn mkdir_native_path(path: OsString, mode: i32) -> AsyncHostResult<()> {
    let path = path_to_cstring(path)?;
    if unsafe { libc::mkdir(path.as_ptr(), mode as libc::mode_t) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn rmdir_native_path(path: OsString) -> AsyncHostResult<()> {
    let path = path_to_cstring(path)?;
    if unsafe { libc::rmdir(path.as_ptr()) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(all(unix, target_os = "linux"))]
fn read_native_dir(file: &mut HostFile, len: usize, restart: bool) -> AsyncHostResult<Vec<u8>> {
    use std::os::fd::AsRawFd;

    let fd = file.file_mut().as_raw_fd();
    if restart {
        file.pending_dir_entries_mut().clear();
        if unsafe { libc::lseek(fd, 0, libc::SEEK_SET) } < 0 {
            return Err(last_native_error());
        }
    }

    let mut out = drain_pending_dir_entries(file, len)?;
    if !out.is_empty() {
        return Ok(out);
    }

    let mut native = vec![0; len];
    let ret = unsafe { libc::syscall(libc::SYS_getdents64, fd, native.as_mut_ptr(), native.len()) };
    if ret < 0 {
        return Err(last_native_error());
    }
    if ret == 0 {
        return Ok(out);
    }

    let ret = usize::try_from(ret).map_err(|_| AsyncHostError::Fault)?;
    let mut offset = 0usize;
    while offset < ret {
        let reclen_end = offset.checked_add(18).ok_or(AsyncHostError::Fault)?;
        let fixed_end = offset.checked_add(19).ok_or(AsyncHostError::Fault)?;
        if fixed_end > ret {
            return Err(AsyncHostError::Fault);
        }

        let file_id = u64::from_le_bytes(
            native[offset..offset + 8]
                .try_into()
                .map_err(|_| AsyncHostError::Fault)?,
        );
        let reclen = u16::from_le_bytes(
            native[offset + 16..reclen_end]
                .try_into()
                .map_err(|_| AsyncHostError::Fault)?,
        ) as usize;
        if reclen == 0 || offset.checked_add(reclen).is_none_or(|end| end > ret) {
            return Err(AsyncHostError::Fault);
        }

        let d_type = native[offset + 18];
        let name_start = offset + 19;
        let name_end = native[name_start..offset + reclen]
            .iter()
            .position(|byte| *byte == 0)
            .map(|pos| name_start + pos)
            .ok_or(AsyncHostError::Fault)?;
        let name = native[name_start..name_end].to_vec();
        file.pending_dir_entries_mut()
            .push_back(encode_dir_entry(EntryRecord {
                is_hidden: name.first() == Some(&b'.'),
                is_dir: match d_type {
                    libc::DT_UNKNOWN => -1,
                    libc::DT_DIR => 1,
                    _ => 0,
                },
                name,
                file_id,
            })?);

        offset += reclen;
    }

    out = drain_pending_dir_entries(file, len)?;
    Ok(out)
}

#[cfg(all(unix, target_os = "macos"))]
fn read_native_dir(file: &mut HostFile, len: usize, restart: bool) -> AsyncHostResult<Vec<u8>> {
    use std::os::fd::AsRawFd;

    let fd = file.file_mut().as_raw_fd();
    if restart {
        file.pending_dir_entries_mut().clear();
        if unsafe { libc::lseek(fd, 0, libc::SEEK_SET) } < 0 {
            return Err(last_native_error());
        }
    }

    let mut out = drain_pending_dir_entries(file, len)?;
    if !out.is_empty() {
        return Ok(out);
    }

    let mut attr_spec = libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: libc::ATTR_CMN_RETURNED_ATTRS
            | libc::ATTR_CMN_NAME
            | libc::ATTR_CMN_OBJTYPE
            | libc::ATTR_CMN_FILEID,
        volattr: 0,
        dirattr: 0,
        fileattr: 0,
        forkattr: 0,
    };
    let mut native = vec![0; len];
    let ret = unsafe {
        libc::getattrlistbulk(
            fd,
            (&mut attr_spec as *mut libc::attrlist).cast(),
            native.as_mut_ptr().cast(),
            native.len(),
            0,
        )
    };
    if ret < 0 {
        return Err(last_native_error());
    }
    if ret == 0 {
        return Ok(out);
    }

    let mut offset = 0usize;
    for _ in 0..ret {
        let record_end = offset.checked_add(44).ok_or(AsyncHostError::Fault)?;
        if record_end > native.len() {
            return Err(AsyncHostError::Fault);
        }

        let reclen = u32::from_ne_bytes(
            native[offset..offset + 4]
                .try_into()
                .map_err(|_| AsyncHostError::Fault)?,
        ) as usize;
        if reclen == 0
            || offset
                .checked_add(reclen)
                .is_none_or(|end| end > native.len())
        {
            return Err(AsyncHostError::Fault);
        }

        let commonattr = u32::from_ne_bytes(
            native[offset + 4..offset + 8]
                .try_into()
                .map_err(|_| AsyncHostError::Fault)?,
        );
        let name_ref_offset = i32::from_ne_bytes(
            native[offset + 24..offset + 28]
                .try_into()
                .map_err(|_| AsyncHostError::Fault)?,
        );
        let name_len = u32::from_ne_bytes(
            native[offset + 28..offset + 32]
                .try_into()
                .map_err(|_| AsyncHostError::Fault)?,
        );
        let d_type = i32::from_ne_bytes(
            native[offset + 32..offset + 36]
                .try_into()
                .map_err(|_| AsyncHostError::Fault)?,
        );
        let file_id = u64::from_ne_bytes(
            native[offset + 36..offset + 44]
                .try_into()
                .map_err(|_| AsyncHostError::Fault)?,
        );

        let name_ref_base = offset.checked_add(24).ok_or(AsyncHostError::Fault)?;
        let name_start = name_ref_base
            .checked_add(usize::try_from(name_ref_offset).map_err(|_| AsyncHostError::Fault)?)
            .ok_or(AsyncHostError::Fault)?;
        let name_len = usize::try_from(name_len).map_err(|_| AsyncHostError::Fault)?;
        let name_len = name_len.checked_sub(1).ok_or(AsyncHostError::Fault)?;
        let name_end = name_start
            .checked_add(name_len)
            .ok_or(AsyncHostError::Fault)?;
        if name_end > offset + reclen {
            return Err(AsyncHostError::Fault);
        }

        let name = native[name_start..name_end].to_vec();
        // vnode.h defines VNON = 0 and VDIR = 2. libc does not currently expose
        // these macOS constants, so keep the native stub's meaning explicit here.
        let is_dir = if (commonattr & libc::ATTR_CMN_OBJTYPE) == 0 || d_type == 0 {
            -1
        } else if d_type == 2 {
            1
        } else {
            0
        };
        file.pending_dir_entries_mut()
            .push_back(encode_dir_entry(EntryRecord {
                is_hidden: name.first() == Some(&b'.'),
                is_dir,
                name,
                file_id,
            })?);

        offset += reclen;
    }

    out = drain_pending_dir_entries(file, len)?;
    Ok(out)
}

fn drain_pending_dir_entries(file: &mut HostFile, len: usize) -> AsyncHostResult<Vec<u8>> {
    let mut out = Vec::new();
    while let Some(entry) = file.pending_dir_entries_mut().front() {
        if out.len() + entry.len() > len {
            if out.is_empty() {
                return Err(AsyncHostError::Inval);
            }
            break;
        }
        let entry = file
            .pending_dir_entries_mut()
            .pop_front()
            .ok_or(AsyncHostError::Inval)?;
        out.extend_from_slice(&entry);
    }
    Ok(out)
}

fn encode_dir_entry(entry: EntryRecord) -> AsyncHostResult<Vec<u8>> {
    let mut encoded = Vec::new();
    entry.encode_into(&mut encoded)?;
    Ok(encoded)
}

#[cfg(all(unix, target_os = "linux"))]
fn rename_native_path(
    old_path: OsString,
    new_path: OsString,
    replace: bool,
) -> AsyncHostResult<()> {
    let old_path = path_to_cstring(old_path)?;
    let new_path = path_to_cstring(new_path)?;
    let flags = if replace { 0 } else { libc::RENAME_NOREPLACE };
    let ret = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            old_path.as_ptr(),
            libc::AT_FDCWD,
            new_path.as_ptr(),
            flags,
        )
    };
    if ret < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(all(unix, target_os = "macos"))]
fn rename_native_path(
    old_path: OsString,
    new_path: OsString,
    replace: bool,
) -> AsyncHostResult<()> {
    let old_path = path_to_cstring(old_path)?;
    let new_path = path_to_cstring(new_path)?;
    let flags = if replace { 0 } else { libc::RENAME_EXCL };
    let ret = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            old_path.as_ptr(),
            libc::AT_FDCWD,
            new_path.as_ptr(),
            flags,
        )
    };
    if ret < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn path_to_cstring(path: OsString) -> AsyncHostResult<std::ffi::CString> {
    use std::os::unix::ffi::OsStringExt;

    std::ffi::CString::new(path.into_vec()).map_err(|_| AsyncHostError::Inval)
}

#[cfg(unix)]
fn native_io_result(ret: libc::ssize_t) -> AsyncHostResult<usize> {
    if ret < 0 {
        Err(last_native_error())
    } else {
        usize::try_from(ret).map_err(|_| AsyncHostError::Fault)
    }
}

#[cfg(windows)]
fn read_from_native_file(file: &File, buf: &mut [u8], position: i64) -> AsyncHostResult<usize> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::ReadFile;
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let overlapped = std::mem::MaybeUninit::<OVERLAPPED>::zeroed();
    let overlapped = unsafe {
        let mut overlapped = overlapped.assume_init();
        if position > 0 {
            overlapped.Anonymous.Anonymous.Offset = position as u32;
            overlapped.Anonymous.Anonymous.OffsetHigh = (position >> 32) as u32;
        }
        overlapped
    };
    let mut overlapped = overlapped;
    let mut bytes_transferred = 0;
    let overlapped_ptr = if position < 0 {
        std::ptr::null_mut()
    } else {
        &mut overlapped
    };
    let handle = file.as_raw_handle() as HANDLE;
    // Synchronous Windows file handles can advance the current file pointer
    // even when ReadFile receives an OVERLAPPED offset. Keep read_at/pread
    // semantics by restoring the original pointer before returning.
    let saved_position = if position < 0 {
        None
    } else {
        Some(current_file_pointer(handle)?)
    };
    let result = unsafe {
        ReadFile(
            handle,
            buf.as_mut_ptr().cast(),
            u32::try_from(buf.len()).map_err(|_| AsyncHostError::Fault)?,
            &mut bytes_transferred,
            overlapped_ptr,
        )
    };
    let result = if result != 0 {
        usize::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)
    } else {
        Err(last_native_error())
    };
    if let Some(saved_position) = saved_position
        && let Err(restore_error) = restore_file_pointer(handle, saved_position)
    {
        return match result {
            Ok(_) => Err(restore_error),
            Err(error) => Err(error),
        };
    }
    result
}

#[cfg(windows)]
fn write_to_native_file(file: &File, data: &[u8], position: i64) -> AsyncHostResult<usize> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::WriteFile;
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let overlapped = std::mem::MaybeUninit::<OVERLAPPED>::zeroed();
    let overlapped = unsafe {
        let mut overlapped = overlapped.assume_init();
        if position > 0 {
            overlapped.Anonymous.Anonymous.Offset = position as u32;
            overlapped.Anonymous.Anonymous.OffsetHigh = (position >> 32) as u32;
        }
        overlapped
    };
    let mut overlapped = overlapped;
    let mut bytes_transferred = 0;
    let overlapped_ptr = if position < 0 {
        std::ptr::null_mut()
    } else {
        &mut overlapped
    };
    let handle = file.as_raw_handle() as HANDLE;
    // See read_from_native_file: positioned writes must not alter the stream
    // offset seen by following non-positioned writes.
    let saved_position = if position < 0 {
        None
    } else {
        Some(current_file_pointer(handle)?)
    };
    let result = unsafe {
        WriteFile(
            handle,
            data.as_ptr().cast(),
            u32::try_from(data.len()).map_err(|_| AsyncHostError::Fault)?,
            &mut bytes_transferred,
            overlapped_ptr,
        )
    };
    let result = if result != 0 {
        usize::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)
    } else {
        Err(last_native_error())
    };
    if let Some(saved_position) = saved_position
        && let Err(restore_error) = restore_file_pointer(handle, saved_position)
    {
        return match result {
            Ok(_) => Err(restore_error),
            Err(error) => Err(error),
        };
    }
    result
}

#[cfg(windows)]
fn current_file_pointer(handle: windows_sys::Win32::Foundation::HANDLE) -> AsyncHostResult<i64> {
    use windows_sys::Win32::Storage::FileSystem::{FILE_CURRENT, SetFilePointerEx};

    let mut current = 0;
    if unsafe { SetFilePointerEx(handle, 0, &mut current, FILE_CURRENT) } == 0 {
        Err(last_native_error())
    } else {
        Ok(current)
    }
}

#[cfg(windows)]
fn restore_file_pointer(
    handle: windows_sys::Win32::Foundation::HANDLE,
    position: i64,
) -> AsyncHostResult<()> {
    use windows_sys::Win32::Storage::FileSystem::{FILE_BEGIN, SetFilePointerEx};

    if unsafe { SetFilePointerEx(handle, position, std::ptr::null_mut(), FILE_BEGIN) } == 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn file_kind_by_path(
    files: &mut impl HostFileTable,
    parent: HostHandle,
    path: OsString,
    follow_symlink: bool,
) -> AsyncHostResult<i32> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, GetFileInformationByHandle, OPEN_EXISTING,
    };

    let mut flags = FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS;
    if !follow_symlink {
        flags |= FILE_FLAG_OPEN_REPARSE_POINT;
    }
    let path = resolve_windows_path_for_parent(files, parent, path)?;
    let path = path_to_wide(path);
    let handle: HANDLE = unsafe {
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
    let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    if unsafe { GetFileInformationByHandle(handle, info.as_mut_ptr()) } == 0 {
        let error = last_native_error();
        unsafe {
            CloseHandle(handle);
        }
        return Err(error);
    }
    unsafe {
        CloseHandle(handle);
    }
    Ok(file_kind_from_attr(
        unsafe { info.assume_init() }.dwFileAttributes,
    ))
}

#[cfg(windows)]
fn resolve_windows_path_for_parent(
    files: &mut impl HostFileTable,
    parent: HostHandle,
    path: OsString,
) -> AsyncHostResult<OsString> {
    if parent < 0 || std::path::Path::new(&path).is_absolute() {
        return Ok(path);
    }

    files.with_file_mut(parent, |file| {
        let mut parent_path = std::path::PathBuf::from(final_path_from_handle(file)?);
        parent_path.push(path);
        Ok(parent_path.into_os_string())
    })
}

#[cfg(windows)]
fn final_path_from_handle(file: &File) -> AsyncHostResult<OsString> {
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
    };

    let mut buffer = vec![0u16; 260];
    loop {
        let len = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                buffer.as_mut_ptr(),
                u32::try_from(buffer.len()).map_err(|_| AsyncHostError::Fault)?,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        };
        if len == 0 {
            return Err(last_native_error());
        }
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        if len < buffer.len() {
            buffer.truncate(len);
            return Ok(OsString::from_wide(&buffer));
        }
        buffer.resize(len + 1, 0);
    }
}

#[cfg(windows)]
fn file_size(file: &File) -> AsyncHostResult<i64> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::GetFileSizeEx;

    let mut size = std::mem::MaybeUninit::<i64>::uninit();
    let result = unsafe { GetFileSizeEx(file.as_raw_handle() as HANDLE, size.as_mut_ptr()) };
    if result == 0 {
        Err(last_native_error())
    } else {
        Ok(unsafe { size.assume_init() })
    }
}

#[cfg(windows)]
fn file_time(file: &File) -> AsyncHostResult<Vec<u8>> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_BASIC_INFO, FileBasicInfo, GetFileInformationByHandleEx,
    };

    let mut info = std::mem::MaybeUninit::<FILE_BASIC_INFO>::uninit();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle() as HANDLE,
            FileBasicInfo,
            info.as_mut_ptr().cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(last_native_error());
    }
    Ok(encode_file_time(&unsafe { info.assume_init() }))
}

#[cfg(windows)]
fn file_time_by_path(path: OsString, follow_symlink: bool) -> AsyncHostResult<Vec<u8>> {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_BASIC_INFO, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FileBasicInfo, GetFileInformationByHandleEx, OPEN_EXISTING,
    };

    let mut flags = FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS;
    if !follow_symlink {
        flags |= FILE_FLAG_OPEN_REPARSE_POINT;
    }
    let path = path_to_wide(path);
    let handle: HANDLE = unsafe {
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

    let mut info = std::mem::MaybeUninit::<FILE_BASIC_INFO>::uninit();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileBasicInfo,
            info.as_mut_ptr().cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    };
    unsafe {
        CloseHandle(handle);
    }
    if ok == 0 {
        return Err(last_native_error());
    }
    Ok(encode_file_time(&unsafe { info.assume_init() }))
}

#[cfg(windows)]
fn access_native_path(path: OsString, access: i32) -> AsyncHostResult<()> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_EXECUTE, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let access_mode = match access {
        0 => 0,
        1 => GENERIC_READ,
        2 => GENERIC_WRITE,
        3 => FILE_EXECUTE,
        _ => return Err(AsyncHostError::Inval),
    };
    let path = path_to_wide(path);
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            access_mode,
            FILE_SHARE_DELETE | FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        Err(last_native_error())
    } else {
        unsafe {
            CloseHandle(handle);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn chmod_native_path(_path: OsString, _mode: i32) -> AsyncHostResult<()> {
    Err(AsyncHostError::NotSupported)
}

#[cfg(windows)]
fn sync_native_file(file: &File, _only_data: bool) -> AsyncHostResult<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::FlushFileBuffers;

    if unsafe { FlushFileBuffers(file.as_raw_handle() as HANDLE) } == 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn lock_native_file(file: &File, exclusive: bool) -> AsyncHostResult<()> {
    use std::mem::zeroed;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{LOCKFILE_EXCLUSIVE_LOCK, LockFileEx};
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let mut overlapped: OVERLAPPED = unsafe { zeroed() };
    // Keep parity with thread_pool.c: lock a one-byte sentinel range beyond
    // ordinary file data so Windows mandatory locks approximate advisory locks.
    overlapped.Anonymous.Anonymous.Offset = 0xfffffffe;
    overlapped.Anonymous.Anonymous.OffsetHigh = 0xffffffff;
    let flags = if exclusive {
        LOCKFILE_EXCLUSIVE_LOCK
    } else {
        0
    };
    if unsafe { LockFileEx(file.as_raw_handle(), flags, 0, 1, 0, &mut overlapped) } == 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn remove_native_path(path: OsString) -> AsyncHostResult<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        DeleteFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, GetFileAttributesW,
        INVALID_FILE_ATTRIBUTES, RemoveDirectoryW,
    };

    let path = path_to_wide(path);
    let attrs = unsafe { GetFileAttributesW(path.as_ptr()) };
    if attrs == INVALID_FILE_ATTRIBUTES {
        return Err(last_native_error());
    }

    let is_directory_link =
        (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0 && (attrs & FILE_ATTRIBUTE_REPARSE_POINT) != 0;
    let ok = if is_directory_link {
        // Windows removes directory symlinks and junctions through RemoveDirectoryW,
        // not DeleteFileW, even though they are reparse points rather than real dirs.
        unsafe { RemoveDirectoryW(path.as_ptr()) }
    } else {
        unsafe { DeleteFileW(path.as_ptr()) }
    };
    if ok == 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn symlink_native_path(target: OsString, path: OsString) -> AsyncHostResult<()> {
    use windows_sys::Win32::Storage::FileSystem::{
        CreateSymbolicLinkW, FILE_ATTRIBUTE_DIRECTORY, GetFileAttributesW, INVALID_FILE_ATTRIBUTES,
        SYMBOLIC_LINK_FLAG_DIRECTORY,
    };

    let target_wide = path_to_wide(target.clone());
    let path_wide = path_to_wide(path.clone());
    let attrs = unsafe { GetFileAttributesW(target_wide.as_ptr()) };
    if attrs == INVALID_FILE_ATTRIBUTES {
        return Err(last_native_error());
    }
    let is_directory = (attrs & FILE_ATTRIBUTE_DIRECTORY) != 0;
    let flags = if is_directory {
        SYMBOLIC_LINK_FLAG_DIRECTORY
    } else {
        0
    };
    if unsafe { CreateSymbolicLinkW(path_wide.as_ptr(), target_wide.as_ptr(), flags) } != 0 {
        return Ok(());
    }

    let symlink_error = last_native_error();
    if is_directory && std::path::Path::new(target.as_os_str()).is_absolute() {
        create_junction_native_path(target, path)
    } else {
        Err(symlink_error)
    }
}

#[cfg(windows)]
fn create_junction_native_path(target: OsString, path: OsString) -> AsyncHostResult<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateDirectoryW, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, OPEN_EXISTING,
        RemoveDirectoryW,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;

    let data = junction_reparse_buffer(target)?;
    let path_wide = path_to_wide(path);
    if unsafe { CreateDirectoryW(path_wide.as_ptr(), std::ptr::null()) } == 0 {
        return Err(last_native_error());
    }

    let handle = unsafe {
        CreateFileW(
            path_wide.as_ptr(),
            FILE_WRITE_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let error = last_native_error();
        unsafe {
            RemoveDirectoryW(path_wide.as_ptr());
        }
        return Err(error);
    }

    let mut bytes_returned = 0;
    let ok = unsafe {
        DeviceIoControl(
            handle,
            FSCTL_SET_REPARSE_POINT,
            data.as_ptr().cast(),
            data.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };
    unsafe {
        CloseHandle(handle);
    }
    if ok == 0 {
        let error = last_native_error();
        unsafe {
            RemoveDirectoryW(path_wide.as_ptr());
        }
        Err(error)
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn junction_reparse_buffer(target: OsString) -> AsyncHostResult<Vec<u8>> {
    use windows_sys::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;

    const MOUNT_POINT_HEADER_LEN: usize = 8;
    const WCHAR_SIZE: usize = 2;
    const UNICODE_NULL_SIZE: usize = WCHAR_SIZE;
    const NON_INTERPRETED_PATH_PREFIX: [u16; 4] =
        ['\\' as u16, '?' as u16, '?' as u16, '\\' as u16];

    let mut substitute = Vec::from(NON_INTERPRETED_PATH_PREFIX);
    substitute.extend(os_string_to_wide(target));
    let substitute_len = substitute
        .len()
        .checked_mul(WCHAR_SIZE)
        .ok_or(AsyncHostError::Inval)?;
    let substitute_len = u16::try_from(substitute_len).map_err(|_| AsyncHostError::Inval)?;
    let print_name_offset = substitute_len
        .checked_add(UNICODE_NULL_SIZE as u16)
        .ok_or(AsyncHostError::Inval)?;
    let reparse_data_len = print_name_offset
        .checked_add(MOUNT_POINT_HEADER_LEN as u16)
        .ok_or(AsyncHostError::Inval)?;

    let mut data = Vec::with_capacity(8 + usize::from(reparse_data_len));
    data.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    data.extend_from_slice(&reparse_data_len.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&substitute_len.to_le_bytes());
    data.extend_from_slice(&print_name_offset.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    for code_unit in substitute {
        data.extend_from_slice(&code_unit.to_le_bytes());
    }
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    Ok(data)
}

#[cfg(windows)]
fn mkdir_native_path(path: OsString, _mode: i32) -> AsyncHostResult<()> {
    use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;

    let path = path_to_wide(path);
    if unsafe { CreateDirectoryW(path.as_ptr(), std::ptr::null()) } == 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn rmdir_native_path(path: OsString) -> AsyncHostResult<()> {
    use windows_sys::Win32::Storage::FileSystem::RemoveDirectoryW;

    let path = path_to_wide(path);
    if unsafe { RemoveDirectoryW(path.as_ptr()) } == 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn read_native_dir(file: &mut HostFile, len: usize, restart: bool) -> AsyncHostResult<Vec<u8>> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{ERROR_NO_MORE_FILES, HANDLE};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_ID_BOTH_DIR_INFO, FileIdBothDirectoryInfo, FileIdBothDirectoryRestartInfo,
        GetFileInformationByHandleEx,
    };

    if restart {
        file.pending_dir_entries_mut().clear();
    }

    let mut out = drain_pending_dir_entries(file, len)?;
    if !out.is_empty() {
        return Ok(out);
    }

    let mut native = vec![0; len];
    let info_class = if restart {
        FileIdBothDirectoryRestartInfo
    } else {
        FileIdBothDirectoryInfo
    };
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.file_mut().as_raw_handle() as HANDLE,
            info_class,
            native.as_mut_ptr().cast(),
            u32::try_from(native.len()).map_err(|_| AsyncHostError::Fault)?,
        )
    };
    if ok == 0 {
        let error = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno());
        if error == ERROR_NO_MORE_FILES as i32 {
            return Ok(out);
        }
        return Err(AsyncHostError::Native(error));
    }

    let mut offset = 0usize;
    loop {
        let fixed_end = offset
            .checked_add(std::mem::size_of::<FILE_ID_BOTH_DIR_INFO>())
            .ok_or(AsyncHostError::Fault)?;
        if fixed_end > native.len() {
            return Err(AsyncHostError::Fault);
        }

        let entry = unsafe {
            std::ptr::read_unaligned(native.as_ptr().add(offset).cast::<FILE_ID_BOTH_DIR_INFO>())
        };
        let name_len = usize::try_from(entry.FileNameLength).map_err(|_| AsyncHostError::Fault)?;
        let name_start = offset + std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
        let name_end = name_start
            .checked_add(name_len)
            .ok_or(AsyncHostError::Fault)?;
        if name_end > native.len() {
            return Err(AsyncHostError::Fault);
        }

        let name = native[name_start..name_end].to_vec();
        let is_dir = if (entry.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) == 0
            && (entry.FileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0
        {
            1
        } else {
            0
        };
        file.pending_dir_entries_mut()
            .push_back(encode_dir_entry(EntryRecord {
                is_hidden: (entry.FileAttributes & FILE_ATTRIBUTE_HIDDEN) != 0,
                is_dir,
                name,
                file_id: entry.FileId as u64,
            })?);

        let next = usize::try_from(entry.NextEntryOffset).map_err(|_| AsyncHostError::Fault)?;
        if next == 0 {
            break;
        }
        offset = offset.checked_add(next).ok_or(AsyncHostError::Fault)?;
    }

    out = drain_pending_dir_entries(file, len)?;
    Ok(out)
}

#[cfg(windows)]
fn rename_native_path(
    old_path: OsString,
    new_path: OsString,
    replace: bool,
) -> AsyncHostResult<()> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_INVALID_PARAMETER, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, DELETE, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS, FILE_RENAME_INFO,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileRenameInfoEx,
        MOVEFILE_COPY_ALLOWED, MOVEFILE_REPLACE_EXISTING, MoveFileExW, OPEN_EXISTING,
        SetFileInformationByHandle,
    };

    let old_path = path_to_wide(old_path);
    let new_path = path_to_wide(new_path);
    let handle: HANDLE = unsafe {
        CreateFileW(
            old_path.as_ptr(),
            DELETE,
            FILE_SHARE_DELETE | FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(last_native_error());
    }

    let filename_len = new_path.len().checked_sub(1).ok_or(AsyncHostError::Inval)?;
    let buffer_size = std::mem::size_of::<FILE_RENAME_INFO>()
        .checked_add(filename_len * std::mem::size_of::<u16>())
        .ok_or(AsyncHostError::Fault)?;
    let mut buffer = vec![0u8; buffer_size];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).Anonymous.Flags = if replace { 3 } else { 0 };
        (*info).RootDirectory = std::ptr::null_mut();
        (*info).FileNameLength = u32::try_from(filename_len * std::mem::size_of::<u16>())
            .map_err(|_| AsyncHostError::Fault)?;
        std::ptr::copy_nonoverlapping(
            new_path.as_ptr(),
            (*info).FileName.as_mut_ptr(),
            filename_len,
        );
    }

    let ret = unsafe {
        SetFileInformationByHandle(handle, FileRenameInfoEx, info.cast(), buffer_size as u32)
    };
    unsafe {
        CloseHandle(handle);
    }
    if ret != 0 {
        return Ok(());
    }

    let error = std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_else(|| AsyncHostError::Inval.errno());
    if error != ERROR_INVALID_PARAMETER as i32 {
        return Err(AsyncHostError::Native(error));
    }

    let flags = MOVEFILE_COPY_ALLOWED
        | if replace {
            MOVEFILE_REPLACE_EXISTING
        } else {
            0
        };
    if unsafe { MoveFileExW(old_path.as_ptr(), new_path.as_ptr(), flags) } == 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn path_to_wide(path: OsString) -> Vec<u16> {
    os_string_to_wide(path)
        .into_iter()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn os_string_to_wide(path: OsString) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().collect()
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}

#[cfg(windows)]
fn native_io_error(error: std::io::Error) -> AsyncHostError {
    AsyncHostError::Native(
        error
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}
