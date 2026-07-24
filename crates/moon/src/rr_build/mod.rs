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
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Context;
use clap::ValueEnum;
use indexmap::IndexMap;
use moonbuild::entry::{N2RunStats, ResultCatcher, create_progress_console};
use moonbuild_rupes_recta::{
    CompileConfig, ResolveConfig, ResolveOutput,
    build_lower::{LoweringEnvironment, WarningCondition},
    build_plan::InputDirective,
    fmt::{FmtConfig, FmtResolveOutput},
    intent::UserIntent,
    model::{
        Artifacts, BuildPlanNode, DirectNativeMode, NativeBackendMode, NativeTarget, PackageId,
        RunBackend, TargetKind, TccRunConfig,
    },
    prebuild::{PrebuildEnvironment, run_prebuild_config},
    target_layout::{ArtifactPathResolver, GENERATED_TEST_DRIVER_PREFIX, TargetLayout},
};
use moonutil::{
    build_options::RunMode,
    cli_support::AutoSyncFlags,
    cli_support::UniversalFlags,
    command_output::CommandOutput,
    compiler_flags::{self, CC},
    cond_expr::OptLevel as BuildProfile,
    constants::{BLACKBOX_TEST_PATCH, MOONBITLANG_CORE, WHITEBOX_TEST_PATCH},
    features::FeatureGate,
    package::SupportedTargetsDeclKind,
    project::{ProjectManifest, WorkspaceEnv},
    render::MooncDiagnostic,
    target::TargetBackend,
    test_metadata::DiagnosticLevel,
    user_log::UserLog,
};
use tracing::{Level, info, instrument};

use crate::build_flags::{BuildFlags, OutputStyle};

mod dry_run;
pub use dry_run::{format_dry_run_command, write_dry_run, write_dry_run_all};

const FINISHED_STYLE: anstyle::Style = anstyle::AnsiColor::Green.on_default().bold();

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

fn warn_local_legacy_supported_targets(resolve_output: &ResolveOutput, user_log: &UserLog) {
    let mut warned = BTreeSet::new();
    for &module_id in resolve_output.local_modules() {
        if let Some(pkgs) = resolve_output.pkg_dirs.packages_for_module(module_id) {
            for &pkg_id in pkgs.values() {
                if !warned.insert(pkg_id) {
                    continue;
                }
                let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
                if pkg.supported_targets_decl == SupportedTargetsDeclKind::LegacyArray {
                    user_log.warn(format!(
                        "Package `{}` uses legacy array syntax for `supported_targets`; use expression syntax like `<backend>` instead",
                        pkg.fqn
                    ));
                }
            }
        }
    }
}

pub(crate) fn local_packages(
    resolve_output: &ResolveOutput,
) -> impl Iterator<Item = PackageId> + '_ {
    resolve_output
        .local_modules()
        .iter()
        .flat_map(|&module_id| {
            resolve_output
                .pkg_dirs
                .packages_for_module(module_id)
                .into_iter()
                .flat_map(|packages| packages.values().copied())
        })
}

