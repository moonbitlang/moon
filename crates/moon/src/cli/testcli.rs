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

use std::{ffi::OsString, path::Path, process::Command};

use moonutil::toolchain;

pub(crate) fn prepare(current_dir: Option<&Path>, args: Vec<OsString>) -> anyhow::Result<Command> {
    let mut command = super::process::command_in_effective_dir(current_dir, |dir| {
        Ok(dir.map_or_else(
            || toolchain::BINARIES.moonrun.clone(),
            toolchain::moonrun_in,
        ))
    })?;
    command
        .arg(toolchain::bin().join("mooncram.wasm"))
        .arg("--")
        .args(args);
    Ok(command)
}
