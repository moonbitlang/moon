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

use std::io::Write;
use std::path::Path;

use anyhow::Context;
use colored::Colorize;

use handlebars::Handlebars;
use moonutil::common::MOON_PKG_JSON;
use moonutil::module::MoonModJSON;
use moonutil::package::MoonPkgJSON;
use moonutil::package::PkgJSONImportItem;

use moonutil::common::MOON_MOD_JSON;

use moonutil::git::{git_init_repo, is_in_git_repo};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Template {
    #[serde(default)]
    files: Vec<TemplateFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum TemplateFile {
    PlainFile {
        path: std::path::PathBuf,
        content: String,
        executable: bool,
    },
    SymLink {
        path: std::path::PathBuf,
        target: std::path::PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TemplateEnv {
    user_name: String,
    module_name: String,
    package_name: String,
}

impl Template {
    fn from_toml(toml_str: &str) -> anyhow::Result<Self> {
        toml::from_str(toml_str).context("Failed to parse template from TOML")
    }

    fn create(&self, base_dir: &Path) -> anyhow::Result<()> {
        for entry in &self.files {
            match &entry {
                TemplateFile::PlainFile {
                    content,
                    path,
                    executable,
                } => {
                    let full_path = base_dir.join(&path);
                    // Create parent directories if they don't exist
                    if let Some(parent) = full_path.parent() {
                        std::fs::create_dir_all(parent)
                            .context(format!("Failed to create directory: {}", parent.display()))?;
                    }
                    let mut file = std::fs::File::create(&full_path)
                        .context(format!("Failed to create file: {}", full_path.display()))?;
                    file.write_all(content.as_bytes())
                        .context(format!("Failed to write to file: {}", full_path.display()))?;
                    #[cfg(unix)]
                    {
                        if *executable && file.set_permissions(
                            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755),
                        )
                        .is_err() {
                        eprintln!(
                    "{} failed to set permissions on pre-commit hook. Please set it executable manually.",
                    "Warning:".bold().yellow(),
                        );
                    }
                    }
                }
                TemplateFile::SymLink { target, path } => {
                    let full_path = base_dir.join(&path);
                    // Create parent directories if they don't exist
                    if let Some(parent) = full_path.parent() {
                        std::fs::create_dir_all(parent)
                            .context(format!("Failed to create directory: {}", parent.display()))?;
                    }
                    // The creation of symbolic links won't fail the whole process.
                    #[cfg(unix)]
                    {
                        if let Err(e) = std::os::unix::fs::symlink(target, &full_path) {
                            eprintln!(
                                "{} failed to create symbolic link: {} -> {}. {}",
                                "Warning:".bold().yellow(),
                                full_path.display(),
                                target.display(),
                                e
                            );
                        }
                    }
                    #[cfg(windows)]
                    {
                        // Determine if target is a directory or file
                        let target_path = base_dir.join(target);
                        if target_path.is_dir() {
                            if let Err(e) = std::os::windows::fs::symlink_dir(target, &full_path) {
                                eprintln!(
                                    "{} failed to create directory symlink: {} -> {}. You may need to enable developer mode or have administrator privileges. {}",
                                    "Warning:".bold().yellow(),
                                    full_path.display(),
                                    target.display(),
                                    e
                                );
                            }
                        } else {
                            if let Err(e) = std::os::windows::fs::symlink_file(target, &full_path) {
                                eprintln!(
                                    "{} failed to create file symlink: {} -> {}. You may need to enable developer mode or have administrator privileges. {}",
                                    "Warning:".bold().yellow(),
                                    full_path.display(),
                                    target.display(),
                                    e
                                );
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

pub fn create_or_warning(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        eprintln!(
            "{} {}",
            "Warning:".bold().yellow(),
            format_args!("{} already exists", path.display())
        );
    } else {
        std::fs::create_dir_all(path).context(format!("failed to create {}", path.display()))?;
    }
    Ok(())
}

pub fn moon_new_default(target_dir: &Path, user: String, name: String) -> anyhow::Result<i32> {
    let template: Template = Template { files: Vec::new() };

    std::fs::create_dir_all(target_dir).context("failed to create target directory")?;

    template.create(target_dir)?;

    if is_in_git_repo(target_dir).is_err() && git_init_repo(target_dir).is_err() {
        eprintln!(
            "{} failed to initialize git repository. Do you have git installed?",
            "Warning:".bold().yellow(),
        );
    }

    println!(
        "{} {}/{} at {}",
        "Created".bold().green(),
        user,
        name,
        target_dir.display()
    );

    Ok(0)
}
