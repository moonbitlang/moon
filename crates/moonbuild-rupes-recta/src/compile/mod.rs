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

use indexmap::IndexMap;
use log::{debug, info};
use moonutil::{build_options::RunMode, cond_expr::OptLevel, user_log::UserLog};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use tracing::{Level, instrument};

use crate::{
    build_lower::{self, LoweringEnvironment, WarningCondition},
    build_plan::{self, BuildEnvironment, InputDirective},
    model::{Artifacts, BuildPlanNode, NativeBackendMode, RunBackend},
    prebuild::PrebuildOutput,
    resolve::ResolveOutput,
    special_cases::should_skip_tests,
    target_layout::ArtifactPathResolver,
};

/// The context that encapsulates all the data needed for the building process.
pub struct CompileConfig {
    /// Target directory, i.e. `_build/`
    pub target_dir: PathBuf,
    /// The backend selected for this build.
    pub target_backend: RunBackend,
    /// Native implementation selected under `RunBackend::Native`.
    pub native_mode: NativeBackendMode,
    /// The optimization level to use for the compilation.
    pub opt_level: OptLevel,
    /// The action done in this operation, currently only used in legacy directory layout
    pub action: RunMode,
    /// Whether to emit debug symbols.
    pub debug_symbols: bool,

    /// The path to the standard library's project root, or `None` if to not
    /// import the standard library during compilation.
    pub stdlib_path: Option<PathBuf>,
    /// Physical artifact path resolver selected for this compile run.
    pub artifact_paths: ArtifactPathResolver,
    /// Host/toolchain facts resolved lazily during lowering.
    pub lowering_environment: LoweringEnvironment,

    // MAINTAINERS: consider moving some of these to per-package/module options.
    /// Whether to export the build plan graph in the compile output.
    /// This should only be used in debugging scenarios.
    pub debug_export_build_plan: bool,
    /// Whether to pass `-wasi` for wasi-oriented wasm builds.
    pub wasi_link: bool,
    /// Enable code coverage instrumentation.
    pub enable_coverage: bool,
    /// Output WAT instead of WASM binary format.
    pub output_wat: bool,
    /// Whether to output JSON or human-readable error code
    pub moonc_output_json: bool,
    /// Whether to output HTML for docs (in serve mode)
    pub docs_serve: bool,
    /// Whether to disallow all warnings
    pub warning_condition: WarningCondition,
    /// List of warnings to enable
    pub warn_list: Option<String>,
    /// Whether to not emit alias when running `mooninfo`
    pub info_no_alias: bool,
}

/// The output information of the compilation.
pub struct CompileOutput {
    /// The n2 compile graph to be executed
    pub build_graph: n2::graph::Graph,

    /// Structured argv for lowered commands keyed by their generated output paths.
    pub command_args_by_output: build_lower::CommandArgMap,

    /// The final artifacts corresponding to the input nodes
    pub artifacts: IndexMap<BuildPlanNode, Artifacts>,

    /// The build plan, but only if we decided to export it.
    pub build_plan: Option<Box<build_plan::BuildPlan>>,
}

/// The two independently planned graphs for a standalone script build.
pub struct StandaloneCompileOutput {
    /// Dependency-package work, executed before the script graph.
    pub dependencies: CompileOutput,
    /// Work belonging to the synthesized script package.
    pub script: CompileOutput,
}

