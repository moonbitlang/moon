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
    ffi::{OsStr, OsString},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context;
use clap::error::ErrorKind;
use clap::{Subcommand, ValueEnum};
use moonbuild_rupes_recta::model::BuildPlanNode;
use moonutil::{
    cli_support::AutoSyncFlags,
    command_output::CommandOutput,
    locks::FileLock,
    project::PackageDirs,
    target::{SurfaceTarget, TargetBackend},
};
use tracing::instrument;

use crate::{
    cli::{BuildSubcommand, process},
    rr_build::{self, BuildConfig},
};

use super::{BuildFlags, UniversalFlags};

/// Run cram tests with project binaries on PATH (experimental)
#[derive(Debug, clap::Parser, Clone)]
#[clap(disable_help_subcommand(true))]
pub(crate) struct CramSubcommand {
    #[clap(subcommand)]
    pub command: Option<CramCommand>,
}

#[derive(Clone, Debug, Subcommand)]
pub(crate) enum CramCommand {
    Test(CramTestSubcommand),

    #[clap(external_subcommand)]
    External(Vec<String>),
}

/// Build native executables, then run cram tests with their directories on PATH
#[derive(Debug, clap::Parser, Clone)]
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
pub(crate) fn run_cram(
    cli: &UniversalFlags,
    cmd: CramSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    match cmd.command {
        Some(CramCommand::Test(cmd)) => run_cram_test(cli, cmd, output),
        Some(CramCommand::External(args)) => delegate_moon_cram(args),
        None => delegate_moon_cram(Vec::new()),
    }
}

fn run_cram_test(
    cli: &UniversalFlags,
    cmd: CramTestSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    let parsed = cram_args(cmd);
    let moon_cram = if cli.dry_run {
        moonutil::toolchain::BINARIES.moon_cram.clone()
    } else {
        resolve_moon_cram()?
    };
    if is_scrut_help_request(&parsed.cram_args) {
        let mut command = Command::new(moon_cram);
        command.args(parsed.cram_args);
        return Ok(process::delegate(&mut command)?.code().unwrap_or(0));
    }

    let dirs = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .package_dirs()?;
    let PackageDirs {
        source_dir,
        target_dir,
        mooncake_bin_dir,
        ..
    } = &dirs;

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
    );
    let synced_env = moonbuild_rupes_recta::sync_dependencies(&resolve_cfg, &dirs)?;
    let resolve_output =
        moonbuild_rupes_recta::resolve_synced_project(&resolve_cfg, synced_env, user_log)?;

    let planned_runs = crate::cli::plan_build_rr_from_resolved_all(
        cli,
        &build_cmd,
        source_dir,
        target_dir,
        mooncake_bin_dir,
        Some(TargetBackend::Native),
        resolve_output,
        user_log,
    )?;

    let executable_dirs = collect_executable_dirs(&planned_runs, source_dir);
    if cli.dry_run {
        output.write_result(|writer| {
            for (build_meta, build_graph) in &planned_runs {
                rr_build::write_dry_run(
                    writer,
                    build_graph,
                    build_meta.artifacts.values(),
                    source_dir,
                    target_dir,
                )?;
            }
            write_dry_run_cram_command(
                writer,
                &moon_cram,
                &parsed.cram_args,
                &executable_dirs,
                source_dir,
            )
        })?;
        return Ok(0);
    }

    let _lock = FileLock::lock(target_dir)?;
    let cfg = BuildConfig::from_flags(&build_cmd.build_flags, &cli.unstable_feature, cli.verbose);
    for (build_meta, build_graph) in planned_runs {
        rr_build::generate_all_pkgs_json(&build_meta)?;
        let result = rr_build::execute_build(&cfg, build_graph, target_dir, user_log)?;
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

fn delegate_moon_cram(args: Vec<String>) -> anyhow::Result<i32> {
    delegate_moon_cram_with_current_dir(None, args)
}

fn delegate_moon_cram_with_current_dir(
    current_dir: Option<&Path>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> anyhow::Result<i32> {
    let mut command = Command::new(resolve_moon_cram()?);
    if let Some(dir) = current_dir {
        command.current_dir(dir);
    }
    command.args(args);
    Ok(process::delegate(&mut command)?.code().unwrap_or(0))
}

pub(crate) fn exit_if_cram_external_request(err: &clap::Error, raw_args: &[OsString]) {
    if err.kind() != ErrorKind::UnknownArgument {
        return;
    }

    let Some((current_dir, args)) = cram_external_args(raw_args) else {
        return;
    };
    match delegate_moon_cram_with_current_dir(current_dir.as_deref(), args) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("Error: {err:?}");
            std::process::exit(-1);
        }
    }
}

