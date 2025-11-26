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
    N2RunStats, ResultCatcher, create_progress_console, render_and_catch_callback,
};
use moonbuild_rupes_recta::{
    CompileConfig, ResolveConfig, ResolveOutput,
    build_lower::{WarningCondition, artifact::n2_db_path},
    build_plan::InputDirective,
    fmt::{FmtConfig, FmtResolveOutput},
    intent::UserIntent,
    model::{Artifacts, BuildPlanNode, PackageId, RunBackend, TargetKind},
    prebuild::run_prebuild_config,
};
use moonutil::{
    cli::UniversalFlags,
    common::{
        BLACKBOX_TEST_PATCH, DiagnosticLevel, MOONBITLANG_CORE, RunMode, TargetBackend,
        WHITEBOX_TEST_PATCH,
    },
    compiler_flags::CC,
    cond_expr::OptLevel,
    features::FeatureGate,
    mooncakes::sync::AutoSyncFlags,
};
use tracing::{Level, info, instrument, warn};

use crate::cli::BuildFlags;

mod dry_run;
pub use dry_run::{dry_print_command, print_dry_run, print_dry_run_all};

/// The function that calculates the user intent for the build process.
///
/// Params:
/// - The output of the resolve step. All modules and packages that this module
///   are available in this value.
/// - The target backend to build for.
///
/// Returns: A vector of [`UserIntent`]s, representing what the user would like
/// to do
pub type CalcUserIntentFn<'b> = dyn for<'a> FnOnce(
        &'a ResolveOutput,
        moonutil::common::TargetBackend,
    ) -> anyhow::Result<CalcUserIntentOutput>
    + 'b;

/// The output of a calculate user intent operation.
pub struct CalcUserIntentOutput {
    /// The list of user intents; will be expanded to concrete BuildPlanNode(s) later.
    pub intents: Vec<UserIntent>,
    /// The input directive that the user wants to apply to the packages
    pub directive: InputDirective,
}

impl CalcUserIntentOutput {
    pub fn new(intents: Vec<UserIntent>, directive: InputDirective) -> Self {
        Self { intents, directive }
    }
}

impl From<Vec<UserIntent>> for CalcUserIntentOutput {
    fn from(intents: Vec<UserIntent>) -> Self {
        Self {
            intents,
            directive: InputDirective::default(),
        }
    }
}

impl From<(Vec<UserIntent>, InputDirective)> for CalcUserIntentOutput {
    fn from((intents, directive): (Vec<UserIntent>, InputDirective)) -> Self {
        Self { intents, directive }
    }
}

/// Convenient function to build a directive based on input kind
pub fn build_patch_directive_for_package(
    pkg: PackageId,
    no_mi: bool,
    value_tracing: Option<PackageId>,
    patch_file: Option<&Path>,
    test_mode: bool,
) -> anyhow::Result<InputDirective> {
    let patch_directive = if let Some(path) = patch_file {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("patch file path is not valid utf-8"))?;
        let kind = if path_str.ends_with(WHITEBOX_TEST_PATCH) {
            TargetKind::WhiteboxTest
        } else if path_str.ends_with(BLACKBOX_TEST_PATCH) {
            TargetKind::BlackboxTest
        } else if test_mode {
            // In tests the patches are applied to tests only
            TargetKind::InlineTest
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
        value_tracing,
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
    pub target_backend: RunBackend,

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
        if self.successful() { 0 } else { 1 }
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
#[derive(Debug)]
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
    moonc_output_json: bool,
    target_dir: PathBuf,
    /// Whether to execute `moondoc` in serve mode, which outputs HTML
    pub docs_serve: bool,
    pub warning_condition: WarningCondition,
    /// Whether to not emit alias when running `mooninfo`
    pub info_no_alias: bool,
    /// Attempt to use `tcc -run` when possible
    pub try_tcc_run: bool,
    warn_list: Option<String>,
    alert_list: Option<String>,
}

impl CompilePreConfig {
    fn into_compile_config(
        self,
        final_target_backend: TargetBackend,
        is_core: bool,
        resolve_output: &ResolveOutput,
        input_nodes: &[BuildPlanNode],
    ) -> CompileConfig {
        info!("Determining compilation configuration");

        let std = self.use_std && !is_core;
        info!(
            "std: self.use_std = {}, is_core = {} => std = {}",
            self.use_std, is_core, std
        );

        let target_backend = final_target_backend;
        info!(
            "Target backend: explicit = {:?} => selected = {:?}",
            self.target_backend, target_backend
        );
        assert!(
            self.target_backend.is_none_or(|x| x == target_backend),
            "The final selected target backend must either be default or match the explicit one"
        );

        let tcc_available = check_tcc_availability(target_backend, resolve_output, input_nodes);
        info!("`tcc -run` availability: {}", tcc_available);

        let target_backend = match target_backend {
            TargetBackend::Wasm => RunBackend::Wasm,
            TargetBackend::WasmGC => RunBackend::WasmGC,
            TargetBackend::Js => RunBackend::Js,
            TargetBackend::Native => {
                if self.try_tcc_run && tcc_available && self.opt_level == OptLevel::Debug {
                    RunBackend::NativeTccRun
                } else {
                    RunBackend::Native
                }
            }
            TargetBackend::LLVM => RunBackend::Llvm,
        };
        info!("Final run backend: {:?}", target_backend);

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
            warning_condition: self.warning_condition,
            warn_list: self.warn_list,
            alert_list: self.alert_list,
            info_no_alias: self.info_no_alias,
            default_cc: CC::default(), // TODO: determine how CC will be set
        }
    }
}

