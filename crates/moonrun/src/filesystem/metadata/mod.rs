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

//! Shared metadata queries and async file-kind encoding.

pub(super) mod legacy;
use super::os;

// Discriminants of moonbitlang/async's FileKind ABI.
pub(super) const FILE_KIND_UNKNOWN: i32 = 0;
pub(super) const FILE_KIND_REGULAR: i32 = 1;
pub(super) const FILE_KIND_DIRECTORY: i32 = 2;
pub(super) const FILE_KIND_SYMLINK: i32 = 3;
pub(super) const FILE_KIND_SOCKET: i32 = 4;
pub(super) const FILE_KIND_PIPE: i32 = 5;
// Only Unix adapters produce this kind; ABI value 6 is reserved on all targets.
#[cfg(unix)]
pub(super) const FILE_KIND_BLOCK_DEVICE: i32 = 6;
pub(super) const FILE_KIND_CHAR_DEVICE: i32 = 7;
pub(super) const FILE_KIND_CONSOLE: i32 = 8;

// Callers authorize the operation before querying. An open Job has already
// admitted its identity receipt, so it needs no additional metadata admission.
pub(super) fn query_resource_stat(
    file: &crate::resource::Resource,
    fields: u32,
) -> crate::async_host::AsyncHostResult<os::stat::StatValues> {
    #[cfg(unix)]
    let raw = {
        use std::os::fd::AsRawFd;
        file.as_file()?.as_raw_fd()
    };
    #[cfg(windows)]
    let raw = {
        use std::os::windows::io::{AsRawHandle, AsRawSocket};
        if file.resource_class().is_socket() {
            file.as_socket()?.as_raw_socket() as usize as os::RawFile
        } else {
            file.as_file()?.as_raw_handle()
        }
    };
    Ok(os::stat::query_handle(raw, fields)?)
}

pub(super) fn file_kind_code(kind: os::stat::FileKind) -> i32 {
    use os::stat::FileKind;
    match kind {
        FileKind::Unknown => FILE_KIND_UNKNOWN,
        FileKind::Regular => FILE_KIND_REGULAR,
        FileKind::Directory => FILE_KIND_DIRECTORY,
        FileKind::Symlink => FILE_KIND_SYMLINK,
        FileKind::Socket => FILE_KIND_SOCKET,
        FileKind::Pipe => FILE_KIND_PIPE,
        #[cfg(unix)]
        FileKind::BlockDevice => FILE_KIND_BLOCK_DEVICE,
        FileKind::CharDevice => FILE_KIND_CHAR_DEVICE,
        FileKind::Console => FILE_KIND_CONSOLE,
    }
}
