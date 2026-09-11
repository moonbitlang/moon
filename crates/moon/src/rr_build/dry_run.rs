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

//! Render concrete execution actions without adapting them to an executor.

use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::LazyLock,
};

use moonbuild_rupes_recta::execution_plan::{ActionId, ExecutionPlan, InputObservation};
use moonutil::path_normalizer::PathNormalizer;

use super::BuildInput;

/// Print commands for default execution roots and explicitly requested artifacts.
pub(crate) fn write_dry_run(
    output: &mut dyn Write,
    input: &BuildInput,
    source_dir: &Path,
) -> std::io::Result<()> {
    let plan = &input.execution_plan;
    let roots = plan.default_output_paths().into_iter().chain(
        plan.requested_artifact_paths()
            .flat_map(|(_, paths)| paths.iter().map(PathBuf::as_path)),
    );
    let actions = ordered_actions(plan, roots);
    let replacer = PathNormalizer::new(source_dir);
    for &id in &actions {
        let command = plan.action(id).command();
        let args = moonutil::shlex::join_native(command.args().iter().map(String::as_str));
        writeln!(output, "{}", replacer.normalize_command(&args))?;
        if let Some(cwd) = command.cwd() {
            let cwd = if cwd.is_absolute() {
                cwd.to_path_buf()
            } else {
                source_dir.join(cwd)
            };
            writeln!(output, "  cwd: {}", replacer.normalize_context_path(&cwd))?;
        }
        if !command.env().is_empty() {
            writeln!(output, "  env:")?;
            for (key, value) in command.env() {
                writeln!(
                    output,
                    "    {key}={}",
                    replacer.normalize_command_arg(value)
                )?;
            }
        }
    }

    // FIXME: Keep the integration-test dump hook until all graph snapshots
    // start from the planner harness and no longer need the compiled CLI.
    static DUMP_PATH: LazyLock<Option<String>> =
        LazyLock::new(|| std::env::var("MOON_TEST_DUMP_BUILD_GRAPH").ok());
    if let Some(path) = DUMP_PATH.as_deref() {
        let mut file = std::fs::File::create(path).expect("Failed to create dry-run dump target");
        write_action_graph(&mut file, plan, actions, &replacer)
            .expect("Failed to dump to target output");
    }
    Ok(())
}

/// Render the selected producer closure for planner snapshots.
#[cfg(test)]
pub(crate) fn write_build_graph<'a>(
    output: &mut dyn Write,
    input: &'a BuildInput,
    roots: impl IntoIterator<Item = &'a Path>,
    source_dir: &Path,
) -> std::io::Result<()> {
    write_action_graph(
        output,
        &input.execution_plan,
        ordered_actions(&input.execution_plan, roots),
        &PathNormalizer::new(source_dir),
    )
}

fn write_action_graph(
    output: &mut dyn Write,
    plan: &ExecutionPlan,
    actions: Vec<ActionId>,
    replacer: &PathNormalizer,
) -> std::io::Result<()> {
    let mut nodes = actions
        .into_iter()
        .map(|id| {
            let action = plan.action(id);
            let command =
                moonutil::shlex::join_native(action.command().args().iter().map(String::as_str));
            let mut inputs = action
                .inputs()
                .iter()
                .filter_map(|input| match input {
                    InputObservation::File(path) => {
                        Some(replacer.normalize_path(&path.to_string_lossy()))
                    }
                    InputObservation::StandardLibraryInterfaces(_) => None,
                })
                .collect::<Vec<_>>();
            inputs.sort();
            let outputs = action
                .outputs()
                .iter()
                .map(|path| replacer.normalize_path(&path.to_string_lossy()))
                .collect::<Vec<_>>();
            (outputs, inputs, replacer.normalize_command(&command))
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|a, b| a.0.cmp(&b.0));
    for (outputs, inputs, command) in nodes {
        serde_json::to_writer(
            &mut *output,
            &serde_json::json!({
                "command": command,
                "inputs": inputs,
                "outputs": outputs,
            }),
        )?;
        writeln!(output)?;
    }
    Ok(())
}

/// Preserve filename-first, dependency-before-consumer dry-run ordering.
/// Sorting paths once avoids repeated normalization during graph traversal.
fn ordered_actions<'a>(
    plan: &'a ExecutionPlan,
    roots: impl IntoIterator<Item = &'a Path>,
) -> Vec<ActionId> {
    let keys = plan
        .action_ids()
        .flat_map(|id| {
            let action = plan.action(id);
            action
                .inputs()
                .iter()
                .map(InputObservation::path)
                .chain(action.outputs().iter().map(PathBuf::as_path))
        })
        .map(|path| {
            let normalized = path.to_string_lossy().replace('\\', "/");
            let last_slash = normalized.rfind('/').map_or(0, |i| i + 1);
            (path, (normalized, last_slash))
        })
        .collect::<HashMap<_, _>>();
    let by_file_name = |path: &&'a Path| {
        let (name, last_slash) = &keys[path];
        (&name[*last_slash..], name)
    };
    // Requested artifacts can be supplied outside this plan (for example by
    // the toolchain); only declared outputs have actions to print.
    let mut roots = roots
        .into_iter()
        .filter(|path| plan.declared_output(path).is_some())
        .collect::<Vec<_>>();
    roots.sort_unstable_by_key(by_file_name);
    let mut stack = roots
        .into_iter()
        .map(|path| (path, false))
        .collect::<Vec<_>>();
    let mut visited = HashSet::new();
    let mut result = Vec::new();
    let mut inputs = Vec::new();
    while let Some((path, finished)) = stack.pop() {
        let Some(output) = plan.declared_output(path) else {
            continue;
        };
        let id = output.producer();
        if finished {
            result.push(id);
        } else if visited.insert(id) {
            stack.push((path, true));
            inputs.extend(
                plan.action(id)
                    .inputs()
                    .iter()
                    .filter_map(|input| match input {
                        InputObservation::File(path) => Some(path.as_path()),
                        InputObservation::StandardLibraryInterfaces(_) => None,
                    }),
            );
            inputs.sort_unstable_by_key(by_file_name);
            stack.extend(inputs.drain(..).map(|path| (path, false)));
        }
    }
    result
}

/// Format a command as it would be executed, with the proper escaping.
///
/// This also replaces paths like [`write_dry_run`] does.
pub fn format_dry_run_command(cmd: &Command, source_dir: &Path) -> String {
    let replacer = PathNormalizer::new(source_dir);

    let args =
        std::iter::once(replacer.normalize_command_program(&cmd.get_program().to_string_lossy()))
            .chain(
                cmd.get_args()
                    .map(|arg| replacer.normalize_command_arg(&arg.to_string_lossy())),
            )
            .collect::<Vec<_>>();

    moonutil::shlex::join_unix(args.iter().map(String::as_str))
}
