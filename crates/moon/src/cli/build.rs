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

mod selection;

use crate::cli::target::lower_surface_targets;
use anyhow::Context;
use moonbuild::BuildMeta;
use moonbuild::compile::{BuildSession, MetadataOutput};
use moonbuild::execution::BuildInput;
use moonbuild_rupes_recta::RunMode;
use moonbuild_rupes_recta::build_plan::ArtifactKey;
use moonbuild_rupes_recta::intent::UserIntent;
use moonbuild_rupes_recta::pkg_name::PackageFQN;
use mooncake::cli::AutoSyncFlags;
use moonutil::cache::{CacheKind, resolve_cache_root};
use moonutil::command_output::CommandOutput;
use moonutil::project::PackageDirs;
use moonutil::target::TargetBackend;
use moonutil::user_log::UserLog;
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::instrument;

use crate::watch::prebuild_output::{PrebuildWatchPaths, rr_get_prebuild_watch_paths};
use crate::watch::{WatchOutput, watching};

use super::{BuildFlags, UniversalFlags};

/// Build the current package
///
/// A successful build also lists each executable and foreign library it built
/// on stdout, with its package, target backend and absolute path. `--quiet`
/// does not hide the list. With `--output-json`, each one is an `artifact`
/// JSON message instead.
#[derive(Debug, clap::Parser, Clone)]
pub(super) struct BuildSubcommand {
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

#[instrument(skip_all)]
pub(super) fn run_build(
    cli: &UniversalFlags,
    cmd: BuildSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let targets = lower_surface_targets(&cmd.build_flags.target);
    if let Some(path) = super::standalone_mbtx_path(&cmd.path, "moon build")? {
        anyhow::ensure!(
            !cmd.watch,
            "standalone `.mbtx` `moon build` does not support `--watch`"
        );
        let single_file = cli.source_tgt_dir.single_file_package_dirs(path)?;
        return run_build_for_single_file_rr(
            cli,
            &cmd,
            &single_file.input_path,
            &single_file.package_dirs,
            &targets,
            output,
        );
    }

    let dirs = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(output.user_log())?
        .package_dirs()?;

    if cmd.build_flags.target.is_empty() {
        return run_build_internal(cli, &cmd, &dirs, None, output);
    }
    // Watch reruns must synchronize and resolve fresh project state each time.
    if cmd.watch {
        let mut ret_value = 0;
        for t in targets {
            let x = run_build_internal(cli, &cmd, &dirs, Some(t), output)
                .context(format!("failed to run build for target {t:?}"))?;
            ret_value = ret_value.max(x);
        }
        return Ok(ret_value);
    }

    let discovered = sync_and_discover_build_project(&cmd, &dirs, output.user_log())?;
    let session = BuildSession::new(&dirs.target_dir, cli.dry_run, output.user_log())?;
    let result = run_build_rr_from_discovered(
        &session, cli, &cmd, &dirs, false, &targets, discovered, output,
    )
    .with_context(|| match targets.as_slice() {
        [target] => format!("failed to run build for target {target:?}"),
        _ => format!("failed to run build for targets {targets:?}"),
    })?;
    Ok(if result.ok { 0 } else { 1 })
}

/// Resolve one standalone `.mbtx` input and build it for every selected Target Backend.
#[allow(clippy::too_many_arguments)]
fn run_build_for_single_file_rr(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    single_file_path: &Path,
    dirs: &PackageDirs,
    selected_target_backends: &[TargetBackend],
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    let PackageDirs {
        source_dir,
        target_dir,
        ..
    } = dirs;
    std::fs::create_dir_all(target_dir).context("failed to create target directory")?;

    let preparation_config = moonbuild::prepare::ProjectPreparationConfig::new(
        cmd.auto_sync_flags.frozen,
        cmd.build_flags.enable_coverage,
    )
    .with_jobs(cmd.build_flags.jobs)
    .with_dependency_source_cache(
        resolve_cache_root(CacheKind::DependencySources)
            .context("Failed to resolve the module dependency graph")?,
    );
    let (discovered, backend) = moonbuild::prepare::discover_single_file_project(
        &preparation_config,
        dirs,
        single_file_path,
        true,
        user_log,
    )?;
    let target_backends = super::selection::single_file_target_backends(
        &discovered,
        selected_target_backends,
        backend,
        user_log,
    );
    let configurations = target_backends
        .iter()
        .map(|&backend| cmd.build_flags.configuration(backend, RunMode::Build))
        .collect::<Vec<_>>();
    let resolved = discovered.resolve_packages(&target_backends, user_log)?;

    let session = BuildSession::new(target_dir, cli.dry_run, user_log)?;

    let package = super::selection::local_packages(&resolved)
        .next()
        .context("single-file project must resolve exactly one local package")?;
    let mut planned_runs = Vec::with_capacity(target_backends.len());
    for configuration in configurations {
        let compile_config = cmd.build_flags.compile_config(
            cli,
            configuration,
            target_dir,
            RunMode::Build,
            user_log,
            &resolved,
        )?;
        planned_runs.push(session.plan(
            compile_config,
            vec![UserIntent::Build(package)].into(),
            resolved.clone(),
            cmd.build_flags.jobs,
            cmd.auto_sync_flags.frozen,
        )?);
    }

    let ok = execute_planned_builds(&session, cli, cmd, planned_runs, source_dir, output)?;
    Ok(if ok { 0 } else { 1 })
}

#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
fn run_build_internal(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    dirs: &PackageDirs,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let f = |watch: bool| run_build_rr(cli, cmd, dirs, watch, selected_target_backend, output);

    if cmd.watch {
        watching(|| f(true), &dirs.source_dir, &dirs.target_dir)
    } else {
        f(false).map(|output| if output.ok { 0 } else { 1 })
    }
}

fn sync_and_discover_build_project(
    cmd: &BuildSubcommand,
    dirs: &PackageDirs,
    user_log: &UserLog,
) -> anyhow::Result<moonbuild_rupes_recta::DiscoveredProject> {
    let preparation_config = moonbuild::prepare::ProjectPreparationConfig::new(
        cmd.auto_sync_flags.frozen,
        cmd.build_flags.enable_coverage,
    )
    .with_jobs(cmd.build_flags.jobs);
    moonbuild::prepare::sync_and_discover_project(&preparation_config, dirs, user_log)
}

/// Run the build routine in RR backend
///
/// - `watch`: True if in watch mode, will output ignore paths for prebuild outputs
#[instrument(skip_all)]
#[allow(clippy::too_many_arguments)]
fn run_build_rr(
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    dirs: &PackageDirs,
    watch: bool,
    selected_target_backend: Option<TargetBackend>,
    output: &CommandOutput,
) -> anyhow::Result<WatchOutput> {
    let discovered = sync_and_discover_build_project(cmd, dirs, output.user_log())?;
    let session = BuildSession::new(&dirs.target_dir, cli.dry_run, output.user_log())?;
    run_build_rr_from_discovered(
        &session,
        cli,
        cmd,
        dirs,
        watch,
        selected_target_backend.as_slice(),
        discovered,
        output,
    )
}

/// Plans and executes a build from a discovered project.
///
/// The supplied session protects planning and execution with the target-directory lock.
#[allow(clippy::too_many_arguments)]
fn run_build_rr_from_discovered(
    session: &BuildSession,
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    dirs: &PackageDirs,
    watch: bool,
    selected_target_backends: &[TargetBackend],
    discovered: moonbuild_rupes_recta::DiscoveredProject,
    output: &CommandOutput,
) -> anyhow::Result<WatchOutput> {
    let user_log = output.user_log();
    let PackageDirs {
        source_dir,
        target_dir,
        ..
    } = dirs;
    let prebuild_list = if watch {
        rr_get_prebuild_watch_paths(&discovered)
    } else {
        PrebuildWatchPaths {
            ignored_paths: Vec::new(),
            watched_paths: Vec::new(),
        }
    };
    let planned_runs = plan_build_rr_from_discovered(
        session,
        cli,
        cmd,
        target_dir,
        selected_target_backends,
        discovered,
        user_log,
    )?;

    if planned_runs.is_empty() {
        return Ok(WatchOutput {
            ok: true,
            additional_ignored_paths: prebuild_list.ignored_paths,
            additional_watched_paths: prebuild_list.watched_paths,
        });
    }

    let ok = execute_planned_builds(session, cli, cmd, planned_runs, source_dir, output)?;
    Ok(WatchOutput {
        ok,
        additional_ignored_paths: prebuild_list.ignored_paths,
        additional_watched_paths: prebuild_list.watched_paths,
    })
}

/// Run the planned builds, or print their commands in a dry run.
///
/// A successful build then reports its outputs; see [`built_outputs`].
fn execute_planned_builds(
    session: &BuildSession,
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    planned_runs: Vec<(BuildMeta, BuildInput)>,
    source_dir: &Path,
    output: &CommandOutput,
) -> anyhow::Result<bool> {
    let (build_metas, build_inputs): (Vec<_>, Vec<_>) = planned_runs.into_iter().unzip();
    if cli.dry_run {
        output.write_result(|writer| {
            let build_input = BuildInput::compose(build_inputs).map_err(std::io::Error::other)?;
            crate::dry_run::write_dry_run(writer, &build_input, source_dir)?;
            Ok::<_, std::io::Error>(())
        })?;
        return Ok(true);
    }

    let build_input = BuildInput::compose(build_inputs)?;
    let config = cmd
        .build_flags
        .execution_config(&cli.unstable_feature, cli.verbose);
    let result = session.execute(
        &config,
        build_input,
        &build_metas.iter().collect::<Vec<_>>(),
        MetadataOutput::Compiler,
    )?;
    result.print_info(cli.quiet, "building")?;
    let outputs = built_outputs(&build_metas);
    output.write_result(|writer| {
        if cmd.build_flags.output_json {
            write_built_outputs_json(writer, &outputs)
        } else {
            write_built_outputs_table(writer, &outputs)
        }
    })?;
    Ok(result.successful())
}

/// An executable or foreign library that a build produced.
struct BuiltOutput<'a> {
    package: &'a PackageFQN,
    target_backend: TargetBackend,
    path: &'a Path,
}

