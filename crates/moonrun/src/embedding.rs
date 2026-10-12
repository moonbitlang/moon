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

use std::{ffi::OsString, fmt, sync::Arc};

pub struct NativeCommand {
    pub program: OsString,
    /// Includes argv[0].
    pub args: Vec<OsString>,
    /// Native NAME=value entries.
    pub env: Vec<OsString>,
    pub cwd: Option<OsString>,
}

type LaunchFn = dyn Fn(&mut NativeCommand) -> std::io::Result<()> + Send + Sync;

#[derive(Clone)]
pub struct ChildLauncher(Arc<LaunchFn>);

impl ChildLauncher {
    /// Prepare or reject an authorized native command before OS execution.
    pub fn new(
        prepare: impl Fn(&mut NativeCommand) -> std::io::Result<()> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(prepare))
    }

    pub(crate) fn prepare(&self, command: &mut NativeCommand) -> std::io::Result<()> {
        (self.0)(command)
    }
}

impl fmt::Debug for ChildLauncher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChildLauncher")
    }
}
