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

//! Concrete action data produced before n2 file registration.

use std::path::{Path, PathBuf};

use moonutil::compiler_flags::Toolchain;

use crate::{
    build_action_plan::{BuildActionId, BuildProduct},
    pkg_name::OptionalPackageFQNWithSource,
};

/// One logical product after its concrete artifact paths have been selected.
#[derive(Debug)]
pub struct LoweredProduct {
    pub(crate) producer: BuildActionId,
    pub(crate) product: BuildProduct,
    pub(crate) paths: Vec<PathBuf>,
}

impl LoweredProduct {
    pub fn producer(&self) -> BuildActionId {
        self.producer
    }

    pub fn product(&self) -> &BuildProduct {
        &self.product
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

/// A response file selected while lowering a command.
#[derive(Debug, Clone)]
pub struct LoweredResponseFile {
    pub path: PathBuf,
    pub content: String,
}

/// How the process command should be transported to the executor.
#[derive(Debug, Clone)]
pub enum LoweredCommandExecution {
    Inline(String),
    ResponseFile {
        command: String,
        file: LoweredResponseFile,
    },
}

/// A concrete process command and its selected process transport.
///
/// `args` retains the logical form for metadata consumers. `execution` records
/// whether the same command is sent inline or through a response file.
#[derive(Debug, Clone)]
pub struct LoweredCommand {
    pub(crate) args: Vec<String>,
    pub(crate) execution: LoweredCommandExecution,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) env: Vec<(String, String)>,
}

impl From<Vec<String>> for LoweredCommand {
    fn from(args: Vec<String>) -> Self {
        let command = moonutil::shlex::join_native(args.iter().map(String::as_str));
        Self {
            args,
            execution: LoweredCommandExecution::Inline(command),
            cwd: None,
            env: Vec::new(),
        }
    }
}

impl LoweredCommand {
    pub(crate) fn inline_command(&self) -> &str {
        let LoweredCommandExecution::Inline(command) = &self.execution else {
            unreachable!("a response-file command is already lowered")
        };
        command
    }

    pub(crate) fn with_response_file(mut self, command: String, file: LoweredResponseFile) -> Self {
        self.execution = LoweredCommandExecution::ResponseFile { command, file };
        self
    }

    fn executable(&self) -> Option<&Path> {
        self.args.first().map(Path::new)
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn execution(&self) -> &LoweredCommandExecution {
        &self.execution
    }

    pub fn cwd(&self) -> Option<&std::path::Path> {
        self.cwd.as_deref()
    }

    pub fn env(&self) -> &[(String, String)] {
        &self.env
    }

    fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }

    fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env.extend(env);
        self
    }
}

/// One concrete action after product paths and command construction have been
/// resolved, but before its paths are registered with n2.
///
/// Dependency products retain their producer action and logical product so a
/// preparation policy can identify the dependency without reconstructing it
/// from n2 file edges.
#[derive(Debug)]
pub struct LoweredAction {
    pub(crate) id: BuildActionId,
    pub(crate) dependencies: Vec<LoweredProduct>,
    pub(crate) external_inputs: Vec<PathBuf>,
    pub(crate) outputs: Vec<LoweredProduct>,
    pub(crate) command: LoweredCommand,
    pub(crate) fileloc: String,
    pub(crate) description: String,
    pub(crate) can_dirty_on_output: bool,
    pub(crate) error_package: OptionalPackageFQNWithSource,
}

impl LoweredAction {
    pub fn id(&self) -> BuildActionId {
        self.id
    }

    pub fn dependencies(&self) -> &[LoweredProduct] {
        &self.dependencies
    }

    pub fn external_inputs(&self) -> &[PathBuf] {
        &self.external_inputs
    }

    pub fn outputs(&self) -> &[LoweredProduct] {
        &self.outputs
    }

    pub fn command(&self) -> &LoweredCommand {
        &self.command
    }

    pub fn can_dirty_on_output(&self) -> bool {
        self.can_dirty_on_output
    }
}

/// Command data produced by action-specific lowering before the common action
/// metadata and products are attached.
pub(super) struct BuildCommand {
    /// Input files in addition to products of dependency actions.
    pub(super) extra_inputs: Vec<PathBuf>,
    pub(super) commandline: LoweredCommand,
}

impl BuildCommand {
    /// Finish the common action-lowering boundary.
    ///
    /// Structured commands carry a concrete executable as `argv[0]`. Keep that
    /// tool file alongside the action's other external inputs so n2 can
    /// invalidate the action when the executable changes in place. If a
    /// dependency product already provides that path, omit the external copy.
    pub(super) fn into_lowered_parts(
        self,
        dependencies: &[LoweredProduct],
    ) -> (LoweredCommand, Vec<PathBuf>) {
        let Self {
            mut extra_inputs,
            commandline,
        } = self;
        if let Some(executable) = commandline.executable() {
            let is_dependency = dependencies
                .iter()
                .flat_map(|product| &product.paths)
                .any(|path| path == executable);
            if is_dependency {
                extra_inputs.retain(|path| path != executable);
            } else {
                extra_inputs.push(executable.to_owned());
            }
        }
        extra_inputs.sort();
        extra_inputs.dedup();
        (commandline, extra_inputs)
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
        let (commandline, external_inputs) = BuildCommand {
            extra_inputs: vec![
                PathBuf::from("z.mbt"),
                executable.clone(),
                PathBuf::from("a.mbt"),
                executable.clone(),
            ],
            commandline,
        }
        .into_lowered_parts(&[]);

        assert_eq!(commandline.executable(), Some(executable.as_path()));
        assert_eq!(
            external_inputs,
            vec![PathBuf::from("a.mbt"), executable, PathBuf::from("z.mbt")]
        );
    }
}
