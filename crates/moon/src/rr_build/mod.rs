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

//! Common build tools for using Rupes Recta builds.
//!
//! This module provides very high-level constructs to drive a compiling process
//! from raw input until all the expected artifacts are built.
//!
//! # How to use this module
//!
//! - If you just want to conveniently compile a thing: Use [`compile`].
//! - If you want to insert dry-running, your compilation process is split in
//!   two parts: [``]

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Context;
use indexmap::IndexMap;
use moonbuild::entry::{
    create_progress_console, render_and_catch_callback, N2RunStats, ResultCatcher,
};
use moonbuild_rupes_recta::{
    build_plan::InputDirective,
    model::{Artifacts, BuildPlanNode, PackageId, TargetKind},
    CompileConfig, ResolveConfig, ResolveOutput,
};
use moonutil::{
    cli::UniversalFlags,
    common::{
        DiagnosticLevel, RunMode, TargetBackend, BLACKBOX_TEST_PATCH, MOONBITLANG_CORE,
        WHITEBOX_TEST_PATCH,
    },
    cond_expr::OptLevel,
    features::FeatureGate,
    mooncakes::{sync::AutoSyncFlags, ModuleId},
};
use tracing::{instrument, Level};

use crate::cli::BuildFlags;

mod dry_run;
pub use dry_run::{dry_print_command, print_dry_run, print_dry_run_all};

/// The function that calculates the user intent for the build process.
///
/// Params:
/// - The output of the resolve step. All modules and packages that this module
///     are available in this value.
/// - The list of modules that were input into the compile process (those that
///     exist in the source directory).
///
/// Returns: A vector of [`UserIntent`]s, representing what the user would like
/// to do
pub type CalcUserIntentFn<'b> = dyn for<'a> FnOnce(&'a ResolveOutput, &'a [ModuleId]) -> anyhow::Result<CalcUserIntentOutput>
    + 'b;

pub struct CalcUserIntentOutput {
    pub intent: Vec<BuildPlanNode>,
    pub directive: InputDirective,
}

impl From<Vec<BuildPlanNode>> for CalcUserIntentOutput {
    fn from(intent: Vec<BuildPlanNode>) -> Self {
        Self {
            intent,
            directive: InputDirective::default(),
        }
    }
}

impl From<(Vec<BuildPlanNode>, InputDirective)> for CalcUserIntentOutput {
    fn from((intent, directive): (Vec<BuildPlanNode>, InputDirective)) -> Self {
        Self { intent, directive }
    }
}

/// Convenient function to build a directive based on input kind
pub fn build_patch_directive_for_package(
    pkg: PackageId,
    no_mi: bool,
    patch_file: Option<&Path>,
) -> anyhow::Result<InputDirective> {
    let patch_directive = if let Some(path) = patch_file {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("patch file path is not valid utf-8"))?;
        let kind = if path_str.ends_with(WHITEBOX_TEST_PATCH) {
            TargetKind::WhiteboxTest
        } else if path_str.ends_with(BLACKBOX_TEST_PATCH) {
            TargetKind::BlackboxTest
        } else {
            TargetKind::Source
        };
        Some((pkg.build_target(kind), path.to_path_buf()))
    } else {
        None
    };

    Ok(InputDirective {
        specify_no_mi_for: no_mi.then_some(pkg),
        specify_patch_file: patch_directive,
    })
}

/// Build metadata containing information needed for build context and results.
/// The build graph is kept separate to allow execute_build to take ownership of it.
pub struct BuildMeta {
    /// The result of the resolve step, containing package metadata
    pub resolve_output: ResolveOutput,

    /// The list of artifacts that will be produced
    pub artifacts: IndexMap<BuildPlanNode, Artifacts>,

    /// The target backend used in this compile process
    pub target_backend: TargetBackend,

    /// The main optimization level used in this compile process
    pub opt_level: OptLevel,
}

