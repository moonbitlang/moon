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
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
    sync::LazyLock,
};

use anyhow::Context;
use indexmap::IndexMap;
use moonbuild_rupes_recta::{
    CompileConfig, ResolveConfig, ResolveOutput,
    build_lower::WarningCondition,
    build_plan::{ArtifactKey, InputDirective},
    execution_plan::{ActionId, ExecutionPlan},
    fmt::{FmtConfig, FmtResolveOutput},
    intent::UserIntent,
    model::{
        BackendConfig, DebugInfoRequest, DebugSymbols, ENV_MOONBIT_NEW_NATIVE, NativeTarget,
        OperatingSystem, PackageId, TargetKind,
    },
    target_layout::{ArtifactPathResolver, TargetLayout},
};
use moonutil::{
    build_options::RunMode,
    cli_support::UniversalFlags,
    compiler_flags,
    cond_expr::OptLevel as BuildProfile,
    constants::{BLACKBOX_TEST_PATCH, MOONBITLANG_CORE, WHITEBOX_TEST_PATCH},
    package::SupportedTargetsDeclKind,
    project::{PackageDirs, ProjectManifest},
    target::TargetBackend,
    user_log::UserLog,
};
use tracing::{Level, info, instrument};

use crate::build_flags::BuildFlags;

pub mod action_identity;
mod dry_run;
mod execution;
mod prebuild;
#[cfg(test)]
pub(crate) use dry_run::write_build_graph;
pub(crate) use dry_run::{format_dry_run_command, write_dry_run};
pub(crate) use execution::{
    BuildConfig, JsonBuildOutput, execute_build, execute_build_json, execute_build_partial,
    execute_test_build,
};

/// Synchronize dependencies and return resolved project data.
/// This step does not acquire the target-directory lock.
pub(crate) fn sync_and_resolve_project(
    resolve_config: &ResolveConfig,
    dirs: &PackageDirs,
    user_log: &UserLog,
) -> anyhow::Result<ResolveOutput> {
    std::fs::create_dir_all(&dirs.target_dir).with_context(|| {
        format!(
            "Failed to create target directory: '{}'",
            dirs.target_dir.display()
        )
    })?;
    let synced_env = moonbuild_rupes_recta::sync_dependencies(resolve_config, dirs, user_log)?;
    let resolve_output =
        moonbuild_rupes_recta::resolve_synced_project(resolve_config, synced_env, user_log)?;
    Ok(resolve_output)
}

/// The output of a calculate user intent operation.
pub struct CalcUserIntentOutput {
    /// The list of user intents; will be expanded to requested artifacts later.
    pub intents: Vec<UserIntent>,
    /// The input directive that the user wants to apply to the packages
    pub directive: InputDirective,
}

impl CalcUserIntentOutput {
    pub fn new(intents: Vec<UserIntent>, directive: InputDirective) -> Self {
        Self { intents, directive }
    }

    fn requested_artifacts(
        &self,
        resolve_output: &ResolveOutput,
        user_log: &UserLog,
        target_backend: TargetBackend,
    ) -> Vec<ArtifactKey> {
        let mut artifacts = Vec::new();
        for intent in &self.intents {
            intent.append_artifacts(
                resolve_output,
                &mut artifacts,
                user_log,
                &self.directive,
                target_backend,
            );
        }
        artifacts
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
        ..Default::default()
    })
}

/// Build metadata containing information needed for build context and results.
/// The build graph is kept separate to allow execute_build to take ownership of it.
pub struct BuildMeta {
    /// The result of the resolve step, containing package metadata
    pub resolve_output: ResolveOutput,

    /// The list of artifacts that will be produced
    pub artifacts: IndexMap<ArtifactKey, Vec<PathBuf>>,

    /// The backend and backend-specific configuration used by this build.
    pub backend: BackendConfig,

    /// The main optimization level used in this compile process
    pub opt_level: BuildProfile,

    /// Physical artifact path resolver selected for this build.
    pub artifact_paths: ArtifactPathResolver,
}

impl BuildMeta {
    pub fn target_backend(&self) -> TargetBackend {
        self.backend.target_backend()
    }
}

/// Represents the result of the build process
#[derive(Debug, Clone)]
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

