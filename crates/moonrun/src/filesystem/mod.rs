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
pub(crate) mod directory_entries;
mod host;
mod job;
mod metadata;
mod os;
#[cfg_attr(target_os = "linux", path = "watch/inotify.rs")]
#[cfg_attr(target_os = "macos", path = "watch/kqueue.rs")]
#[cfg_attr(windows, path = "watch/windows.rs")]
pub(crate) mod watch;

pub(crate) use authorization::entry_before_directory_suffix;
pub(crate) use authorization::{
    FileGrant, FsAuthorizer, FsOperation, FsPath, FsTarget, PolicyAuthorizer,
};
pub(crate) use host::{Directory, FileOpenOptions, FsError, HostFs};
pub(crate) use job::Job;
pub(crate) use os::RawFile;
pub(crate) use os::lock::errno_is_lock_violation;
pub(crate) use os::pipe::pipe;
#[cfg(unix)]
pub(crate) use os::pipe::set_cloexec;

#[cfg(test)]
pub(crate) use host::TEMP_DIR_PORTED_SYMBOLS;

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
