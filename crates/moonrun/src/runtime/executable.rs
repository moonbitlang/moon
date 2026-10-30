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

//! Executable identity exposed by one Runtime.
//!
//! A file-backed module captures its absolute load path once. The path is a
//! name, not a live file identity: later cwd, rename, and removal changes do
//! not alter it. Modules compiled from bytes have no executable path.

use std::path::{Path, PathBuf};

use crate::async_host::{AsyncHostError, AsyncHostResult};

#[derive(Clone, Debug)]
pub(crate) struct Executable(AsyncHostResult<PathBuf>);

impl Executable {
    pub(crate) fn from_file(path: &Path) -> Self {
        Self(std::path::absolute(path).map_err(io_error))
    }

    pub(crate) fn unavailable() -> Self {
        Self(Err(AsyncHostError::Inval))
    }

    pub(crate) fn path(&self) -> AsyncHostResult<&Path> {
        self.0.as_deref().map_err(|error| *error)
    }

    pub(crate) fn directory(&self) -> AsyncHostResult<&Path> {
        self.path()?.parent().ok_or(AsyncHostError::Inval)
    }
}

fn io_error(error: std::io::Error) -> AsyncHostError {
    error
        .raw_os_error()
        .map(AsyncHostError::Native)
        .unwrap_or(AsyncHostError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_path_is_made_absolute_without_accessing_the_file() {
        let path = Path::new("missing/../guest.wasm");
        let executable = Executable::from_file(path);

        assert_eq!(
            executable.path().unwrap(),
            std::path::absolute(path).unwrap()
        );
        assert_eq!(
            executable.directory().unwrap(),
            std::path::absolute(path).unwrap().parent().unwrap()
        );
    }

    #[test]
    fn in_memory_module_has_no_executable_path() {
        assert_eq!(Executable::unavailable().path(), Err(AsyncHostError::Inval));
        assert_eq!(
            Executable::unavailable().directory(),
            Err(AsyncHostError::Inval)
        );
    }
}
