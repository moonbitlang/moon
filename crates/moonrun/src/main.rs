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
use moonrun::{Engine, EngineConfig, RunOptions, RunOutcome};
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
    stack_size: Option<usize>,

    /// Experimental: sandbox wasm runtime host access using a JSON policy file.
    #[clap(
        long,
        value_name = "PATH",
        long_help = r#"Experimental: Sandbox wasm runtime host access using a JSON policy file. WASI is not covered.

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

Setting process.spawn to true grants child processes the host user's ambient filesystem, network, and process access; the other policy sections do not sandbox child processes. Scoped rules authorize the logical request, not the executable eventually selected through PATH or other OS lookup."#
    )]
    policy: Option<PathBuf>,

    /// Override the directory used to resolve relative policy paths.
    #[clap(long, value_name = "PATH", hide = true, requires = "policy")]
    policy_source_dir: Option<PathBuf>,
}

fn get_moonrun_version() -> String {
    format!(
        "{} ({} {})",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        std::env!("VERGEN_BUILD_DATE")
    )
}

fn main() -> anyhow::Result<()> {
    let inherited_policy = moonutil::policy_transport::PolicyTransfer::take_from_env()?
        .map(moonutil::policy_transport::PolicyTransfer::read)
        .transpose()?;
    let matches = Commandline::parse();
    let engine_config = match matches.stack_size {
        Some(stack_size) => EngineConfig::default().with_stack_size(stack_size),
        None => EngineConfig::default(),
    };
    let mut options = RunOptions::default().with_args(matches.args);
    if matches.no_stack_trace {
        options = options.without_stack_trace();
    }
    if let Some(test_args) = matches.test_args {
        options = options.with_test_args(test_args);
    }
    // An inherited policy is host-owned and cannot be replaced by a child CLI
    // argument. Each Run publishes its own canonical copy for future children.
    if let Some(policy) = inherited_policy {
        options = options.with_inherited_policy(policy);
    } else if let Some(policy) = matches.policy {
        options = match matches.policy_source_dir {
            Some(source_dir) => options.with_policy_file_source_dir(policy, source_dir),
            None => options.with_policy_file(policy),
        };
    }

    match Engine::new(engine_config).run_file(matches.path, options)? {
        RunOutcome::Completed => Ok(()),
        RunOutcome::Exited(code) => std::process::exit(code),
        RunOutcome::KilledBySignal(signal) => {
            terminate_process_by_signal(signal);
            Ok(())
        }
    }
}

#[cfg(unix)]
fn terminate_process_by_signal(signal: i32) {
    let mut signal_set = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    unsafe {
        libc::sigemptyset(&mut signal_set);
        libc::sigaddset(&mut signal_set, signal);
        libc::pthread_sigmask(libc::SIG_UNBLOCK, &signal_set, std::ptr::null_mut());
        libc::fflush(std::ptr::null_mut());
        libc::raise(signal);
    }
}

#[cfg(windows)]
fn terminate_process_by_signal(signal: i32) {
    const STATUS_CONTROL_C_EXIT: u32 = 0xC000_013A;
    let _ = signal;
    unsafe {
        windows_sys::Win32::System::Threading::ExitProcess(STATUS_CONTROL_C_EXIT);
    }
}
