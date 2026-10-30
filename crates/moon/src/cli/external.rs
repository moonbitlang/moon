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

use anyhow::{Context as _, bail};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

use super::process;

pub(crate) fn prepare_external(
    args: impl IntoIterator<Item = OsString>,
    current_dir: Option<&Path>,
) -> anyhow::Result<Command> {
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        bail!("no external subcommand provided")
    };
    let subcmd = subcmd
        .to_str()
        .context("external subcommand name is not valid UTF-8")?;
    let mut command = process::command_in_effective_dir(current_dir, |current_dir| {
        resolve_external_subcommand_in(subcmd, current_dir)
    })?;
    command.args(args);
    Ok(command)
}

fn resolve_external_subcommand_in(
    subcmd: &str,
    current_dir: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    if subcmd == "-" {
        bail!(
            "`-` is only supported in `moon run -`, which reads `.mbtx` source from stdin.\n\
             Try: `moon run -`"
        );
    }
    let bin = &format!("moon-{subcmd}");
    let resolved = match current_dir {
        Some(dir) => moonutil::toolchain::resolve_executable_in(bin, dir),
        None => moonutil::toolchain::resolve_executable(bin),
    };
    resolved.with_context(|| {
        format!(
            "no such subcommand: `{subcmd}`, is `{bin}` a valid executable accessible via your `PATH`?"
        )
    })
}
