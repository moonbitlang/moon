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

use super::MoonBuildCli;
use clap::CommandFactory;
use clap_complete::{generate, Shell};
use moonutil::cli::UniversalFlags;
use std::io;

/// Generate shell completion for bash/elvish/fish/pwsh/zsh to stdout
#[derive(Debug, clap::Parser)]
pub struct ShellCompSubCommand {
    /// The shell to generate completion for
    #[clap(value_enum, long, ignore_case = true, value_parser = clap::builder::EnumValueParser::<Shell>::new(), default_value_t = Shell::from_env().unwrap_or(Shell::Bash), value_name = "SHELL")]
    pub shell: Shell,
}

pub fn gen_shellcomp(_cli: &UniversalFlags, cmd: ShellCompSubCommand) -> anyhow::Result<i32> {
    if _cli.dry_run {
        anyhow::bail!("this command has no side effects, dry run is not needed.")
    }
    let mut _moon = MoonBuildCli::command();
    generate(cmd.shell, &mut _moon, "moon", &mut io::stdout());
    Ok(0)
}
