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
mod os;
mod path;

pub(crate) use authorization::{FileGrant, FsOperation, FsPath, FsTarget};
pub(crate) use host::{Directory, FileOpenOptions, FsError, HostFs};
pub(crate) use job::{Job, stdio_kind};
pub(crate) use path::entry_before_directory_suffix;

#[cfg(test)]
pub(crate) use job::{STAT_OPEN_IDENTITY, compat_symbols};