#[derive(Debug, thiserror::Error)]
pub enum CompileGraphError {
    #[error("Failed to build a build plan for the modules")]
    BuildPlanError(#[from] build_plan::BuildPlanConstructError),
    #[error("Failed to lower the build plan")]
    LowerError(#[from] build_lower::LoweringError),
}

#[instrument(skip_all)]
pub fn compile(
    cx: &CompileConfig,
    mooncake_bin_dir: &Path,
    resolve_output: &ResolveOutput,
    input_nodes: &[BuildPlanNode],
    input_directive: &InputDirective,
    prebuild_config: Option<&PrebuildOutput>,
    user_log: &UserLog,
) -> Result<CompileOutput, CompileGraphError> {
    info!(
        "Building compilation plan for {} build nodes",
        input_nodes.len()
    );

    let input_nodes = input_nodes
        .iter()
        .cloned()
        .filter(|x| filter_special_case_input_nodes(*x, resolve_output))
        .collect::<Vec<_>>();

    let build_env = build_environment(cx);
    let plan = build_plan::build_plan(
        resolve_output,
        mooncake_bin_dir,
        &build_env,
        input_nodes.into_iter(),
        input_directive,
        prebuild_config,
        user_log,
    )?;

    info!("Build plan created successfully");
    debug!("Build plan contains {} nodes", plan.node_count());

    lower_plan(cx, resolve_output, plan)
}

#[allow(clippy::too_many_arguments)]
#[instrument(skip_all)]
pub fn compile_standalone(
    cx: &CompileConfig,
    mooncake_bin_dir: &Path,
    resolve_output: &ResolveOutput,
    input_nodes: &[BuildPlanNode],
    script_package: crate::model::PackageId,
    input_directive: &InputDirective,
    prebuild_config: Option<&PrebuildOutput>,
    user_log: &UserLog,
) -> Result<StandaloneCompileOutput, CompileGraphError> {
    info!("Building separate standalone dependency and script plans");
    let input_nodes = input_nodes
        .iter()
        .copied()
        .filter(|node| filter_special_case_input_nodes(*node, resolve_output))
        .collect::<Vec<_>>();
    let build_env = build_environment(cx);
    let script_plan = build_plan::build_standalone_script_plan(
        resolve_output,
        mooncake_bin_dir,
        &build_env,
        input_nodes.into_iter(),
        script_package,
        input_directive,
        prebuild_config,
        user_log,
    )?;
    let dependency_inputs = script_plan
        .external_dependency_nodes()
        .collect::<BTreeSet<_>>();
    let dependency_plan = build_plan::build_plan(
        resolve_output,
        mooncake_bin_dir,
        &build_env,
        dependency_inputs.into_iter(),
        input_directive,
        prebuild_config,
        user_log,
    )?;

    debug!(
        "Standalone plans contain {} dependency nodes and {} script nodes",
        dependency_plan.node_count(),
        script_plan.node_count()
    );
    let dependencies = lower_plan(cx, resolve_output, dependency_plan)?;
    let script = lower_plan(cx, resolve_output, script_plan)?;
    Ok(StandaloneCompileOutput {
        dependencies,
        script,
    })
}

fn build_environment(cx: &CompileConfig) -> BuildEnvironment {
    BuildEnvironment {
        target_backend: cx.target_backend,
        native_mode: cx.native_mode.clone(),
        opt_level: cx.opt_level,
        action: cx.action,
        std: cx.stdlib_path.is_some(),
        warn_list: cx.warn_list.clone(),
    }
}

fn lower_plan(
    cx: &CompileConfig,
    resolve_output: &ResolveOutput,
    plan: build_plan::BuildPlan,
) -> Result<CompileOutput, CompileGraphError> {
    let lower_env = build_lower::BuildOptions {
        artifact_paths: cx.artifact_paths.clone(),
        target_backend: cx.target_backend,
        native_mode: cx.native_mode.clone(),
        selected_backend: build_lower::SelectedBackend::new(
            cx.target_backend,
            &cx.native_mode,
            cx.output_wat,
        ),
        opt_level: cx.opt_level,
        action: cx.action,
        enable_coverage: cx.enable_coverage,
        debug_symbols: cx.debug_symbols,
        output_wat: cx.output_wat,
        moonc_output_json: cx.moonc_output_json,
        docs_serve: cx.docs_serve,
        warning_condition: cx.warning_condition,
        info_no_alias: cx.info_no_alias,
        wasi_link: cx.wasi_link,
        stdlib_path: cx.stdlib_path.clone(),
        lowering_environment: cx.lowering_environment.clone(),
    };
    let (build_graph, command_args_by_output, artifacts) = {
        let action_plan = plan.build_action_plan();
        let res = build_lower::lower_build_plan(resolve_output, &action_plan, &lower_env)?;
        let artifacts = res
            .artifacts
            .into_iter()
            .map(|(action, artifacts)| {
                let node = action_plan.build_plan_node(action);
                (node, Artifacts { node, artifacts })
            })
            .collect();
        (res.build_graph, res.command_args_by_output, artifacts)
    };

    info!("Build graph lowering completed successfully");
    debug!("Final build graph created with n2");

    Ok(CompileOutput {
        build_graph,
        command_args_by_output,
        artifacts,
        build_plan: if cx.debug_export_build_plan {
            Some(Box::new(plan))
        } else {
            None
        },
    })
}

/// A filter to remove build plan nodes that are invalid. Returns `true` if the
/// node should be retained.
///
/// See [`crate::special_cases`] for more information.
#[instrument(level = Level::DEBUG, skip_all)]
fn filter_special_case_input_nodes(node: BuildPlanNode, resolve_output: &ResolveOutput) -> bool {
    match node.extract_target() {
        Some(tgt) if tgt.kind.is_test() => {
            let pkg_name = &resolve_output.pkg_dirs.get_package(tgt.package).fqn;
            !should_skip_tests(pkg_name)
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use indexmap::IndexSet;
    use moonutil::{
        build_options::RunMode,
        cond_expr::OptLevel,
        manifest::MoonMod,
        package::{MoonPkg, MoonPkgFormatter, SupportedTargetsDeclKind},
        resolution::{DEFAULT_VERSION, DirSyncResult, ModuleName, ModuleSource, ResolvedEnv},
        target::TargetBackend,
        user_log::UserLog,
    };

    use crate::{
        ResolveOutput,
        build_lower::{LoweringEnvironment, WarningCondition},
        build_plan::InputDirective,
        discover::{DiscoverResult, DiscoveredPackage},
        model::{BuildPlanNode, NativeBackendMode, RunBackend, TargetKind},
        pkg_name::{PackageFQN, PackagePath},
        pkg_solve::{DepEdge, DepRelationship},
        target_layout::{ArtifactPathResolver, TargetLayout, TargetLayoutMode},
    };

    use super::{CompileConfig, compile_standalone};

    fn moon_mod() -> MoonMod {
        MoonMod {
            name: "test/single".to_string(),
            version: None,
            deps: Default::default(),
            bin_deps: None,
            readme: None,
            repository: None,
            license: None,
            keywords: None,
            description: None,
            compile_flags: None,
            link_flags: None,
            checksum: None,
            source: None,
            rule: None,
            ext: Default::default(),
            warn_list: None,
            include: None,
            exclude: None,
            preferred_target: None,
            supported_targets: None,
            scripts: None,
            __moonbit_unstable_prebuild: None,
        }
    }

    fn moon_pkg(is_main: bool) -> MoonPkg {
        MoonPkg {
            name: None,
            is_main,
            force_link: false,
            sub_package: None,
            imports: Vec::new(),
            wbtest_imports: Vec::new(),
            test_imports: Vec::new(),
            formatter: MoonPkgFormatter {
                ignore: Default::default(),
            },
            link: None,
            warn_list: None,
            proof_enabled: false,
            targets: None,
            pre_build: None,
            bin_name: None,
            bin_target: None,
            supported_targets: TargetBackend::all().iter().copied().collect(),
            native_stub: None,
            virtual_pkg: None,
            implement: None,
            overrides: None,
            max_concurrent_tests: None,
            regex_backend: None,
            local_rules: None,
        }
    }

    fn package(
        module: moonutil::resolution::ModuleId,
        module_source: &ModuleSource,
        path: &str,
        is_single_file: bool,
        is_main: bool,
    ) -> DiscoveredPackage {
        let package_path = PackagePath::new(path).expect("test package path should parse");
        DiscoveredPackage {
            root_path: PathBuf::from(path),
            module,
            fqn: PackageFQN::new(module_source.clone(), package_path),
            is_single_file,
            manifest_path: (!is_single_file).then(|| PathBuf::from(path).join("moon.pkg.json")),
            raw: Box::new(moon_pkg(is_main)),
            supported_targets_decl: SupportedTargetsDeclKind::Omitted,
            effective_supported_targets: TargetBackend::all().iter().copied().collect(),
            source_files: vec![PathBuf::from(path).join(format!("{path}.mbt"))],
            mbt_lex_files: Vec::new(),
            mbt_yacc_files: Vec::new(),
            mbt_md_files: Vec::new(),
            mbtp_files: Vec::new(),
            c_stub_files: Vec::new(),
            virtual_mbti: None,
            is_stdlib: false,
        }
    }

    #[test]
    fn standalone_compile_lowers_dependency_and_script_products_to_separate_graphs() {
        let module_source = ModuleSource::local_path(
            "test/single"
                .parse::<ModuleName>()
                .expect("test module should parse"),
            PathBuf::from("."),
            DEFAULT_VERSION.clone(),
        );
        let (modules, module) = ResolvedEnv::only_one_module(module_source.clone(), moon_mod());
        let mut packages = DiscoverResult::default();
        packages.test_register_module(module, moon_mod());
        let script = packages.test_add_package(
            module,
            PackagePath::new("script").expect("script path should parse"),
            package(module, &module_source, "script", true, true),
        );
        let dependency = packages.test_add_package(
            module,
            PackagePath::new("dependency").expect("dependency path should parse"),
            package(module, &module_source, "dependency", false, false),
        );
        let script_target = script.build_target(TargetKind::Source);
        let dependency_target = dependency.build_target(TargetKind::Source);
        let mut relationship = DepRelationship::default();
        relationship.dep_graph.add_edge(
            script_target,
            dependency_target,
            DepEdge {
                short_alias: "dependency".into(),
                kind: TargetKind::Source,
            },
        );
        let supported = TargetBackend::all()
            .iter()
            .copied()
            .collect::<IndexSet<_>>();
        relationship
            .realizable_supported_targets
            .insert(script_target, supported.clone());
        relationship
            .realizable_supported_targets
            .insert(dependency_target, supported);
        let mut module_dirs = DirSyncResult::default();
        module_dirs.insert(module, PathBuf::from("."));
        let resolved = ResolveOutput {
            module_rel: modules,
            module_dirs,
            pkg_dirs: packages,
            pkg_rel: relationship,
        };
        let artifact_paths = ArtifactPathResolver::new(
            TargetLayout::new(
                PathBuf::from("_build"),
                TargetLayoutMode::Mono {
                    main_module: module_source,
                },
                OptLevel::Debug,
                RunMode::Run,
            ),
            None,
        );
        let config = CompileConfig {
            target_dir: PathBuf::from("_build"),
            target_backend: RunBackend::WasmGC,
            native_mode: NativeBackendMode::GeneratedC,
            opt_level: OptLevel::Debug,
            action: RunMode::Run,
            debug_symbols: false,
            stdlib_path: None,
            artifact_paths,
            lowering_environment: LoweringEnvironment::default(),
            debug_export_build_plan: false,
            wasi_link: false,
            enable_coverage: false,
            output_wat: false,
            moonc_output_json: false,
            docs_serve: false,
            warning_condition: WarningCondition::Default,
            warn_list: None,
            info_no_alias: false,
        };

        let output = compile_standalone(
            &config,
            Path::new("."),
            &resolved,
            &[BuildPlanNode::MakeExecutable(script_target)],
            script,
            &InputDirective::default(),
            None,
            &UserLog::new(log::LevelFilter::Error),
        )
        .expect("standalone plans should lower");

        assert_eq!(output.dependencies.build_graph.builds.iter().count(), 1);
        assert_eq!(output.script.build_graph.builds.iter().count(), 2);

        let dependency_outputs = output
            .dependencies
            .build_graph
            .builds
            .iter()
            .flat_map(|build| build.outs.ids.iter())
            .map(|id| {
                output.dependencies.build_graph.files.by_id[*id]
                    .name
                    .clone()
            })
            .collect::<Vec<_>>();
        assert!(
            dependency_outputs
                .iter()
                .any(|path| path.ends_with("dependency.mi"))
        );
        assert!(
            dependency_outputs
                .iter()
                .any(|path| path.ends_with("dependency.core"))
        );
        assert!(
            !dependency_outputs
                .iter()
                .any(|path| path.ends_with("script.core"))
        );

        let script_inputs = output
            .script
            .build_graph
            .builds
            .iter()
            .flat_map(|build| build.ins.ids.iter())
            .map(|id| output.script.build_graph.files.by_id[*id].name.clone())
            .collect::<Vec<_>>();
        assert!(
            script_inputs
                .iter()
                .any(|path| path.ends_with("dependency.mi"))
        );
        assert!(
            script_inputs
                .iter()
                .any(|path| path.ends_with("dependency.core"))
        );
        let script_outputs = output
            .script
            .build_graph
            .builds
            .iter()
            .flat_map(|build| build.outs.ids.iter())
            .map(|id| output.script.build_graph.files.by_id[*id].name.clone())
            .collect::<Vec<_>>();
        assert!(
            !script_outputs
                .iter()
                .any(|path| path.ends_with("dependency.mi"))
        );
        assert!(
            !script_outputs
                .iter()
                .any(|path| path.ends_with("dependency.core"))
        );
    }
}
