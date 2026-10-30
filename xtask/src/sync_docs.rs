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

use std::path::{Path, PathBuf};

use anyhow::Context;

pub(crate) fn run(path: &Path) -> anyhow::Result<()> {
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
