// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::path::{Path, PathBuf};

use moonutil::compiler_flags::Toolchain;

use crate::execution_plan::{InputObservation, LoweredCommand};

/// Command data produced by action-specific lowering before common execution
/// metadata and declared outputs are attached.
pub(super) struct BuildCommand {
    /// Input files in addition to artifacts produced by dependency actions.
    pub(super) extra_inputs: Vec<PathBuf>,
    pub(super) commandline: LoweredCommand,
}

impl BuildCommand {
    /// Finish the common action-lowering boundary.
    ///
    /// Structured commands carry a concrete executable as `argv[0]`. Keep that
    /// tool file alongside the action's other input observations so execution
    /// adapters can invalidate the action when the executable changes in place.
    /// If another input source already provides that path, do not add a second
    /// declaration for the executable.
    pub(super) fn into_lowered_parts<'a>(
        self,
        dependency_paths: impl IntoIterator<Item = &'a Path>,
    ) -> (LoweredCommand, Vec<InputObservation>) {
        let Self {
            mut extra_inputs,
            commandline,
        } = self;
        if let Some(executable) = commandline.executable() {
            let is_dependency = dependency_paths.into_iter().any(|path| path == executable);
            if is_dependency {
                extra_inputs.retain(|path| path != executable);
            } else if !extra_inputs.iter().any(|path| path == executable) {
                extra_inputs.push(executable.to_owned());
            }
        }
        extra_inputs.sort();
        assert!(
            extra_inputs.windows(2).all(|pair| pair[0] != pair[1]),
            "lowered command inputs must be declared once"
        );
        (
            commandline,
            extra_inputs
                .into_iter()
                .map(InputObservation::File)
                .collect(),
        )
    }

    pub(super) fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.commandline = self.commandline.with_cwd(cwd);
        self
    }

    pub(super) fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.commandline = self.commandline.with_env(env);
        self
    }

    pub(super) fn with_msvc_env(self, toolchain: &Toolchain) -> Self {
        self.with_env(super::compiler::msvc::command_env(toolchain))
    }
}

#[cfg(test)]
mod tests {
    use crate::execution_plan::{LoweredCommandExecution, LoweredResponseFile};

    use super::*;

    #[test]
    fn structured_executable_is_derived_from_original_args_and_inputs_are_canonical() {
        let executable = PathBuf::from("toolchain/bin/moonc");
        let commandline = LoweredCommand::from(vec![
            executable.display().to_string(),
            "build-package".to_string(),
        ])
        .with_response_file(
            "transported-command @command.rsp".to_string(),
            LoweredResponseFile {
                path: PathBuf::from("command.rsp"),
                content: "build-package".to_string(),
            },
        );
        let (commandline, inputs) = BuildCommand {
            extra_inputs: vec![
                PathBuf::from("z.mbt"),
                executable.clone(),
                PathBuf::from("a.mbt"),
            ],
            commandline,
        }
        .into_lowered_parts(std::iter::empty());

        assert_eq!(
            inputs,
            vec![
                InputObservation::File(PathBuf::from("a.mbt")),
                InputObservation::File(executable),
                InputObservation::File(PathBuf::from("z.mbt")),
            ]
        );
        assert!(matches!(
            commandline.execution(),
            LoweredCommandExecution::ResponseFile { .. }
        ));
    }
}
