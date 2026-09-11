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

use std::{fs::File, io::Write, str::FromStr};

use anyhow::{Context, bail};
use mooncake::registry::{
    RegistryClient, RegistryModuleManifest, RegistryRelease, RegistryUserModules,
};
use moonutil::{
    MOON_HOME,
    cli_support::UniversalFlags,
    command_output::CommandOutput,
    registry::{Credentials, validate_username},
    resolution::ModuleName,
    user_log::{UserLogEntry, UserLogEntryLevel},
};
use semver::Version;
use serde::Serialize;

use super::{
    invocation::{JsonCommand, JsonCommandOutcome},
    search::{is_safe_registry_module_name, sanitize_registry_text},
};

/// View a registry module or a user's published modules
///
/// `moon view <username/module[@version]>` shows live registry metadata for the
/// latest release or an exact version. Add --versions to list every release,
/// newest first, including deprecated versions and their reasons.
///
/// `moon view <username>` lists all modules on that user's public registry
/// profile, sorted by name, with their latest versions and deprecation reasons.
/// No login is required. Use `moon view --my` as a shortcut for your saved
/// username after running `moon login`.
///
/// These commands work outside a project and respect the configured registry.
/// With --json, a versioned report contains a result (module metadata, an array
/// of releases for --versions, or a user profile), status, and messages.
#[derive(Debug, clap::Parser)]
pub(crate) struct ViewSubcommand {
    /// Username to list, or username/module[@version] to inspect
    #[arg(
        value_name = "USER_OR_MODULE",
        required_unless_present = "my",
        conflicts_with = "my"
    )]
    target: Option<ViewTarget>,

    /// List all modules published under your logged-in username
    #[arg(long)]
    my: bool,

    /// List all published versions of the module
    #[arg(long, requires = "target", conflicts_with = "my")]
    versions: bool,

    /// Print the result as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone)]
enum ViewTarget {
    User(String),
    Module {
        name: ModuleName,
        version: Option<Version>,
    },
}

impl FromStr for ViewTarget {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if !input.contains('/') {
            validate_username(input).map_err(anyhow::Error::msg)?;
            return Ok(Self::User(input.to_owned()));
        }
        let (name, version) = match input.rsplit_once('@') {
            Some((name, version)) => (
                name,
                Some(Version::parse(version).context("expected an exact semantic version")?),
            ),
            None => (input, None),
        };
        if !name.contains('/')
            || !is_safe_registry_module_name(name)
            || name.contains(['#', '?', '%'])
        {
            bail!("expected a module name in the form `username/module[@version]`");
        }
        Ok(Self::Module {
            name: name.into(),
            version,
        })
    }
}

#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum ViewResult {
    Module(Box<RegistryModuleManifest>),
    Versions(Vec<RegistryRelease>),
    Published(RegistryUserModules),
}

impl ViewSubcommand {
    fn fetch(&self) -> anyhow::Result<ViewResult> {
        let registry = RegistryClient::configured();
        match &self.target {
            Some(ViewTarget::Module { name, version }) => {
                let manifest = registry.module_manifest(name, version.as_ref())?;
                if self.versions {
                    Ok(ViewResult::Versions(manifest.versions))
                } else {
                    Ok(ViewResult::Module(Box::new(manifest)))
                }
            }
            Some(ViewTarget::User(username)) => {
                if self.versions {
                    bail!("--versions requires a module in the form `username/module`");
                }
                Ok(ViewResult::Published(registry.user_modules(username)?))
            }
            None => {
                let credentials_path = MOON_HOME.credentials_path();
                if !credentials_path.exists() {
                    bail!("Not logged in. Run `moon login` before using `moon view --my`.");
                }
                let file =
                    File::open(credentials_path).context("failed to open registry credentials")?;
                let credentials: Credentials = serde_json_lenient::from_reader(file)
                    .context("failed to parse registry credentials")?;
                if credentials.token.trim().is_empty() {
                    bail!("Not logged in. Run `moon login` before using `moon view --my`.");
                }
                let username = credentials.username.filter(|name| !name.trim().is_empty())
                    .context("Username is unavailable. Run `moon login` again before using `moon view --my`.")?;
                Ok(ViewResult::Published(registry.user_modules(&username)?))
            }
        }
    }
}

pub(crate) fn run_view(command: ViewSubcommand, output: &CommandOutput) -> anyhow::Result<i32> {
    let result = command.fetch()?;
    output.write_result(|writer| render_view(writer, &result))?;
    Ok(0)
}

pub(crate) fn json_command(command: ViewSubcommand) -> Box<dyn JsonCommand> {
    Box::new(command)
}

impl JsonCommand for ViewSubcommand {
    fn run(&self, _flags: &UniversalFlags, _output: &CommandOutput) -> JsonCommandOutcome {
        view_json_outcome(self.fetch())
    }

    fn bootstrap_error(&self, message: String) -> JsonCommandOutcome {
        view_json_outcome(Err(anyhow::anyhow!(message)))
    }
}

#[derive(Serialize)]
struct ViewJsonReport {
    version: u8,
    status: &'static str,
    result: Option<ViewResult>,
    messages: Vec<UserLogEntry>,
}