/// Represents the result of the build process
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildResult {
    /// The build succeeded with the given number of tasks executed.
    Succeeded(usize),
    /// The build failed.
    Failed,
}

impl BuildResult {
    /// Whether the build was successful.
    pub fn successful(&self) -> bool {
        matches!(self, BuildResult::Succeeded(_))
    }

    /// Get the return code that should be returned to the shell.
    pub fn return_code_for_success(&self) -> i32 {
        if self.successful() {
            0
        } else {
            1
        }
    }

    /// Print information about the build result.
    pub fn print_info(&self) {
        match self {
            BuildResult::Succeeded(n) => {
                println!("{} task(s) executed.", n);
            }
            BuildResult::Failed => {
                println!("Build failed.");
            }
        }
    }
}

/// A preliminary configuration that does not require run-time information to
/// populate. Will be transformed into [`CompileConfig`] later in the pipeline.
///
/// This type might be subject to change.
pub struct CompilePreConfig {
    frozen: bool,
    target_backend: Option<TargetBackend>,
    opt_level: OptLevel,
    action: RunMode,
    debug_symbols: bool,
    use_std: bool,
    debug_export_build_plan: bool,
    enable_coverage: bool,
    output_wat: bool,
    /// Whether to output JSON when compiling with moonc.
    /// Set separately because we don't have the same
    pub moonc_output_json: bool,
    target_dir: PathBuf,
    /// Whether to execute `moondoc` in serve mode, which outputs HTML
    pub docs_serve: bool,
    pub deny_warn: bool,
    /// Whether to not emit alias when running `mooninfo`
    pub info_no_alias: bool,
    warn_list: Option<String>,
    alert_list: Option<String>,
}

impl CompilePreConfig {
    fn into_compile_config(
        self,
        preferred_backend: Option<TargetBackend>,
        is_core: bool,
    ) -> CompileConfig {
        let std = self.use_std && !is_core;
        let target_backend = self
            .target_backend
            .or(preferred_backend)
            .unwrap_or_default();

        CompileConfig {
            target_dir: self.target_dir,
            target_backend,
            opt_level: self.opt_level,
            action: self.action,
            debug_symbols: self.debug_symbols,
            stdlib_path: if std {
                Some(moonutil::moon_dir::core())
            } else {
                None
            },
            enable_coverage: self.enable_coverage,
            output_wat: self.output_wat,
            debug_export_build_plan: self.debug_export_build_plan,
            moonc_output_json: self.moonc_output_json,
            docs_serve: self.docs_serve,
            deny_warn: self.deny_warn,
            warn_list: self.warn_list,
            alert_list: self.alert_list,
            info_no_alias: self.info_no_alias,
        }
    }
}

/// Read in the commandline flags and build flags to create a
/// [`CompilePreConfig`] for compilation usage.
#[instrument(level = Level::DEBUG, skip_all)]
pub fn preconfig_compile(
    auto_sync_flags: &AutoSyncFlags,
    cli: &UniversalFlags,
    build_flags: &BuildFlags,
    target_dir: &Path,
    default_opt_level: OptLevel,
    action: RunMode,
) -> CompilePreConfig {
    let opt_level = if build_flags.release {
        OptLevel::Release
    } else if build_flags.debug {
        OptLevel::Debug
    } else {
        default_opt_level
    };
    CompilePreConfig {
        frozen: auto_sync_flags.frozen,
        target_dir: target_dir.to_owned(),
        target_backend: build_flags.target_backend,
        opt_level,
        action,
        debug_symbols: !build_flags.strip(),
        use_std: build_flags.std(),
        enable_coverage: build_flags.enable_coverage,
        output_wat: build_flags.output_wat,
        debug_export_build_plan: cli.unstable_feature.rr_export_build_plan,
        // In legacy impl, dry run always force no json
        moonc_output_json: !build_flags.no_render && !cli.dry_run,
        docs_serve: false,
        info_no_alias: false,
        deny_warn: build_flags.deny_warn,
        warn_list: build_flags.warn_list.clone(),
        alert_list: build_flags.alert_list.clone(),
    }
}

