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

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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
        #[serde(skip_serializing_if = "is_false")]
        executable: bool,
    },
    SymLink {
        path: std::path::PathBuf,
        target: std::path::PathBuf,
    },
}

fn is_false(b: &bool) -> bool {
    !*b
}

pub(crate) fn run() -> anyhow::Result<()> {
    println!("Bundling template from moon_new_template folder...");

    // Define paths
    let project_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get parent directory")
        .to_path_buf();

    let template_dir = project_dir
        .join("crates")
        .join("moonbuild")
        .join("template")
        .join("moon_new_template");

    let output_file = project_dir
        .join("crates")
        .join("moonbuild")
        .join("template")
        .join("moon_new_template.toml");

    println!("Template directory: {}", template_dir.display());
    println!("Output file: {}", output_file.display());

    // Generate template
    let template = generate_template(&template_dir)?;

    // Serialize to TOML
    let toml_content =
        toml::to_string_pretty(&template).context("Failed to serialize template to TOML")?;

    // Write to file
    fs::write(&output_file, toml_content).context(format!(
        "Failed to write to file: {}",
        output_file.display()
    ))?;

    println!(
        "Successfully generated template TOML at: {}",
        output_file.display()
    );

    Ok(())
}

fn generate_template(template_dir: &Path) -> anyhow::Result<Template> {
    let mut template = Template { files: Vec::new() };

    // Walk through all files in the template directory
    for entry in WalkDir::new(template_dir)
        .follow_links(false) // Don't follow symlinks to avoid potential loops
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Skip the template directory itself
        if path == template_dir {
            continue;
        }

        // Get the relative path from the template directory
        let rel_path = path.strip_prefix(template_dir).context(format!(
            "Failed to strip prefix from path: {}",
            path.display()
        ))?;

        if entry.file_type().is_symlink() {
            // Handle symlinks
            let target_path = fs::read_link(path)
                .context(format!("Failed to read symlink target: {}", path.display()))?;

            template.files.push(TemplateFile::SymLink {
                path: rel_path.to_path_buf(),
                target: target_path,
            });
        } else if entry.file_type().is_file() {
            // Handle regular files
            let mut file =
                fs::File::open(path).context(format!("Failed to open file: {}", path.display()))?;

            let mut content = String::new();
            file.read_to_string(&mut content)
                .context(format!("Failed to read file: {}", path.display()))?;

            // Check if the file is executable based on permissions or has a shebang
            let executable = is_executable(path, &content);

            let plain_file = if executable {
                TemplateFile::PlainFile {
                    path: rel_path.to_path_buf(),
                    content,
                    executable: true,
                }
            } else {
                TemplateFile::PlainFile {
                    path: rel_path.to_path_buf(),
                    content,
                    executable: false, // This will be skipped in serialization
                }
            };

            template.files.push(plain_file);
        }
        // Skip directories as they'll be created implicitly
    }

    Ok(template)
}

#[cfg(unix)]
fn is_executable(path: &Path, content: &str) -> bool {
    // First check if the file has Unix executable permissions
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = fs::metadata(path) {
        let permissions = metadata.permissions();
        if permissions.mode() & 0o111 != 0 {
            // If any execute bit is set
            return true;
        }
    }

    // Also check for shebang line
    has_shebang(content)
}

#[cfg(not(unix))]
fn is_executable(path: &Path, content: &str) -> bool {
    // On non-Unix platforms, only check for shebang
    has_shebang(content)
}

fn has_shebang(content: &str) -> bool {
    // Check if the content starts with a shebang line (#!)
    content.starts_with("#!")
        && content
            .lines()
            .next()
            .map(|line| line.len() > 2)
            .unwrap_or(false)
}
