use std::path::PathBuf;

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
    // TODO: more knobs
}

/// The output information of the compilation.
pub struct CompileOutput {
    pub build_graph: n2::graph::Graph,
    // artifacts: Vec<PathBuf>,
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
    #[error("Failed to build the plan: {0}")]
    BuildPlanError(#[from] build_plan::BuildPlanConstructError),
    #[error("Failed to lower the build plan: {0}")]
    LowerError(#[from] build_lower::LoweringError),
}

pub fn compile(
    cx: &CompileContext,
    intents: &[UserIntent],
) -> Result<CompileOutput, CompileGraphError> {
    let input = intents
        .iter()
        .flat_map(translate_intent)
        .collect::<Vec<_>>();

    compile_with_raw_nodes(cx, &input)
}

pub fn compile_with_raw_nodes(
    cx: &CompileContext,
    input_nodes: &[BuildPlanNode],
) -> Result<CompileOutput, CompileGraphError> {
    let build_env = BuildEnvironment {
        target_backend: cx.target_backend,
        opt_level: cx.opt_level,
    };
    let plan = build_plan::build_plan(
        &cx.resolve_output.pkg_dirs,
        &cx.resolve_output.pkg_rel,
        &build_env,
        input_nodes,
    )?;

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
    };
    let res = build_lower::lower_build_plan(&cx.resolve_output.pkg_dirs, &plan, &lower_env)?;

    Ok(CompileOutput {
        build_graph: res,
        // artifacts: todo!(),
    })
}

pub fn translate_intent(intent: &UserIntent) -> Vec<BuildPlanNode> {
    match intent {
        UserIntent::Check(targets) => targets
            .iter()
            .map(|&target| BuildPlanNode {
                target,
                action: TargetAction::Check,
            })
            .collect(),
        UserIntent::BuildCore(targets) => targets
            .iter()
            .map(|&target| BuildPlanNode {
                target,
                action: TargetAction::Build,
            })
            .collect(),
        UserIntent::BuildExecutable(targets) => targets
            .iter()
            .map(|&target| BuildPlanNode {
                target,
                action: TargetAction::MakeExecutable,
            })
            .collect(),
        UserIntent::Format(_ids) => todo!(),
        UserIntent::Info(_targets) => todo!(),
        UserIntent::Bundle(_module_id) => todo!(),
    }
}
