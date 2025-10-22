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

use std::{path::PathBuf, str::FromStr};

use indexmap::IndexMap;
use log::{debug, info};
use moonutil::{
    common::{RunMode, TargetBackend},
    compiler_flags::CompilerPaths,
    cond_expr::OptLevel,
    moon_dir::MOON_DIRS,
};
use tracing::{Level, instrument};

use crate::{
    build_lower,
    build_plan::{self, BuildEnvironment, InputDirective},
    model::{Artifacts, BuildPlanNode, OperatingSystem},
    prebuild::PrebuildOutput,
    resolve::ResolveOutput,
    special_cases::should_skip_tests,
};

/// The context that encapsulates all the data needed for the building process.
pub struct CompileConfig {
    /// Target directory, i.e. `target/`
    pub target_dir: PathBuf,
    /// The backend to use for the compilation.
    pub target_backend: TargetBackend,
    /// The optimization level to use for the compilation.
    pub opt_level: OptLevel,
    /// The action done in this operation, currently only used in legacy directory layout
    pub action: RunMode,
    /// Whether to emit debug symbols.
    pub debug_symbols: bool,

    /// The path to the standard library's project root, or `None` if to not
    /// import the standard library during compilation.
    ///
    /// TODO: This should be a per-module or per-package setting, instead of a
    /// global one.
    pub stdlib_path: Option<PathBuf>,

    // TODO: more knobs
    // TODO: Some of these knobs should be migrated to be applied by configs and similar
    /// Whether to export the build plan graph in the compile output.
    /// This should only be used in debugging scenarios.
    pub debug_export_build_plan: bool,
    /// Enable code coverage instrumentation.
    pub enable_coverage: bool,
    /// Output WAT instead of WASM binary format.
    pub output_wat: bool,
    /// Whether to output JSON or human-readable error code
    pub moonc_output_json: bool,
    /// Whether to output HTML for docs (in serve mode)
    pub docs_serve: bool,
    /// Whether to disallow all warnings
    pub deny_warn: bool,
    /// List of warnings to enable
    pub warn_list: Option<String>,
    /// List of alerts to enable
    pub alert_list: Option<String>,
    /// Whether to not emit alias when running `mooninfo`
    pub info_no_alias: bool,
}

/// The output information of the compilation.
pub struct CompileOutput {
    /// The n2 compile graph to be executed
    pub build_graph: n2::graph::Graph,

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
    resolve_output: &ResolveOutput,
    input_nodes: &[BuildPlanNode],
    input_directive: &InputDirective,
    prebuild_config: Option<&PrebuildOutput>,
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
        opt_level: cx.opt_level,
        std: cx.stdlib_path.is_some(),
        warn_list: cx.warn_list.clone(),
        alert_list: cx.alert_list.clone(),
    };
    let plan = build_plan::build_plan(
        resolve_output,
        &build_env,
        input_nodes,
        input_directive,
        prebuild_config,
    )?;

    info!("Build plan created successfully");
    debug!("Build plan contains {} nodes", plan.node_count());

    let lower_env = build_lower::BuildOptions {
        main_module: if let &[module] = resolve_output.module_rel.input_module_ids() {
            Some(resolve_output.module_rel.mod_name_from_id(module).clone())
        } else {
            None
        },
        target_dir_root: cx.target_dir.clone(),
        target_backend: cx.target_backend,
        opt_level: cx.opt_level,
        action: cx.action,

        enable_coverage: cx.enable_coverage,
        debug_symbols: cx.debug_symbols,
        output_wat: cx.output_wat,
        moonc_output_json: cx.moonc_output_json,
        docs_serve: cx.docs_serve,
        deny_warn: cx.deny_warn,
        info_no_alias: cx.info_no_alias,

        stdlib_path: cx.stdlib_path.clone(),
        compiler_paths: CompilerPaths::from_moon_dirs(), // change to external
        os: OperatingSystem::from_str(std::env::consts::OS).expect("Unknown"),
        runtime_dot_c_path: MOON_DIRS.moon_lib_path.join("runtime.c"), // FIXME: don't calculate here
    };
    let res = build_lower::lower_build_plan(resolve_output, &plan, &lower_env)?;

    info!("Build graph lowering completed successfully");
    debug!("Final build graph created with n2");

    Ok(CompileOutput {
        build_graph: res.build_graph,
        artifacts: res.artifacts,
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
        Some(tgt) if tgt.kind.is_test() => !should_skip_tests(tgt.package, resolve_output),
        _ => true,
    }
}
