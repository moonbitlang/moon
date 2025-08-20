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
use colored::Colorize;
use moonutil::{
    common::MOON_MOD_JSON,
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
    /// The name of the package
    pub package_name: Option<String>,

    /// Create a library package instead of an executable
    #[clap(long, hide = true)]
    #[deprecated]
    pub lib: bool,

    #[clap(flatten)]
    pub path_user_name: NewPathUserName,

    /// The license of the package
    #[clap(long, default_value = "Apache-2.0")]
    pub license: Option<String>,

    /// Do not set a license for the package
    #[clap(long)]
    pub no_license: bool,
}

#[derive(Debug, Clone, clap::Args)]
#[group(required(false), requires_all(["path", "user", "name"]))]
pub struct NewPathUserName {
    /// Output path of the package
    #[clap(long)]
    pub path: Option<PathBuf>,

    /// The user name of the package
    #[clap(long)]
    pub user: Option<String>,

    /// The name part of the package
    #[clap(long)]
    pub name: Option<String>,
}

pub fn run_new(_cli: &UniversalFlags, cmd: NewSubcommand) -> anyhow::Result<i32> {
    if _cli.dry_run {
        bail!("dry-run is not implemented for new")
    }
    #[allow(deprecated)]
    if cmd.lib {
        eprintln!(
            "{}: The `--lib` flag is deprecated. Currently the template is unified.",
            "Warning".yellow().bold()
        );
    }

    let package_name = cmd.package_name.as_ref();
    let license = if cmd.no_license {
        None
    } else {
        match cmd.license.as_deref() {
            Some("") => None,
            Some("\"\"") => None,
            Some("\'\'") => None,
            Some(x) => Some(x),
            _ => None,
        }
    };

    if let Some(name) = package_name {
        let target_dir = PathBuf::from(name);
        let existing_username = get_existing_username();
        let username = existing_username.unwrap_or_else(|| "username".to_string());
        return moonbuild::new::moon_new_default(&target_dir, username, "hello".into(), license);
    }

    let NewPathUserName { path, user, name } = cmd.path_user_name;

    ctrlc::set_handler(moonutil::common::dialoguer_ctrlc_handler)?;

    let (target_dir, user, name, license) =
        if let (Some(path), Some(user), Some(name)) = (path, user, name) {
            (path, user, name, license.map(|s| s.to_string()))
        } else {
            let tmp = dialoguer::Input::<String>::new()
                .with_prompt("Enter the path to create the project (. for current directory)")
                .default("my-project".to_string())
                .show_default(true)
                .validate_with(|input: &String| -> Result<(), String> {
                    let p = input.trim();
                    let dot = p == ".";

                    let p = PathBuf::from(p);

                    if p.exists() {
                        if p.join(MOON_MOD_JSON).exists() {
                            Err(format!(
                                "A MoonBit project already exists in `{}`.",
                                p.display()
                            ))
                        } else {
                            if !dot {
                                eprintln!(
                                    "{}: The directory is already exists.",
                                    "Warning".yellow().bold(),
                                );
                            };
                            Ok(())
                        }
                    } else {
                        Ok(())
                    }
                })
                .interact()?;
            let path = PathBuf::from(tmp);

            let existing_username = get_existing_username();
            let default_username = existing_username.unwrap_or_else(|| "username".to_string());

            let username = dialoguer::Input::<String>::new()
                .with_prompt("Enter your username")
                .default(default_username)
                .validate_with(|input: &String| -> Result<(), String> { validate_username(input) })
                .show_default(true)
                .interact()?;

            let package_name = dialoguer::Input::<String>::new()
                .with_prompt("Enter your project name")
                .default("hello".to_string())
                .show_default(true)
                .interact()?;

            let license = dialoguer::Input::<String>::new()
                .with_prompt("Enter your license")
                .default("Apache-2.0".to_string())
                .show_default(true)
                .interact()?;

            (path, username, package_name, Some(license))
        };

    if target_dir.exists() && target_dir.join(MOON_MOD_JSON).exists() {
        bail!(
            "A MoonBit project already exists in `{}`.",
            target_dir.display()
        );
    }

    moonbuild::new::moon_new_default(&target_dir, user, name, license.as_deref())
}
