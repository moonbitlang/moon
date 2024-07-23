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
use colored::Colorize;
use moonutil::mooncakes::RegistryConfig;

fn clone_registry_index(
    registry_config: &RegistryConfig,
    target_dir: &Path,
) -> anyhow::Result<i32> {
    let output = std::process::Command::new("git")
        .arg("clone")
        .arg(&registry_config.index)
        .arg(target_dir)
        .spawn()?
        .wait();
    match output {
        Ok(status) => {
            if !status.success() {
                bail!("Failed to clone registry index");
            }
            Ok(0)
        }
        Err(e) => {
            eprintln!("Failed to clone registry index: {}", e);
            bail!("Failed to clone registry index");
        }
    }
}

fn pull_latest_registry_index(
    _registry_config: &RegistryConfig,
    target_dir: &Path,
) -> anyhow::Result<i32> {
    let output = std::process::Command::new("git")
        .arg("pull")
        .arg("origin")
        .arg("main")
        .current_dir(target_dir)
        .spawn()?
        .wait()?;

    match output.code() {
        Some(code) => {
            if code != 0 {
                bail!("Failed to pull registry index");
            }
            Ok(0)
        }
        None => {
            eprintln!("Failed to pull registry index");
            bail!("Failed to pull registry index");
        }
    }
}

pub fn update(target_dir: &Path, registry_config: &RegistryConfig) -> anyhow::Result<i32> {
    if target_dir.exists() {
        let output = std::process::Command::new("git")
            .arg("remote")
            .arg("get-url")
            .arg("origin")
            .current_dir(target_dir)
            .output()?;

        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if url == registry_config.index {
            let result = pull_latest_registry_index(registry_config, target_dir);
            if result.is_err() {
                eprintln!(
                    "Failed to update registry, {}",
                    "re-cloning".bold().yellow()
                );
                std::fs::remove_dir_all(target_dir)?;
                clone_registry_index(registry_config, target_dir)?;
                eprintln!("{}", "Registry index re-cloned successfully".bold().green());
                Ok(0)
            } else {
                eprintln!("{}", "Registry index updated successfully".bold().green());
                Ok(0)
            }
        } else {
            eprintln!(
                "Registry index is not cloned from the same URL, {}",
                "re-cloning".yellow().bold()
            );
            std::fs::remove_dir_all(target_dir)?;
            clone_registry_index(registry_config, target_dir)?;
            eprintln!("{}", "Registry index re-cloned successfully".bold().green());
            Ok(0)
        }
    } else {
        clone_registry_index(registry_config, target_dir)?;
        eprintln!("{}", "Registry index cloned successfully".bold().green());
        Ok(0)
    }
}
