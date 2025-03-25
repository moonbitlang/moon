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

use std::path::{Path, PathBuf};

use anyhow::Context;

pub fn run(path: &Path) -> anyhow::Result<()> {
    let moon_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let moon_root = moon_root.parent().context("moon root not found")?;
    let moon_manual_dir = moon_root.join("docs").join("manual").join("src");

    let moonbit_docs_moon_dir = path.join("next").join("toolchain").join("moon");

    let from = dunce::canonicalize(moon_manual_dir.join("commands.md")).unwrap();
    let to = dunce::canonicalize(moonbit_docs_moon_dir.join("commands.md")).unwrap();
    process_commands(&from, &to)
}

fn process_commands(from: &Path, to: &Path) -> anyhow::Result<()> {
    let commands_md_content = std::fs::read_to_string(from)?;
    let commands_md_content = commands_md_content.replace("###### ", "");
    std::fs::write(to, commands_md_content)?;
    Ok(())
}
