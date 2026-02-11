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

#[derive(Debug, clap::Parser)]
pub(crate) struct WriteTccRspFile {
    /// The file path to write the response file to.
    pub output: PathBuf,

    /// The command line arguments to include in the response file.
    #[clap(name = "args", allow_hyphen_values = true)]
    pub args: Vec<String>,
}

pub(crate) fn write_tcc_rsp_file(cmd: WriteTccRspFile) -> anyhow::Result<i32> {
    use std::fs::File;
    use std::io::Write;

    let file = File::create(&cmd.output)
        .with_context(|| format!("Failed to create response file at {}", cmd.output.display()))?;
    let mut writer = std::io::BufWriter::new(file);

    // We're not using shlex here because TCC only recognizes double quotes.
    for arg in cmd.args {
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
