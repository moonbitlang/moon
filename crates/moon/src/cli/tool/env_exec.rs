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

use std::{collections::BTreeMap, path::PathBuf, process::Command};

use anyhow::{Context, bail};

#[derive(Debug, clap::Parser)]
pub(crate) struct EnvExec {
    /// The serialized environment file to merge into the child process.
    #[clap(long, value_name = "PATH")]
    pub env_file: PathBuf,

    /// The command to run after applying the detected environment.
    #[clap(
        name = "args",
        trailing_var_arg = true,
        allow_hyphen_values = true,
        required = true
    )]
    pub args: Vec<String>,
}

pub(crate) fn run_env_exec(cmd: EnvExec) -> anyhow::Result<i32> {
    let detected_env: BTreeMap<String, String> = serde_json::from_str(
        &std::fs::read_to_string(&cmd.env_file)
            .with_context(|| format!("Failed to read {}", cmd.env_file.display()))?,
    )
    .with_context(|| format!("Failed to parse {}", cmd.env_file.display()))?;

    let Some((program, args)) = cmd.args.split_first() else {
        bail!("no program provided to `moon tool env-exec`");
    };

    let mut child = Command::new(program);
    child.args(args);
    child.envs(&detected_env);

    let status = child
        .status()
        .with_context(|| format!("Failed to execute `{program}` with scoped MSVC env"))?;
    Ok(status.code().unwrap_or(1))
}
