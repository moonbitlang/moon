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

use std::path::PathBuf;

use log::{debug, info};
use moonutil::{common::TargetBackend, cond_expr::OptLevel, mooncakes::ModuleId};

use crate::{
    build_lower,
    build_plan::{self, BuildEnvironment, BuildPlanNode},
    model::{BuildTarget, PackageId, TargetAction},
    resolve::ResolveOutput,
};

/// The context that encapsulates all the data needed for the building process.
pub struct CompileContext<'a> {
    /// The resolved environment for compiling
    pub resolve_output: &'a ResolveOutput,
    /// Target directory, i.e. `target/`
    pub target_dir: PathBuf,
    /// The backend to use for the compilation.
    pub target_backend: TargetBackend,
    /// The optimization level to use for the compilation.
    pub opt_level: OptLevel,
    /// Whether to emit debug symbols.
    pub debug_symbols: bool,

    /// The path to the standard library's project root, or `None` if to not
    /// import the standard library during compilation.
    ///
    /// TODO: This should be a per-module or per-package setting, instead of a
    /// global one.
    pub stdlib_path: Option<PathBuf>,

    // TODO: more knobs
    /// Whether to export the build plan graph in the compile output.
    /// This should only be used in debugging scenarios.
    pub debug_export_build_plan: bool,
}

/// The output information of the compilation.
pub struct CompileOutput {
    /// The n2 compile graph to be executed
    pub build_graph: n2::graph::Graph,

    // /// The final artifacts corresponding to the user intents
    // pub artifacts: Vec<PathBuf>,
    /// The build plan, but only if we decided to export it.
    pub build_plan: Option<Box<build_plan::BuildPlan>>,
}

/// The high-level intent of the user.
///
/// TODO: Do we actually need this, or should we directly let the user supply
/// the build commands? The translation process is relatively straightforward.
#[derive(Clone, Debug)]
pub enum UserIntent {
    /// A `moon check` of the given targets. This directly maps to
    /// `moon check -p ...`.
    Check(Vec<BuildTarget>),

    /// Build the core IR of the given targets. This directly maps to
    /// `moon build -p ...` when the given package does not link into an
    /// executable, or `moon build` when the whole module does not contain any
    /// linkable packages.
    BuildCore(Vec<BuildTarget>),

    /// Build the final executable of the given targets. This directly maps to
    /// `moon build` when the target links into an executable.
    BuildExecutable(Vec<BuildTarget>),

    /// Format all packages (note there's no build target here) in the list.
    /// This directly maps to `moon fmt`.
    Format(Vec<PackageId>),

    /// Generate the MBTI interface files for all packages in the list.
    /// This directly maps to `moon info`.
    Info(Vec<BuildTarget>),

    /// Bundles all packages in the given module.
    Bundle(ModuleId),
}

#[derive(Debug, thiserror::Error)]
pub enum CompileGraphError {
    #[error("Failed to build a build plan for the modules")]
    BuildPlanError(#[from] build_plan::BuildPlanConstructError),
    #[error("Failed to lower the build plan")]
    LowerError(#[from] build_lower::LoweringError),
}

pub fn compile(
    cx: &CompileContext,
    intents: &[UserIntent],
) -> Result<CompileOutput, CompileGraphError> {
    info!("Starting compilation with user intents {:?}", intents);
    debug!(
        "Target backend: {:?}, optimization level: {:?}",
        cx.target_backend, cx.opt_level
    );

    let input = intents
        .iter()
        .flat_map(translate_intent)
        .collect::<Vec<_>>();

    debug!(
        "Translated {} intents into {} build plan nodes",
        intents.len(),
        input.len()
    );

    compile_with_raw_nodes(cx, &input)
}

pub fn compile_with_raw_nodes(
    cx: &CompileContext,
    input_nodes: &[BuildPlanNode],
) -> Result<CompileOutput, CompileGraphError> {
    info!(
        "Building compilation plan for {} build nodes",
        input_nodes.len()
    );

    let build_env = BuildEnvironment {
        target_backend: cx.target_backend,
        opt_level: cx.opt_level,
        std: cx.stdlib_path.is_some(),
    };
    let plan = build_plan::build_plan(
        &cx.resolve_output.pkg_dirs,
        &cx.resolve_output.pkg_rel,
        &build_env,
        input_nodes,
    )?;

    info!("Build plan created successfully");
    debug!("Build plan contains {} nodes", plan.node_count());

    let lower_env = build_lower::BuildOptions {
        main_module: if let &[module] = cx.resolve_output.module_rel.input_module_ids() {
            Some(
                cx.resolve_output
                    .module_rel
                    .mod_name_from_id(module)
                    .clone(),
            )
        } else {
            None
        },
        target_dir_root: cx.target_dir.clone(),
        target_backend: cx.target_backend,
        opt_level: cx.opt_level,
        debug_symbols: cx.debug_symbols,
        stdlib_path: cx.stdlib_path.clone(),
    };
    let res = build_lower::lower_build_plan(
        &cx.resolve_output.pkg_dirs,
        &cx.resolve_output.pkg_rel,
        &plan,
        &lower_env,
    )?;

    info!("Build graph lowering completed successfully");
    debug!("Final build graph created with n2");

    Ok(CompileOutput {
        build_graph: res,
        // artifacts: todo!(),
        build_plan: if cx.debug_export_build_plan {
            Some(Box::new(plan))
        } else {
            None
        },
    })
}

pub fn translate_intent(intent: &UserIntent) -> Vec<BuildPlanNode> {
    match intent {
        UserIntent::Check(targets) => {
            debug!("Translating Check intent for {} targets", targets.len());
            targets
                .iter()
                .map(|&target| BuildPlanNode {
                    target,
                    action: TargetAction::Check,
                })
                .collect()
        }
        UserIntent::BuildCore(targets) => {
            debug!("Translating BuildCore intent for {} targets", targets.len());
            targets
                .iter()
                .map(|&target| BuildPlanNode {
                    target,
                    action: TargetAction::Build,
                })
                .collect()
        }
        UserIntent::BuildExecutable(targets) => {
            debug!(
                "Translating BuildExecutable intent for {} targets",
                targets.len()
            );
            targets
                .iter()
                .map(|&target| BuildPlanNode {
                    target,
                    action: TargetAction::MakeExecutable,
                })
                .collect()
        }
        UserIntent::Format(_ids) => {
            todo!()
        }
        UserIntent::Info(_targets) => {
            todo!()
        }
        UserIntent::Bundle(_module_id) => {
            todo!()
        }
    }
}