/// Read in the commandline flags and build flags to create a
/// [`CompilePreConfig`] for compilation usage.
///
/// - `auto_sync_flags`: The flags to control module download & sync behavior.
/// - `cli`: The universal CLI flags.
/// - `build_flags`: The build-specific flags.
/// - `target_dir`: The target directory for the build.
/// - `default_opt_level`: The default optimization level to use if not specified.
/// - `default_cc`: The default C/C++ toolchain to use, when not overridden by optimization level.
///   This field is used to force using TCC by default for some release builds. When `None`,
///   TCC will be used in debug builds, and system default toolchain will be used otherwise.
/// - `action`: The run mode (build, test, bench, etc.), only affects target directory layout.
///   This is different from the legacy code where action also affects the actual compilation
///   behavior.
#[instrument(level = Level::DEBUG, skip_all)]
pub fn preconfig_compile(
    auto_sync_flags: &AutoSyncFlags,
    cli: &UniversalFlags,
    build_flags: &BuildFlags,
    target_dir: &Path,
    default_opt_level: OptLevel,
    action: RunMode,
) -> CompilePreConfig {
    let opt_level = if build_flags.debug {
        OptLevel::Debug
    } else if build_flags.release {
        OptLevel::Release
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
        moonc_output_json: !cli.dry_run && build_flags.output_style().needs_moonc_json(),
        docs_serve: false,
        info_no_alias: false,
        try_tcc_run: false,
        warning_condition: if build_flags.deny_warn {
            WarningCondition::Deny
        } else {
            WarningCondition::Default
        },
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
) -> anyhow::Result<(BuildMeta, BuildInput)> {
    info!("Starting build planning");

    let cfg = ResolveConfig::new_with_load_defaults(preconfig.frozen, !preconfig.use_std);
    let resolve_output = moonbuild_rupes_recta::resolve(&cfg, source_dir)?;

    info!("Resolve completed");

    plan_build_from_resolved(
        preconfig,
        unstable_features,
        target_dir,
        calc_user_intent,
        resolve_output,
    )
}

/// Plan the build process from an already resolved environment.
///
/// This function exists because [someone demands target determination **after**
/// resolving completes](crate::cli::tool::build_binary_dep). For most cases,
/// use [`plan_build`] instead.
pub fn plan_build_from_resolved<'a>(
    preconfig: CompilePreConfig,
    unstable_features: &'a FeatureGate,
    target_dir: &'a Path,
    calc_user_intent: Box<CalcUserIntentFn<'a>>,
    resolve_output: ResolveOutput,
) -> anyhow::Result<(BuildMeta, BuildInput)> {
    // A couple of debug things:
    if unstable_features.rr_export_module_graph {
        info!("Exporting module graph DOT file");
        moonbuild_rupes_recta::util::print_resolved_env_dot(
            &resolve_output.module_rel,
            &mut std::fs::File::create(target_dir.join("module_graph.dot"))?,
        )?;
    }
    if unstable_features.rr_export_package_graph {
        info!("Exporting package graph DOT file");
        moonbuild_rupes_recta::util::print_dep_relationship_dot(
            &resolve_output.pkg_rel,
            &resolve_output.pkg_dirs,
            &mut std::fs::File::create(target_dir.join("package_graph.dot"))?,
        )?;
    }

    info!("Checking main module and backend");
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
    info!("Preferred backend: {:?}", preferred_backend);

    let target_backend = preconfig
        .target_backend
        .or(preferred_backend)
        .unwrap_or_default();

    info!("Calculating user intent");
    let intent = calc_user_intent(&resolve_output, target_backend)?;
    info!("User intent calculated: {:?}", intent.intents);

    // std or no-std?
    // Ultimately we want to determine this from config instead of special cases.
    let is_core = main_module.name == MOONBITLANG_CORE;
    info!("is_core: {}", is_core);

    // Run prebuild config if any
    info!("Running prebuild configuration");
    let prebuild_config = run_prebuild_config(&resolve_output)?;

    // Expand user intents to concrete BuildPlanNode inputs
    info!("Expanding user intents to build plan nodes");
    let mut input_nodes: Vec<BuildPlanNode> = Vec::new();
    for i in &intent.intents {
        i.append_nodes(&resolve_output, &mut input_nodes, &intent.directive);
    }

    let cx = preconfig.into_compile_config(target_backend, is_core, &resolve_output, &input_nodes);
    info!("Begin lowering to build graph");
    let compile_output = moonbuild_rupes_recta::compile(
        &cx,
        &resolve_output,
        &input_nodes,
        &intent.directive,
        Some(&prebuild_config),
    )?;

    if unstable_features.rr_export_build_plan
        && let Some(plan) = compile_output.build_plan
    {
        info!("Exporting build plan DOT file");
        moonbuild_rupes_recta::util::print_build_plan_dot(
            &plan,
            &resolve_output.module_rel,
            &resolve_output.pkg_dirs,
            &mut std::fs::File::create(target_dir.join("build_plan.dot"))?,
        )?;
    }

    let build_meta = BuildMeta {
        resolve_output,
        artifacts: compile_output.artifacts,
        target_backend: cx.target_backend,
        opt_level: cx.opt_level,
    };

    let db_path = n2_db_path(
        target_dir,
        cx.target_backend.into(),
        cx.opt_level,
        cx.action,
    );
    let input = BuildInput {
        graph: compile_output.build_graph,
        db_path,
    };

    info!("Build planning completed successfully");

    Ok((build_meta, input))
}

