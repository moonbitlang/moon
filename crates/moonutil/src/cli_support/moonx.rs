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

//! The canonical moonx grammar and opaque program-argument boundary.

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum MoonxTarget {
    #[default]
    Wasm,
}

#[derive(Debug, Parser)]
#[command(
    name = "moonx",
    about = "Run a .mbtx file or a package from the Mooncakes registry",
    long_about = r#"Run a standalone .mbtx file or a package from the Mooncakes registry without installing it.

Accepted input forms:
  moonx script.mbtx
  moonx user/module/package
  moonx user/module/package@1.2.3
  moonx user/module/package@latest

Standalone .mbtx files always use the linear-memory Wasm backend. Pinned
package coordinates use the requested version directly. `@latest` refreshes the
registry index before resolving the latest version. Unpinned coordinates use
the latest version already known to the local registry index."#,
    override_usage = "moonx [OPTIONS] <MBTX|PACKAGE> [PROGRAM_ARGS]...",
    version
)]
pub struct MoonxCli {
    #[arg(long, value_enum, default_value_t)]
    pub target: MoonxTarget,

    /// Experimental moonrun policy file
    #[arg(long = "experimental-policy", value_name = "PATH")]
    pub experimental_policy: Option<PathBuf>,

    /// Run Wasm descendants in the same virtual process tree (experimental)
    #[arg(long)]
    pub virtual_process_tree: bool,

    /// Show progress and execution details
    #[arg(short = 'v', long)]
    pub verbose: bool,

    #[command(subcommand)]
    input: MoonxInput,
}

#[derive(Debug, Subcommand)]
enum MoonxInput {
    #[command(external_subcommand)]
    External(Vec<String>),
}

impl MoonxCli {
    /// Input and opaque program arguments, with one optional separator removed.
    pub fn input_and_args(&self) -> (&str, &[String]) {
        let MoonxInput::External(input_and_args) = &self.input;
        let (input, args) = input_and_args
            .split_first()
            .expect("external subcommand contains its name");
        let args = match args {
            [separator, args @ ..] if separator == "--" => args,
            _ => args,
        };
        (input, args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn rejects_removed_options_and_requires_input() {
        assert_eq!(
            MoonxCli::try_parse_from(["moonx", "--quiet", "user/module"])
                .unwrap_err()
                .kind(),
            ErrorKind::UnknownArgument
        );
        assert_eq!(
            MoonxCli::try_parse_from(["moonx"]).unwrap_err().kind(),
            ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        );
    }

    #[test]
    fn program_tail_is_opaque_and_removes_only_one_separator() {
        for (args, expected) in [
            (
                vec!["moonx", "script.mbtx", "--help", "--target", "native"],
                vec!["--help", "--target", "native"],
            ),
            (
                vec!["moonx", "--verbose", "script.mbtx", "--", "--version"],
                vec!["--version"],
            ),
            (
                vec!["moonx", "script.mbtx", "--", "--", "-v"],
                vec!["--", "-v"],
            ),
        ] {
            let parsed = MoonxCli::try_parse_from(args).unwrap();
            let (input, tail) = parsed.input_and_args();
            assert_eq!(input, "script.mbtx");
            assert_eq!(tail, expected);
        }
    }

    #[test]
    fn frontend_help_and_version_stop_before_program_arguments() {
        for (arg, kind) in [
            ("--help", ErrorKind::DisplayHelp),
            ("-h", ErrorKind::DisplayHelp),
            ("--version", ErrorKind::DisplayVersion),
            ("-V", ErrorKind::DisplayVersion),
        ] {
            assert_eq!(
                MoonxCli::try_parse_from(["moonx", arg]).unwrap_err().kind(),
                kind
            );
            let parsed = MoonxCli::try_parse_from(["moonx", "user/module", arg]).unwrap();
            assert_eq!(
                parsed.input_and_args(),
                ("user/module", [arg.to_owned()].as_slice())
            );
        }
    }
}
