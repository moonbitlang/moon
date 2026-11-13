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

use std::{
    fs::{self, File},
    io::{BufReader, Write},
    path::PathBuf,
};

use anyhow::{Context, bail};
use moonutil::{
    MOON_HOME,
    constants::{is_moon_mod_exist, is_moon_pkg_exist},
    registry::{Credentials, validate_username},
    toolchain,
    user_log::UserLog,
};

use super::{UniversalFlags, process::ProcessAction};
use crate::run::{ExecutionMode, command_for};

/// Read the existing username from the credentials file
fn get_existing_username(user_log: &UserLog) -> Option<String> {
    let credentials_path = MOON_HOME.credentials_path();
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
) -> anyhow::Result<ProcessAction> {
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

    let generator = toolchain::bin().join("moon_new.wasm");
    if generator
        .try_exists()
        .with_context(|| format!("failed to locate {}", generator.display()))?
    {
        let mut command = command_for(ExecutionMode::MoonRun, &generator, None);
        command
            .arg(format!("--user={username}"))
            .arg(format!("--name={project_name}"))
            .arg("--")
            .arg(&path);
        return Ok(ProcessAction::Delegate(command));
    }

    // FIXME: Remove this minimal fallback once all supported
    // SDKs ship moon_new.wasm.
    fs::create_dir_all(&path).context("failed to create target directory")?;
    let module = format!("name = \"{username}/{project_name}\"\n");
    // Create the entry point first so an existing main.mbt is rejected before
    // writing manifests. Exclusive creation never overwrites existing files.
    for (name, content) in [
        ("main.mbt", "///|\nfn main {\n  println(\"Hello\")\n}\n"),
        ("moon.pkg", "pkgtype(kind: \"executable\")\n"),
        ("moon.mod", module.as_str()),
    ] {
        let file = path.join(name);
        File::create_new(&file)
            .and_then(|mut file| file.write_all(content.as_bytes()))
            .with_context(|| format!("failed to create {}", file.display()))?;
    }
    println!("Created {username}/{project_name} at {}", path.display());
    Ok(ProcessAction::Exit(0))
}
