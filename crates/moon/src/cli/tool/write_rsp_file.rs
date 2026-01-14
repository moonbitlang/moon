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

//! Writes a response file for a given command line.

use std::path::PathBuf;

use anyhow::Context;

fn tcc_bt_flag_from_env() -> anyhow::Result<Option<String>> {
    // TinyCC expects `-bt[N]` (no space). `-bt 15` will be parsed as a filename `15`.
    // Allow users to override:
    // - MOON_NATIVE_BT=0  => disable
    // - MOON_NATIVE_BT=15 => use 15 callers
    match std::env::var("MOON_NATIVE_BT") {
        Ok(v) => {
            let v = v.trim();
            if v.is_empty() || v == "0" {
                return Ok(None);
            }
            let n: u32 = v
                .parse()
                .with_context(|| format!("MOON_NATIVE_BT must be a positive integer, got `{v}`"))?;
            Ok(Some(format!("-bt{n}")))
        }
        Err(std::env::VarError::NotPresent) => Ok(Some("-bt15".to_string())),
        Err(e) => Err(e).context("failed to read MOON_NATIVE_BT"),
    }
}

#[derive(Debug, clap::Parser)]
pub struct WriteTccRspFile {
    /// The file path to write the response file to.
    pub output: PathBuf,

    /// The command line arguments to include in the response file.
    #[clap(name = "args", allow_hyphen_values = true)]
    pub args: Vec<String>,
}

pub fn write_tcc_rsp_file(cmd: WriteTccRspFile) -> anyhow::Result<i32> {
    use std::fs::File;
    use std::io::Write;

    let file = File::create(&cmd.output)
        .with_context(|| format!("Failed to create response file at {}", cmd.output.display()))?;
    let mut writer = std::io::BufWriter::new(file);

    // If this rspfile is used for `tcc -run`, inject a default backtrace depth.
    // This greatly improves native stack traces (more than ~3 frames) on some platforms.
    let mut args = cmd.args;
    if args.iter().any(|a| a == "-run") && !args.iter().any(|a| a.starts_with("-bt")) {
        if let Some(bt) = tcc_bt_flag_from_env()? {
            if let Some(run_pos) = args.iter().position(|a| a == "-run") {
                args.insert(run_pos, bt);
            } else {
                // Shouldn't happen because we checked `any(|a| a == "-run")`,
                // but keep it robust.
                args.push(bt);
            }
        }
    }

    // We're not using shlex here because TCC only recognizes double quotes.
    for arg in args {
        if arg.contains(' ') || arg.contains('"') {
            let escaped = arg.replace('"', "\\\"");
            writeln!(writer, "\"{}\"", escaped)?;
        } else {
            writeln!(writer, "{}", arg)?;
        }
    }

    writer.flush()?;

    Ok(0)
}
