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

//! Lowers a [Build plan](crate::build_plan) into `n2`'s Build graph

use std::path::PathBuf;

use indexmap::IndexMap;
use log::{debug, info};
use moonutil::{
    common::RunMode,
    compiler_flags::{CC, CompilerPaths},
    cond_expr::OptLevel,
    mooncakes::ModuleSource,
};
use n2::graph::Graph as N2Graph;
use tracing::instrument;

use crate::{
    ResolveOutput,
    build_plan::BuildPlan,
    model::{Artifacts, BuildPlanNode, OperatingSystem, RunBackend},
    pkg_name::OptionalPackageFQNWithSource,
};

pub mod artifact;
mod compiler;
mod context;
mod lower_aux;
mod lower_build;
mod utils;

pub use utils::{build_ins, build_n2_fileloc, build_outs};

use crate::build_lower::artifact::LegacyLayoutBuilder;
use context::BuildPlanLowerContext;

/// Knobs to tweak during build. Affects behaviors during lowering.
pub struct BuildOptions {
    pub main_module: Option<ModuleSource>,
    pub target_dir_root: PathBuf,
    // FIXME: This overlaps with `crate::build_plan::BuildEnvironment`
    pub target_backend: RunBackend,
    pub os: OperatingSystem,
    pub opt_level: OptLevel,
    pub action: RunMode,

    // Detailed configuration -- some of them might live better in configs
    pub debug_symbols: bool,
    pub enable_coverage: bool,
    pub output_wat: bool,
    pub moonc_output_json: bool,
    pub docs_serve: bool,
    pub deny_warn: bool,
    pub info_no_alias: bool,

    // Environments
    /// Only `Some` if we import standard library.
    pub stdlib_path: Option<PathBuf>,
    pub runtime_dot_c_path: PathBuf,
    pub compiler_paths: CompilerPaths,
    /// Preferred default C/C++ toolchain to use (overrides CC::default()).
    pub default_cc: CC,
}

/// An error that may be raised during build plan lowering
#[derive(thiserror::Error, Debug)]
pub enum LoweringError {
    #[error(
        "An error was reported by n2 (the build graph executor), \
        when lowering for package {package}, build node {node:?}"
    )]
    N2 {
        package: OptionalPackageFQNWithSource,
        node: BuildPlanNode,
        source: anyhow::Error,
    },
}

pub struct LoweringResult {
    /// The lowered n2 build graph.
    pub build_graph: N2Graph,

    /// The list of artifacts corresponding to the root input nodes.
    ///
    /// Rationale for being a map: users (especially tests) need to look up
    /// artifacts corresponding to specific nodes for rebuilding.
    pub artifacts: IndexMap<BuildPlanNode, Artifacts>,
}

/// The command to execute for n2.
///
/// # How n2 handles commandlines
///
/// N2 (and ninja) use different conventions for handling commandlines on
/// different platforms.
///
/// - On Unix-like platforms, the command string will be fed into `sh -c`. Thus,
///   shell features like variable expansion are supported.
/// - On Windows, the command string will be directly passed to
///   `CreateProcessA`. No shell features are supported.
///
/// For most build commands, this is not an issue. All executables and argument
/// paths are absolute paths, and there's no shell features involved.
///
/// However, for prebuild commands, the commandline is expected to be copied
/// verbatim (with minimal resolving) to the generated build script. Thus,
/// splitting, resolving and quoting again may lead to e.g. shell features being
/// lost.
///
/// Thus, we're currently providing a `Verbatim` variant to handle such cases.
///
/// # Future improvements
///
/// Future design might want to omit shell features entirely for better
/// cross-platform consistency. Env var expansion are already used by some
/// libraries, so maintainers must be careful not to break those while doing so.
///
/// An idea is to use unix-style shell splitting and expansion everywhere,
/// performing the env var expansion ourselves during build graph execution
/// time. Other shell features should be disallowed. The result will then be
/// handled like `Args` native to the platform.
#[derive(Debug, Clone)]
enum Commandline {
    /// This commandline will be joined using the platform's default convention.
    Args(Vec<String>),

    /// This verbatim string will be plugged into the build graph as-is.
    /// Use with caution.
    ///
    /// This variant currently is only used in prebuild commands.
    Verbatim(String),
}

impl From<Vec<String>> for Commandline {
    fn from(v: Vec<String>) -> Self {
        Commandline::Args(v)
    }
}

impl Commandline {
    /// Convert this to the string representation expected by n2.
    fn to_n2_string(&self) -> String {
        match self {
            Commandline::Args(args) => {
                moonutil::shlex::join_native(args.iter().map(|x| x.as_str()))
            }
            Commandline::Verbatim(s) => s.clone(),
        }
    }
}

/// Represents the essential information needed to construct an [`Build`] value
/// that cannot be derived fromthe build plan graph.
struct BuildCommand {
    /// The **extra** input files needed for this command, **in addition to**
    /// the artifacts of the build steps this command depends on.
    extra_inputs: Vec<PathBuf>,

    /// The command to execute.
    commandline: Commandline,
}

/// Lowers a [`BuildPlan`] into a n2 [Build Graph](n2::graph::Graph).
#[instrument(skip_all)]
pub fn lower_build_plan(
    resolve_output: &ResolveOutput,
    build_plan: &BuildPlan,
    opt: &BuildOptions,
) -> Result<LoweringResult, LoweringError> {
    info!("Starting build plan lowering to n2 graph");
    debug!(
        "Build options: backend={:?}, opt_level={:?}, debug_symbols={}",
        opt.target_backend, opt.opt_level, opt.debug_symbols
    );

    let layout = LegacyLayoutBuilder::default()
        .target_base_dir(opt.target_dir_root.to_owned())
        .main_module(opt.main_module.clone())
        .stdlib_dir(opt.stdlib_path.clone())
        .opt_level(opt.opt_level)
        .run_mode(opt.action)
        .build()
        .expect("Failed to build legacy layout");

    let mut ctx = BuildPlanLowerContext::new(layout, resolve_output, build_plan, opt);

    for node in build_plan.all_nodes() {
        debug!("Lowering build node: {:?}", node);
        ctx.lower_node(node)?;
    }

    let mut out_artifcts = IndexMap::with_capacity(build_plan.input_nodes().len());
    for n in build_plan.input_nodes() {
        let mut a = vec![];
        ctx.append_all_artifacts_of(*n, &mut a);
        out_artifcts.insert(
            *n,
            Artifacts {
                node: *n,
                artifacts: a,
            },
        );
    }

    info!("Build plan lowering completed successfully");
    Ok(LoweringResult {
        build_graph: ctx.graph,
        artifacts: out_artifcts,
    })
}