/// The executables and foreign libraries the builds produced, in the order of
/// the selected target backends.
///
/// These are the outputs users consume outside moon. Other requested
/// artifacts, such as a library package's `.core` file, are build
/// intermediates and are not listed.
fn built_outputs(build_metas: &[BuildMeta]) -> Vec<BuiltOutput<'_>> {
    let mut outputs = Vec::new();
    for build_meta in build_metas {
        let target_backend = build_meta.target_backend();
        for (artifact, paths) in &build_meta.artifacts {
            let (ArtifactKey::Executable { package, .. } | ArtifactKey::ForeignLibrary { package }) =
                artifact
            else {
                continue;
            };
            let package = &build_meta
                .resolved_project
                .pkg_dirs
                .get_package(*package)
                .fqn;
            outputs.extend(paths.iter().map(|path| BuiltOutput {
                package,
                target_backend,
                path: dunce::simplified(path),
            }));
        }
    }
    outputs
}

/// Write the built outputs as a table of package, target backend and path,
/// with the path last so it is easy to copy:
///
/// ```text
/// Artifacts:
///   username/foreign/lib  native  /p/_build/native/release/build/lib/lib.a
/// ```
fn write_built_outputs_table(
    writer: &mut dyn Write,
    outputs: &[BuiltOutput<'_>],
) -> std::io::Result<()> {
    if outputs.is_empty() {
        return Ok(());
    }
    let packages: Vec<String> = outputs.iter().map(|o| o.package.to_string()).collect();
    let package_width = packages.iter().map(String::len).max().unwrap_or_default();
    let backend_width = outputs
        .iter()
        .map(|o| o.target_backend.to_flag().len())
        .max()
        .unwrap_or_default();
    writeln!(writer, "Artifacts:")?;
    for (output, package) in outputs.iter().zip(&packages) {
        writeln!(
            writer,
            "  {package:<package_width$}  {:<backend_width$}  {}",
            output.target_backend.to_flag(),
            output.path.display()
        )?;
    }
    Ok(())
}

/// One built output in the `--output-json` stream, next to the compiler's
/// `diagnostic` messages.
#[derive(Serialize)]
struct BuiltOutputMessage<'a> {
    #[serde(rename = "$message_type")]
    message_type: &'static str,
    package: String,
    target_backend: &'static str,
    path: &'a Path,
}

