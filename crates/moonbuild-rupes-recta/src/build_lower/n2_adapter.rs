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

use log::debug;
use n2::graph::{Build, Graph as N2Graph, RspFile};

use super::{
    CommandArgMap, LoweredAction, LoweredCommandExecution, LoweringError,
    utils::{build_ins, build_n2_fileloc, build_outs},
};

pub(super) struct N2GraphBuilder {
    pub(super) graph: N2Graph,
    pub(super) command_args_by_output: CommandArgMap,
}

impl N2GraphBuilder {
    pub(super) fn new() -> Self {
        Self {
            graph: N2Graph::default(),
            command_args_by_output: CommandArgMap::new(),
        }
    }

    pub(super) fn add_action(&mut self, action: LoweredAction) -> Result<(), LoweringError> {
        let LoweredAction {
            id,
            dependencies,
            external_inputs,
            outputs,
            command,
            cache_eligible: _,
            fileloc,
            description,
            can_dirty_on_output,
            error_package,
        } = action;

        let mut input_paths = dependencies
            .into_iter()
            .flat_map(|product| product.paths)
            .chain(
                external_inputs
                    .iter()
                    .filter_map(|input| input.n2_path().map(ToOwned::to_owned)),
            )
            .collect::<Vec<_>>();
        input_paths.sort();

        let output_paths = outputs
            .into_iter()
            .flat_map(|product| product.paths)
            .collect::<Vec<_>>();
        for output_path in &output_paths {
            self.command_args_by_output
                .insert(output_path.clone(), command.args.clone());
        }

        let ins = build_ins(&mut self.graph, &input_paths);
        let outs = build_outs(&mut self.graph, &output_paths);
        let (commandline, rspfile) = match command.execution {
            LoweredCommandExecution::Inline(command) => (command, None),
            LoweredCommandExecution::ResponseFile { command, file } => (
                command,
                Some(RspFile {
                    path: file.path,
                    content: file.content,
                }),
            ),
        };

        let mut build = Build::new(build_n2_fileloc(fileloc), ins, outs);
        build.cmdline = Some(commandline);
        build.rspfile = rspfile;
        build.cwd = command.cwd.map(|cwd| cwd.display().to_string());
        build.env = command.env;
        build.desc = Some(description);
        build.can_dirty_on_output = can_dirty_on_output;

        if log::log_enabled!(log::Level::Debug) {
            debug!(
                "lowered: {:?}\n into {:?};\n ins: {:?};\n outs: {:?}",
                id, build.cmdline, input_paths, output_paths
            );
        }

        self.graph
            .add_build(build)
            .map(|_| ())
            .map_err(|source| LoweringError::N2 {
                package: error_package,
                action: id,
                source,
            })
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use n2::graph::BuildId;

    use crate::{
        build_action_plan::{BuildActionId, BuildProduct},
        build_lower::{
            LoweredAction, LoweredCommand, LoweredExternalInput, LoweredProduct,
            LoweredResponseFile, lowered_action::BuildCommand,
        },
        pkg_name::PackageFQN,
    };

    use super::N2GraphBuilder;

    #[test]
    fn preserves_lowered_action_data_and_executable_edge_in_n2() {
        let action = BuildActionId(0);
        let executable = PathBuf::from("toolchain/bin/moonc");
        let command_args = vec![
            executable.display().to_string(),
            "build-package".to_string(),
        ];
        let response_file_command = "toolchain/bin/moonc -rsp-file build/main.core.rsp".to_string();
        let dependencies = vec![LoweredProduct {
            producer: action,
            product: BuildProduct::PrebuildOutputPath {
                path: executable.clone(),
            },
            paths: vec![executable],
        }];
        let (command, mut external_inputs) = BuildCommand {
            extra_inputs: vec![PathBuf::from("src/main.mbt")],
            commandline: LoweredCommand::from(command_args.clone()).with_response_file(
                response_file_command.clone(),
                LoweredResponseFile {
                    path: PathBuf::from("build/main.core.rsp"),
                    content: "build-package\n".to_string(),
                },
            ),
        }
        .into_lowered_parts(&dependencies);
        external_inputs.push(LoweredExternalInput::StandardLibraryInterfaces(
            PathBuf::from("toolchain/lib/core/bundle"),
        ));
        let lowered = LoweredAction {
            id: action,
            dependencies,
            external_inputs,
            outputs: vec![LoweredProduct {
                producer: action,
                product: BuildProduct::PrebuildOutputPath {
                    path: PathBuf::from("build/main.core"),
                },
                paths: vec![PathBuf::from("build/main.core")],
            }],
            command,
            cache_eligible: true,
            fileloc: "build main".to_string(),
            description: "build main".to_string(),
            can_dirty_on_output: true,
            error_package: None::<PackageFQN>.into(),
        };

        let mut n2 = N2GraphBuilder::new();
        n2.add_action(lowered).expect("action should enter n2");

        let build = &n2.graph.builds[BuildId::from(0)];
        assert_eq!(
            build.cmdline.as_deref(),
            Some(response_file_command.as_str())
        );
        let rspfile = build.rspfile.as_ref().expect("response file should remain");
        assert_eq!(rspfile.path, Path::new("build/main.core.rsp"));
        assert_eq!(rspfile.content, "build-package\n");
        assert!(build.can_dirty_on_output);
        assert_eq!(
            build
                .ins
                .ids
                .iter()
                .map(|id| n2.graph.files.by_id[*id].name.as_str())
                .collect::<Vec<_>>(),
            vec!["src/main.mbt", "toolchain/bin/moonc"]
        );
        assert_eq!(
            n2.command_args_by_output.get(Path::new("build/main.core")),
            Some(&command_args)
        );
    }
}
