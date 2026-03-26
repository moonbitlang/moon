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
use moonbuild_rupes_recta::intent::UserIntent;
use moonutil::cli::UniversalFlags;
use moonutil::common::RunMode;
use moonutil::common::WATCH_MODE_DIR;
use moonutil::common::{BUILD_DIR, lower_surface_targets};
use moonutil::common::{FileLock, TargetBackend};
use moonutil::mooncakes::sync::AutoSyncFlags;
use std::path::{Path, PathBuf};
use tracing::{Level, instrument};

use crate::filter::{
    canonicalize_with_filename, ensure_package_supports_backend, filter_pkg_by_dir,
    package_supports_backend, select_supported_packages,
};
use crate::rr_build::{self, BuildConfig, CalcUserIntentOutput, preconfig_compile};
use crate::user_diagnostics::UserDiagnostics;
use crate::watch::prebuild_output::{PrebuildWatchPaths, rr_get_prebuild_watch_paths};
use crate::watch::{WatchOutput, watching};

use super::BuildFlags;

/// Check the current package, but don't build object files
#[derive(Debug, clap::Parser)]
#[clap(group = clap::ArgGroup::new("package_selector").multiple(false))]
pub(crate) struct CheckSubcommand {
    #[clap(flatten)]
    pub build_flags: BuildFlags,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Monitor the file system and automatically check files
    #[clap(long, short)]
    pub watch: bool,

    /// Legacy package directory path relative to the module source root (`source` in `moon.mod.json`)
    //
    // This selects a package directory under the module source root, not an arbitrary
    // filesystem path. Use positional `PATH` for filesystem paths.
    // TODO: Unify the `-p` flag to specifying package name, see #1139
    #[clap(
        long,
        short_alias = 'p',
        value_name = "PACKAGE_DIR",
        hide = true,
        group = "package_selector"
    )]
    pub package_path: Option<PathBuf>,

    /// The patch file to check. Only valid when the selector resolves to a single package.
    #[clap(long, requires = "package_selector")]
    pub patch_file: Option<PathBuf>,

    /// Whether to skip the mi generation. Only valid when the selector resolves to a single package.
    #[clap(long, requires = "package_selector")]
    pub no_mi: bool,

    /// Whether to explain the error code with details.
    #[clap(long)]
    pub explain: bool,

    /// Filesystem path to a package directory or `.mbt` / `.mbt.md` file
    #[clap(conflicts_with = "watch", name = "PATH", group = "package_selector")]
    pub path: Vec<PathBuf>,

    /// Check whether the code is properly formatted
    #[clap(long)]
    pub fmt: bool,
}

#[instrument(skip_all)]
pub(crate) fn run_check(cli: &UniversalFlags, cmd: &CheckSubcommand) -> anyhow::Result<i32> {
    let output = UserDiagnostics::from_flags(cli);
    if cmd.fmt {
        let mut cli_for_fmt = cli.clone();
        cli_for_fmt.quiet = true;
        let fmt_exit_code = crate::cli::fmt::run_fmt(
            &cli_for_fmt,
            crate::cli::FmtSubcommand {
                check: false,
                sort_input: false,
                block_style: None,
                warn: true,
                path: cmd.path.clone(),
                args: vec![],
            },
        )?;
        if fmt_exit_code != 0 {
            output.warn("formatting code failed");
        }
    }

    // Check if we're running within a project
    let (source_dir, target_dir, single_file, project_manifest_path) = match cli
        .source_tgt_dir
        .try_into_package_dirs()
    {
        Ok(dirs) => (
            dirs.source_dir,
            dirs.target_dir,
            false,
            Some(dirs.project_manifest_path),
        ),
        Err(e) if e.allows_single_file_fallback() => {
            // Now we're talking about real single-file scenario.
            match cmd.path.as_slice() {
                [path] => {
                    let single_file_path = dunce::canonicalize(path).with_context(|| {
                        format!("failed to resolve file path `{}`", path.display())
                    })?;
                    let source_dir = single_file_path
                        .parent()
                        .context("file path must have a parent directory")?
                        .to_path_buf();
                    let target_dir = source_dir.join(BUILD_DIR);
                    (source_dir, target_dir, true, None)
                }
                [] => return Err(e.into()),
                _ => {
                    anyhow::bail!("standalone single-file `moon check` expects exactly one `PATH`");
                }
            }
        }
        Err(e) => {
            return Err(e.into());
        }
    };

    if cmd.build_flags.target.is_empty() {
        return run_check_internal(
            cli,
            cmd,
            &source_dir,
            &target_dir,
            single_file,
            project_manifest_path.as_deref(),
            None,
        );
    }

    let surface_targets = cmd.build_flags.target.clone();
    let targets = lower_surface_targets(&surface_targets);
    let mut ret_value = 0;
    for t in targets {
        let x = run_check_internal(
            cli,
            cmd,
            &source_dir,
            &target_dir,
            single_file,
            project_manifest_path.as_deref(),
            Some(t),
        )
        .context(format!("failed to run check for target {t:?}"))?;
        ret_value = ret_value.max(x);
    }
    Ok(ret_value)
}

