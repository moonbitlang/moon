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

mod grant;
mod host;
mod job;
mod os;

pub(crate) use grant::{FileGrant, FileOperation};
pub(crate) use host::{Directory, FileOpenOptions, FsError, HostFs, entry_before_directory_suffix};
pub(crate) use job::{Job, stdio_kind};

#[cfg(test)]
pub(crate) use job::{STAT_OPEN_IDENTITY, compat_symbols};
