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

use std::{io::BufRead, path::PathBuf, process::Stdio};

use anyhow::Context;

/// Format the code and print the difference
#[derive(Debug, clap::Parser)]
pub(crate) struct FormatAndDiffSubcommand {
    /// The source path of the code which needs to be formatted
    #[clap(long)]
    old: PathBuf,

    /// The target path of the formatted code
    #[clap(long)]
    new: PathBuf,

    /// Warn instead of showing differences
    #[clap(long)]
    warn: bool,

    pub args: Vec<String>,
}

pub(crate) fn run_format_and_diff(cmd: FormatAndDiffSubcommand) -> anyhow::Result<i32> {
    let mut moonfmt = std::process::Command::new(&*moonutil::toolchain::BINARIES.moonfmt);
    moonfmt
        .arg("-exit-code")
        .arg(&cmd.old)
        .arg("-o")
        .arg(&cmd.new);
    let mut execution = moonfmt
        .args(&cmd.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    let x = execution.wait()?;
    let exit_code = x.code().unwrap_or(1);
    if exit_code == 0 {
        // when the return code is 0, it means no difference
        return Ok(0);
    }

    // handle the case when there are differences
    if cmd.warn {
        // This command is executed by n2, so colored message using with
        // `eprintln!` is not displayed correctly.
        println!("File not formatted: {}", cmd.old.display());
        return Ok(0);
    }

    let mut execution = std::process::Command::new(moonutil::toolchain::BINARIES.git_or_default())
        .args(["--no-pager", "diff", "--color=always", "--no-index"])
        .arg(&cmd.old)
        .arg(&cmd.new)
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
                cmd.old.display(),
                cmd.new.display()
            );
            Ok(1)
        }
    }
}
