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

//! Handles dry-run printing of build commands.

use std::{path::Path, process::Command, sync::LazyLock};

use moonbuild_debug::graph::debug_dump_build_graph;
use moonbuild_rupes_recta::model::Artifacts;

const ENV_VAR: &str = "MOON_TEST_DUMP_BUILD_GRAPH";
static DRY_RUN_TEST_OUTPUT: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var(ENV_VAR).ok());

/// Print what would be executed in a dry-run.
///
/// This is a helper function that prints the build commands from a build graph.
pub fn print_dry_run<'a>(
    build_graph: &n2::graph::Graph,
    artifacts: impl IntoIterator<Item = &'a Artifacts>,
    source_dir: &Path,
    target_dir: &Path,
) {
    let default_files = artifacts
        .into_iter()
        .flat_map(|art| {
            art.artifacts
                .iter()
                .flat_map(|file| build_graph.files.lookup(&file.to_string_lossy()))
        })
        .collect::<Vec<_>>();

    if let Some(out_file) = &*DRY_RUN_TEST_OUTPUT {
        debug_dump_build_graph_to_file(build_graph, &default_files, out_file);
    }

    moonbuild::dry_run::print_build_commands(build_graph, &default_files, source_dir, target_dir);
}

fn debug_dump_build_graph_to_file(
    build_graph: &n2::graph::Graph,
    default_files: &[n2::graph::FileId],
    out_file: &str,
) {
    let file = std::fs::File::create(out_file).expect("Failed to create dry-run dump target");
    let dump = debug_dump_build_graph(build_graph, default_files);
    dump.dump_to(file).expect("Failed to dump to target output");
}

/// Print all commands in a dry-run.
///
/// Similar to [`print_dry_run`], but assumes *all* files in the build graph are to be built.
pub fn print_dry_run_all(build_graph: &n2::graph::Graph, source_dir: &Path, target_dir: &Path) {
    let default_files = build_graph.get_start_nodes();
    moonbuild::dry_run::print_build_commands(build_graph, &default_files, source_dir, target_dir);
}

/// Print a command as it would be executed, with the proper escaping.
pub fn dry_print_command(cmd: &Command) {
    let args = std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|x| x.to_string_lossy())
        .collect::<Vec<_>>();
    let cmd = shlex::try_join(args.iter().map(|x| &**x)).expect("null in args, should not happen");
    println!("{cmd}");
}
