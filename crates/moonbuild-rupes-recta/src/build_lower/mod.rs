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

//! Lowers the normalized action plan into `n2`'s build graph.

use std::{collections::BTreeMap, path::PathBuf};

use indexmap::IndexMap;
use log::{debug, info};
use moonutil::{
    common::RunMode, compiler_flags::CompilerPaths, cond_expr::OptLevel, mooncakes::ModuleSource,
};
use n2::graph::Graph as N2Graph;
use tracing::instrument;

use crate::{
    ResolveOutput,
    build_action_plan::{BuildActionId, BuildActionPlan},
    model::{NativeTarget, OperatingSystem, RunBackend, TccRunConfig},
    pkg_name::OptionalPackageFQNWithSource,
};

pub mod artifact;
mod compiler;
mod context;
mod lower_aux;
mod lower_build;
mod utils;

pub use utils::{build_ins, build_n2_fileloc, build_outs};

use crate::build_lower::artifact::{ExecutableArtifact, LegacyLayoutBuilder};
use context::LoweringContext;

/// Knobs to tweak during build. Affects behaviors during lowering.
pub struct BuildOptions {
    pub main_module: Option<ModuleSource>,
    pub target_dir_root: PathBuf,
    // FIXME: This overlaps with `crate::build_plan::BuildEnvironment`
    pub target_backend: RunBackend,
    pub native_target: Option<NativeTarget>,
    pub tcc_run: Option<TccRunConfig>,
    pub os: OperatingSystem,
    pub opt_level: OptLevel,
    pub action: RunMode,

    // Detailed configuration -- some of them might live better in configs
    pub debug_symbols: bool,
    pub enable_coverage: bool,
    pub output_wat: bool,
    pub moonc_output_json: bool,
    pub docs_serve: bool,
    pub warning_condition: WarningCondition,
    pub info_no_alias: bool,
    pub wasi_link: bool,

    // Environments
    /// Only `Some` if we import standard library.
    pub stdlib_path: Option<PathBuf>,
    pub runtime_dot_c_path: PathBuf,
    pub compiler_paths: CompilerPaths,
}

impl BuildOptions {
    pub fn use_tcc_run(&self) -> bool {
        let use_tcc_run = self.tcc_run.is_some();
        debug_assert!(!use_tcc_run || self.target_backend == RunBackend::Native);
        debug_assert!(!use_tcc_run || self.native_target.is_none());
        use_tcc_run
    }

    pub fn executable_artifact(&self, legacy_behavior: bool) -> ExecutableArtifact {
        match self.target_backend {
            RunBackend::Wasm => ExecutableArtifact::Wasm {
                use_wat: self.output_wat,
            },
            RunBackend::WasmGC => ExecutableArtifact::WasmGC {
                use_wat: self.output_wat,
            },
            RunBackend::Js => ExecutableArtifact::Js,
            RunBackend::Native if self.use_tcc_run() => ExecutableArtifact::TccRunResponseFile,
            RunBackend::Native => ExecutableArtifact::NativeExecutable {
                os: self.os,
                legacy_behavior,
            },
            RunBackend::Llvm => ExecutableArtifact::LlvmExecutable {
                os: self.os,
                legacy_behavior,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WarningCondition {
    /// The default behavior: warnings are shown.
    Default,
    /// Deny all warnings: treat warnings as errors.
    Deny,
    /// Allow all warnings: do not show any warnings.
    Allow,
}

/// An error that may be raised during action plan lowering.
#[derive(thiserror::Error, Debug)]
pub enum LoweringError {
    #[error(
        "An error was reported by n2 (the build graph executor), \
        when lowering for package {package}, action {action:?}"
    )]
    N2 {
        package: OptionalPackageFQNWithSource,
        action: BuildActionId,
        source: anyhow::Error,
    },
    #[error("Failed to resolve native C toolchain for runtime")]
    RuntimeNativeToolchain(#[source] anyhow::Error),
}

/// Structured command argv keyed by each generated output path.
pub type CommandArgMap = BTreeMap<PathBuf, Vec<String>>;

pub struct LoweringResult {
    /// The lowered n2 build graph.
    pub build_graph: N2Graph,

    /// Structured argv for lowered commands that are represented as argument
    /// vectors before they are rendered into n2 command strings.
    pub command_args_by_output: CommandArgMap,

    /// The list of artifacts corresponding to the root input actions.
    ///
    /// Rationale for being a map: users (especially tests) need to look up
    /// artifacts corresponding to specific actions for rebuilding.
    pub artifacts: IndexMap<BuildActionId, LoweredArtifacts>,
}

pub struct LoweredArtifacts {
    pub action: BuildActionId,
    pub artifacts: Vec<PathBuf>,
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

/// Lowers a normalized action plan into an n2 [Build Graph](n2::graph::Graph).
#[instrument(skip_all)]
pub fn lower_build_plan(
    resolve_output: &ResolveOutput,
    plan: &BuildActionPlan<'_>,
    opt: &BuildOptions,
) -> Result<LoweringResult, LoweringError> {
    info!("Starting action plan lowering to n2 graph");
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

    let mut ctx = LoweringContext::new(layout, resolve_output, plan, opt);

    for id in plan.action_ids() {
        debug!("Lowering action: {:?}", id);
        ctx.lower_action(id)?;
    }

    let mut out_artifacts = IndexMap::with_capacity(plan.input_action_ids().len());
    for &action in plan.input_action_ids() {
        let artifacts = ctx.planned_artifact_paths(plan.output_artifacts(action));
        out_artifacts.insert(action, LoweredArtifacts { action, artifacts });
    }

    info!("Action plan lowering completed successfully");
    Ok(LoweringResult {
        build_graph: ctx.graph,
        command_args_by_output: ctx.command_args_by_output,
        artifacts: out_artifacts,
    })
}