pub fn plan_fmt(
    resolved: &FmtResolveOutput,
    cfg: &FmtConfig,
    target_dir: &Path,
) -> anyhow::Result<BuildInput> {
    let graph = moonbuild_rupes_recta::fmt::build_graph_for_fmt(resolved, cfg, target_dir)?;
    let db_path = n2_db_path(
        target_dir,
        TargetBackend::default(),
        OptLevel::Debug,
        RunMode::Format,
    );
    let input = BuildInput { graph, db_path };
    Ok(input)
}

/// Check if we can actually run `tcc -run`.
///
/// This is for usage in `moon run` and `moon test`. Based on the legacy impl,
/// only if no packages override their C/C++ toolchain, we can use `tcc -run`.
fn check_tcc_availability(
    target_backend: TargetBackend,
    resolve_output: &ResolveOutput,
    input_nodes: &[BuildPlanNode],
) -> bool {
    // Only for native target. Yes, not even LLVM.
    if target_backend != TargetBackend::Native {
        info!("Disabling `tcc -run`: Only available for native target backend");
        return false;
    }

    // Check platform availability
    if !(cfg!(target_os = "linux") || cfg!(target_os = "macos")) {
        info!("`tcc -run` is only supported on Linux and macOS");
        return false;
    }

    // Check if TCC is available
    let _tcc = match CC::internal_tcc() {
        Ok(t) => t,
        Err(_) => {
            warn!("Cannot find TCC compiler in the system; disabling `tcc -run`");
            return false;
        }
    };

    // Check if any package overrides the C/C++ toolchain
    for node in input_nodes {
        if let BuildPlanNode::MakeExecutable(build_target) = node {
            let package = resolve_output.pkg_dirs.get_package(build_target.package);
            // Check native config
            let Some(native) = package.raw.link.as_ref().and_then(|x| x.native.as_ref()) else {
                continue;
            };
            if native.cc.is_some() {
                warn!(
                    "Package '{}' overrides C/C++ toolchain, `tcc -run` will be disabled",
                    package.fqn
                );
                return false;
            }
            if native.cc_flags.is_some() {
                warn!(
                    "Package '{}' overrides C/C++ compiler flags, `tcc -run` will be disabled",
                    package.fqn
                );
                return false;
            }
            if native.cc_link_flags.is_some() {
                warn!(
                    "Package '{}' overrides C/C++ linker flags, `tcc -run` will be disabled",
                    package.fqn
                );
                return false;
            }
        }
    }

    true
}

