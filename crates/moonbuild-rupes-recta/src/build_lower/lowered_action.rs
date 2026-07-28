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

use std::path::PathBuf;

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

/// The representation used to construct the process command.
///
/// Most commands retain structured arguments so tools can inspect them before
/// the platform-specific command string is executed. Commands that deliberately
/// use shell composition, such as prebuild commands, remain verbatim.
#[derive(Debug, Clone)]
pub(crate) enum LoweredCommandKind {
    /// Structured argv rendered using the platform's command-line convention.
    Args(Vec<String>),

    /// A command string that intentionally relies on shell composition.
    Verbatim,
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
/// `kind` retains the logical form for metadata consumers. `execution` records
/// whether the same command is sent inline or through a response file.
#[derive(Debug, Clone)]
pub struct LoweredCommand {
    pub(crate) kind: LoweredCommandKind,
    pub(crate) execution: LoweredCommandExecution,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) env: Vec<(String, String)>,
}

impl From<Vec<String>> for LoweredCommand {
    fn from(args: Vec<String>) -> Self {
        let command = moonutil::shlex::join_native(args.iter().map(String::as_str));
        Self {
            kind: LoweredCommandKind::Args(args),
            execution: LoweredCommandExecution::Inline(command),
            cwd: None,
            env: Vec::new(),
        }
    }
}

impl LoweredCommand {
    pub(crate) fn verbatim(command: String) -> Self {
        Self {
            kind: LoweredCommandKind::Verbatim,
            execution: LoweredCommandExecution::Inline(command),
            cwd: None,
            env: Vec::new(),
        }
    }

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

    pub fn args(&self) -> Option<&[String]> {
        match &self.kind {
            LoweredCommandKind::Args(args) => Some(args),
            LoweredCommandKind::Verbatim => None,
        }
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
