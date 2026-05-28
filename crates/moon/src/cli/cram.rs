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

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context;
use clap::{Subcommand, ValueEnum};
use moonbuild_rupes_recta::model::BuildPlanNode;
use moonutil::{
    common::{FileLock, RunMode, SurfaceTarget, TargetBackend},
    dirs::PackageDirs,
    mooncakes::sync::AutoSyncFlags,
};
use tracing::instrument;

use crate::{
    cli::{BuildSubcommand, process},
    rr_build::{self, BuildConfig},
    user_diagnostics::UserDiagnostics,
};

use super::{BuildFlags, UniversalFlags};

/// Run cram tests with project binaries on PATH (experimental)
#[derive(Debug, clap::Parser, Clone)]
#[clap(disable_help_flag(true))]
pub(crate) struct CramSubcommand {
    #[clap(subcommand)]
    pub command: CramCommand,
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum CramCommand {
    Test(CramTestSubcommand),
}

/// Build native executables, then run cram tests with their directories on PATH
#[derive(Debug, clap::Parser, Clone)]
#[clap(disable_help_flag(true))]
pub(crate) struct CramTestSubcommand {
    /// Build native release executables
    #[clap(long)]
    pub release: bool,

    /// Native is the only target supported by cram test
    #[clap(long, value_enum)]
    pub target: Option<CramTarget>,

    /// Arguments passed to `moon-cram test`
    #[clap(name = "SCRUT_ARGS", allow_hyphen_values(true), trailing_var_arg(true))]
    pub args: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum CramTarget {
    Native,
}

#[derive(Debug)]
struct ParsedCramArgs {
    build_flags: BuildFlags,
    auto_sync_flags: AutoSyncFlags,
    cram_args: Vec<String>,
}

#[instrument(skip_all)]
pub(crate) fn run_cram(cli: &UniversalFlags, cmd: CramSubcommand) -> anyhow::Result<i32> {
    let parsed = cram_args(cmd.command);

    let moon_cram = moonutil::BINARIES.moon_cram.clone();
    if is_scrut_help_request(&parsed.cram_args) {
        let mut command = Command::new(moon_cram);
        command.args(parsed.cram_args);
        return Ok(process::delegate(&mut command)?.code().unwrap_or(0));
    }

    let PackageDirs {
        source_dir,
        target_dir,
        mooncakes_dir,
        project_manifest_path,
    } = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .package_dirs()?;

    let build_cmd = BuildSubcommand {
        path: Vec::new(),
        build_flags: parsed.build_flags,
        auto_sync_flags: parsed.auto_sync_flags,
        watch: false,
        package: None,
    };

    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        build_cmd.auto_sync_flags.clone(),
        !build_cmd.build_flags.std(),
        build_cmd.build_flags.enable_coverage,
        cli.workspace_env.clone(),
    )
    .with_project_manifest_path(project_manifest_path.as_deref());
    let resolve_output = moonbuild_rupes_recta::resolve(&resolve_cfg, &source_dir, &mooncakes_dir)?;

    let planned_runs = crate::cli::plan_build_rr_from_resolved_all(
        cli,
        &build_cmd,
        &source_dir,
        &target_dir,
        Some(TargetBackend::Native),
        resolve_output,
    )?;

    let executable_dirs = collect_executable_dirs(&planned_runs, &source_dir);
    if cli.dry_run {
        for (build_meta, build_graph) in &planned_runs {
            rr_build::print_dry_run(
                build_graph,
                build_meta.artifacts.values(),
                &source_dir,
                &target_dir,
            );
        }
        print_dry_run_cram_command(&moon_cram, &parsed.cram_args, &executable_dirs, &source_dir);
        return Ok(0);
    }

    let _lock = FileLock::lock(&target_dir)?;
    let cfg = BuildConfig::from_flags(
        &build_cmd.build_flags,
        &cli.unstable_feature,
        cli.verbose,
        UserDiagnostics::from_flags(cli),
    );
    for (build_meta, build_graph) in planned_runs {
        rr_build::generate_all_pkgs_json(&target_dir, &build_meta, RunMode::Build)?;
        let result = rr_build::execute_build(&cfg, build_graph, &target_dir)?;
        result.print_info(cli.quiet, "building")?;
        if !result.successful() {
            return Ok(result.return_code_for_success());
        }
    }
    drop(_lock);

