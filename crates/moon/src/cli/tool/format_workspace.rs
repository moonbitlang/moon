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

use std::{
    io::BufRead,
    path::{Path, PathBuf},
    process::Stdio,
};

#[derive(Debug, clap::Parser)]
pub(crate) struct FormatWorkspaceSubcommand {
    /// The source path of the workspace file to format
    #[clap(long)]
    old: PathBuf,

    /// The target path of the formatted workspace file
    #[clap(long)]
    new: PathBuf,

    /// Check formatting and print the difference
    #[clap(long, conflicts_with = "warn")]
    check: bool,

    /// Warn instead of showing differences
    #[clap(long, conflicts_with = "check")]
    warn: bool,
}

pub(crate) fn run_format_workspace(cmd: FormatWorkspaceSubcommand) -> anyhow::Result<i32> {
    let formatted = moonutil::workspace::format_workspace_file(&cmd.old)?;

    if let Some(parent) = cmd.new.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&cmd.new, formatted.as_bytes())?;

    if !cmd.check && !cmd.warn {
        return Ok(0);
    }

    let old = std::fs::read_to_string(&cmd.old)?;
    if old == formatted {
        return Ok(0);
    }

    if cmd.warn {
        tracing::warn!("File not formatted: {}", cmd.old.display());
        return Ok(0);
    }

    print_diff(&cmd.old, &cmd.new)
}

fn print_diff(old: &Path, new: &Path) -> anyhow::Result<i32> {
    let mut execution = std::process::Command::new(moonutil::BINARIES.git_or_default())
        .args([
            "--no-pager",
            "diff",
            "--color=always",
            "--no-index",
            old.to_str().unwrap(),
            new.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let child_stdout = execution.stdout.take().unwrap();
    let mut buf = String::new();
    let mut bufread = std::io::BufReader::new(child_stdout);
    while let Ok(n) = bufread.read_line(&mut buf) {
        if n > 0 {
            print!("{buf}");
            buf.clear()
        } else {
            break;
        }
    }
    let status = execution.wait()?;
    match status.code() {
        Some(0) => Ok(0),
        _ => {
            eprintln!(
                "failed to execute `git --no-pager diff --color=always --no-index {} {}`",
                old.to_str().unwrap(),
                new.to_str().unwrap()
            );
            Ok(1)
        }
    }
}
