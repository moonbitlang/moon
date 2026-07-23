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
    constants::{is_moon_mod_exist, is_moon_pkg_exist},
    registry::{Credentials, credentials_json, validate_username},
    user_log::UserLog,
};

use super::UniversalFlags;

/// Read the existing username from the credentials file
fn get_existing_username(user_log: &UserLog) -> Option<String> {
    let credentials_path = credentials_json();
    if !credentials_path.exists() {
        user_log.warn(
            "Using default username. You may login with `moon login` to store your username, or provide one with `--user <username>`.",
        );
        return None;
    }

    // Try to read the credentials file
    if let Ok(file) = File::open(&credentials_path) {
        let reader = BufReader::new(file);
        if let Ok(credentials) = serde_json_lenient::from_reader::<_, Credentials>(reader) {
            return credentials.username;
        } else {
            user_log.warn(
                "Using default username. You may relogin with `moon login` to store your username, or provide one with `--user <username>`.",
            );
        }
    }
    None
}

/// Create a new MoonBit module
#[derive(Debug, clap::Parser)]
pub(crate) struct NewSubcommand {
    /// The path of the new project.
    pub path: String,

    /// The username of the module. Default to the logged-in username.
    #[clap(long)]
    pub user: Option<String>,

    /// The name of the module. Default to the last part of the path.
    #[clap(long)]
    pub name: Option<String>,
}

pub(crate) fn run_new(
    cli: &UniversalFlags,
    cmd: NewSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not supported for new")
    }

    let path = PathBuf::from(cmd.path);

    if path.exists() && (is_moon_mod_exist(&path)) || is_moon_pkg_exist(&path) {
        bail!("A MoonBit project already exists in `{}`.", path.display());
    }

    let username = cmd
        .user
        .or_else(|| get_existing_username(user_log))
        .unwrap_or("username".to_string());
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
    let mut name_chars = project_name.chars();
    let has_valid_start = name_chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    let has_valid_rest = name_chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if !has_valid_start || !has_valid_rest {
        bail!(
            "Project name {} contains invalid characters. Names must match package id syntax: start with an ASCII letter or underscore, followed by ASCII letters, digits, dashes, or underscores.",
            project_name
        );
    }

    // Warn about reserved suffixes that may cause template files to be misrecognized
    if project_name.ends_with("_test") || project_name.ends_with("_wbtest") {
        user_log.warn(format!(
            "Project name '{}' ends with a reserved suffix. \
             In MoonBit, files ending with '_test.mbt' or '_wbtest.mbt' are treated as test files. \
             Template files in this project may be incorrectly recognized as test files.",
            project_name
        ));
    }

    moonbuild::new::moon_new_default(&path, username, project_name)
}