#[instrument(skip_all)]
fn run_check_internal(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    single_file: bool,
    project_manifest_path: Option<&Path>,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    if single_file {
        run_check_for_single_file(cli, cmd, selected_target_backend)
    } else {
        run_check_normal_internal(
            cli,
            cmd,
            source_dir,
            target_dir,
            project_manifest_path,
            selected_target_backend,
        )
    }
}

fn run_check_for_single_file(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    run_check_for_single_file_rr(cli, cmd, selected_target_backend)
}

fn run_check_for_single_file_rr(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    if cmd.no_mi {
        anyhow::bail!("standalone single-file `moon check` does not support `--no-mi`");
    }
    if cmd.patch_file.is_some() {
        anyhow::bail!("standalone single-file `moon check` does not support `--patch-file`");
    }

    let path = cmd
        .path
        .first()
        .expect("path should be set in single-file mode");
    let single_file_path = dunce::canonicalize(path)
        .with_context(|| format!("failed to resolve file path `{}`", path.display()))?;
    let source_dir = single_file_path
        .parent()
        .context("file path must have a parent directory")?
        .to_path_buf();
    let raw_target_dir = source_dir.join(BUILD_DIR);
    std::fs::create_dir_all(&raw_target_dir).context("failed to create target directory")?;

    // Manually synthesize and resolve single file project
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        false,
        cmd.build_flags.enable_coverage,
    );
    let (resolved, backend) = moonbuild_rupes_recta::resolve::resolve_single_file_project(
        &resolve_cfg,
        raw_target_dir.as_path(),
        &single_file_path,
        false,
    )?;
    let selected_target_backend = selected_target_backend
        .or(cmd.build_flags.resolve_single_target_backend()?)
        .or(backend);

    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        &raw_target_dir,
        RunMode::Check,
    );

    // The rest is similar to normal check flow
    let output = UserDiagnostics::from_flags(cli);
    let (build_meta, build_graph) = rr_build::plan_build_from_resolved(
        preconfig,
        &cli.unstable_feature,
        &raw_target_dir,
        output,
        Box::new(get_user_intents_single_file),
        resolved,
    )
    .context("Failed to calculate build plan")?;

    if cli.dry_run {
        rr_build::print_dry_run(
            &build_graph,
            build_meta.artifacts.values(),
            &source_dir,
            &raw_target_dir,
        );
        return Ok(0);
    }

    let _lock = FileLock::lock(&raw_target_dir)?;

    // Generate all_pkgs.json for indirect dependency resolution
    rr_build::generate_all_pkgs_json(&raw_target_dir, &build_meta, RunMode::Check)?;
    let filename = single_file_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(String::from);
    rr_build::generate_metadata(
        &source_dir,
        &raw_target_dir,
        &build_meta,
        RunMode::Check,
        filename.as_deref(),
    )?;

    let mut cfg = BuildConfig::from_flags(
        &cmd.build_flags,
        &cli.unstable_feature,
        cli.verbose,
        UserDiagnostics::from_flags(cli),
    );
    cfg.patch_file = cmd.patch_file.clone();
    cfg.explain_errors |= cmd.explain;

    let result = rr_build::execute_build(&cfg, build_graph, &raw_target_dir)?;
    result.print_info(cli.quiet, "checking")?;

    Ok(if result.successful() { 0 } else { 1 })
}