fn local_modules_preferred_target(
    resolve_output: &ResolveOutput,
    user_log: &UserLog,
) -> Option<TargetBackend> {
    let preferred = resolve_output
        .local_modules()
        .iter()
        .filter_map(|&module_id| resolve_output.module_info(module_id).preferred_target)
        .collect::<BTreeSet<_>>();

    if preferred.len() > 1 {
        user_log.warn(
            "Multiple local modules specify different preferred targets; pass `--target` to choose one explicitly",
        );
        None
    } else {
        preferred.into_iter().next()
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
        prove_why3_config: None,
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
    /// Experimental direct object-code backend selected under native, if any.
    pub native_target: Option<NativeTarget>,
    /// Configuration for `tcc -run`, if this compile process selected it.
    pub tcc_run: Option<TccRunConfig>,

    /// The main optimization level used in this compile process
    pub opt_level: BuildProfile,

    /// Physical artifact path resolver selected for this build.
    pub artifact_paths: ArtifactPathResolver,
}

/// A preliminary configuration that does not require run-time information to
/// populate. Will be transformed into [`CompileConfig`] later in the pipeline.
///
/// This type might be subject to change.
#[derive(Debug)]
pub struct CompilePreConfig {
    frozen: bool,
    target_backend: Option<TargetBackend>,
    opt_level: BuildProfile,
    action: RunMode,
    debug_symbols: bool,
    use_std: bool,
    debug_export_build_plan: bool,
    wasi_link: bool,
    enable_coverage: bool,
    workspace_env: WorkspaceEnv,
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
}

impl CompilePreConfig {
    pub(crate) fn resolve_config(&self) -> ResolveConfig {
        ResolveConfig::new_with_load_defaults(
            self.frozen,
            !self.use_std,
            self.enable_coverage,
            self.workspace_env.clone(),
        )
    }

    fn into_compile_config(
        self,
        final_target_backend: TargetBackend,
        is_core: bool,
        resolve_output: &ResolveOutput,
        input_nodes: &[BuildPlanNode],
        user_log: &UserLog,
    ) -> anyhow::Result<CompileConfig> {
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

        let native_target = match (target_backend, self.opt_level) {
            (TargetBackend::Native, BuildProfile::Debug) => NativeTarget::from_env_for_host(),
            _ => None,
        };
        info!("New native target: {:?}", native_target);
        let (run_backend, native_mode) = match target_backend {
            TargetBackend::Wasm => (RunBackend::Wasm, NativeBackendMode::GeneratedC),
            TargetBackend::WasmGC => (RunBackend::WasmGC, NativeBackendMode::GeneratedC),
            TargetBackend::Js => (RunBackend::Js, NativeBackendMode::GeneratedC),
            TargetBackend::Native => {
                let native_mode = if let Some(native_target) = native_target {
                    info!("Disabling `tcc -run`: new native backend selected");
                    NativeBackendMode::DirectObject(self.direct_native_mode(native_target))
                } else if let Some(tcc_run) =
                    self.select_tcc_run_config(resolve_output, input_nodes, user_log)
                {
                    NativeBackendMode::TccRun(tcc_run)
                } else {
                    NativeBackendMode::GeneratedC
                };
                (RunBackend::Native, native_mode)
            }
            TargetBackend::LLVM => (RunBackend::Llvm, NativeBackendMode::GeneratedC),
        };
        info!(
            "Final run backend: {:?}, native mode: {:?}",
            run_backend, native_mode
        );
        let stdlib_path = if std {
            Some(moonutil::toolchain::core())
        } else {
            None
        };
        let target_layout = TargetLayout::from_resolve_output(
            self.target_dir.clone(),
            resolve_output,
            self.opt_level,
            self.action,
        );
        let artifact_paths = ArtifactPathResolver::new(target_layout, stdlib_path.clone());

        Ok(CompileConfig {
            target_dir: self.target_dir,
            target_backend: run_backend,
            native_mode,
            opt_level: self.opt_level,
            action: self.action,
            debug_symbols: self.debug_symbols,
            stdlib_path,
            artifact_paths,
            lowering_environment: LoweringEnvironment::default(),
            enable_coverage: self.enable_coverage,
            output_wat: self.output_wat,
            debug_export_build_plan: self.debug_export_build_plan,
            wasi_link: self.wasi_link,
            moonc_output_json: self.moonc_output_json,
            docs_serve: self.docs_serve,
            warning_condition: self.warning_condition,
            warn_list: self.warn_list,
            info_no_alias: self.info_no_alias,
        })
    }

    fn select_tcc_run_config(
        &self,
        resolve_output: &ResolveOutput,
        input_nodes: &[BuildPlanNode],
        user_log: &UserLog,
    ) -> Option<TccRunConfig> {
        if !self.try_tcc_run {
            info!("Disabling `tcc -run`: not requested");
            return None;
        }
        if self.opt_level != BuildProfile::Debug {
            info!("Disabling `tcc -run`: only available for debug builds");
            return None;
        }
        if !(cfg!(target_os = "linux") || cfg!(target_os = "macos")) {
            info!("Disabling `tcc -run`: only supported on Linux and macOS");
            return None;
        }

        let Some(internal_tcc) = check_tcc_run_availability(resolve_output, input_nodes, user_log)
        else {
            info!("`tcc -run` availability: false");
            return None;
        };

        info!("`tcc -run` availability: true");
        Some(TccRunConfig::new(internal_tcc))
    }

    fn direct_native_mode(&self, native_target: NativeTarget) -> DirectNativeMode {
        DirectNativeMode::Target(native_target)
    }
}

/// Read in the commandline flags and build flags to create a
/// [`CompilePreConfig`] for compilation usage.
///
/// - `auto_sync_flags`: The flags to control module download & sync behavior.
/// - `cli`: The universal CLI flags.
/// - `build_flags`: The build-specific flags.
/// - `selected_target_backend`: The backend selected for this invocation, if explicit.
/// - `target_dir`: The target directory for the build.
/// - `action`: The run mode (build, test, bench, etc.). This also affects the
///   default build profile (`moon build`/`run`/`test`/`fmt`/`check` default to
///   debug; `moon bench`/`bundle` default to release).
#[instrument(level = Level::DEBUG, skip_all)]
pub fn preconfig_compile(
    auto_sync_flags: &AutoSyncFlags,
    cli: &UniversalFlags,
    build_flags: &BuildFlags,
    selected_target_backend: Option<TargetBackend>,
    target_dir: &Path,
    action: RunMode,
) -> CompilePreConfig {
    let opt_level = build_flags.effective_profile(action);

    CompilePreConfig {
        frozen: auto_sync_flags.frozen,
        target_dir: target_dir.to_owned(),
        target_backend: selected_target_backend,
        opt_level,
        action,
        debug_symbols: build_flags.debug_symbols_for(action),
        use_std: build_flags.std(),
        enable_coverage: build_flags.enable_coverage,
        workspace_env: cli.workspace_env.clone(),
        output_wat: build_flags.output_wat,
        debug_export_build_plan: cli.unstable_feature.rr_export_build_plan,
        wasi_link: cli.unstable_feature.wasi_link
            && std::env::var("MOON_WASI_LINK").as_deref() != Ok("0"),
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
    }
}

pub(crate) struct ResolvedBuildPlanningContext {
    target_backend: TargetBackend,
    is_core: bool,
}

impl ResolvedBuildPlanningContext {
    pub(crate) fn target_backend(&self) -> TargetBackend {
        self.target_backend
    }
}

/// Prepare the resolved build context before command intent is calculated.
///
/// This step emits resolve-time diagnostics and determines the effective target
/// backend. Commands that already resolved raw CLI selectors can use the
/// returned backend to compute `CalcUserIntentOutput` outside the shared RR
/// planning pipeline.
#[instrument(level = Level::DEBUG, skip_all)]
pub(crate) fn prepare_resolved_build(
    preconfig: &CompilePreConfig,
    unstable_features: &FeatureGate,
    target_dir: &Path,
    user_log: &UserLog,
    resolve_output: &ResolveOutput,
) -> anyhow::Result<ResolvedBuildPlanningContext> {
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

    // Preferred backend
    info!("Checking local modules and backend");
    let main_module = match resolve_output.local_modules() {
        &[module_id] => Some(resolve_output.module_info(module_id)),
        _ => None,
    };
    let preferred_target = if preconfig.target_backend.is_some() {
        None
    } else {
        local_modules_preferred_target(resolve_output, user_log)
    };
    info!("Preferred backend: {:?}", preferred_target);

    let target_backend = preconfig
        .target_backend
        .or(preferred_target)
        .unwrap_or_default();

    // TODO: remove this once LLVM backend is well supported
    if target_backend == TargetBackend::LLVM {
        user_log.warn(
            "LLVM backend is experimental and only supported on nightly moonbit toolchain for now",
        );
    }
    warn_local_legacy_supported_targets(resolve_output, user_log);

    // std or no-std?
    // Ultimately we want to determine this from config instead of special cases.
    let is_core = main_module.is_some_and(|module| module.name == MOONBITLANG_CORE);
    info!("is_core: {}", is_core);

    Ok(ResolvedBuildPlanningContext {
        target_backend,
        is_core,
    })
}

/// Plan a build graph from an already resolved project and command intent.
///
/// At this boundary, command adapters have already resolved user selectors and
/// command-specific directives into `CalcUserIntentOutput`. RR consumes those
/// identities plus precomputed build-context paths from the command adapter.
#[instrument(level = Level::DEBUG, skip_all)]
pub(crate) fn plan_resolved_build_from_intent(
    preconfig: CompilePreConfig,
    unstable_features: &FeatureGate,
    user_log: &UserLog,
    planning_context: ResolvedBuildPlanningContext,
    intent: CalcUserIntentOutput,
    mooncake_bin_dir: &Path,
    resolve_output: ResolveOutput,
) -> anyhow::Result<(BuildMeta, BuildInput)> {
    let target_dir = preconfig.target_dir.clone();
    info!("User intent calculated: {:?}", intent.intents);

    let prebuild_config = if preconfig.action == RunMode::Check {
        info!("Skipping prebuild configuration for check run mode");
        None
    } else {
        info!("Running prebuild configuration");
        let prebuild_environment = PrebuildEnvironment::new(std::env::vars().collect());
        Some(run_prebuild_config(&resolve_output, &prebuild_environment)?)
    };

    // Expand user intents to concrete BuildPlanNode inputs
    info!("Expanding user intents to build plan nodes");
    let mut input_nodes: Vec<BuildPlanNode> = Vec::new();
    for i in &intent.intents {
        i.append_nodes(
            &resolve_output,
            &mut input_nodes,
            user_log,
            &intent.directive,
            planning_context.target_backend,
        );
    }
    let cx = preconfig.into_compile_config(
        planning_context.target_backend,
        planning_context.is_core,
        &resolve_output,
        &input_nodes,
        user_log,
    )?;
    info!("Begin lowering to build graph");
    let compile_output = moonbuild_rupes_recta::compile(
        &cx,
        mooncake_bin_dir,
        &resolve_output,
        &input_nodes,
        &intent.directive,
        prebuild_config.as_ref(),
        user_log,
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
        native_target: cx.native_mode.direct_target(),
        tcc_run: cx.native_mode.tcc_run().cloned(),
        opt_level: cx.opt_level,
        artifact_paths: cx.artifact_paths.clone(),
    };

    let db_path = cx
        .artifact_paths
        .target_layout()
        .n2_db_path(cx.target_backend.into());
    let input = BuildInput {
        graph: compile_output.build_graph,
        command_args_by_output: compile_output.command_args_by_output,
        db_path,
    };

    info!("Build planning completed successfully");

    Ok((build_meta, input))
}

pub fn plan_fmt(
    resolved: &FmtResolveOutput,
    cfg: &FmtConfig,
    target_dir: &Path,
    selected_packages: &[PackageId],
    project_manifest: &ProjectManifest,
    user_log: &UserLog,
) -> anyhow::Result<BuildInput> {
    let graph = moonbuild_rupes_recta::fmt::build_graph_for_fmt(
        resolved,
        cfg,
        target_dir,
        selected_packages,
        project_manifest,
        user_log,
    )?;
    let layout = TargetLayout::from_fmt_resolve_output(
        target_dir.to_path_buf(),
        resolved,
        BuildProfile::Debug,
    );
    let db_path = layout.n2_db_path(TargetBackend::default());
    Ok(BuildInput {
        graph,
        command_args_by_output: Default::default(),
        db_path,
    })
}

/// Check if we can actually run `tcc -run`.
///
/// This is for usage in `moon run` and `moon test`. Based on the legacy impl,
/// only if neither the user nor any package overrides the C/C++ toolchain, we
/// can use `tcc -run`.
fn check_tcc_run_availability(
    resolve_output: &ResolveOutput,
    input_nodes: &[BuildPlanNode],
    user_log: &UserLog,
) -> Option<CC> {
    if compiler_flags::has_cc_env_override() {
        info!("Disabling `tcc -run`: MOON_CC is set");
        return None;
    }

    // Check if any package overrides the C/C++ toolchain before probing TCC.
    for node in input_nodes {
        if let BuildPlanNode::MakeExecutable(build_target) = node {
            let package = resolve_output.pkg_dirs.get_package(build_target.package);
            // Check native config
            let Some(native) = package.raw.link.as_ref().and_then(|x| x.native.as_ref()) else {
                continue;
            };
            if native.cc.is_some() {
                user_log.warn(format!(
                    "Package '{}' overrides C/C++ toolchain, `tcc -run` will be disabled",
                    package.fqn
                ));
                return None;
            }
            if native.cc_flags.is_some() {
                user_log.warn(format!(
                    "Package '{}' overrides C/C++ compiler flags, `tcc -run` will be disabled",
                    package.fqn
                ));
                return None;
            }
            if native.cc_link_flags.is_some() {
                user_log.warn(format!(
                    "Package '{}' overrides C/C++ linker flags, `tcc -run` will be disabled",
                    package.fqn
                ));
                return None;
            }
        }
    }

    match CC::internal_tcc() {
        Ok(tcc) => Some(tcc),
        Err(_) => {
            user_log.warn("Cannot find TCC compiler in the system; disabling `tcc -run`");
            None
        }
    }
}

/// Generate metadata file `packages.json` in the target directory.
///
/// To ensure the correct paths are generated, `build_meta` should come from the
/// same configuration used in [`plan_build`].
///
/// If the caller is from a single-file build, `single_file_filename` should
/// be set to the filename (with extension) of the single file being built.
#[instrument(level = Level::DEBUG, skip_all)]
pub fn generate_metadata(
    source_dir: &Path,
    target_dir: &Path,
    build_meta: &BuildMeta,
    build_input: &BuildInput,
    single_file_filename: Option<&str>,
) -> anyhow::Result<()> {
    let metadata_file = if let Some(filename) = single_file_filename {
        target_dir.join(format!("{}.packages.json", filename))
    } else {
        target_dir.join("packages.json")
    };

    let check_commands = collect_check_commands_by_output(build_input);
    let metadata = moonbuild_rupes_recta::metadata::gen_metadata_json(
        &build_meta.resolve_output,
        source_dir,
        &build_meta.artifact_paths,
        build_meta.opt_level,
        build_meta.target_backend.into(),
        &check_commands,
    );
    let orig_meta = std::fs::read_to_string(&metadata_file);
    let meta = serde_json::to_string_pretty(&metadata).context("Failed to serialize metadata")?;

    // Only overwrite if changed
    if !orig_meta.is_ok_and(|o| o == meta) {
        std::fs::write(&metadata_file, meta).with_context(|| {
            format!(
                "Failed to write build metadata to {}",
                metadata_file.display()
            )
        })?;
    }
    Ok(())
}

fn collect_check_commands_by_output(
    build_input: &BuildInput,
) -> moonbuild_rupes_recta::metadata::CheckCommandMap {
    let mut commands = BTreeMap::new();
    for (output_path, args) in &build_input.command_args_by_output {
        let Some(command_args) = check_command_args_without_executable(args) else {
            continue;
        };
        commands.insert(output_path.clone(), command_args);
    }
    commands
}

fn check_command_args_without_executable(args: &[String]) -> Option<Vec<String>> {
    let (_executable, command_args) = args.split_first()?;
    command_args
        .first()
        .is_some_and(|arg| arg == "check")
        .then(|| command_args.to_vec())
}

pub fn generate_all_pkgs_json(build_meta: &BuildMeta) -> anyhow::Result<()> {
    let all_pkgs_path = build_meta
        .artifact_paths
        .target_layout()
        .all_pkgs_of_build_target(build_meta.target_backend.into());
    let all_pkgs_json = moonbuild_rupes_recta::all_pkgs::gen_all_pkgs_json(
        &build_meta.resolve_output,
        &build_meta.artifact_paths,
        build_meta.target_backend.into(),
    );
    let orig_all_pkgs = std::fs::read_to_string(&all_pkgs_path);
    let all_pkgs_str =
        serde_json::to_string_pretty(&all_pkgs_json).context("Failed to serialize metadata")?;

    // Only overwrite if changed
    if !orig_all_pkgs.is_ok_and(|o| o == all_pkgs_str) {
        // Ensure parent directory exists
        if let Some(parent) = all_pkgs_path.parent() {
            std::fs::create_dir_all(parent).context(format!(
                "Failed to create directory for all_pkgs at {}",
                parent.display()
            ))?;
        }
        std::fs::write(&all_pkgs_path, all_pkgs_str).context(format!(
            "Failed to write all_pkgs to the path {}",
            all_pkgs_path.display()
        ))?;
    }
    Ok(())
}

pub struct BuildConfig {
    /// The level of parallelism to use. If `None`, will use the number of
    /// available CPU cores.
    parallelism: Option<usize>,
    /// The output style for errors and warnings
    output_style: OutputStyle,
    /// Render no-location diagnostics above this level
    render_no_loc: DiagnosticLevel,
    /// Maximum number of diagnostics to display after deduplication.
    diagnostic_limit: Option<usize>,

    /// Generate metadata file `packages.json`
    pub generate_metadata: bool,

    /// Explain and warnings in diagnostics
    pub explain_errors: bool,

    /// Ask n2 to explain rerun reasons
    pub n2_explain: bool,

    /// Verbose output for build progress and command echo
    verbose: bool,
    suppress_progress: bool,

    /// The patch file to use
    pub patch_file: Option<PathBuf>,
}

impl BuildConfig {
    pub(crate) fn from_flags(
        flags: &BuildFlags,
        unstable_features: &FeatureGate,
        verbose: bool,
    ) -> Self {
        BuildConfig {
            parallelism: flags.jobs,
            output_style: flags.output_style(),
            render_no_loc: flags.render_no_loc,
            diagnostic_limit: flags.diagnostic_limit,
            generate_metadata: false,
            explain_errors: false,
            n2_explain: unstable_features.rr_n2_explain,
            verbose,
            suppress_progress: false,
            patch_file: None,
        }
    }

    pub(crate) fn with_suppressed_progress(mut self, suppress_progress: bool) -> Self {
        self.suppress_progress = suppress_progress;
        self
    }
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            parallelism: None,
            output_style: OutputStyle::Raw,
            render_no_loc: DiagnosticLevel::Error,
            diagnostic_limit: None,
            generate_metadata: false,
            explain_errors: false,
            n2_explain: false,
            verbose: false,
            suppress_progress: false,
            patch_file: None,
        }
    }
}

