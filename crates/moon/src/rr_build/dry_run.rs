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

use std::{path::Path, process::Command};

use moonbuild_rupes_recta::model::Artifacts;

use crate::rr_build::BuildInput;

/// Print what would be executed in a dry-run.
///
/// This is a helper function that prints the build commands from a build graph.
pub fn print_dry_run<'a>(
    input: &BuildInput,
    artifacts: impl IntoIterator<Item = &'a Artifacts>,
    source_dir: &Path,
    target_dir: &Path,
) {
    let graph = &input.graph;
    let default_files = artifacts
        .into_iter()
        .flat_map(|art| {
            art.artifacts
                .iter()
                .flat_map(|file| graph.files.lookup(&file.to_string_lossy()))
        })
        .collect::<Vec<_>>();

    moonbuild::dry_run::print_build_commands(graph, &default_files, source_dir, target_dir);
}

/// Print all commands in a dry-run.
///
/// Similar to [`print_dry_run`], but assumes *all* files in the build graph are to be built.
pub fn print_dry_run_all(input: &BuildInput, source_dir: &Path, target_dir: &Path) {
    let default_files = input.graph.get_start_nodes();
    moonbuild::dry_run::print_build_commands(&input.graph, &default_files, source_dir, target_dir);
}

/// Print a command as it would be executed, with the proper escaping.
///
/// This also replaces paths like `print_dry_run` does.
pub fn dry_print_command(cmd: &Command, source_dir: &Path) {
    let args = std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|x| x.to_string_lossy())
        .collect::<Vec<_>>();
    let cmd = shlex::try_join(args.iter().map(|x| &**x)).expect("null in args, should not happen");
    let res = moonbuild::dry_run::replace_path(source_dir, true, &cmd);
    println!("{}", res);
}