/// Plan the build process without executing it.
///
/// This function performs all the preparation steps: resolve dependencies,
/// calculate user intent, and create the build graph, but does not execute
/// the actual build tasks.
///
/// Returns the execution plan (metadata) and build graph separately, allowing
/// execute_build to take ownership of just the graph while callers retain
/// access to the metadata.
#[instrument(skip_all)]
pub fn plan_build<'a>(
    preconfig: CompilePreConfig,
    unstable_features: &'a FeatureGate,
    source_dir: &'a Path,
    target_dir: &'a Path,
    calc_user_intent: Box<CalcUserIntentFn<'a>>,
) -> anyhow::Result<(BuildMeta, n2::graph::Graph)> {
    let cfg = ResolveConfig::new_with_load_defaults(preconfig.frozen);
    let resolve_output = moonbuild_rupes_recta::resolve(&cfg, source_dir)?;

    // A couple of debug things:
    if unstable_features.rr_export_module_graph {
        moonbuild_rupes_recta::util::print_resolved_env_dot(
            &resolve_output.module_rel,
            &mut std::fs::File::create(target_dir.join("module_graph.dot"))?,
        )?;
    }
    if unstable_features.rr_export_package_graph {
        moonbuild_rupes_recta::util::print_dep_relationship_dot(
            &resolve_output.pkg_rel,
            &resolve_output.pkg_dirs,
            &mut std::fs::File::create(target_dir.join("package_graph.dot"))?,
        )?;
    }

    assert_eq!(
        resolve_output.local_modules().len(),
        1,
        "There should be exactly one main local module, got {:?}",
        resolve_output.local_modules()
    );
    let main_module_id = resolve_output.local_modules()[0];
    let main_module = resolve_output.module_rel.module_info(main_module_id);

    // Preferred backend
    let preferred_backend = main_module.preferred_target;

    let intent = calc_user_intent(&resolve_output, &[main_module_id])?;

    // std or no-std?
    // Ultimately we want to determine this from config instead of special cases.
    let is_core = main_module.name == MOONBITLANG_CORE;

    let cx = preconfig.into_compile_config(preferred_backend, is_core);
    let compile_output =
        moonbuild_rupes_recta::compile(&cx, &resolve_output, &intent.intent, &intent.directive)?;

    if unstable_features.rr_export_build_plan {
        if let Some(plan) = compile_output.build_plan {
            moonbuild_rupes_recta::util::print_build_plan_dot(
                &plan,
                &resolve_output.module_rel,
                &resolve_output.pkg_dirs,
                &mut std::fs::File::create(target_dir.join("build_plan.dot"))?,
            )?;
        }
    }

    let build_meta = BuildMeta {
        resolve_output,
        artifacts: compile_output.artifacts,
        target_backend: cx.target_backend,
        opt_level: cx.opt_level,
    };

    Ok((build_meta, compile_output.build_graph))
}

/// Generate metadata file `packages.json` in the target directory.
#[instrument(level = Level::DEBUG, skip_all)]
pub fn generate_metadata(
    source_dir: &Path,
    target_dir: &Path,
    build_meta: &BuildMeta,
) -> anyhow::Result<()> {
    let metadata_file = target_dir.join("packages.json");
    let metadata = moonbuild_rupes_recta::metadata::gen_metadata_json(
        &build_meta.resolve_output,
        source_dir,
        target_dir,
        build_meta.opt_level,
        build_meta.target_backend,
    );
    std::fs::write(
        &metadata_file,
        serde_json::to_string_pretty(&metadata).context("Failed to serialize metadata")?,
    )
    .context("Failed to write build metadata")?;
    Ok(())
}

