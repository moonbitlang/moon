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

mod cli;

use clap::Parser;
use cli::{ProcessSignals, Session};
use moonrun::{Engine, EngineConfig, RunOptions, RunOutcome, signal_channel};
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

process.virtualize defaults to false. Set it to true for Engine-managed identities, owned environment/cwd and internal moonx execution (V8 only). The same spawn/allow rules authorize every guest spawn in both modes. For a moonx-only virtual tree use process.virtualize=true with process.allow=[{"program":"moonx"}].

process.ipc=true enables a Unix/V8 execution session for native descendants. A descendant moonrun connects to this Engine before initializing its own runtime. Each Run keeps its own cwd/env/stdio and inherits its parent's permission ceiling. IPC currently requires process.virtualize=false. With no inherited session and ipc disabled, execution stays local.

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
    // Read the selected configuration before choosing the CLI signal owner.
    // Its restrictions intersect the separately inherited authority ceiling.
    if let Some(authority) = inherited_policy {
        options = options.with_inherited_policy(authority);
    }
    if let Some(policy) = matches.policy {
        options =
            options.with_policy_file_source_dir(policy, matches.policy_source_dir.as_deref())?;
    }
    #[cfg(unix)]
    if let Some(endpoint) = std::env::var_os(moonrun::ipc::ENDPOINT_ENV) {
        // Decide placement before constructing an Engine or initializing V8.
        let run =
            moonrun::ipc::ClientRun::connect(endpoint, &matches.path, options, engine_config)?;
        let _signals = ProcessSignals::install_remote(run.control())?;
        return present_outcome(run.wait()?);
    }
    anyhow::ensure!(
        !(options.ipc() && options.virtualize()),
        "IPC virtual process sessions are not supported yet"
    );
    #[cfg(unix)]
    {
        options = options.preserve_current_signal_mask_for_children()?;
    }

    let (signal_sender, signal_receiver) = signal_channel();
    let session = options
        .virtualize()
        .then(|| Session::new(signal_sender.clone()));
    let _process_signals = if let Some(session) = &session {
        ProcessSignals::install_session(session.clone())?
    } else if options.ipc() {
        let (terminator, receiver) = moonrun::termination_channel();
        options = options.with_termination_receiver(receiver);
        ProcessSignals::install_controlled(signal_sender, terminator)?
    } else {
        ProcessSignals::install(signal_sender)?
    };
    let engine = Engine::new(engine_config);
    let outcome = if let Some(session) = session {
        session.attach_engine(&engine);
        // Keep admission errors inside the session: a signal received during
        // Engine initialization may already have closed its process namespace.
        let outcome = engine
            .spawn_executable_with_signal_receiver(
                None,
                &matches.path,
                moonutil::run_handoff::ExecutionKind::Wasm,
                options,
                signal_receiver,
            )
            .and_then(|root| engine.wait(root));
        // The CLI owns the entire tree. An exiting parent leaves children
        // with this supervisor; only claim adopted roots so live parents keep
        // their own guest waits. Descendant status does not replace root status.
        while let Some(orphan) = engine
            .snapshot()
            .into_iter()
            .find(|process| process.parent.is_none())
        {
            let _ = engine.wait(orphan.pid);
        }
        session.finish(outcome).map_err(Into::into)
    } else {
        engine.run_file_with_signal_receiver(matches.path, options, signal_receiver)
    };

    present_outcome(outcome?)
}

fn present_outcome(outcome: RunOutcome) -> anyhow::Result<()> {
    match outcome {
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