fn get_user_intents_single_file(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    _backend: TargetBackend,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let m_packages = resolve_output
        .pkg_dirs
        .packages_for_module(resolve_output.local_modules()[0])
        .expect("Local module must exist");
    let pkg = *m_packages
        .iter()
        .next()
        .expect("Only one package should be resolved for single file package")
        .1;

    Ok(vec![UserIntent::Check(pkg)].into())
}

#[instrument(skip_all)]
fn run_check_normal_internal(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    project_manifest_path: Option<&Path>,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<i32> {
    let run_once = |watch: bool, target_dir: &Path| -> anyhow::Result<WatchOutput> {
        run_check_normal_internal_rr(
            cli,
            cmd,
            source_dir,
            target_dir,
            project_manifest_path,
            watch,
            selected_target_backend,
        )
    };
    if cmd.watch {
        // For checks, the actual target dir is a subdir of the original target
        let watch_target = target_dir.join(WATCH_MODE_DIR);
        std::fs::create_dir_all(&watch_target).with_context(|| {
            format!(
                "Failed to create target directory: '{}'",
                watch_target.display()
            )
        })?;
        watching(
            || run_once(true, &watch_target),
            source_dir,
            source_dir,
            target_dir,
        )
    } else {
        run_once(false, target_dir).map(|output| if output.ok { 0 } else { 1 })
    }
}

#[instrument(skip_all)]
fn run_check_normal_internal_rr(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    project_manifest_path: Option<&Path>,
    _watch: bool,
    selected_target_backend: Option<TargetBackend>,
) -> anyhow::Result<WatchOutput> {
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new(
        cmd.auto_sync_flags.clone(),
        !cmd.build_flags.std(),
        cmd.build_flags.enable_coverage,
    )
    .with_project_manifest_path(project_manifest_path);
    let resolve_output = moonbuild_rupes_recta::resolve(&resolve_cfg, source_dir)
        .context("Failed to calculate build plan")?;
    let (build_meta, build_graph) = plan_check_rr_from_resolved(
        cli,
        cmd,
        source_dir,
        target_dir,
        selected_target_backend,
        resolve_output,
    )
    .context("Failed to calculate build plan")?;

    let prebuild_list = if _watch {
        rr_get_prebuild_watch_paths(&build_meta.resolve_output)
    } else {
        PrebuildWatchPaths {
            ignored_paths: Vec::new(),
            watched_paths: Vec::new(),
        }
    };

    if cli.dry_run {
        rr_build::print_dry_run(
            &build_graph,
            build_meta.artifacts.values(),
            source_dir,
            target_dir,
        );
        Ok(WatchOutput {
            ok: true,
            additional_ignored_paths: prebuild_list.ignored_paths,
            additional_watched_paths: prebuild_list.watched_paths,
        })
    } else {
        let _lock = FileLock::lock(target_dir)?;
        // Generate all_pkgs.json for indirect dependency resolution
        rr_build::generate_all_pkgs_json(target_dir, &build_meta, RunMode::Check)?;
        // Generate metadata for IDE
        rr_build::generate_metadata(source_dir, target_dir, &build_meta, RunMode::Check, None)?;

        let mut cfg = BuildConfig::from_flags(
            &cmd.build_flags,
            &cli.unstable_feature,
            cli.verbose,
            UserDiagnostics::from_flags(cli),
        );
        cfg.patch_file = cmd.patch_file.clone();
        cfg.explain_errors |= cmd.explain;
        let result = rr_build::execute_build(&cfg, build_graph, target_dir)?;
        result.print_info(cli.quiet, "checking")?;
        Ok(WatchOutput {
            ok: result.successful(),
            additional_ignored_paths: prebuild_list.ignored_paths,
            additional_watched_paths: prebuild_list.watched_paths,
        })
    }
}

pub(crate) fn plan_check_rr_from_resolved(
    cli: &UniversalFlags,
    cmd: &CheckSubcommand,
    source_dir: &Path,
    target_dir: &Path,
    selected_target_backend: Option<TargetBackend>,
    resolve_output: moonbuild_rupes_recta::ResolveOutput,
) -> anyhow::Result<(rr_build::BuildMeta, rr_build::BuildInput)> {
    let preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &cmd.build_flags,
        selected_target_backend,
        target_dir,
        RunMode::Check,
    );
    let output = UserDiagnostics::from_flags(cli);

    rr_build::plan_build_from_resolved(
        preconfig,
        &cli.unstable_feature,
        target_dir,
        output,
        Box::new(|resolved, target_backend| {
            if let Some(filter_path) = cmd.package_path.as_deref() {
                return calc_user_intent_from_package_path(
                    resolved,
                    source_dir,
                    filter_path,
                    target_backend,
                    cmd.no_mi,
                    cmd.patch_file.as_deref(),
                );
            }

            calc_user_intent(
                resolved,
                &cmd.path,
                target_backend,
                cmd.no_mi,
                cmd.patch_file.as_deref(),
                output,
            )
        }),
        resolve_output,
    )
}