/// Select the backend and construct the final configuration for a resolved project.
///
/// CLI policy is resolved here while both the original flags and selected backend
/// are available. Command adapters may then use that backend to expand their
/// intent; RR receives one completed configuration with no CLI parsing types.
#[instrument(level = Level::DEBUG, skip_all)]
pub(crate) fn prepare_resolved_build(
    cli: &UniversalFlags,
    build_flags: &BuildFlags,
    selected_target_backend: Option<TargetBackend>,
    target_dir: &Path,
    action: RunMode,
    user_log: &UserLog,
    resolve_output: &ResolveOutput,
) -> anyhow::Result<CompileConfig> {
    // A couple of debug things:
    if cli.unstable_feature.rr_export_module_graph {
        info!("Exporting module graph DOT file");
        moonbuild_rupes_recta::util::print_resolved_env_dot(
            &resolve_output.module_rel,
            &mut std::fs::File::create(target_dir.join("module_graph.dot"))?,
        )?;
    }
    if cli.unstable_feature.rr_export_package_graph {
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
    let preferred_target = if selected_target_backend.is_some() {
        None
    } else {
        local_modules_preferred_target(resolve_output, user_log)
    };
    info!("Preferred backend: {:?}", preferred_target);

    let target_backend = selected_target_backend
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

    let opt_level = build_flags.effective_profile(action);
    let strip = build_flags.strip_for(action);
    let debug_info = DebugInfoRequest {
        symbols: if strip {
            DebugSymbols::None
        } else if action == RunMode::Run
            && target_backend == TargetBackend::Native
            && !build_flags.debug
            && !build_flags.no_strip
        {
            // Planning chooses how to retain source backtraces after selecting
            // generated C or direct object output for this Native run.
            DebugSymbols::Backtrace
        } else {
            DebugSymbols::Full
        },
        // Stripping symbols does not disable a debug-profile run's native
        // backtrace machinery. Other commands enable it with debug symbols.
        runtime_backtrace: target_backend.is_native()
            && (!strip || (action == RunMode::Run && opt_level == BuildProfile::Debug)),
    };
    let backend = match target_backend {
        TargetBackend::Wasm => BackendConfig::Wasm {
            use_wat: build_flags.output_wat,
            wasi_link: cli.unstable_feature.wasi_link
                && std::env::var("MOON_WASI_LINK").as_deref() != Ok("0"),
        },
        TargetBackend::WasmGC => BackendConfig::WasmGc {
            use_wat: build_flags.output_wat,
        },
        TargetBackend::Js => BackendConfig::Js,
        TargetBackend::Native => {
            let new_native_env = std::env::var(ENV_MOONBIT_NEW_NATIVE).ok();
            BackendConfig::Native {
                direct_object_candidate: NativeTarget::from_host_with_new_native_env(
                    std::env::consts::ARCH,
                    std::env::consts::OS,
                    new_native_env.as_deref(),
                ),
                allocator: compiler_flags::NativeAllocator::from_env()?,
                os: std::env::consts::OS
                    .parse::<OperatingSystem>()
                    .expect("Unknown"),
                compiler_paths: compiler_flags::CompilerPaths::from_moon_dirs(),
            }
        }
        TargetBackend::LLVM => BackendConfig::Llvm {
            allocator: compiler_flags::NativeAllocator::from_env()?,
            os: std::env::consts::OS
                .parse::<OperatingSystem>()
                .expect("Unknown"),
            compiler_paths: compiler_flags::CompilerPaths::from_moon_dirs(),
        },
    };
    info!("Final backend configuration: {:?}", backend);
    let stdlib_path = (build_flags.std() && !is_core).then(moonutil::toolchain::core);
    let target_layout =
        TargetLayout::from_resolve_output(target_dir.to_owned(), resolve_output, opt_level, action);
    let artifact_paths = ArtifactPathResolver::new(target_layout, stdlib_path.clone());
    Ok(CompileConfig {
        target_dir: target_dir.to_owned(),
        backend,
        opt_level,
        action,
        debug_info,
        stdlib_path,
        artifact_paths,
        enable_coverage: build_flags.enable_coverage,
        debug_export_build_plan: cli.unstable_feature.rr_export_build_plan,
        // In legacy impl, dry run always forces no JSON.
        moonc_output_json: !cli.dry_run && build_flags.output_style().needs_moonc_json(),
        docs_serve: false,
        warning_condition: if build_flags.deny_warn {
            WarningCondition::Deny
        } else {
            WarningCondition::Default
        },
        warn_list: build_flags.warn_list.clone(),
        info_no_alias: false,
    })
}

/// Plan a build graph from an already resolved project and command intent.
///
/// At this boundary, command adapters have already resolved user selectors and
/// command-specific directives into `CalcUserIntentOutput`. RR consumes those
/// identities plus precomputed build-context paths from the command adapter.
/// For actual builds, callers hold the target-directory lock while prebuild
/// scripts run and their outputs are consumed. Dry-run callers leave locking to
/// this function: it locks only when the resolved backend and modules require
/// scripts, and keeps the lock through planning.
#[instrument(level = Level::DEBUG, skip_all)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_resolved_build_from_intent(
    cx: CompileConfig,
    user_log: &UserLog,
    intent: CalcUserIntentOutput,
    mooncake_bin_dir: &Path,
    resolve_output: ResolveOutput,
    jobs: Option<usize>,
    frozen: bool,
    dry_run: bool,
) -> anyhow::Result<(BuildMeta, BuildInput)> {
    let target_dir = cx.target_dir.clone();
    info!("User intent calculated: {:?}", intent.intents);

    // Decide once, after backend selection, whether planning will execute a
    // script. Dry runs that only lower commands must not acquire a write lock.
    let run_prebuild = cx.action != RunMode::Check
        && cx.backend.target_backend().is_native()
        && resolve_output
            .module_rel
            .all_modules_and_id()
            .any(|(m, _)| {
                resolve_output
                    .module_info(m)
                    .__moonbit_unstable_prebuild
                    .is_some()
            });
    let _dry_run_lock = if dry_run && run_prebuild {
        Some(moonutil::locks::lock_directory(&target_dir, user_log)?)
    } else {
        None
    };
    let prebuild_config = if run_prebuild {
        info!("Running prebuild configuration");
        Some(prebuild::run_prebuild_config(
            &resolve_output,
            &cx,
            resolve_parallelism(jobs),
            frozen,
        )?)
    } else {
        info!("Skipping prebuild configuration: no applicable scripts");
        None
    };

    info!("Expanding user intents to requested artifacts");
    let requested_artifacts =
        intent.requested_artifacts(&resolve_output, user_log, cx.backend.target_backend());
    info!("Begin lowering to build graph");
    let compile_output = moonbuild_rupes_recta::compile(
        &cx,
        mooncake_bin_dir,
        &resolve_output,
        &requested_artifacts,
        &intent.directive,
        prebuild_config.as_ref(),
        user_log,
    )?;

    if cx.debug_export_build_plan
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

    let artifacts = compile_output
        .execution_plan
        .requested_artifact_paths()
        .map(|(artifact, paths)| (artifact.clone(), paths.to_vec()))
        .collect();
    let action_backends = compile_output
        .execution_plan
        .action_ids()
        .map(|id| (id, Some(cx.backend.target_backend())))
        .collect();
    let execution_plan = Rc::new(compile_output.execution_plan);
    let build_meta = BuildMeta {
        resolve_output,
        artifacts,
        backend: cx.backend.clone(),
        opt_level: cx.opt_level,
        artifact_paths: cx.artifact_paths.clone(),
    };

    let input = BuildInput {
        execution_plan,
        action_backends,
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
    let execution_plan = Rc::new(moonbuild_rupes_recta::fmt::build_execution_plan_for_fmt(
        resolved,
        cfg,
        target_dir,
        selected_packages,
        project_manifest,
        user_log,
    )?);
    let action_backends = execution_plan
        .action_ids()
        .map(|action| (action, None))
        .collect();
    Ok(BuildInput {
        execution_plan,
        action_backends,
    })
}

/// Generate the backend/profile/run-mode-scoped `packages.json` document.
///
/// To ensure the correct paths are generated, `build_meta` should come from the
/// same configuration used in [`plan_build`].
#[instrument(level = Level::DEBUG, skip_all)]
pub fn generate_metadata(
    source_dir: &Path,
    build_meta: &BuildMeta,
    build_input: &BuildInput,
) -> anyhow::Result<()> {
    let layout = build_meta.artifact_paths.target_layout();
    let scoped_metadata_file = layout.packages_json_path(build_meta.target_backend());

    let check_commands = collect_check_commands_by_output(build_input);
    let metadata = moonbuild_rupes_recta::metadata::gen_metadata_json(
        &build_meta.resolve_output,
        source_dir,
        &build_meta.artifact_paths,
        build_meta.opt_level,
        build_meta.target_backend(),
        &check_commands,
    );
    let meta = serde_json::to_string_pretty(&metadata).context("Failed to serialize metadata")?;
    write_metadata_if_changed(&scoped_metadata_file, &meta)
}

/// Generate the universal `packages.json` selector for one scoped document.
pub fn generate_metadata_selector(build_meta: &BuildMeta) -> anyhow::Result<()> {
    let selector = moonutil::manifest::PackagesSelectorJSON {
        backend: build_meta.target_backend().to_string(),
        opt_level: build_meta.opt_level.as_str().to_string(),
    };
    let selector = serde_json::to_string_pretty(&selector)
        .context("Failed to serialize universal packages metadata")?;
    let layout = build_meta.artifact_paths.target_layout();
    let metadata_file = layout.packages_selector_path();
    write_metadata_if_changed(&metadata_file, &selector)
}

/// Generate the backend inventory for the package metadata published by one
/// full Check invocation.
pub fn generate_metadata_index<'a>(
    build_metas: impl IntoIterator<Item = &'a BuildMeta>,
) -> anyhow::Result<()> {
    let mut build_metas = build_metas.into_iter();
    let first = build_metas
        .next()
        .context("Cannot generate packages metadata index without a Check plan")?;
    let metadata_file = first.artifact_paths.target_layout().packages_index_path();
    let backends = std::iter::once(first)
        .chain(build_metas)
        .map(BuildMeta::target_backend)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|backend| backend.to_string())
        .collect::<Vec<_>>();
    let index = serde_json::to_string_pretty(&backends)
        .context("Failed to serialize packages metadata index")?;
    write_metadata_if_changed(&metadata_file, &index)
}