pub struct BuildConfig {
    /// The level of parallelism to use. If `None`, will use the number of
    /// available CPU cores.
    parallelism: Option<usize>,
    /// Skip rendering compiler diagnostics to console
    pub no_render: bool,
    /// Render no-location diagnostics above this level
    render_no_loc: DiagnosticLevel,

    /// Generate metadata file `packages.json`
    pub generate_metadata: bool,

    /// Ask n2 to explain rerun reasons
    pub explain: bool,
}

impl BuildConfig {
    pub fn from_flags(flags: &BuildFlags, unstable_features: &FeatureGate) -> Self {
        BuildConfig {
            parallelism: flags.jobs,
            no_render: flags.no_render,
            render_no_loc: flags.render_no_loc,
            generate_metadata: false,
            explain: unstable_features.rr_n2_explain,
        }
    }
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            parallelism: None,
            no_render: false,
            render_no_loc: DiagnosticLevel::Error,
            generate_metadata: false,
            explain: false,
        }
    }
}

/// Execute a build plan.
///
/// Takes ownership of the build graph and executes the actual build tasks.
/// Returns just the build result - callers should use the resolve data and
/// artifacts from the planning phase for any metadata they need.
#[instrument(skip_all)]
pub fn execute_build(
    cfg: &BuildConfig,
    build_graph: n2::graph::Graph,
    target_dir: &Path,
) -> anyhow::Result<N2RunStats> {
    execute_build_partial(
        cfg,
        build_graph,
        target_dir,
        Box::new(|work| work.want_every_file(None)),
    )
}

/// Callback on the [`n2::work::Work`] to be done for target artifacts.
type WantFileFn<'b> = dyn for<'a> FnOnce(&'a mut n2::work::Work) -> anyhow::Result<()> + 'b;

/// Partially execute a build graph, same as [`execute_build`] otherwise.
///
/// Pass `want_files` callback to determine which artifacts to build.
///
/// This function is primarily used for rebuilding tests after snapshot test
/// promotion.
#[instrument(skip_all)]
pub fn execute_build_partial(
    cfg: &BuildConfig,
    mut build_graph: n2::graph::Graph,
    target_dir: &Path,
    want_files: Box<WantFileFn>,
) -> anyhow::Result<N2RunStats> {
    // Generate n2 state
    // FIXME: This is extremely verbose and barebones, only for testing purpose
    let mut hashes = n2::graph::Hashes::default();
    let n2_db = n2::db::open(
        &target_dir.join("moon.rupes-recta.db"),
        &mut build_graph,
        &mut hashes,
    )?;

    let parallelism = cfg
        .parallelism
        .or_else(|| std::thread::available_parallelism().ok().map(|x| x.into()))
        .unwrap();

    // FIXME: Rewrite the rendering mechanism
    let result_catcher = Arc::new(Mutex::new(ResultCatcher::default()));
    let callback = render_and_catch_callback(
        Arc::clone(&result_catcher),
        cfg.no_render,
        n2::terminal::use_fancy(),
        None,
        false,
        cfg.render_no_loc,
        PathBuf::new(),
        target_dir.into(),
    );
    let mut prog_console = create_progress_console(Some(Box::new(callback)), false);
    let mut work = n2::work::Work::new(
        build_graph,
        hashes,
        n2_db,
        &n2::work::Options {
            failures_left: Some(1),
            parallelism,
            explain: cfg.explain,
            adopt: false,
            dirty_on_output: true,
        },
        &mut *prog_console,
        n2::smallmap::SmallMap::default(),
    );
    want_files(&mut work).context("Failed to determine the files to be built")?;

    // The actual execution done by the n2 executor
    let res = work.run()?;

    let result_catcher = result_catcher.lock().unwrap();
    let stats = N2RunStats {
        n_tasks_executed: res,
        n_errors: result_catcher.n_errors,
        n_warnings: result_catcher.n_warnings,
    };

    Ok(stats)
}