fn cram_external_args(raw_args: &[OsString]) -> Option<(Option<PathBuf>, Vec<OsString>)> {
    let mut current_dir = None;
    let mut index = 1;
    while index < raw_args.len() {
        let arg = &raw_args[index];

        if arg == OsStr::new("-C") {
            index += 1;
            if let Some(dir) = raw_args.get(index) {
                current_dir = Some(PathBuf::from(dir));
            }
        } else if matches!(
            arg.to_str(),
            Some("--target-dir" | "--unstable-feature" | "-Z")
        ) {
            index += 1;
        } else if is_global_bool_arg(arg) {
        } else {
            let tail = &raw_args[index + 1..];
            return (arg == OsStr::new("cram") && is_external_cram_tail(tail))
                .then(|| (current_dir, tail.to_vec()));
        }
        index += 1;
    }
    None
}

fn is_global_bool_arg(arg: &OsStr) -> bool {
    matches!(
        arg.to_str(),
        Some(
            "-V" | "--version"
                | "-q"
                | "--quiet"
                | "-v"
                | "--verbose"
                | "--trace"
                | "--dry-run"
                | "--build-graph"
        )
    )
}

fn is_external_cram_tail(tail: &[OsString]) -> bool {
    matches!(tail.first(), Some(arg) if arg != OsStr::new("test"))
}

fn resolve_moon_cram() -> anyhow::Result<PathBuf> {
    let moon_cram = moonutil::toolchain::BINARIES.moon_cram.clone();
    if moon_cram.is_absolute() || moon_cram.components().count() > 1 {
        return Ok(moon_cram);
    }
    which::which(&moon_cram).with_context(|| {
        format!(
            "no such subcommand: `cram`, is `{}` a valid executable accessible via your `PATH`?",
            moon_cram.display()
        )
    })
}

fn cram_args(cmd: CramTestSubcommand) -> ParsedCramArgs {
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

fn write_dry_run_cram_command(
    output: &mut dyn Write,
    moon_cram: &Path,
    cram_args: &[String],
    executable_dirs: &[PathBuf],
    source_dir: &Path,
) -> std::io::Result<()> {
    let replacer = moonbuild::dry_run::PathNormalizer::new(source_dir);
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
    writeln!(
        output,
        "{}",
        moonutil::shlex::join_unix(args.iter().map(String::as_str))
    )
}

fn display_path_with_executable_dirs(
    executable_dirs: &[PathBuf],
    replacer: &moonbuild::dry_run::PathNormalizer,
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

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    fn parse_command(args: &[&str]) -> CramCommand {
        CramSubcommand::try_parse_from(std::iter::once("moon cram").chain(args.iter().copied()))
            .unwrap()
            .command
            .unwrap()
    }

    fn parse(args: &[&str]) -> ParsedCramArgs {
        let CramCommand::Test(cmd) = parse_command(args) else {
            panic!("expected `moon cram test` to parse as the built-in cram test wrapper");
        };
        cram_args(cmd)
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
    fn shows_wrapper_help_for_test_help_flag() {
        let err = CramSubcommand::try_parse_from(["moon cram", "test", "--help"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::DisplayHelp);
        let help = err.to_string();
        assert!(help.contains("--release"));
        assert!(help.contains("--target <TARGET>"));
        assert!(help.contains("Arguments passed to `moon-cram test`"));
    }

    #[test]
    fn forwards_scrut_help_flag_after_separator() {
        let parsed = parse(&["test", "--", "--help"]);
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
    fn delegates_unknown_cram_subcommand() {
        let CramCommand::External(args) = parse_command(&["list", "--json"]) else {
            panic!("expected unknown cram subcommand to parse as external args");
        };
        assert_eq!(args, ["list", "--json"]);
    }

    #[test]
    fn shows_wrapper_help_for_parent_help_flag() {
        let err = CramSubcommand::try_parse_from(["moon cram", "--help"]).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::DisplayHelp);
        let help = err.to_string();
        assert!(help.contains("Run cram tests with project binaries on PATH"));
        assert!(help.contains("test"));
    }

    #[test]
    fn detects_parent_flag_as_external_cram_args() {
        assert_eq!(
            cram_external_args(&os(&["moon", "cram", "--version"])),
            Some((None, os(&["--version"])))
        );
    }

    #[test]
    fn detects_parent_flag_as_external_cram_args_after_global_flag() {
        assert_eq!(
            cram_external_args(&os(&["moon", "-q", "cram", "--version"])),
            Some((None, os(&["--version"])))
        );
    }

    #[test]
    fn detects_parent_flag_as_external_cram_args_after_global_value_flag() {
        assert_eq!(
            cram_external_args(&os(&[
                "moon",
                "--target-dir",
                "_build-alt",
                "cram",
                "--version"
            ])),
            Some((None, os(&["--version"])))
        );
    }

    #[test]
    fn preserves_chdir_for_external_cram_args() {
        assert_eq!(
            cram_external_args(&os(&["moon", "-C", "sub", "cram", "--version"])),
            Some((Some(PathBuf::from("sub")), os(&["--version"])))
        );
    }

    #[test]
    fn ignores_cram_argument_under_other_top_level_subcommand() {
        assert_eq!(
            cram_external_args(&os(&["moon", "build", "cram", "--bad-flag"])),
            None
        );
    }

    #[test]
    fn keeps_builtin_test_parse_errors_in_moon() {
        assert_eq!(cram_external_args(&os(&["moon", "cram", "test"])), None);
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
