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
use std::path::{Path, PathBuf};
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
    /// Whether lowering should describe registry dependency build actions for
    /// standalone script cache preparation.
    pub collect_dependency_build_actions: bool,
}

/// The output information of the compilation.
pub struct CompileOutput {
    /// The n2 compile graph to be executed
    pub build_graph: n2::graph::Graph,

    /// Structured argv for lowered commands keyed by their generated output paths.
    pub command_args_by_output: build_lower::CommandArgMap,

    /// Registry dependency compiler actions eligible for dependency-graph
    /// preparation by standalone script execution.
    pub dependency_build_actions: Vec<crate::dependency_build_cache::DependencyBuildAction>,

    /// The final artifacts corresponding to the input nodes
    pub artifacts: IndexMap<BuildPlanNode, Artifacts>,

    /// The build plan, but only if we decided to export it.
    pub build_plan: Option<Box<build_plan::BuildPlan>>,
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
        .filter(|x| filter_special_case_input_nodes(*x, resolve_output));

    let build_env = BuildEnvironment {
        target_backend: cx.target_backend,
        native_mode: cx.native_mode.clone(),
        opt_level: cx.opt_level,
        action: cx.action,
        std: cx.stdlib_path.is_some(),
        warn_list: cx.warn_list.clone(),
    };
    let plan = build_plan::build_plan(
        resolve_output,
        mooncake_bin_dir,
        &build_env,
        input_nodes,
        input_directive,
        prebuild_config,
        user_log,
    )?;

    info!("Build plan created successfully");
    debug!("Build plan contains {} nodes", plan.node_count());

    let selected_backend =
        build_lower::SelectedBackend::new(cx.target_backend, &cx.native_mode, cx.output_wat);
    let lower_env = build_lower::BuildOptions {
        artifact_paths: cx.artifact_paths.clone(),
        target_backend: cx.target_backend,
        native_mode: cx.native_mode.clone(),
        selected_backend,
        opt_level: cx.opt_level,
        action: cx.action,

        enable_coverage: cx.enable_coverage,
        debug_symbols: cx.debug_symbols,
        output_wat: cx.output_wat,
        moonc_output_json: cx.moonc_output_json,
        docs_serve: cx.docs_serve,
        warning_condition: cx.warning_condition,
        info_no_alias: cx.info_no_alias,
        collect_dependency_build_actions: cx.collect_dependency_build_actions,
        wasi_link: cx.wasi_link,

        stdlib_path: cx.stdlib_path.clone(),
        lowering_environment: cx.lowering_environment.clone(),
    };
    let (build_graph, command_args_by_output, dependency_build_actions, artifacts) = {
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
        (
            res.build_graph,
            res.command_args_by_output,
            res.dependency_build_actions,
            artifacts,
        )
    };

    info!("Build graph lowering completed successfully");
    debug!("Final build graph created with n2");

    Ok(CompileOutput {
        build_graph,
        command_args_by_output,
        dependency_build_actions,
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
