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

//! Caller-owned execution environment for embedding moonrun host services.

use crate::runtime::Stdio;
use crate::{ChildLauncher, WorkingDirectory};
use std::{fs::File, path::Path};

#[derive(Clone, Debug)]
pub struct ExecutionContext {
    pub(crate) working_directory: WorkingDirectory,
    pub(crate) stdio: Stdio,
    pub(crate) child_launcher: Option<ChildLauncher>,
}

impl ExecutionContext {
    /// Bind one run to a directory and streams owned by the caller.
    pub fn new(
        directory: impl AsRef<Path>,
        stdin: File,
        stdout: File,
        stderr: File,
    ) -> std::io::Result<Self> {
        let directory = dunce::canonicalize(directory)?;
        if !directory.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "execution directory must be a directory",
            ));
        }
        Ok(Self {
            working_directory: WorkingDirectory::Fixed(directory),
            stdio: Stdio::owned(stdin, stdout, stderr),
            child_launcher: None,
        })
    }

    /// Prepare or reject each Unix native child after guest policy authorization.
    #[cfg(unix)]
    pub fn with_child_launcher(mut self, launcher: ChildLauncher) -> Self {
        self.child_launcher = Some(launcher);
        self
    }

    pub(crate) fn ambient(working_directory: WorkingDirectory) -> Self {
        Self {
            working_directory,
            stdio: Stdio::Ambient,
            child_launcher: None,
        }
    }
}
