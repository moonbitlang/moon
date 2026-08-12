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

use clap::Parser;
use moonrun::{RunOptions, Runtime, RuntimeConfig, apply_cli_outcome, get_moonrun_version};
use std::path::PathBuf;

#[derive(Debug, clap::Parser)]
#[command(version = get_moonrun_version())]
struct Commandline {
    /// The path of the file to run
    path: PathBuf,

    /// Additional arguments
    #[clap(allow_hyphen_values = true)]
    args: Vec<String>,

    /// Don't print stack trace
    #[clap(long)]
    no_stack_trace: bool,

    #[clap(long)]
    test_args: Option<String>,

    #[clap(long)]
    stack_size: Option<String>,

    /// Experimental: sandbox wasm runtime host access using a JSON policy file.
    #[clap(
        long,
        value_name = "PATH",
        long_help = r#"Experimental: Sandbox wasm runtime host access using a JSON policy file. WASI is not covered.

Supplying --policy enables deny-by-default mode: omitted or empty fs, net, and env objects deny that surface, and process spawning is disabled unless explicitly enabled.

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

Process spawning is disabled by default. Setting process.spawn to true grants child processes the host user's ambient filesystem, network, and process access; the other policy sections do not sandbox child processes."#
    )]
    policy: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let matches = Commandline::parse();
    let runtime_config = match matches.stack_size {
        Some(stack_size) => RuntimeConfig::default().with_stack_size(stack_size),
        None => RuntimeConfig::default(),
    };
    let mut options = RunOptions::default().with_args(matches.args);
    if matches.no_stack_trace {
        options = options.without_stack_trace();
    }
    if let Some(test_args) = matches.test_args {
        options = options.with_test_args(test_args);
    }
    if let Some(policy) = matches.policy {
        options = options.with_policy_file(policy);
    }

    let outcome = Runtime::new(runtime_config).run_file(matches.path, options)?;
    apply_cli_outcome(outcome)
}
