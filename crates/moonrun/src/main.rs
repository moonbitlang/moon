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
use cli::{CliSession, HostSignalPolicy, ProcessSignals};
use moonrun::cli_support::MoonrunCli;
use moonrun::{Engine, EngineConfig, RunOptions, RunOutcome, signal_channel};

fn main() -> anyhow::Result<()> {
    let inherited_policy = moonutil::policy_transport::PolicyTransfer::take_from_env()?
        .map(moonutil::policy_transport::PolicyTransfer::read)
        .transpose()?;
    let matches = MoonrunCli::parse();
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
    if matches.daemon {
        options = options.with_ipc(true);
    }
    if matches.virtual_process {
        options = options.with_virtualize(true);
    }
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
    let session = if options.ipc() {
        Some(CliSession::new(HostSignalPolicy::ShutdownEngine))
    } else {
        options
            .virtualize()
            .then(|| CliSession::new(HostSignalPolicy::RootThenShutdown(signal_sender.clone())))
    };
    let _process_signals = if let Some(session) = &session {
        ProcessSignals::install_session(session.clone())?
    } else {
        ProcessSignals::install(signal_sender)?
    };
    let engine = Engine::new(engine_config);
    let outcome = if let Some(session) = session {
        session.attach_engine(&engine);
        // Keep admission errors inside the session: a signal received during
        // Engine initialization may already have closed its process namespace.
        let outcome = engine
            .spawn_file_with_signal_receiver(None, &matches.path, options, signal_receiver)
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
