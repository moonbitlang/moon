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

//! Canonical moonrun command-line grammar, shared by executable and library frontends.
//!
//! Parsing only interprets arguments; it does not load policies or Wasm, observe
//! inherited discovery, initialize an Engine, or install process signal handlers.
//! The executable uses `Parser::parse`; embedding frontends can use
//! `Parser::try_parse_from` and handle help/errors without exiting the host.

use std::path::PathBuf;

#[derive(Debug, clap::Parser)]
#[command(name = "moonrun", version = get_moonrun_version())]
pub struct MoonrunCli {
    /// The path of the file to run
    pub path: PathBuf,

    /// Additional arguments
    #[clap(allow_hyphen_values = true)]
    pub args: Vec<String>,

    /// Don't print stack trace
    #[clap(long)]
    pub no_stack_trace: bool,

    #[clap(long)]
    pub test_args: Option<String>,

    #[clap(long)]
    pub stack_size: Option<usize>,

    /// Experimental: sandbox wasm runtime host access using a JSON, TOML, or .mbtx policy file.
    #[clap(
        long,
        value_name = "PATH",
        long_help = r#"Experimental: Sandbox wasm runtime host access using a JSON, TOML, or .mbtx policy file.

Supplying --policy enables deny-by-default mode: omitted or empty fs, net, and env objects deny that surface, and process spawning is disabled unless explicitly allowed.

Common allow-all policy:
  {
    "env": { "from_host": ["*"] },
    "fs": {
      "read": ["*"],
      "write": ["*"]
    },
    "net": {
      "dns": ["*"],
      "connect": ["*:*"],
      "bind": ["*:*"]
    },
    "process": { "spawn": true }
  }

Filesystem roots are host paths. Relative roots are resolved relative to the policy file. "*" allows every host path on every platform.

Environment values default to empty in sandbox policy mode. Use env.from_host for optional host variables, env.required_from_host for required host variables and secrets, and env.set for non-secret literals. env.set overrides copied host values.

Network connect controls outbound sockets; bind controls local bind/listen addresses. Hostname connect rules also permit DNS lookup for those hostnames, so net.connect containing "api.deepseek.com:443" does not require a separate dns entry. Bind rules must use IP addresses or *.

Process spawning is disabled by default. process.allow entries match the exact requested program and, when args_prefix is present, a prefix of complete argument tokens. Omitting args_prefix allows any arguments for that program. Multiple entries are alternatives. process.spawn and process.allow cannot be used together.

process.virtualize defaults to false. Set it to true for Engine-managed identities, owned environment/cwd and internal moonx execution (V8 only). Other authorized commands run as supervised native children with their OS PIDs. The same spawn/allow rules authorize every guest spawn in both modes. For a moonx-only virtual tree use process.virtualize=true with process.allow=[{"program":"moonx"}].

process.ipc=true enables a V8 execution session for native descendants. A descendant moonrun connects to this Engine before initializing its own runtime. Each Run keeps its own cwd/env/stdio and inherits its parent's permission ceiling. IPC currently requires process.virtualize=false. With no inherited session and ipc disabled, execution stays local.

Setting process.spawn to true grants child processes the host user's ambient filesystem, network, and process access; the other policy sections do not sandbox child processes. Scoped rules authorize the logical request, not the executable eventually selected through PATH or other OS lookup."#
    )]
    pub policy: Option<PathBuf>,

    /// Override the directory used to resolve relative policy paths.
    #[clap(long, value_name = "PATH", hide = true, requires = "policy")]
    pub policy_source_dir: Option<PathBuf>,

    /// Share this execution host with native descendants (V8 only; no persistent daemon).
    #[clap(long, conflicts_with = "virtual_process")]
    pub daemon: bool,

    /// Use virtual process identities and internal moonx execution (V8 only).
    #[clap(long, conflicts_with = "daemon")]
    pub virtual_process: bool,
}

fn get_moonrun_version() -> String {
    format!(
        "{} ({} {})",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        std::env!("VERGEN_BUILD_DATE")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, error::ErrorKind};

    #[test]
    fn parsing_keeps_execution_inputs_unresolved() {
        let cli = MoonrunCli::try_parse_from([
            "moonrun",
            "--policy",
            "missing-policy.toml",
            "--policy-source-dir",
            "missing-dir",
            "--stack-size",
            "4096",
            "--test-args",
            "test-driver input",
            "missing.wasm",
            "--no-stack-trace",
        ])
        .unwrap();
        assert_eq!(cli.path, PathBuf::from("missing.wasm"));
        assert_eq!(cli.policy, Some(PathBuf::from("missing-policy.toml")));
        assert_eq!(cli.policy_source_dir, Some(PathBuf::from("missing-dir")));
        assert_eq!(cli.stack_size, Some(4096));
        assert_eq!(cli.test_args.as_deref(), Some("test-driver input"));
        assert!(cli.no_stack_trace);
        assert!(!cli.daemon && !cli.virtual_process);
    }

    #[test]
    fn execution_flags_preserve_the_program_argument_boundary() {
        for flag in ["--daemon", "--virtual-process"] {
            let cli = MoonrunCli::try_parse_from([
                "moonrun",
                flag,
                "program.wasm",
                "--",
                "--daemon",
                "--virtual-process",
                "--help",
            ])
            .unwrap();
            assert_eq!(cli.daemon, flag == "--daemon");
            assert_eq!(cli.virtual_process, flag == "--virtual-process");
            assert_eq!(cli.args, ["--daemon", "--virtual-process", "--help"]);
        }
    }

    #[test]
    fn frontend_responses_and_invalid_combinations_return_errors() {
        for (args, kind) in [
            (vec!["moonrun", "--help"], ErrorKind::DisplayHelp),
            (vec!["moonrun", "--version"], ErrorKind::DisplayVersion),
            (
                vec!["moonrun", "--daemon", "--virtual-process", "program.wasm"],
                ErrorKind::ArgumentConflict,
            ),
            (
                vec!["moonrun", "--virtual-process", "--daemon", "program.wasm"],
                ErrorKind::ArgumentConflict,
            ),
            (
                vec!["moonrun", "--policy-source-dir", "dir", "program.wasm"],
                ErrorKind::MissingRequiredArgument,
            ),
            (
                vec!["moonrun", "--ipc-session", "program.wasm"],
                ErrorKind::UnknownArgument,
            ),
            (
                vec!["moonrun", "--virtual-process-tree", "program.wasm"],
                ErrorKind::UnknownArgument,
            ),
        ] {
            assert_eq!(
                MoonrunCli::try_parse_from(&args).unwrap_err().kind(),
                kind,
                "{args:?}"
            );
        }
    }
}
