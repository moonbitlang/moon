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

use moonutil::{cli::UniversalFlags, common::DEP_PATH};

use super::BuildFlags;

#[derive(Debug, clap::Parser, Clone)]
pub(crate) struct QuerySubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    pub mod_name: String,
}

pub(crate) fn run_query(_cli: UniversalFlags, cmd: QuerySubcommand) -> anyhow::Result<i32> {
    let temp_dir = std::env::temp_dir();
    let moon_repl_dir = temp_dir.join("moon_repl");

    if !moon_repl_dir.exists() {
        std::fs::create_dir_all(&moon_repl_dir)?;
    }

    let mod_json_path = moon_repl_dir.join("moon.mod.json");
    if !mod_json_path.exists() {
        let mod_json_content = r#"{
            "name": "moon/repl",
            "version": "0.0.1"
        }"#;
        std::fs::write(mod_json_path, mod_json_content)?;
    }

    let mod_name = cmd.mod_name;

    let moon_path = std::env::current_exe()
        .map_or_else(|_| "moon".into(), |x| x.to_string_lossy().into_owned());

    let moon_add_output = std::process::Command::new(&moon_path)
        .arg("-C")
        .arg(moon_repl_dir.to_str().unwrap())
        .arg("add")
        .arg(&mod_name)
        .output()?;

    if !moon_add_output.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&moon_add_output.stdout));
        eprintln!("{}", String::from_utf8_lossy(&moon_add_output.stderr));
        anyhow::bail!("Failed to add module {}", mod_name);
    }

    let moon_build_output = std::process::Command::new(&moon_path)
        .arg("-C")
        .arg(
            moon_repl_dir
                .join(DEP_PATH)
                .join(&mod_name)
                .to_str()
                .unwrap(),
        )
        .arg("build")
        .arg("--show-artifacts")
        .output()?;

    println!("{}", String::from_utf8_lossy(&moon_build_output.stdout));
    if !moon_build_output.status.success() {
        anyhow::bail!("Failed to build module {}", mod_name);
    }

    Ok(0)
}