/// Write each built output as an `artifact` JSON message on its own line.
fn write_built_outputs_json(
    writer: &mut dyn Write,
    outputs: &[BuiltOutput<'_>],
) -> std::io::Result<()> {
    for output in outputs {
        let message = BuiltOutputMessage {
            message_type: "artifact",
            package: output.package.to_string(),
            target_backend: output.target_backend.to_flag(),
            path: output.path,
        };
        serde_json::to_writer(&mut *writer, &message)?;
        writeln!(writer)?;
    }
    Ok(())
}

/// Select complete requests, resolve their backends, then plan each request.
#[allow(clippy::too_many_arguments)]
pub(super) fn plan_build_rr_from_discovered(
    session: &BuildSession,
    cli: &UniversalFlags,
    cmd: &BuildSubcommand,
    target_dir: &Path,
    selected_target_backends: &[TargetBackend],
    discovered: moonbuild_rupes_recta::DiscoveredProject,
    user_log: &UserLog,
) -> anyhow::Result<Vec<(BuildMeta, BuildInput)>> {
    let requests =
        selection::select_requests(&discovered, cmd, selected_target_backends, user_log)?;
    let configurations = requests
        .iter()
        .map(|(backend, _)| cmd.build_flags.configuration(*backend, RunMode::Build))
        .collect::<Vec<_>>();
    let target_backends = configurations
        .iter()
        .map(|configuration| configuration.target_backend())
        .collect::<Vec<_>>();
    let resolved_project = discovered.resolve_packages(&target_backends, user_log)?;
    requests
        .into_iter()
        .zip(configurations)
        .map(|((_, request), configuration)| {
            let compile_config = cmd.build_flags.compile_config(
                cli,
                configuration,
                target_dir,
                RunMode::Build,
                user_log,
                &resolved_project,
            )?;
            session.plan(
                compile_config,
                request,
                resolved_project.clone(),
                cmd.build_flags.jobs,
                cmd.auto_sync_flags.frozen,
            )
        })
        .collect()
}