/// Generate metadata file `packages.json` in the target directory.
///
/// To ensure the correct paths are generated, `mode` should match your
/// corresponding `preconfig` used in [`plan_build`].
#[instrument(level = Level::DEBUG, skip_all)]
pub fn generate_metadata(
    source_dir: &Path,
    target_dir: &Path,
    build_meta: &BuildMeta,
    mode: RunMode,
) -> anyhow::Result<()> {
    let metadata_file = target_dir.join("packages.json");
    let metadata = moonbuild_rupes_recta::metadata::gen_metadata_json(
        &build_meta.resolve_output,
        source_dir,
        target_dir,
        build_meta.opt_level,
        build_meta.target_backend.into(),
        mode,
    );
    let orig_meta = std::fs::read_to_string(&metadata_file);
    let meta = serde_json::to_string_pretty(&metadata).context("Failed to serialize metadata")?;

    // Only overwrite if changed
    if !orig_meta.is_ok_and(|o| o == meta) {
        std::fs::write(&metadata_file, meta).context("Failed to write build metadata")?;
    }
    Ok(())
}

pub struct BuildConfig {
    /// The level of parallelism to use. If `None`, will use the number of
    /// available CPU cores.
    parallelism: Option<usize>,
    /// Skip rendering compiler diagnostics to console
    no_render: bool,
    /// Render no-location diagnostics above this level
    render_no_loc: DiagnosticLevel,

    /// Generate metadata file `packages.json`
    pub generate_metadata: bool,

    /// Explain and warnings in diagnostics
    pub explain_errors: bool,

    /// Ask n2 to explain rerun reasons
    pub n2_explain: bool,

    /// The patch file to use
    pub patch_file: Option<PathBuf>,
}

impl BuildConfig {
    pub fn from_flags(flags: &BuildFlags, unstable_features: &FeatureGate) -> Self {
        BuildConfig {
            parallelism: flags.jobs,
            no_render: flags.output_style().needs_no_render(),
            render_no_loc: flags.render_no_loc,
            generate_metadata: false,
            explain_errors: false,
            n2_explain: unstable_features.rr_n2_explain,
            patch_file: None,
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
            explain_errors: false,
            n2_explain: false,
            patch_file: None,
        }
    }
}

/// The input to a build execution.
#[derive(Debug, Clone)]
pub struct BuildInput {
    /// The build graph to execute
    graph: n2::graph::Graph,

    /// The build cache database path for n2
    ///
    /// This path is passed here because it changes between different execution configurations.
    db_path: PathBuf,
}

/// Execute a build plan.
///
/// Takes ownership of the build graph and executes the actual build tasks.
/// Returns just the build result - callers should use the resolve data and
/// artifacts from the planning phase for any metadata they need.
#[instrument(skip_all)]
pub fn execute_build(
    cfg: &BuildConfig,
    input: BuildInput,
    target_dir: &Path,
) -> anyhow::Result<N2RunStats> {
    // Get start nodes (leaf outputs) before moving the graph
    let start_nodes = input.graph.get_start_nodes();

    execute_build_partial(
        cfg,
        input,
        target_dir,
        Box::new(|work| {
            // Want only the leaf output files, not all files including stdlib
            for file_id in start_nodes {
                work.want_file(file_id)?;
            }
            Ok(())
        }),
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
    input: BuildInput,
    target_dir: &Path,
    want_files: Box<WantFileFn>,
) -> anyhow::Result<N2RunStats> {
    // Ensure target directory exists
    std::fs::create_dir_all(target_dir).context(format!(
        "Failed to create target directory: '{}'",
        target_dir.display()
    ))?;

    let mut build_graph = input.graph;
    let db_path = input.db_path;
    db_path
        .parent()
        .map(std::fs::create_dir_all)
        .transpose()
        .context("Failed to create parent for build cache DB")?;

    // Generate n2 state
    // FIXME: This is extremely verbose and barebones, only for testing purpose

    let mut hashes = n2::graph::Hashes::default();
    let n2_db = n2::db::open(&db_path, &mut build_graph, &mut hashes)?;

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
        cfg.patch_file.clone(),
        cfg.explain_errors,
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
            failures_left: Some(10), // FIXME: This value is to match legacy, but might TBD
            parallelism,
            explain: cfg.n2_explain,
            adopt: false,
            dirty_on_output: true,
        },
        &mut *prog_console,
        n2::smallmap::SmallMap::default(),
    );
    want_files(&mut work).context("Failed to determine the files to be built")?;

    // The actual execution done by the n2 executor
    let res = work.run().context("Failed to run n2 graph")?;

    let result_catcher = result_catcher.lock().unwrap();
    let stats = N2RunStats {
        n_tasks_executed: res,
        n_errors: result_catcher.n_errors,
        n_warnings: result_catcher.n_warnings,
    };

    Ok(stats)
}
