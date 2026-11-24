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

//! Filesystem authority and execution, independent of the Wasm execution backend.

mod authorization;
pub(crate) mod directory;
mod host;
mod job;
mod legacy_metadata;
mod os;
mod path;
mod temp_dir;
#[cfg_attr(target_os = "linux", path = "watch/inotify.rs")]
#[cfg_attr(target_os = "macos", path = "watch/kqueue.rs")]
#[cfg_attr(windows, path = "watch/windows.rs")]
pub(crate) mod watch;

// Discriminants of moonbitlang/async's FileKind ABI.
const FILE_KIND_UNKNOWN: i32 = 0;
const FILE_KIND_REGULAR: i32 = 1;
const FILE_KIND_DIRECTORY: i32 = 2;
const FILE_KIND_SYMLINK: i32 = 3;
const FILE_KIND_SOCKET: i32 = 4;
const FILE_KIND_PIPE: i32 = 5;
// Only Unix adapters produce this kind; ABI value 6 is reserved on all targets.
#[cfg(unix)]
const FILE_KIND_BLOCK_DEVICE: i32 = 6;
const FILE_KIND_CHAR_DEVICE: i32 = 7;
const FILE_KIND_CONSOLE: i32 = 8;

pub(crate) use authorization::{FileGrant, FsOperation, FsPath, FsTarget};
pub(crate) use host::{Directory, FileOpenOptions, FsError, HostFs};
pub(crate) use job::Job;
pub(crate) use os::RawFile;
pub(crate) use os::lock::errno_is_lock_violation;
pub(crate) use os::pipe::pipe;
#[cfg(unix)]
pub(crate) use os::pipe::set_cloexec;
pub(crate) use path::entry_before_directory_suffix;

#[cfg(test)]
pub(crate) use temp_dir::PORTED_SYMBOLS as TEMP_DIR_PORTED_SYMBOLS;

#[cfg(test)]
pub(crate) use job::{STAT_OPEN_IDENTITY, compat_symbols};

#[cfg(test)]
pub(crate) use os::pipe::PORTED_SYMBOLS as PIPE_PORTED_SYMBOLS;

#[cfg(test)]
pub(crate) use os::io::PORTED_SYMBOLS as IO_PORTED_SYMBOLS;

#[cfg(test)]
pub(crate) use os::lock::PORTED_SYMBOLS as LOCK_PORTED_SYMBOLS;

#[cfg(all(test, unix))]
pub(crate) use os::watch::PORTED_SYMBOLS as WATCH_PORTED_SYMBOLS;

// Callers authorize the operation before querying. An open Job has already
// admitted its identity receipt and must not recheck the resource's saved path.
fn query_resource_stat(
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

fn file_kind_code(kind: os::stat::FileKind) -> i32 {
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
