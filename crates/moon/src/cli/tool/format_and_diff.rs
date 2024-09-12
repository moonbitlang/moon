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

use std::{io::BufRead, path::PathBuf, process::Stdio};

#[derive(Debug, clap::Parser)]
pub struct FormatAndDiffSubcommand {
    #[clap(long)]
    old: PathBuf,
    #[clap(long)]
    new: PathBuf,
}

pub fn run_format_and_diff(cmd: FormatAndDiffSubcommand) -> anyhow::Result<i32> {
    let mut execution = std::process::Command::new("moonfmt")
        .arg("-exit-code")
        .arg(cmd.old.to_str().unwrap())
        .arg("-o")
        .arg(cmd.new.to_str().unwrap())
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    let x = execution.wait()?;
    let exit_code = x.code().unwrap_or(1);
    if exit_code == 0 {
        return Ok(0);
    }
    let mut execution = std::process::Command::new("git")
        .args([
            "--no-pager",
            "diff",
            "--color=always",
            "--no-index",
            cmd.old.to_str().unwrap(),
            cmd.new.to_str().unwrap(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let child_stdout = execution.stdout.take().unwrap();
    let mut buf = String::new();
    let mut bufread = std::io::BufReader::new(child_stdout);
    while let Ok(n) = bufread.read_line(&mut buf) {
        if n > 0 {
            print!("{}", buf);
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
                &cmd.old.to_str().unwrap(),
                &cmd.new.to_str().unwrap()
            );
            Ok(1)
        }
    }
}
