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

use std::fs::File;
use std::io;
use std::path::Path;

use crate::runtime::Stdio;
use crate::{ChildCommandPreparer, WorkingDirectory};

/// Caller-supplied streams, working directory and native command preparation.
///
/// Pass this value to [`crate::RunOptions::with_context`]. Its fixed directory
/// takes precedence over [`crate::RunOptions::with_working_directory`] without
/// changing the process cwd. The context configures host services; it does not
/// itself grant filesystem permissions or provide an operating-system sandbox.
/// Clones share the streams and any command preparation callback.
#[derive(Clone, Debug)]
pub struct ExecutionContext {
    pub(crate) working_directory: WorkingDirectory,
    pub(crate) stdio: Stdio,
    pub(crate) child_command_preparer: Option<ChildCommandPreparer>,
}

impl ExecutionContext {
    /// Bind one run to a directory and streams owned by the caller.
    ///
    /// The directory must exist and is canonicalized before use. Moonrun takes
    /// ownership of the files and uses them for guest standard I/O and default
    /// child streams. Explicit guest-supplied child streams take precedence.
    /// Returns an I/O error if the directory cannot be resolved or is not a
    /// directory.
    pub fn new(
        directory: impl AsRef<Path>,
        stdin: File,
        stdout: File,
        stderr: File,
    ) -> io::Result<Self> {
        let directory = dunce::canonicalize(directory)?;
        if !directory.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "execution directory must be a directory",
            ));
        }
        Ok(Self {
            working_directory: WorkingDirectory::Fixed(directory),
            stdio: Stdio::owned(stdin, stdout, stderr),
            child_command_preparer: None,
        })
    }

    /// Prepare or reject Unix native child commands after policy authorization.
    ///
    /// The callback runs after cwd configuration and before default stdio and
    /// inherited-policy setup. Moonrun still performs the native spawn.
    #[cfg(unix)]
    pub fn with_child_command_preparer(mut self, preparer: ChildCommandPreparer) -> Self {
        self.child_command_preparer = Some(preparer);
        self
    }

    pub(crate) fn ambient(working_directory: WorkingDirectory) -> Self {
        Self {
            working_directory,
            stdio: Stdio::Ambient,
            child_command_preparer: None,
        }
    }
}