fn view_json_outcome(result: anyhow::Result<ViewResult>) -> JsonCommandOutcome {
    JsonCommandOutcome::new(
        if result.is_ok() { 0 } else { -1 },
        move |output, capture| {
            let mut messages = capture.take();
            let (status, result) = match result {
                Ok(result) => ("success", Some(result)),
                Err(error) => {
                    messages.push(UserLogEntry {
                        level: UserLogEntryLevel::Error,
                        message: format!("{error:#}"),
                    });
                    ("failure", None)
                }
            };
            let report = ViewJsonReport {
                version: 1,
                status,
                result,
                messages,
            };
            output.write_result(|writer| -> anyhow::Result<()> {
                serde_json::to_writer(&mut *writer, &report)?;
                writeln!(writer)?;
                Ok(())
            })
        },
    )
}

pub(super) fn render_view(writer: &mut dyn Write, result: &ViewResult) -> anyhow::Result<()> {
    match result {
        ViewResult::Module(manifest) => {
            write!(writer, "{}@", manifest.module)?;
            render_release(writer, &manifest.release)?;
            for (key, label) in [
                ("description", "Description"),
                ("license", "License"),
                ("repository", "Repository"),
                ("created_at", "Published"),
            ] {
                if let Some(text) = manifest.metadata.get(key).and_then(|value| value.as_str()) {
                    let text = sanitize_registry_text(text);
                    if !text.is_empty() {
                        writeln!(writer, "{label}: {text}")?;
                    }
                }
            }
            if let Some(downloads) = manifest.downloads {
                writeln!(writer, "Downloads: {downloads}")?;
            }
            writeln!(writer, "Latest: {}", manifest.latest_version)?;
            writeln!(
                writer,
                "Versions: {} (use --versions to list)",
                manifest.versions.len()
            )?;
        }
        ViewResult::Versions(versions) => {
            for release in versions {
                render_release(writer, release)?;
            }
        }
        ViewResult::Published(profile) => {
            // Check every coordinate before emitting a partial list. Metadata
            // and reasons are sanitized separately; JSON preserves raw text.
            if profile
                .modules
                .iter()
                .any(|module| !is_safe_registry_module_name(&module.name))
            {
                bail!("registry user response contains an invalid module name");
            }
            if profile.modules.is_empty() {
                writeln!(writer, "No published modules.")?;
            } else {
                for module in &profile.modules {
                    write!(writer, "{}@", module.name)?;
                    render_release(writer, &module.release)?;
                }
            }
        }
    }
    Ok(())
}

fn render_release(writer: &mut dyn Write, release: &RegistryRelease) -> std::io::Result<()> {
    write!(writer, "{}", release.version)?;
    if release.yanked {
        let reason = release
            .yanked_reason
            .as_deref()
            .map(sanitize_registry_text)
            .unwrap_or_default();
        if reason.is_empty() {
            write!(writer, " (deprecated)")?;
        } else {
            write!(writer, " (deprecated: {reason})")?;
        }
    }
    writeln!(writer)
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::MoonBuildCli;

    use super::*;

    #[test]
    fn view_requires_one_target() {
        for args in [
            vec!["moon", "view"],
            vec!["moon", "view", "--my", "alice/tools"],
            vec!["moon", "view", "--my", "alice"],
            vec!["moon", "view", "--my", "--versions"],
            vec!["moon", "view", "--versions"],
            vec!["moon", "view", "alice@1.0.0"],
            vec!["moon", "view", "alice?query"],
            vec!["moon", "view", ".."],
            vec!["moon", "view", "alice/../tools"],
            vec!["moon", "view", "alice/tools@latest"],
            vec!["moon", "view", "alice/tools@1.0.0/extra"],
            vec!["moon", "view", "alice/tools@1.0.0@2.0.0"],
            vec!["moon", "view", "alice/tools?query"],
            vec!["moon", "view", "alice/\u{1b}[31mtools"],
        ] {
            assert!(MoonBuildCli::try_parse_from(&args).is_err(), "{args:?}");
        }
        for args in [
            vec!["moon", "view", "alice"],
            vec!["moon", "view", "Alice-123", "--json"],
            vec!["moon", "view", "alice/tools"],
            vec![
                "moon",
                "view",
                "Alice/nested/tools@1.0.0-beta.1+build",
                "--json",
            ],
            vec!["moon", "view", "alice/tools", "--versions"],
            vec!["moon", "view", "--my", "--json", "--quiet"],
        ] {
            assert!(MoonBuildCli::try_parse_from(&args).is_ok(), "{args:?}");
        }
    }

    #[test]
    fn parses_user_and_versioned_module_targets() {
        let ViewTarget::User(username) = "Alice".parse().unwrap() else {
            panic!("expected a user target");
        };
        assert_eq!(username, "Alice");

        let ViewTarget::Module { name, version } =
            "Alice/nested/tools@1.0.0-beta.1+build".parse().unwrap()
        else {
            panic!("expected a module target");
        };
        assert_eq!(name.to_string(), "Alice/nested/tools");
        assert_eq!(version.unwrap().to_string(), "1.0.0-beta.1+build");
    }
}