/// Generate user intent of checking all packages in the current workspace.
///
#[instrument(level = Level::DEBUG, skip_all)]
fn calc_user_intent_from_package_path(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    source_dir: &Path,
    filter_path: &Path,
    target_backend: TargetBackend,
    no_mi: bool,
    patch_file: Option<&Path>,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let (dir, _) = canonicalize_with_filename(&source_dir.join(filter_path))?;
    let pkg = filter_pkg_by_dir(resolve_output, &dir)?;
    ensure_package_supports_backend(resolve_output, pkg, target_backend)?;
    let directive =
        rr_build::build_patch_directive_for_package(pkg, no_mi, None, patch_file, false)?;
    Ok((vec![UserIntent::Check(pkg)], directive).into())
}

#[instrument(level = Level::DEBUG, skip_all)]
fn calc_user_intent(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    paths: &[PathBuf],
    target_backend: TargetBackend,
    no_mi: bool,
    patch_file: Option<&Path>,
    output: UserDiagnostics,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    if !paths.is_empty() {
        let selected = select_supported_packages(resolve_output, paths, target_backend, output)?;
        let directive = build_directive_for_selected_packages(&selected, no_mi, patch_file)?;
        Ok((
            selected.into_iter().map(UserIntent::Check).collect(),
            directive,
        )
            .into())
    } else {
        let intents: Vec<_> = rr_build::local_packages(resolve_output)
            .filter(|&pkg| package_supports_backend(resolve_output, pkg, target_backend))
            .map(UserIntent::Check)
            .collect();
        Ok(intents.into())
    }
}

fn build_directive_for_selected_packages(
    selected: &[moonbuild_rupes_recta::model::PackageId],
    no_mi: bool,
    patch_file: Option<&Path>,
) -> anyhow::Result<moonbuild_rupes_recta::build_plan::InputDirective> {
    match selected {
        [pkg] => rr_build::build_patch_directive_for_package(*pkg, no_mi, None, patch_file, false),
        [] => {
            if no_mi {
                anyhow::bail!("`--no-mi` requires the selector to resolve to a single package");
            }
            if patch_file.is_some() {
                anyhow::bail!(
                    "`--patch-file` requires the selector to resolve to a single package"
                );
            }
            Ok(Default::default())
        }
        _ => {
            if no_mi {
                anyhow::bail!("`--no-mi` requires the selector to resolve to a single package");
            }
            if patch_file.is_some() {
                anyhow::bail!(
                    "`--patch-file` requires the selector to resolve to a single package"
                );
            }
            Ok(Default::default())
        }
    }
}
