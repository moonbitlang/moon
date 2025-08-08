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

//! Common build tools for using Rupes Recta builds

use std::path::Path;

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

/// The output of the compile and execution process.
///
/// Not to be confused with [`moonbuild_rupes_recta::compile::CompileOutput`].
pub struct CompileOutput {
    /// The number of tasks executed during the build. `None` if the build failed.
    pub tasks_executed: Option<usize>,

    /// The result of the resolve step, containing package metadata
    pub resolve_output: ResolveOutput,

    /// The list of artifacts produced, corresponding to the input user intent
    /// calculated by [`CalcUserIntentFn`].
    pub artifacts: Vec<Artifacts>,

    /// The target backend used in this compile process. Will be removed in the
    /// future as we migrate this into build plan node definition.
    pub target_backend: TargetBackend,
}

impl CompileOutput {
    /// Whether the compilation was successful.
    pub fn successful(&self) -> bool {
        self.tasks_executed.is_some()
    }

    pub fn return_code_for_success(&self) -> i32 {
        if self.successful() {
            0
        } else {
            1
        }
    }

    pub fn print_info(&self) {
        if let Some(n) = self.tasks_executed {
            println!("{} task(s) executed.", n);
        } else {
            println!("Build failed.");
        }
    }
}

/// Compile everything using the given parameters.
///
/// - `calc_user_intent` should be a callback function that returns the
///     intent(s) of the user (build, link, check, etc.) given the list of
///     modules within the current workspace. See [`CalcUserIntentFn`] for more.
pub fn compile(
    cli: &UniversalFlags,
    auto_sync_flags: &AutoSyncFlags,
    build_flags: &BuildFlags,
    source_dir: &Path,
    target_dir: &Path,

    calc_user_intent: Box<CalcUserIntentFn>,
) -> anyhow::Result<CompileOutput> {
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

    let cx = CompileContext {
        resolve_output: &resolve_output,
        target_dir: target_dir.to_owned(),
        target_backend: build_flags
            .target_backend
            .or(preferred_backend)
            .unwrap_or_default(),
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
                &resolve_output.pkg_dirs,
                &mut std::fs::File::create(target_dir.join("build_plan.dot"))?,
            )?;
        }
    }

    // Generate n2 state
    // FIXME: This is extremely verbose and barebones, only for testing purpose
    let mut graph = compile_output.build_graph;
    let mut hashes = n2::graph::Hashes::default();
    let n2_db = n2::db::open(
        &target_dir.join("moon.rupes-recta.db"),
        &mut graph,
        &mut hashes,
    )?;
    let mut prog_console = n2::progress::DumbConsoleProgress::new(false, None);
    let mut work = n2::work::Work::new(
        graph,
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

    let target_backend = cx.target_backend;
    Ok(CompileOutput {
        tasks_executed: res,
        resolve_output,
        artifacts: compile_output.artifacts,
        target_backend,
    })
}
