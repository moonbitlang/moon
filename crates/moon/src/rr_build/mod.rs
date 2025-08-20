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

use std::path::Path;

use moonbuild::dry_run;
use moonbuild_rupes_recta::{
    model::{Artifacts, BuildPlanNode},
    CompileContext, ResolveConfig, ResolveOutput,
};
use moonutil::{
    cli::UniversalFlags,
    common::{TargetBackend, MOONBITLANG_CORE},
    cond_expr::OptLevel,
    mooncakes::{sync::AutoSyncFlags, ModuleId},
};

use crate::cli::BuildFlags;

/// The function that calculates the user intent for the build process.
///
/// Params:
/// - The output of the resolve step. All modules and packages that this module
///     are available in this value.
/// - The list of modules that were input into the compile process (those that
///     exist in the source directory).
///
/// Returns: A vector of [`UserIntent`]s, representing what the user would like
/// to do
pub type CalcUserIntentFn =
    dyn FnOnce(&ResolveOutput, &[ModuleId]) -> anyhow::Result<Vec<BuildPlanNode>>;

/// Build metadata containing information needed for build context and results.
/// The build graph is kept separate to allow execute_build to take ownership of it.
pub struct BuildMeta {
    /// The result of the resolve step, containing package metadata
    pub resolve_output: ResolveOutput,

    /// The list of artifacts that will be produced
    pub artifacts: Vec<Artifacts>,

    /// The target backend used in this compile process
    pub target_backend: TargetBackend,
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
        if self.successful() {
            0
        } else {
            1
        }
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

/// Plan the build process without executing it.
///
/// This function performs all the preparation steps: resolve dependencies,
/// calculate user intent, and create the build graph, but does not execute
/// the actual build tasks.
///
/// Returns the execution plan (metadata) and build graph separately, allowing
/// execute_build to take ownership of just the graph while callers retain
/// access to the metadata.
pub fn plan_build(
    cli: &UniversalFlags,
    auto_sync_flags: &AutoSyncFlags,
    build_flags: &BuildFlags,
    source_dir: &Path,
    target_dir: &Path,
    calc_user_intent: Box<CalcUserIntentFn>,
) -> anyhow::Result<(BuildMeta, n2::graph::Graph)> {
    let cfg = ResolveConfig::new_with_load_defaults(auto_sync_flags.frozen);
    let resolve_output = moonbuild_rupes_recta::resolve(&cfg, source_dir)?;

    // A couple of debug things:
    if cli.unstable_feature.rr_export_module_graph {
        moonbuild_rupes_recta::util::print_resolved_env_dot(
            &resolve_output.module_rel,
            &mut std::fs::File::create(target_dir.join("module_graph.dot"))?,
        )?;
    }
    if cli.unstable_feature.rr_export_package_graph {
        moonbuild_rupes_recta::util::print_dep_relationship_dot(
            &resolve_output.pkg_rel,
            &resolve_output.pkg_dirs,
            &mut std::fs::File::create(target_dir.join("package_graph.dot"))?,
        )?;
    }

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

    let intent = calc_user_intent(&resolve_output, &[main_module_id])?;

    // std or no-std?
    // Ultimately we want to determine this from config instead of special cases.
    let use_std = build_flags.std() && main_module.name != MOONBITLANG_CORE;
    let stdlib_path = use_std.then(moonutil::moon_dir::core);

    let target_backend = build_flags
        .target_backend
        .or(preferred_backend)
        .unwrap_or_default();

    let cx = CompileContext {
        resolve_output: &resolve_output,
        target_dir: target_dir.to_owned(),
        target_backend,
        opt_level: if build_flags.release {
            OptLevel::Release
        } else {
            OptLevel::Debug
        },
        debug_symbols: !build_flags.release || build_flags.debug,
        stdlib_path,
        debug_export_build_plan: cli.unstable_feature.rr_export_build_plan,
    };
    let compile_output = moonbuild_rupes_recta::compile(&cx, &intent)?;

    if cli.unstable_feature.rr_export_build_plan {
        if let Some(plan) = compile_output.build_plan {
            moonbuild_rupes_recta::util::print_build_plan_dot(
                &plan,
                &resolve_output.module_rel,
                &resolve_output.pkg_dirs,
                &mut std::fs::File::create(target_dir.join("build_plan.dot"))?,
            )?;
        }
    }

    let build_meta = BuildMeta {
        resolve_output,
        artifacts: compile_output.artifacts,
        target_backend,
    };

    Ok((build_meta, compile_output.build_graph))
}

/// Execute a build plan.
///
/// Takes ownership of the build graph and executes the actual build tasks.
/// Returns just the build result - callers should use the resolve data and
/// artifacts from the planning phase for any metadata they need.
pub fn execute_build(
    mut build_graph: n2::graph::Graph,
    target_dir: &Path,
) -> anyhow::Result<BuildResult> {
    // Generate n2 state
    // FIXME: This is extremely verbose and barebones, only for testing purpose
    let mut hashes = n2::graph::Hashes::default();
    let n2_db = n2::db::open(
        &target_dir.join("moon.rupes-recta.db"),
        &mut build_graph,
        &mut hashes,
    )?;
    let mut prog_console = n2::progress::DumbConsoleProgress::new(false, None);
    let mut work = n2::work::Work::new(
        build_graph,
        hashes,
        n2_db,
        &n2::work::Options {
            failures_left: Some(1),
            parallelism: 1,
            explain: false,
            adopt: false,
            dirty_on_output: true,
        },
        &mut prog_console,
        n2::smallmap::SmallMap::default(),
    );
    work.want_every_file(None)?;

    // The actual execution done by the n2 executor
    let res = work.run()?;

    Ok(match res {
        Some(n) => BuildResult::Succeeded(n),
        None => BuildResult::Failed,
    })
}

/// Print what would be executed in a dry-run.
///
/// This is a helper function that prints the build commands from a build graph.
pub fn print_dry_run(
    build_graph: &n2::graph::Graph,
    artifacts: &[Artifacts],
    source_dir: &Path,
    target_dir: &Path,
) {
    let default_files = artifacts
        .iter()
        .flat_map(|art| {
            art.artifacts
                .iter()
                .flat_map(|file| build_graph.files.lookup(&file.to_string_lossy()))
        })
        .collect::<Vec<_>>();
    dry_run::print_build_commands(build_graph, &default_files, source_dir, target_dir);
}
