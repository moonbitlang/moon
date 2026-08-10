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

use std::{io::Write, path::Path, process::Command};

use moonbuild_rupes_recta::model::Artifacts;

use crate::rr_build::{BuildInput, StandaloneBuildInput};

/// Write what would be executed in a dry-run.
///
/// This is a helper function that renders the build commands from a build graph.
pub fn write_dry_run<'a>(
    output: &mut dyn Write,
    input: &BuildInput,
    artifacts: impl IntoIterator<Item = &'a Artifacts>,
    source_dir: &Path,
    target_dir: &Path,
) -> std::io::Result<()> {
    let graph = &input.graph;
    let default_files = graph
        .get_start_nodes()
        .into_iter()
        .chain(artifacts.into_iter().flat_map(|art| {
            art.artifacts
                .iter()
                .flat_map(|file| graph.files.lookup(&file.to_string_lossy()))
        }))
        .collect::<Vec<_>>();

    moonbuild::dry_run::write_build_commands(
        output,
        graph,
        &default_files,
        &input.command_args_by_output,
        source_dir,
        target_dir,
    )
}

/// Write all commands in a dry-run.
///
/// Similar to [`write_dry_run`], but assumes *all* files in the build graph are to be built.
pub fn write_dry_run_all(
    output: &mut dyn Write,
    input: &BuildInput,
    source_dir: &Path,
    target_dir: &Path,
) -> std::io::Result<()> {
    let default_files = input.graph.get_start_nodes();
    moonbuild::dry_run::write_build_commands(
        output,
        &input.graph,
        &default_files,
        &input.command_args_by_output,
        source_dir,
        target_dir,
    )
}

/// Write standalone dependency commands before the selected script commands.
pub fn write_standalone_dry_run<'a>(
    output: &mut dyn Write,
    input: &StandaloneBuildInput,
    artifacts: impl IntoIterator<Item = &'a Artifacts>,
    source_dir: &Path,
    target_dir: &Path,
) -> std::io::Result<()> {
    if let Some(dependencies) = input.dependencies.as_ref() {
        write_dry_run_all(output, dependencies, source_dir, target_dir)?;
    }
    write_dry_run(output, &input.script, artifacts, source_dir, target_dir)
}

/// Format a command as it would be executed, with the proper escaping.
///
/// This also replaces paths like [`write_dry_run`] does.
pub fn format_dry_run_command(cmd: &Command, source_dir: &Path) -> String {
    let replacer = moonbuild::dry_run::PathNormalizer::new(source_dir);

    let args =
        std::iter::once(replacer.normalize_command_program(&cmd.get_program().to_string_lossy()))
            .chain(
                cmd.get_args()
                    .map(|arg| replacer.normalize_command_arg(&arg.to_string_lossy())),
            )
            .collect::<Vec<_>>();

    moonutil::shlex::join_unix(args.iter().map(String::as_str))
}