/// The input to a build execution.
#[derive(Debug, Clone)]
pub struct BuildInput {
    /// The build graph to execute
    graph: n2::graph::Graph,

    /// Structured command argv keyed by generated output path.
    command_args_by_output: moonbuild_rupes_recta::build_lower::CommandArgMap,

    /// The build cache database path for n2
    ///
    /// This path is passed here because it changes between different execution configurations.
    db_path: PathBuf,
}

#[cfg(test)]
impl BuildInput {
    pub(crate) fn graph_for_test(&self) -> &n2::graph::Graph {
        &self.graph
    }

    pub(crate) fn command_args_for_test(
        &self,
    ) -> &moonbuild_rupes_recta::build_lower::CommandArgMap {
        &self.command_args_by_output
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
    input: BuildInput,
    target_dir: &Path,
    user_log: &UserLog,
) -> anyhow::Result<N2RunStats> {
    // Get start nodes (leaf outputs) before moving the graph
    let start_nodes = input.graph.get_start_nodes();

    execute_build_partial(
        cfg,
        input,
        target_dir,
        None,
        user_log,
        Box::new(|work| {
            // Want only the leaf output files, not all files including stdlib
            for file_id in start_nodes {
                work.want_file(file_id)?;
            }
            Ok(())
        }),
    )
}

/// Execute a test build.
///
/// Test builds may report diagnostics for generated drivers using their
/// source-tree paths. The test build metadata lets the diagnostic processing
/// stage resolve those paths through the target layout.
pub fn execute_test_build(
    cfg: &BuildConfig,
    input: BuildInput,
    target_dir: &Path,
    build_meta: &BuildMeta,
    user_log: &UserLog,
) -> anyhow::Result<N2RunStats> {
    let start_nodes = input.graph.get_start_nodes();

    execute_build_partial(
        cfg,
        input,
        target_dir,
        Some(build_meta),
        user_log,
        Box::new(|work| {
            for file_id in start_nodes {
                work.want_file(file_id)?;
            }
            Ok(())
        }),
    )
}

#[derive(Debug, Clone, Copy)]
pub enum BuildOperation {
    Build,
    Bundle,
    Check,
    Format,
    GenerateMbti,
}

/// Report the user-facing result of a complete command build.
///
/// Compiler diagnostics are emitted independently during execution. This
/// function owns only Moon's durable command result and status messages.
pub fn report_build_result(
    result: &N2RunStats,
    operation: BuildOperation,
    cfg: &BuildConfig,
    output: &CommandOutput,
) -> anyhow::Result<()> {
    let Some(n_tasks) = result.n_tasks_executed else {
        if cfg.output_style != OutputStyle::Json {
            output.write_result(|writer| {
                writeln!(
                    writer,
                    "Failed with {} warnings, {} errors.",
                    result.n_warnings, result.n_errors
                )
            })?;
        }
        let operation = match operation {
            BuildOperation::Build => "building",
            BuildOperation::Bundle => "bundling",
            BuildOperation::Check => "checking",
            BuildOperation::Format => "formatting",
            BuildOperation::GenerateMbti => "generating mbti files",
        };
        output
            .user_log()
            .error(format!("failed when {operation} project"));
        return Ok(());
    };

    let warnings_errors = if result.n_warnings > 0 || result.n_errors > 0 {
        format!(
            " ({} warnings, {} errors)",
            result.n_warnings, result.n_errors
        )
    } else {
        String::new()
    };
    if n_tasks == 0 {
        output.user_log().info(format_args!(
            "{FINISHED_STYLE}Finished.{FINISHED_STYLE:#} moon: no work to do{warnings_errors}"
        ));
    } else {
        let task_plural = if n_tasks == 1 { "" } else { "s" };
        output.user_log().info(format_args!(
            "{FINISHED_STYLE}Finished.{FINISHED_STYLE:#} moon: ran {n_tasks} task{task_plural}, now up to date{warnings_errors}"
        ));
    }
    Ok(())
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
    build_meta: Option<&BuildMeta>,
    user_log: &UserLog,
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
        .with_context(|| {
            format!(
                "Failed to create parent for build cache DB at {}",
                db_path.display()
            )
        })?;

    // Generate n2 state

    let mut hashes = n2::graph::Hashes::default();
    let n2_db = n2::db::open(&db_path, &mut build_graph, &mut hashes)
        .with_context(|| format!("Failed to open build cache DB at {}", db_path.display()))?;

    let parallelism = cfg
        .parallelism
        .or_else(|| std::thread::available_parallelism().ok().map(|x| x.into()))
        .unwrap();

    let result_catcher = Arc::new(Mutex::new(ResultCatcher::default()));
    let mut prog_console: Box<dyn n2::progress::Progress> = create_progress_console(
        Some(Box::new(capture_diagnostics_callback(Arc::clone(
            &result_catcher,
        )))),
        cfg.verbose,
        cfg.suppress_progress,
    );
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
    let res = work.run().context("Failed to run n2 graph");
    drop(work);
    drop(prog_console); // Ensure the progress bar won't mess with diagnostic output
    let res = res?;
    let build_succeeded = res.is_some();

    let mut result_catcher = result_catcher.lock().unwrap();
    process_captured_diagnostics(
        &mut result_catcher,
        cfg,
        build_succeeded,
        build_meta,
        user_log,
    );
    let stats = N2RunStats {
        n_tasks_executed: res,
        n_errors: result_catcher.n_errors,
        n_warnings: result_catcher.n_warnings,
    };

    Ok(stats)
}

/// Capture compiler output from n2 so it can be processed after the build.
fn capture_diagnostics_callback(catcher: Arc<Mutex<ResultCatcher>>) -> impl Fn(&str) {
    move |output: &str| {
        let mut catcher = catcher.lock().unwrap();
        output
            .split('\n')
            .filter(|it| !it.is_empty())
            .for_each(|content| {
                catcher.append_content(content, None);
            });
    }
}

fn should_render_non_diagnostic_build_output(cfg: &BuildConfig, build_succeeded: bool) -> bool {
    !(cfg.suppress_progress && build_succeeded)
}

fn process_captured_diagnostics(
    catcher: &mut ResultCatcher,
    cfg: &BuildConfig,
    build_succeeded: bool,
    build_meta: Option<&BuildMeta>,
    user_log: &UserLog,
) {
    let captured = catcher.content_writer.iter().map(|content| {
        let Some(meta) = build_meta else {
            return content.to_owned();
        };
        let layout = meta.artifact_paths.target_layout();
        let packages = &meta.resolve_output.pkg_dirs;
        let backend = meta.target_backend.into();

        if cfg.output_style.needs_moonc_json() {
            let Ok(mut value) = serde_json::from_str::<serde_json::Value>(content) else {
                return content.to_owned();
            };
            let mut changed = false;
            let mut diagnostics = vec![&mut value];
            while let Some(diagnostic) = diagnostics.pop() {
                let Some(object) = diagnostic.as_object_mut() else {
                    continue;
                };
                if let Some(serde_json::Value::String(path)) = object.get_mut("path")
                    && let Some(physical) = layout.generated_test_driver_diagnostic_path(
                        packages,
                        Path::new(path),
                        backend,
                    )
                {
                    *path = physical.to_string_lossy().into_owned();
                    changed = true;
                }
                if let Some(serde_json::Value::Array(children)) = object.get_mut("children") {
                    diagnostics.extend(children.iter_mut());
                }
            }
            return if changed {
                serde_json::to_string(&value).expect("diagnostic JSON should serialize")
            } else {
                content.to_owned()
            };
        }

        let Some(prefix_start) = content.find(GENERATED_TEST_DRIVER_PREFIX) else {
            return content.to_owned();
        };
        let Some(extension_end) = content[prefix_start..].find(".mbt") else {
            return content.to_owned();
        };
        let path_end = prefix_start + extension_end + ".mbt".len();
        let Some(physical) = layout.generated_test_driver_diagnostic_path(
            packages,
            Path::new(&content[..path_end]),
            backend,
        ) else {
            return content.to_owned();
        };
        format!("{}{}", physical.display(), &content[path_end..])
    });

    match cfg.output_style {
        OutputStyle::Json => {
            let mut by_file = BTreeMap::<String, BTreeSet<(MooncDiagnostic, String)>>::new();
            for content in captured {
                match serde_json::from_str::<moonutil::render::MooncDiagnostic>(&content) {
                    Ok(d) => {
                        if diagnostic_is_generated_test_driver_warning(&d) {
                            continue;
                        }
                        let file_key = d.path.clone();
                        by_file.entry(file_key).or_default().insert((d, content));
                    }
                    Err(_) => {
                        // Non-diagnostics output, just print as-is
                        // This could happen for installing binaries dependencies etc.
                        if should_render_non_diagnostic_build_output(cfg, build_succeeded) {
                            eprintln!("{content}");
                        }
                    }
                };
            }

            // In JSON mode, just print raw content after dedup.
            match cfg.diagnostic_limit {
                None => {
                    for file_diagnostics in by_file.values() {
                        for (diag, content) in file_diagnostics {
                            println!("{content}");
                            catcher.append_diag(diag);
                        }
                    }
                }
                Some(limit) => {
                    let mut displayed = 0;
                    let mut hidden_errors = 0;
                    let mut total_warnings = 0;
                    let mut displayed_warnings = 0;
                    let mut non_errors = Vec::new();

                    for file_diagnostics in by_file.values() {
                        for (diag, content) in file_diagnostics {
                            if diagnostic_is_error(diag) {
                                if displayed < limit {
                                    println!("{content}");
                                    catcher.append_diag(diag);
                                    displayed += 1;
                                } else {
                                    hidden_errors += 1;
                                }
                                continue;
                            }

                            if diagnostic_is_warning(diag) {
                                total_warnings += 1;
                            }
                            if displayed < limit {
                                non_errors.push((diag, content));
                            }
                        }
                    }

                    if displayed < limit {
                        for (diag, content) in non_errors {
                            println!("{content}");
                            catcher.append_diag(diag);
                            displayed += 1;
                            if diagnostic_is_warning(diag) {
                                displayed_warnings += 1;
                            }
                            if displayed == limit {
                                break;
                            }
                        }
                    }

                    let hidden_warnings = total_warnings - displayed_warnings;
                    warn_limited_diagnostics(hidden_errors, hidden_warnings, user_log);
                    catcher.n_errors += hidden_errors;
                    catcher.n_warnings += hidden_warnings;
                }
            }
        }
        OutputStyle::Fancy => {
            let mut by_file = BTreeMap::<String, BTreeSet<MooncDiagnostic>>::new();
            for content in captured {
                match serde_json::from_str::<moonutil::render::MooncDiagnostic>(&content) {
                    Ok(d) => {
                        if diagnostic_is_generated_test_driver_warning(&d) {
                            continue;
                        }
                        by_file.entry(d.path.clone()).or_default().insert(d);
                    }
                    Err(_) => {
                        // Non-diagnostics output, just print as-is
                        // This could happen for installing binaries dependencies etc.
                        if should_render_non_diagnostic_build_output(cfg, build_succeeded) {
                            eprintln!("{content}");
                        }
                    }
                };
            }

            let patch_file = cfg.patch_file.as_ref();
            match cfg.diagnostic_limit {
                None => {
                    for file_diagnostics in by_file.values() {
                        for diag in file_diagnostics {
                            let kind = diag.render_diagnostics(
                                n2::terminal::use_fancy(),
                                patch_file,
                                cfg.explain_errors,
                                cfg.render_no_loc,
                            );
                            catcher.append_kind(kind);
                        }
                    }
                }
                Some(limit) => {
                    let build_config = cfg;
                    let mut displayed = 0;
                    let mut hidden_errors = 0;
                    let mut total_warnings = 0;
                    let mut displayed_warnings = 0;
                    let mut non_errors = Vec::new();

                    for file_diagnostics in by_file.values() {
                        for diag in file_diagnostics {
                            if !diagnostic_is_renderable(diag, build_config) {
                                continue;
                            }

                            if diagnostic_is_error(diag) {
                                if displayed < limit {
                                    let kind = diag.render_diagnostics(
                                        n2::terminal::use_fancy(),
                                        patch_file,
                                        build_config.explain_errors,
                                        build_config.render_no_loc,
                                    );
                                    catcher.append_kind(kind);
                                    displayed += 1;
                                } else {
                                    hidden_errors += 1;
                                }
                                continue;
                            }

                            if diagnostic_is_warning(diag) {
                                total_warnings += 1;
                            }
                            if displayed < limit {
                                non_errors.push(diag);
                            }
                        }
                    }

                    if displayed < limit {
                        for diag in non_errors {
                            let kind = diag.render_diagnostics(
                                n2::terminal::use_fancy(),
                                patch_file,
                                build_config.explain_errors,
                                build_config.render_no_loc,
                            );
                            catcher.append_kind(kind);
                            displayed += 1;
                            if diagnostic_is_warning(diag) {
                                displayed_warnings += 1;
                            }
                            if displayed == limit {
                                break;
                            }
                        }
                    }

                    let hidden_warnings = total_warnings - displayed_warnings;
                    warn_limited_diagnostics(hidden_errors, hidden_warnings, user_log);
                    catcher.n_errors += hidden_errors;
                    catcher.n_warnings += hidden_warnings;
                }
            }
        }
        OutputStyle::Raw => {
            for content in captured {
                println!("{content}");
            }
        }
    }
}

fn diagnostic_is_error(diag: &MooncDiagnostic) -> bool {
    diag.level == "error"
}

fn diagnostic_is_warning(diag: &MooncDiagnostic) -> bool {
    matches!(diag.level.as_str(), "warn" | "warning")
}

fn diagnostic_is_generated_test_driver_warning(diag: &MooncDiagnostic) -> bool {
    diagnostic_is_warning(diag) && diag.path.contains("__generated_driver_for_")
}

fn diagnostic_is_renderable(diag: &MooncDiagnostic, cfg: &BuildConfig) -> bool {
    if !diag.path.is_empty() {
        return true;
    }

    DiagnosticLevel::from_str(&diag.level, true).is_ok_and(|level| level >= cfg.render_no_loc)
}

fn warn_limited_diagnostics(hidden_errors: usize, hidden_warnings: usize, user_log: &UserLog) {
    if hidden_errors != 0 || hidden_warnings != 0 {
        user_log.warn(format!(
            "diagnostic output limited by --diagnostic-limit: {} errors and {} warnings were not displayed.",
            hidden_errors, hidden_warnings
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonutil::render::{Loc, Position};

    fn diagnostic(path: &str, level: &str) -> MooncDiagnostic {
        MooncDiagnostic {
            path: path.to_string(),
            loc: Loc {
                start: Position { line: 1, col: 1 },
                end: Position { line: 1, col: 2 },
            },
            level: level.to_string(),
            message: String::new(),
            error_code: 0,
            children: Vec::new(),
        }
    }

    #[test]
    fn suppresses_generated_test_driver_warnings_only() {
        assert!(diagnostic_is_generated_test_driver_warning(&diagnostic(
            "./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt",
            "warning"
        )));
        assert!(!diagnostic_is_generated_test_driver_warning(&diagnostic(
            "./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt",
            "error"
        )));
        assert!(!diagnostic_is_generated_test_driver_warning(&diagnostic(
            "./lib/hello.mbt",
            "warning"
        )));
    }

    #[test]
    fn generated_test_driver_errors_are_counted() {
        let generated_driver_path =
            "./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt";
        let warning = diagnostic(generated_driver_path, "warning");
        let error = diagnostic(generated_driver_path, "error");
        let mut catcher = ResultCatcher::default();
        catcher.append_content(serde_json::to_string(&warning).unwrap(), None);
        catcher.append_content(serde_json::to_string(&error).unwrap(), None);

        let cfg = BuildConfig {
            output_style: OutputStyle::Json,
            ..Default::default()
        };
        process_captured_diagnostics(
            &mut catcher,
            &cfg,
            false,
            None,
            &UserLog::new(log::LevelFilter::Warn),
        );

        assert_eq!(catcher.n_warnings, 0);
        assert_eq!(catcher.n_errors, 1);
    }
}
