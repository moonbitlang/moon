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

//! Caller-provided native command preparation after policy authorization.

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::sync::Arc;

/// A native child command passed to a [`ChildCommandPreparer`].
///
/// The command reflects the guest request after process-policy authorization
/// and Runtime working-directory configuration. The preparation callback may
/// replace these fields before moonrun starts the child process.
pub struct NativeCommand {
    /// The executable name or path passed to the native spawn operation.
    pub program: OsString,
    /// The argument vector, including `argv[0]`.
    pub args: Vec<OsString>,
    /// The child environment as native `NAME=value` entries.
    pub env: Vec<OsString>,
    /// The child working directory; an explicit empty path remains empty.
    pub cwd: Option<OsString>,
}

type PrepareFn = dyn Fn(&mut NativeCommand) -> io::Result<()> + Send + Sync;

/// A caller-provided callback that prepares or rejects native child commands.
///
/// Install it on Unix with `ExecutionContext::with_child_command_preparer`.
/// Moonrun checks the guest's process policy before invoking the callback and
/// remains responsible for spawning the child afterwards.
/// Modified commands are trusted caller input and are not reauthorized.
///
/// Clones share the callback, which may be invoked concurrently by host workers.
#[derive(Clone)]
pub struct ChildCommandPreparer(Arc<PrepareFn>);

impl ChildCommandPreparer {
    /// Create a callback for preparing authorized native child commands.
    ///
    /// Returning `Ok(())` allows spawning with the modified command. Returning
    /// an error prevents spawning and reports a permission-denied Job error
    /// to the guest.
    pub fn new(
        prepare: impl Fn(&mut NativeCommand) -> io::Result<()> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(prepare))
    }

    pub(crate) fn prepare(&self, command: &mut NativeCommand) -> io::Result<()> {
        (self.0)(command)
    }
}

impl fmt::Debug for ChildCommandPreparer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChildCommandPreparer")
    }
}
