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

//! Abstraction for `moonc build-interface`.

use std::borrow::Cow;
use std::path::Path;

use crate::build_lower::compiler::{CmdlineAbstraction, CompiledPackageName, MiDependency};

/// Command-line abstraction for `moonc build-interface`.
///
/// This builds a MoonBit interface file (`.mi`) from a MoonBit interface
/// contract file (`.mbti`). It is the reverse of `mooninfo`.
///
/// This mirrors the invocation assembled in
/// [`gen_build_interface_command`](crates/moonbuild/src/gen/gen_build.rs:369) and
/// is used to parametrise interface generation for virtual packages.
#[derive(Debug)]
pub(crate) struct MooncBuildInterface<'a> {
    /// The source `.mbti` contract.
    pub mbti_input: Cow<'a, Path>,
    /// Destination `.mi` path.
    pub mi_output: Cow<'a, Path>,
    /// Interface dependencies (each rendered as `<path>:<alias>`).
    pub mi_deps: &'a [MiDependency<'a>],
    /// Fully-qualified package name.
    pub package_name: CompiledPackageName<'a>,
    /// Absolute source directory for the package.
    pub package_source: Cow<'a, Path>,
    /// Whether to emit structured diagnostics (`-error-format=json`).
    pub json_errors: bool,
}

impl<'a> MooncBuildInterface<'a> {
    /// Create a new command abstraction with default settings.
    pub(crate) fn new(
        mbti_input: impl Into<Cow<'a, Path>>,
        mi_output: impl Into<Cow<'a, Path>>,
        mi_deps: &'a [MiDependency<'a>],
        package_name: CompiledPackageName<'a>,
        package_source: impl Into<Cow<'a, Path>>,
    ) -> Self {
        Self {
            mbti_input: mbti_input.into(),
            mi_output: mi_output.into(),
            mi_deps,
            package_name,
            package_source: package_source.into(),
            json_errors: true,
        }
    }
}

impl CmdlineAbstraction for MooncBuildInterface<'_> {
    fn to_args(&self, args: &mut Vec<String>) {
        args.push("build-interface".into());

        // Input and output paths
        args.push(dunce::simplified(&self.mbti_input).display().to_string());
        args.extend([
            "-o".to_string(),
            dunce::simplified(&self.mi_output).display().to_string(),
        ]);

        // Interface dependencies
        for dep in self.mi_deps {
            args.extend(["-i".to_string(), dep.to_alias_arg()]);
        }

        // Package metadata
        args.extend(["-pkg".to_string(), self.package_name.to_string()]);
        args.extend([
            "-pkg-sources".to_string(),
            format!(
                "{}:{}",
                self.package_name,
                dunce::simplified(&self.package_source).display()
            ),
        ]);

        // Virtual packages always require this flag
        args.push("-virtual".to_string());

        if self.json_errors {
            args.extend(["-error-format".to_string(), "json".to_string()]);
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::{
        build_lower::{LoweredCommandExecution, moonc_command},
        model::TargetKind,
    };

    #[test]
    fn windows_file_arguments_are_normalized_before_transport() {
        let fqn = crate::pkg_name::PackageFQN::new(
            "test/paths@0.1.0".parse().expect("valid module name"),
            "".parse().expect("valid root package path"),
        );
        let source_root = Path::new(r"C:\Users\user'quote\project");
        let output = Path::new(r"\\?\C:\Users\user'quote\project\_build\abort.mi");
        let long_source = Path::new(r"\\?\C:\Users\user'quote\project")
            .join("segment\\".repeat(40))
            .join("pkg.mbti");
        let command = MooncBuildInterface::new(
            &long_source,
            output,
            &[],
            CompiledPackageName::new(&fqn, TargetKind::Source),
            source_root,
        );
        let args = command.build_command("moonc");
        let inline = moonc_command::lower(args.clone(), output).expect("valid inline command");
        assert!(matches!(
            inline.execution(),
            LoweredCommandExecution::Inline(_)
        ));
        let mut large = args.clone();
        large.extend(["-w".to_owned(), "a".repeat(16 * 1024)]);
        let response = moonc_command::lower(large, output).expect("valid response-file command");
        let LoweredCommandExecution::ResponseFile { command, file } = response.execution() else {
            panic!("expected response-file transport");
        };
        assert_eq!(
            file.content
                .lines()
                .take(args.len() - 1)
                .collect::<Vec<_>>(),
            args[1..]
        );
        // Snapshot Windows spelling and quoting directly. Cross-platform dry-run
        // normalization would hide the prefix behavior this test protects.
        // Omit only the synthetic padding used to force response-file transport.
        let response_body = file
            .content
            .lines()
            .take(args.len() - 1)
            .collect::<Vec<_>>()
            .join("\n");
        expect_test::expect_file!["snapshots/windows_build_interface.snap"].assert_eq(&format!(
            "inline:\n{}\n\nresponse command:\n{command}\n\nresponse arguments:\n{response_body}\n",
            inline.inline_command(),
        ));
        assert!(
            args[2].starts_with(r"\\?\C:\"),
            "long source: {:?}",
            args[2]
        );
        assert_eq!(args[4], r"C:\Users\user'quote\project\_build\abort.mi");
        assert_eq!(
            output.to_str().expect("UTF-8 fixture path"),
            r"\\?\C:\Users\user'quote\project\_build\abort.mi"
        );
        let transport = moonutil::shlex::split_native(command);
        assert_eq!(
            transport[2],
            r"C:\Users\user'quote\project\_build\abort.mi.rsp"
        );
    }
}
