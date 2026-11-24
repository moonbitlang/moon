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

use std::mem::MaybeUninit;

use windows_sys::Win32::Foundation::{GetLastError, HANDLE, SetLastError};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_ATTRIBUTE_TAG_INFO, FILE_BASIC_INFO, FILE_INFO_BY_HANDLE_CLASS, FILE_TYPE_CHAR,
    FILE_TYPE_DISK, FILE_TYPE_PIPE, FILE_TYPE_UNKNOWN, FileAttributeTagInfo, FileBasicInfo,
    GetFileInformationByHandle, GetFileInformationByHandleEx, GetFileSize, INVALID_FILE_SIZE,
};

use super::*;
use crate::filesystem::os::metadata;

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

pub(super) fn query_handle(handle: HANDLE, fields: u32) -> io::Result<StatValues> {
    let values = query(handle, fields)?;
    // Even an empty or unsupported query must validate the native handle.
    if values == StatValues::default() {
        metadata::file_type(handle)?;
    }
    Ok(values)
}

pub(super) fn query(handle: HANDLE, fields: u32) -> io::Result<StatValues> {
    let mut stat = WindowsStat::default();
    let mut remaining = fields;

    if fields & KIND != 0 {
        let (file_type, is_socket) = metadata::file_type(handle)?;
        stat.file_type = Some(file_type);
        stat.is_socket = is_socket;
        if file_type != FILE_TYPE_DISK {
            let mut values = to_values(fields, stat);
            let mut mode = 0;
            if file_type == FILE_TYPE_CHAR
                && unsafe { windows_sys::Win32::System::Console::GetConsoleMode(handle, &mut mode) }
                    != 0
            {
                values.kind = Some(FileKind::Console);
            }
            return Ok(values);
        }
    }

    const BASIC_PROPERTIES: u32 = KIND | ACCESS_TIME | MODIFY_TIME | CHANGE_TIME | CREATE_TIME;
    if remaining & !KIND == 0 {
        if let Some(info) = query_info::<FILE_ATTRIBUTE_TAG_INFO>(handle, FileAttributeTagInfo) {
            stat.attributes = Some(info.FileAttributes);
            remaining &= !KIND;
        }
    } else if (remaining & CHANGE_TIME != 0 || remaining & !BASIC_PROPERTIES == 0)
        && let Some(info) = query_info::<FILE_BASIC_INFO>(handle, FileBasicInfo)
    {
        stat.attributes = Some(info.FileAttributes);
        stat.access_time = Some(info.LastAccessTime);
        stat.modify_time = Some(info.LastWriteTime);
        stat.change_time = Some(info.ChangeTime);
        stat.create_time = Some(info.CreationTime);
        remaining &= !BASIC_PROPERTIES;
    }

    if remaining == SIZE {
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
            stat.size = Some(i64::from(info.nFileSizeHigh) << 32 | i64::from(info.nFileSizeLow));
            stat.device_id = Some(u64::from(info.dwVolumeSerialNumber));
            stat.file_id =
                Some(u64::from(info.nFileIndexHigh) << 32 | u64::from(info.nFileIndexLow));
            stat.access_time = Some(filetime_ticks(info.ftLastAccessTime));
            stat.modify_time = Some(filetime_ticks(info.ftLastWriteTime));
            stat.create_time = Some(filetime_ticks(info.ftCreationTime));
        }
    }

    Ok(to_values(fields, stat))
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

fn to_values(fields: u32, stat: WindowsStat) -> StatValues {
    StatValues {
        kind: (fields & KIND != 0)
            .then(|| windows_file_kind(stat.file_type?, stat.is_socket, stat.attributes))
            .flatten(),
        size: (fields & SIZE != 0).then_some(stat.size).flatten(),
        device_id: (fields & DEVICE_ID != 0)
            .then_some(stat.device_id)
            .flatten(),
        file_id: (fields & FILE_ID != 0).then_some(stat.file_id).flatten(),
        access_time: requested_timestamp(fields, ACCESS_TIME, stat.access_time),
        modify_time: requested_timestamp(fields, MODIFY_TIME, stat.modify_time),
        change_time: requested_timestamp(fields, CHANGE_TIME, stat.change_time),
        create_time: requested_timestamp(fields, CREATE_TIME, stat.create_time),
    }
}

fn requested_timestamp(fields: u32, property: u32, ticks: Option<i64>) -> Option<StatTimestamp> {
    (fields & property != 0)
        .then_some(ticks)
        .flatten()
        .map(windows_timestamp)
}

fn windows_file_kind(file_type: u32, is_socket: bool, attributes: Option<u32>) -> Option<FileKind> {
    match file_type {
        FILE_TYPE_DISK => attributes.map(|attributes| {
            if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                FileKind::Symlink
            } else if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
                FileKind::Directory
            } else {
                FileKind::Regular
            }
        }),
        FILE_TYPE_CHAR => Some(FileKind::CharDevice),
        FILE_TYPE_PIPE | FILE_TYPE_UNKNOWN => {
            if is_socket {
                Some(FileKind::Socket)
            } else if file_type == FILE_TYPE_PIPE {
                Some(FileKind::Pipe)
            } else {
                Some(FileKind::Unknown)
            }
        }
        _ => Some(FileKind::Unknown),
    }
}

fn filetime_ticks(value: windows_sys::Win32::Foundation::FILETIME) -> i64 {
    i64::from(value.dwHighDateTime) << 32 | i64::from(value.dwLowDateTime)
}
