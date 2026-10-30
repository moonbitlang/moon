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

//! Shared Moon root and build grammar; execution remains with each caller.

use super::{AutoSyncFlags, BuildFlags, UniversalFlags};
use clap::{CommandFactory, FromArgMatches};
use std::{
    ffi::{OsStr, OsString},
    path::PathBuf,
};

#[derive(Debug, clap::Parser)]
#[clap(
    name = "moon",
    about = "The build system and package manager for MoonBit.",
    override_usage = "moon [OPTIONS] <COMMAND>"
)]
pub struct MoonCli<C: clap::Subcommand> {
    #[clap(subcommand)]
    pub subcommand: Option<C>,

    /// Print all version information and exit
    #[clap(short = 'V', long)]
    pub version: bool,

    #[clap(flatten)]
    pub flags: UniversalFlags,
}

impl<C: clap::Subcommand> MoonCli<C> {
    /// Parse using caller-supplied environment observations. The grammar and
    /// feature parser do not read the host environment on this path.
    pub fn try_parse_from_with_env<I, T>(
        args: I,
        env: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let unstable = env("MOON_UNSTABLE").unwrap_or_default();
        let new_mod = env("NEW_MOON_MOD").as_deref() != Some(OsStr::new("0"));
        let new_pkg = env("NEW_MOON_PKG").as_deref() != Some(OsStr::new("0"));
        let command = Self::command().mut_arg("unstable_feature", |arg| {
            arg.default_value(unstable)
                .value_parser(move |value: &str| {
                    crate::features::FeatureGate::parse_with_manifest_defaults(
                        value, new_mod, new_pkg,
                    )
                    .map(Box::new)
                })
        });
        Self::from_arg_matches(&command.try_get_matches_from(args)?)
    }
}

/// Build the current package
///
/// A successful build also lists each executable and foreign library it built
/// on stdout, with its package, target backend and absolute path. `--quiet`
/// does not hide the list. With `--output-json`, each one is an `artifact`
/// JSON message instead.
#[derive(Debug, clap::Parser, Clone)]
pub struct BuildSubcommand {
    /// Paths to the packages that should be built, or one standalone `.mbtx` file.
    #[clap(name = "PATH", conflicts_with("package"))]
    pub path: Vec<PathBuf>,

    #[clap(flatten)]
    pub build_flags: BuildFlags,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Monitor the file system and automatically build artifacts
    #[clap(long, short)]
    pub watch: bool,

    // package name (username/hello/lib)
    #[clap(long, hide = true)]
    pub package: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, error::ErrorKind};

    #[derive(Debug, clap::Subcommand)]
    enum Commands {
        Build(Box<BuildSubcommand>),
        #[command(external_subcommand)]
        Other(Vec<OsString>),
    }

    #[test]
    fn explicit_environment_supplies_defaults_without_overriding_arguments() {
        let parsed =
            MoonCli::<Commands>::try_parse_from_with_env(["moon", "build"], |name| match name {
                "MOON_UNSTABLE" => Some("rr_export_build_plan".into()),
                "NEW_MOON_MOD" => Some("0".into()),
                "NEW_MOON_PKG" => Some("1".into()),
                _ => None,
            })
            .unwrap();
        let Some(Commands::Build(build)) = &parsed.subcommand else {
            panic!("expected build")
        };
        assert!(build.path.is_empty());
        assert!(parsed.flags.unstable_feature.rr_export_build_plan);
        assert!(!parsed.flags.unstable_feature.rr_moon_mod);
        assert!(parsed.flags.unstable_feature.rr_moon_pkg);

        // A malformed environment default is ignored when -Z is explicit,
        // just as Clap's native environment-backed argument behaves.
        let parsed = MoonCli::<Commands>::try_parse_from_with_env(
            ["moon", "-Z", "rr_n2_explain", "build"],
            |name| (name == "MOON_UNSTABLE").then(|| "invalid-env-default".into()),
        )
        .unwrap();
        assert!(parsed.flags.unstable_feature.rr_n2_explain);
        assert!(!parsed.flags.unstable_feature.rr_export_build_plan);
        assert!(parsed.flags.unstable_feature.rr_moon_mod);
        assert!(parsed.flags.unstable_feature.rr_moon_pkg);

        let error = MoonCli::<Commands>::try_parse_from_with_env(["moon", "build"], |name| {
            (name == "MOON_UNSTABLE").then(|| "invalid-env-default".into())
        })
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ValueValidation);
    }

    #[test]
    fn parsing_does_not_consume_host_environment() {
        const CHILD: &str = "MOON_SHARED_PARSER_ENV_TEST";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "cli_support::moon::tests::parsing_does_not_consume_host_environment",
                ])
                .env(CHILD, "1")
                .env("MOON_UNSTABLE", "invalid-host-default")
                .env("NEW_MOON_MOD", "0")
                .env("NEW_MOON_PKG", "0")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        for parsed in [
            MoonCli::<Commands>::try_parse_from(["moon", "build"]).unwrap(),
            MoonCli::<Commands>::try_parse_from_with_env(["moon", "build"], |_| None).unwrap(),
        ] {
            assert!(!parsed.flags.unstable_feature.rr_export_build_plan);
            assert!(parsed.flags.unstable_feature.rr_moon_mod);
            assert!(parsed.flags.unstable_feature.rr_moon_pkg);
        }
    }

    #[test]
    fn opaque_other_command_is_not_reclassified_by_its_arguments() {
        for args in [
            vec!["moon", "run", "build", "--", "--watch"],
            vec!["moon", "tool", "build"],
        ] {
            let parsed = MoonCli::<Commands>::try_parse_from(&args).unwrap();
            let Some(Commands::Other(tail)) = parsed.subcommand else {
                panic!("expected opaque command")
            };
            assert_eq!(
                tail,
                args[1..].iter().map(OsString::from).collect::<Vec<_>>()
            );
        }
    }
}
