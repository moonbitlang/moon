// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
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
