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

use std::{fs::File, io::BufReader, path::PathBuf};

use anyhow::bail;
use moonutil::{
    common::{MOON_MOD_JSON, MOON_PKG_JSON},
    moon_dir,
    mooncakes::{validate_username, Credentials},
};

use super::UniversalFlags;

/// Read the existing username from the credentials file
fn get_existing_username() -> Option<String> {
    let credentials_path = moon_dir::credentials_json();
    if !credentials_path.exists() {
        return None;
    }

    // Try to read the credentials file
    if let Ok(file) = File::open(&credentials_path) {
        let reader = BufReader::new(file);
        if let Ok(credentials) = serde_json_lenient::from_reader::<_, Credentials>(reader) {
            return credentials.username;
        }
    }
    None
}

/// Create a new MoonBit module
#[derive(Debug, clap::Parser)]
pub struct NewSubcommand {
    /// The path of the new project.
    pub path: String,

    /// The username of the module. Default to the logged-in username.
    #[clap(long)]
    pub user: Option<String>,

    /// The name of the module. Default to the last part of the path.
    #[clap(long)]
    pub name: Option<String>,
}

pub fn run_new(_cli: &UniversalFlags, cmd: NewSubcommand) -> anyhow::Result<i32> {
    if _cli.dry_run {
        bail!("dry-run is not implemented for new")
    }

    let path = PathBuf::from(cmd.path);

    if path.exists() && (path.join(MOON_MOD_JSON).exists() || path.join(MOON_PKG_JSON).exists()) {
        bail!("A MoonBit project already exists in `{}`.", path.display());
    }

    let existing_username = get_existing_username();
    let username = cmd
        .user
        .or(existing_username)
        .unwrap_or_else(|| "username".to_string());
    validate_username(&username).map_err(|e| anyhow::anyhow!(e))?;

    let project_name = match cmd.name {
        Some(name) => name,
        None => {
            let absolute_path = if path.is_relative() {
                match std::env::current_dir() {
                    Ok(current_dir) => current_dir.join(&path),
                    Err(_) => path.clone(),
                }
            } else {
                path.clone()
            };
            absolute_path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "hello".to_string())
        }
    };
    if project_name
        .chars()
        .any(|c| !c.is_ascii_alphanumeric() && c != '_')
    {
        bail!("Project name {} contains invalid characters. Only alphanumeric characters and underscore are allowed.", project_name);
    }
    moonbuild::new::moon_new_default(&path, username, project_name)
}
