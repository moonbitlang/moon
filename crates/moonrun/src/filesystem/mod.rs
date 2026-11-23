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
mod host;
mod job;
mod legacy_metadata;
mod os;
mod path;

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
pub(crate) use job::{Job, stdio_kind};
pub(crate) use os::lock::errno_is_lock_violation;
pub(crate) use path::entry_before_directory_suffix;

#[cfg(test)]
pub(crate) use job::{STAT_OPEN_IDENTITY, compat_symbols};

#[cfg(test)]
pub(crate) use os::io::PORTED_SYMBOLS as IO_PORTED_SYMBOLS;

#[cfg(test)]
pub(crate) use os::lock::PORTED_SYMBOLS as LOCK_PORTED_SYMBOLS;
