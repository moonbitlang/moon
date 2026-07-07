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

use std::path::Path;

use anyhow::bail;

use crate::common::read_module_desc_file_in_dir;

pub enum PrePostBuild {
    PreBuild,
}

impl PrePostBuild {
    pub fn name(&self) -> String {
        match self {
            PrePostBuild::PreBuild => "pre-build".into(),
        }
    }

    pub fn dbname(&self) -> String {
        format!("{}.db", self.name())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum IgnoredMoonScript {
    Prebuild,
    Postadd,
}

impl IgnoredMoonScript {
    pub fn env_var(self) -> &'static str {
        match self {
            IgnoredMoonScript::Prebuild => "MOON_IGNORE_PREBUILD",
            IgnoredMoonScript::Postadd => "MOON_IGNORE_POSTADD",
        }
    }
}

pub fn is_moon_script_ignored(script: IgnoredMoonScript) -> bool {
    std::env::var_os(script.env_var()).is_some()
}

pub fn execute_postadd_script(dir: &Path) -> anyhow::Result<()> {
    if is_moon_script_ignored(IgnoredMoonScript::Postadd) {
        return Ok(());
    }
    let m = read_module_desc_file_in_dir(dir)?;
    if let Some(scripts) = &m.scripts
        && scripts.contains_key("postadd")
    {
        let postadd = scripts
            .get("postadd")
            .unwrap()
            .split(' ')
            .collect::<Vec<_>>();
        if !postadd.is_empty() {
            let command = postadd[0];
            let args = &postadd[1..];
            let output = std::process::Command::new(command)
                .args(args)
                .current_dir(dir)
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .output()?;
            if !output.status.success() {
                bail!(
                    "failed to execute postadd script in {},\ncommand: {},\n{}",
                    dir.display(),
                    command,
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
    Ok(())
}
