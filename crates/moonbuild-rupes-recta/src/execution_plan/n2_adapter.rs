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

use std::{collections::BTreeMap, path::PathBuf};

use log::debug;
use n2::graph::{Build, Graph, RspFile};

use super::{ActionId, ExecutionPlan, LoweredCommandExecution};
use crate::{
    build_lower::{build_ins, build_n2_fileloc, build_outs},
    pkg_name::OptionalPackageFQNWithSource,
};

/// Structured command argv keyed by each generated output path.
pub type CommandArgMap = BTreeMap<PathBuf, Vec<String>>;

#[derive(thiserror::Error, Debug)]
pub enum N2AdapterError {
    #[error(
        "An error was reported by n2 (the build graph executor), \
        when adapting package {package}, action {action:?}"
    )]
    N2 {
        package: OptionalPackageFQNWithSource,
        action: ActionId,
        source: anyhow::Error,
    },
}

pub(super) fn to_n2_graph(
    plan: &ExecutionPlan,
    actions: impl IntoIterator<Item = ActionId>,
) -> Result<(Graph, CommandArgMap), N2AdapterError> {
    let mut graph = Graph::default();
    let mut command_args_by_output = CommandArgMap::new();

    for id in actions {
        let action = plan.action(id);
        let mut input_paths = action
            .artifact_inputs
            .iter()
            .flat_map(|artifact| plan.artifact_paths(artifact))
            .map(PathBuf::from)
            .chain(
                action
                    .external_inputs
                    .iter()
                    .filter_map(|input| input.n2_path().map(ToOwned::to_owned)),
            )
            .collect::<Vec<_>>();
        input_paths.sort();

        let output_paths = action
            .outputs
            .iter()
            .map(|output| plan.output(*output).path().to_owned())
            .collect::<Vec<_>>();
        for output_path in &output_paths {
            command_args_by_output.insert(output_path.clone(), action.command.args.clone());
        }

        let ins = build_ins(&mut graph, &input_paths);
        let outs = build_outs(&mut graph, &output_paths);
        let (commandline, rspfile) = match &action.command.execution {
            LoweredCommandExecution::Inline(command) => (command.clone(), None),
            LoweredCommandExecution::ResponseFile { command, file } => (
                command.clone(),
                Some(RspFile {
                    path: file.path.clone(),
                    content: file.content.clone(),
                }),
            ),
        };

        let mut build = Build::new(build_n2_fileloc(action.fileloc.clone()), ins, outs);
        build.cmdline = Some(commandline);
        build.rspfile = rspfile;
        build.cwd = action
            .command
            .cwd
            .as_ref()
            .map(|cwd| cwd.display().to_string());
        build.env = action.command.env.clone();
        build.desc = Some(action.description.clone());
        build.can_dirty_on_output = action.can_dirty_on_output;

        if log::log_enabled!(log::Level::Debug) {
            debug!(
                "adapted: {:?}\n into {:?};\n ins: {:?};\n outs: {:?}",
                id, build.cmdline, input_paths, output_paths
            );
        }

        graph
            .add_build(build)
            .map(|_| ())
            .map_err(|source| N2AdapterError::N2 {
                package: action.error_package.clone(),
                action: id,
                source,
            })?;
    }

    Ok((graph, command_args_by_output))
}
