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
use moonrun::{RunOptions, RunOutcome, Runtime, RuntimeConfig, SignalSender, signal_channel};
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

fn get_moonrun_version() -> String {
    format!(
        "{} ({} {})",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        std::env!("VERGEN_BUILD_DATE")
    )
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

    let (signal_sender, signal_receiver) = signal_channel();
    options = options.with_signal_receiver(signal_receiver);
    let process_signals = ProcessSignalAdapter::install(signal_sender)?;
    let outcome = Runtime::new(runtime_config).run_file(matches.path, options);
    drop(process_signals);

    match outcome? {
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
        libc::signal(signal, libc::SIG_DFL);
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

#[cfg(unix)]
struct ProcessSignalAdapter {
    handle: signal_hook::iterator::Handle,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(unix)]
impl ProcessSignalAdapter {
    fn install(sender: SignalSender) -> std::io::Result<Self> {
        let mut signals = signal_hook::iterator::Signals::new([
            signal_hook::consts::SIGINT,
            signal_hook::consts::SIGTERM,
            signal_hook::consts::SIGHUP,
        ])?;
        let handle = signals.handle();
        let thread = std::thread::Builder::new()
            .name("moonrun-process-signals".to_string())
            .spawn(move || {
                for signal in signals.forever() {
                    if sender.send(signal).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            handle,
            thread: Some(thread),
        })
    }
}

#[cfg(unix)]
impl Drop for ProcessSignalAdapter {
    fn drop(&mut self) {
        self.handle.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(windows)]
struct ProcessSignalAdapter;

#[cfg(windows)]
static PROCESS_SIGNAL_TARGET: std::sync::Mutex<Option<SignalSender>> = std::sync::Mutex::new(None);

#[cfg(windows)]
impl ProcessSignalAdapter {
    fn install(sender: SignalSender) -> std::io::Result<Self> {
        use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

        let mut target = PROCESS_SIGNAL_TARGET.lock().unwrap();
        if target.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "moonrun process signal adapter is already installed",
            ));
        }
        *target = Some(sender);
        if unsafe { SetConsoleCtrlHandler(Some(process_console_control_handler), 1) } == 0 {
            *target = None;
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self)
    }
}

#[cfg(windows)]
impl Drop for ProcessSignalAdapter {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

        unsafe {
            SetConsoleCtrlHandler(Some(process_console_control_handler), 0);
        }
        *PROCESS_SIGNAL_TARGET.lock().unwrap() = None;
    }
}

#[cfg(windows)]
unsafe extern "system" fn process_console_control_handler(control: u32) -> i32 {
    PROCESS_SIGNAL_TARGET
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|sender| sender.send(control as i32) == Ok(true))
        .into()
}