fn write_metadata_if_changed(metadata_file: &Path, metadata: &str) -> anyhow::Result<()> {
    let orig_meta = std::fs::read_to_string(metadata_file);
    if !orig_meta.is_ok_and(|original| original == metadata) {
        let parent = metadata_file
            .parent()
            .context("build metadata path must have a parent directory")?;
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create directory for build metadata at {}",
                parent.display()
            )
        })?;
        let mut staged = tempfile::NamedTempFile::new_in(parent).with_context(|| {
            format!(
                "Failed to stage build metadata for {}",
                metadata_file.display()
            )
        })?;
        staged.write_all(metadata.as_bytes()).with_context(|| {
            format!(
                "Failed to write staged build metadata for {}",
                metadata_file.display()
            )
        })?;
        staged
            .persist(metadata_file)
            .map_err(|error| error.error)
            .with_context(|| {
                format!(
                    "Failed to publish build metadata to {}",
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
    for id in build_input.execution_plan.action_ids() {
        let action = build_input.execution_plan.action(id);
        let Some(command_args) = check_command_args_without_executable(action.command().args())
        else {
            continue;
        };
        for output in action.outputs() {
            commands.insert(output.clone(), command_args.clone());
        }
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
        .all_pkgs_of_build_target(build_meta.target_backend());
    let all_pkgs_json = moonbuild_rupes_recta::all_pkgs::gen_all_pkgs_json(
        &build_meta.resolve_output,
        &build_meta.artifact_paths,
        build_meta.target_backend(),
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

/// Share the default observation between prebuild scripts and the executor so
/// both see the same job limit throughout this Moon process.
fn resolve_parallelism(jobs: Option<usize>) -> usize {
    static DEFAULT_PARALLELISM: LazyLock<usize> =
        LazyLock::new(|| std::thread::available_parallelism().map_or(1, usize::from));
    jobs.unwrap_or_else(|| *DEFAULT_PARALLELISM)
}

/// A complete execution plan and the context needed to execute it.
#[derive(Debug, Clone)]
pub struct BuildInput {
    /// Executor-neutral actions shared by execution and its projections.
    execution_plan: Rc<ExecutionPlan>,

    /// Target Backend for each action. Shared actions have no single backend.
    action_backends: HashMap<ActionId, Option<TargetBackend>>,
}

impl BuildInput {
    fn compose(inputs: Vec<Self>) -> anyhow::Result<Self> {
        let mut inputs = inputs.into_iter();
        let first = inputs
            .next()
            .context("cannot compose an empty build invocation")?;
        let Some(second) = inputs.next() else {
            return Ok(first);
        };

        let mut execution_plan = ExecutionPlan::default();
        let mut action_backends = HashMap::new();

        for input in std::iter::once(first)
            .chain(std::iter::once(second))
            .chain(inputs)
        {
            let existing_actions = execution_plan.action_ids().collect::<HashSet<_>>();
            let remapped = execution_plan.merge(&input.execution_plan)?;
            for (old, new) in input.execution_plan.action_ids().zip(remapped) {
                if existing_actions.contains(&new) {
                    if action_backends.get(&new) != input.action_backends.get(&old) {
                        action_backends.insert(new, None);
                    }
                } else {
                    action_backends.insert(new, input.action_backends.get(&old).copied().flatten());
                }
            }
        }

        Ok(Self {
            execution_plan: Rc::new(execution_plan),
            action_backends,
        })
    }
}

pub(crate) fn compose_build_inputs(inputs: Vec<BuildInput>) -> anyhow::Result<BuildInput> {
    BuildInput::compose(inputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn metadata_write_preserves_equal_files_and_replaces_changed_files() {
        use std::os::unix::fs::MetadataExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metadata");

        write_metadata_if_changed(&path, "first").unwrap();
        let first_inode = path.metadata().unwrap().ino();

        write_metadata_if_changed(&path, "first").unwrap();
        assert_eq!(path.metadata().unwrap().ino(), first_inode);

        write_metadata_if_changed(&path, "second").unwrap();
        assert_ne!(path.metadata().unwrap().ino(), first_inode);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "second");
    }
}
