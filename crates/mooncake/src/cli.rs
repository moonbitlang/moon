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

//! Shared command-line flags and stdin payloads for package-management frontends.

use clap::Subcommand;
use serde::{Deserialize, Serialize};

pub use moonutil::project::{SourceTargetDirs, WorkspaceEnv};

#[derive(Debug, clap::Parser, Serialize, Deserialize, Clone)]
#[clap(next_help_heading = "Manifest Options")]
pub struct AutoSyncFlags {
    /// Do not sync dependencies, assuming local dependencies are up-to-date
    #[clap(long)]
    pub frozen: bool,
}

/// Options owned by package-management commands.
///
/// Stdin decoding accepts extra fields from older, broader Moon payloads.
#[derive(Debug, clap::Parser, Serialize, Deserialize, Clone)]
#[clap(next_help_heading = "Common Options")]
pub struct MooncakeFlags {
    #[clap(flatten)]
    pub source_tgt_dir: SourceTargetDirs,

    #[clap(skip)]
    #[serde(skip, default)]
    pub workspace_env: WorkspaceEnv,

    /// Suppress output
    #[clap(long, short = 'q', global = true)]
    pub quiet: bool,

    /// Increase verbosity
    #[clap(long, short = 'v', global = true)]
    pub verbose: bool,

    /// Do not actually run the command
    #[clap(long, global = true)]
    pub dry_run: bool,
}

impl MooncakeFlags {
    pub fn user_log_level(&self) -> log::LevelFilter {
        moonutil::user_log::user_log_level(self.verbose, self.quiet)
    }
}

#[derive(Subcommand, Serialize, Deserialize)]
pub enum MooncakeSubcommands {
    Login(LoginSubcommand),
    Register(RegisterSubcommand),
    Publish(PublishSubcommand),
    Package(PackageSubcommand),
    Deprecate(DeprecateSubcommand),
}

/// Log in to your account
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct LoginSubcommand {}

/// Register an account at mooncakes.io
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct RegisterSubcommand {}

/// Publish the current module
///
/// The optional `private` field in moon.mod requests restricted
/// access to the module after publication when true. Omission or false requests
/// public visibility.
///
/// Publication permissions and visibility are checked by the registry.
/// With --dry-run, validate the publication without creating a release.
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct PublishSubcommand {
    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,
}

/// Package the current module
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct PackageSubcommand {
    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    #[clap(long)]
    pub list: bool,
}

/// Deprecate or restore all existing versions of a published module
///
/// Pass `--reason <REASON>` to deprecate or `--undo` to restore.
///
/// Specify the full module name without a version selector. This command works
/// outside a project and uses the same saved credentials as `moon publish`.
/// Versions published later start undeprecated. Restoring clears all reasons;
/// it does not recover previous per-version states.
///
/// Deprecation does not affect version selection. Commands that resolve
/// dependencies warn about every selected deprecated module, including
/// transitive dependencies, using the local registry index without refreshing
/// it. `--quiet` hides these warnings.
///
/// With `--dry-run`, fetch the module's currently published versions and print
/// the intended change for each release without modifying the registry.
/// The preview uses optional API-bound credentials for the read-only lookup.
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct DeprecateSubcommand {
    /// Full module name, without a version selector
    pub module: String,

    /// Deprecation reason, replacing any previous reasons
    #[arg(
        long,
        required_unless_present = "undo",
        conflicts_with = "undo",
        value_parser = clap::builder::NonEmptyStringValueParser::new()
    )]
    pub reason: Option<String>,

    /// Clear deprecation and reasons on all existing versions
    #[arg(long)]
    #[serde(default)]
    pub undo: bool,
}

#[cfg(test)]
mod tests {
    use clap::{Parser, error::ErrorKind};

    use super::{MooncakeFlags, WorkspaceEnv};

    #[test]
    fn package_management_flags_parse_without_build_options() {
        let flags = MooncakeFlags::try_parse_from([
            "mooncake",
            "-C",
            "project",
            "--target-dir",
            "output",
            "--quiet",
            "--dry-run",
        ])
        .unwrap();
        assert_eq!(
            flags.source_tgt_dir.cwd.as_deref(),
            Some(std::path::Path::new("project"))
        );
        assert_eq!(
            flags.source_tgt_dir.target_dir.as_deref(),
            Some(std::path::Path::new("output"))
        );
        assert!(matches!(flags.workspace_env, WorkspaceEnv::Auto));
        assert!(flags.dry_run);
        assert_eq!(flags.user_log_level(), log::LevelFilter::Error);
    }

    #[test]
    fn moon_build_options_are_not_mooncake_options() {
        for args in [
            vec!["mooncake", "--trace"],
            vec!["mooncake", "-Z", "rr_export_module_graph"],
        ] {
            assert_eq!(
                MooncakeFlags::try_parse_from(args).unwrap_err().kind(),
                ErrorKind::UnknownArgument
            );
        }
    }
}