    let mut command = Command::new(moon_cram);
    command.args(parsed.cram_args);
    command.env("PATH", path_with_executable_dirs(&executable_dirs)?);
    Ok(process::delegate(&mut command)?.code().unwrap_or(0))
}

fn cram_args(cmd: CramCommand) -> ParsedCramArgs {
    let CramCommand::Test(cmd) = cmd;
    let build_flags = BuildFlags {
        release: cmd.release,
        target: cmd
            .target
            .map_or_else(Vec::new, |_| vec![SurfaceTarget::Native]),
        ..Default::default()
    };

    ParsedCramArgs {
        build_flags,
        auto_sync_flags: AutoSyncFlags { frozen: false },
        cram_args: std::iter::once("test".to_string())
            .chain(cmd.args)
            .collect(),
    }
}

fn is_scrut_help_request(cram_args: &[String]) -> bool {
    cram_args
        .iter()
        .skip(1)
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
}

fn collect_executable_dirs(
    planned_runs: &[(rr_build::BuildMeta, rr_build::BuildInput)],
    source_dir: &Path,
) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for (build_meta, _) in planned_runs {
        for (node, artifacts) in &build_meta.artifacts {
            if !matches!(node, BuildPlanNode::MakeExecutable(_)) {
                continue;
            }
            for artifact in &artifacts.artifacts {
                let artifact = absolutize_artifact(source_dir, artifact);
                if let Some(parent) = artifact.parent()
                    && !dirs.iter().any(|dir| dir == parent)
                {
                    dirs.push(parent.to_path_buf());
                }
            }
        }
    }
    dirs
}

fn absolutize_artifact(source_dir: &Path, artifact: &Path) -> PathBuf {
    if artifact.is_absolute() {
        artifact.to_path_buf()
    } else {
        source_dir.join(artifact)
    }
}

fn path_with_executable_dirs(executable_dirs: &[PathBuf]) -> anyhow::Result<OsString> {
    let paths = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    std::env::join_paths(executable_dirs.iter().cloned().chain(paths))
        .context("failed to construct PATH for `moon-cram`")
}

fn print_dry_run_cram_command(
    moon_cram: &Path,
    cram_args: &[String],
    executable_dirs: &[PathBuf],
    source_dir: &Path,
) {
    let replacer = moonbuild_debug::graph::PathNormalizer::new(source_dir);
    let mut args = vec![
        format!(
            "PATH={}",
            display_path_with_executable_dirs(executable_dirs, &replacer)
        ),
        replacer.normalize_command_arg(&moon_cram.to_string_lossy()),
    ];
    args.extend(
        cram_args
            .iter()
            .map(|arg| replacer.normalize_command_arg(arg)),
    );
    println!(
        "{}",
        moonutil::shlex::join_unix(args.iter().map(String::as_str))
    );
}

fn display_path_with_executable_dirs(
    executable_dirs: &[PathBuf],
    replacer: &moonbuild_debug::graph::PathNormalizer,
) -> String {
    let separator = if cfg!(windows) { ";" } else { ":" };
    executable_dirs
        .iter()
        .map(|dir| replacer.normalize_command_arg(&dir.to_string_lossy()))
        .chain(std::iter::once("$PATH".to_string()))
        .collect::<Vec<_>>()
        .join(separator)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse(args: &[&str]) -> ParsedCramArgs {
        let cmd = CramSubcommand::try_parse_from(
            std::iter::once("moon cram").chain(args.iter().copied()),
        )
        .unwrap();
        cram_args(cmd.command)
    }

    #[test]
    fn parses_release_and_forwards_remaining_args() {
        let parsed = parse(&["test", "--release", "--", "--shell", "bash"]);
        assert!(parsed.build_flags.release);
        assert_eq!(parsed.cram_args, ["test", "--shell", "bash"]);
    }

    #[test]
    fn forwards_scrut_debug_flag() {
        let parsed = parse(&["test", "--debug", "--shell", "bash", "--release"]);
        assert!(!parsed.build_flags.debug);
        assert!(!parsed.build_flags.release);
        assert_eq!(
            parsed.cram_args,
            ["test", "--debug", "--shell", "bash", "--release"]
        );
    }

    #[test]
    fn forwards_scrut_help_flag() {
        let parsed = parse(&["test", "--help"]);
        assert_eq!(parsed.cram_args, ["test", "--help"]);
        assert!(is_scrut_help_request(&parsed.cram_args));
    }

    #[test]
    fn forwards_frozen_to_cram() {
        let parsed = parse(&["test", "--frozen", "--debug"]);
        assert!(!parsed.auto_sync_flags.frozen);
        assert_eq!(parsed.cram_args, ["test", "--frozen", "--debug"]);
    }

    #[test]
    fn accepts_native_target() {
        let parsed = parse(&["test", "--target", "native"]);
        assert_eq!(parsed.build_flags.target, [SurfaceTarget::Native]);
    }

    #[test]
    fn rejects_non_native_target() {
        let err =
            CramSubcommand::try_parse_from(["moon cram", "test", "--target=wasm-gc"]).unwrap_err();
        assert!(err.to_string().contains("invalid value"));
    }

    #[test]
    fn forwards_unknown_short_flag_to_cram() {
        let parsed = parse(&["test", "-j4", "--", "--shell", "bash"]);
        assert_eq!(parsed.build_flags.jobs, None);
        assert_eq!(parsed.cram_args, ["test", "-j4", "--", "--shell", "bash"]);
    }

    #[test]
    fn forwards_args_after_separator() {
        let parsed = parse(&["test", "--release", "--", "--list"]);
        assert!(parsed.build_flags.release);
        assert_eq!(parsed.cram_args, ["test", "--list"]);
    }
}
