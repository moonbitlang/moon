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

use std::{
    io::BufRead,
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::Context;
use moonutil::user_log::UserLog;

#[derive(Debug, clap::Parser)]
pub(crate) struct FormatWorkspaceSubcommand {
    /// The source path of the workspace file to format
    #[clap(long)]
    old: PathBuf,

    /// The target path of the formatted workspace file
    #[clap(long)]
    new: PathBuf,

    /// Write the formatted output back to the source file
    #[clap(short = 'w', long = "write")]
    write: bool,

    /// Check formatting and print the difference
    #[clap(long, conflicts_with = "warn")]
    check: bool,

    /// Warn instead of showing differences
    #[clap(long, conflicts_with = "check")]
    warn: bool,
}

pub(crate) fn run_format_workspace(
    cmd: FormatWorkspaceSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    let workspace = moonutil::workspace::MoonWork::read(&cmd.old)?;
    if workspace.preferred_target.is_some() {
        user_log.warn(moonutil::workspace::PREFERRED_TARGET_DEPRECATION_WARNING);
    }
    let formatted = moonutil::workspace::format_workspace_members(&workspace)?;

    if let Some(parent) = cmd.new.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&cmd.new, formatted.as_bytes())?;

    if cmd.write {
        std::fs::write(&cmd.old, formatted.as_bytes())?;
    }

    if !cmd.check && !cmd.warn {
        return Ok(0);
    }

    let old = std::fs::read_to_string(&cmd.old)?;
    if old == formatted {
        return Ok(0);
    }

    if cmd.warn {
        println!("File not formatted: {}", cmd.old.display());
        return Ok(0);
    }

    print_diff(&cmd.old, &cmd.new)
}

fn print_diff(old: &Path, new: &Path) -> anyhow::Result<i32> {
    let mut execution = std::process::Command::new(moonutil::toolchain::BINARIES.git_or_default())
        .args(["--no-pager", "diff", "--color=always", "--no-index"])
        .arg(old)
        .arg(new)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let child_stdout = execution
        .stdout
        .take()
        .context("failed to capture `git diff` stdout")?;
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
                old.display(),
                new.display()
            );
            Ok(1)
        }
    }
}
