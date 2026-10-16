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

use moonutil::{
    compiler_flags::{ArchiverConfig, CompilerPaths, Toolchain, make_archiver_command_resolved},
    toolchain::BINARIES,
};

use super::compiler::{CmdlineAbstraction, MoonArchive};
use crate::execution_plan::{InputObservation, LoweredCommand};

/// Command data produced by action-specific lowering before common execution
/// metadata and declared outputs are attached.
pub(super) struct BuildCommand {
    /// Input files in addition to artifacts produced by dependency actions.
    pub(super) extra_inputs: Vec<PathBuf>,
    pub(super) commandline: LoweredCommand,
}

impl BuildCommand {
    /// Create the static archive `archive` from exactly `members`.
    ///
    /// Archivers that update an existing archive in place, such as `ar -r`,
    /// would keep members that are no longer listed. They run through
    /// `moon tool archive`, which deletes the old archive first:
    ///
    /// ```text
    /// moon tool archive --output lib.a -- ar -r -c -s lib.a a.o b.o
    /// ```
    pub(super) fn archive(
        toolchain: &Toolchain,
        config: ArchiverConfig,
        members: &[PathBuf],
        archive: &Path,
        compiler_paths: &CompilerPaths,
    ) -> Self {
        let cc = toolchain.cc().clone();
        let archive_arg = archive.display().to_string();
        let archiver_command = make_archiver_command_resolved(
            cc.clone(),
            config,
            &members
                .iter()
                .map(|member| member.to_string_lossy())
                .collect::<Vec<_>>(),
            &archive_arg,
            compiler_paths,
        );
        let command = if cc.archiver_updates_existing_archive() {
            let command = MoonArchive {
                output: archive.into(),
                archiver: &archiver_command,
            }
            .build_command(&*BINARIES.moonbuild);
            Self {
                // The archiver is no longer `argv[0]`, so observe it explicitly.
                extra_inputs: vec![PathBuf::from(&cc.ar_path)],
                commandline: command.into(),
            }
        } else {
            Self {
                extra_inputs: vec![],
                commandline: archiver_command.into(),
            }
        };
        command.with_msvc_env(toolchain)
    }

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
    use moonutil::compiler_flags::{ARKind, ArchiverConfigBuilder, CC, CCKind};

    use crate::execution_plan::{LoweredCommandExecution, LoweredResponseFile};

    use super::*;

    fn archive_with(ar_kind: ARKind, ar_path: &str) -> BuildCommand {
        let toolchain = Toolchain::from_path_probe(CC {
            cc_kind: CCKind::Gcc,
            cc_path: "/toolchain/bin/cc".to_string(),
            ar_kind,
            ar_path: ar_path.to_string(),
            target_triple: None,
            is_env_override: false,
        });
        BuildCommand::archive(
            &toolchain,
            ArchiverConfigBuilder::default()
                .archive_moonbitrun(false)
                .build()
                .expect("archiver configuration should build"),
            &[PathBuf::from("a.o"), PathBuf::from("b.o")],
            Path::new("lib.a"),
            &CompilerPaths {
                include_path: "/toolchain/include".to_string(),
                lib_path: "/toolchain/lib".to_string(),
            },
        )
    }

    #[test]
    fn update_style_archivers_replace_the_archive() {
        let command = archive_with(ARKind::GnuAr, "/toolchain/bin/ar");
        // Command construction drops the Windows `\\?\` prefix from tool paths.
        let moon = dunce::simplified(&BINARIES.moonbuild).display().to_string();
        assert_eq!(
            command.commandline.args(),
            [
                moon.as_str(),
                "tool",
                "archive",
                "--output",
                "lib.a",
                "--",
                "/toolchain/bin/ar",
                "-r",
                "-c",
                "-s",
                "lib.a",
                "a.o",
                "b.o",
            ]
        );
        assert_eq!(command.extra_inputs, [PathBuf::from("/toolchain/bin/ar")]);
    }

    #[test]
    fn recreating_archivers_run_directly() {
        let command = archive_with(ARKind::AppleLibtool, "/toolchain/bin/libtool");
        assert_eq!(
            command.commandline.args(),
            [
                "/toolchain/bin/libtool",
                "-static",
                "-o",
                "lib.a",
                "a.o",
                "b.o"
            ]
        );
        assert!(command.extra_inputs.is_empty());
    }

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
