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

use moonrun::SignalSender;

use super::session::CliSession;

enum SignalTarget {
    Run(SignalSender),
    Session(CliSession),
    Remote(moonrun::ipc::ClientRunControl),
}

impl SignalTarget {
    fn send(&self, signal: i32) -> bool {
        #[cfg(windows)]
        if matches!(self, Self::Remote(..)) && ![0, 1].contains(&signal) {
            // Only C/BREAK permit asynchronous cleanup after this handler
            // returns. CLOSE/LOGOFF/SHUTDOWN retain native forced-loss behavior.
            return false;
        }
        #[cfg(unix)]
        if signal == libc::SIGWINCH {
            // A resize only reaches a Run that listens for it, and never
            // terminates anything. IPC does not carry it to a remote Run.
            return match self {
                Self::Run(sender) => sender.send(signal) == Ok(true),
                Self::Session(session) => session.send_resize(),
                Self::Remote(..) => false,
            };
        }
        match self {
            Self::Run(sender) => sender.send(signal) == Ok(true),
            Self::Session(session) => session.send(signal),
            Self::Remote(remote) => remote.signal(signal).is_ok(),
        }
    }
}

impl ProcessSignals {
    pub(crate) fn install(sender: SignalSender) -> std::io::Result<Self> {
        Self::install_target(SignalTarget::Run(sender))
    }

    pub(crate) fn install_remote(control: moonrun::ipc::ClientRunControl) -> std::io::Result<Self> {
        Self::install_target(SignalTarget::Remote(control))
    }

    pub(crate) fn install_session(session: CliSession) -> std::io::Result<Self> {
        Self::install_target(SignalTarget::Session(session))
    }
}

#[cfg(unix)]
pub(crate) struct ProcessSignals;

#[cfg(unix)]
// `SIGWINCH` reports terminal resizes to a Run that listens for it.
const MANAGED_SIGNALS: [i32; 4] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGWINCH];

#[cfg(unix)]
impl ProcessSignals {
    fn install_target(target: SignalTarget) -> std::io::Result<Self> {
        install_resize_handler()?;
        let signals = managed_signal_set()?;
        let mut old_mask = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        check_pthread_call(unsafe {
            libc::pthread_sigmask(libc::SIG_BLOCK, &signals, &mut old_mask)
        })?;
        // macOS represents pthread_t as a raw pointer, which is not Send.
        // Its value may still be passed to pthread_kill from another thread.
        let installing_thread = unsafe { libc::pthread_self() } as usize;

        let mut inherited_blocked_signals = 0_u32;
        // An unaccepted `SIGWINCH` is dropped instead, see below.
        for signal in MANAGED_SIGNALS.into_iter().filter(|&s| s != libc::SIGWINCH) {
            let blocked = unsafe { libc::sigismember(&old_mask, signal) };
            if blocked < 0 {
                let error = std::io::Error::last_os_error();
                unsafe {
                    libc::pthread_sigmask(libc::SIG_SETMASK, &old_mask, std::ptr::null_mut());
                }
                return Err(error);
            }
            if blocked != 0 {
                inherited_blocked_signals |= 1_u32 << signal;
            }
        }

        let thread = std::thread::Builder::new()
            .name("moonrun-process-signals".to_owned())
            .spawn(move || {
                let mut signals = signals;
                loop {
                    let mut signal = 0;
                    if unsafe { libc::sigwait(&signals, &mut signal) } != 0 {
                        std::process::abort();
                    }
                    if !target.send(signal) {
                        if signal == libc::SIGWINCH {
                            // Ignoring it is its default action, or the no-op
                            // handler's. Keep waiting for it, so a Run that
                            // opens the terminal later still sees resizes.
                            continue;
                        }
                        if inherited_blocked_signals & (1_u32 << signal) != 0 {
                            unsafe {
                                // Return ownership to the inherited thread
                                // mask. Stop consuming this signal before
                                // re-queueing it on the thread whose mask we
                                // inherited, so it remains pending there.
                                if libc::sigdelset(&mut signals, signal) != 0
                                    || libc::pthread_kill(
                                        installing_thread as libc::pthread_t,
                                        signal,
                                    ) != 0
                                {
                                    libc::_exit(128 + signal);
                                }
                            }
                        } else {
                            apply_process_signal(signal);
                        }
                    }
                }
            });
        if let Err(error) = thread {
            unsafe {
                libc::pthread_sigmask(libc::SIG_SETMASK, &old_mask, std::ptr::null_mut());
            }
            return Err(error);
        }
        Ok(Self)
    }
}

#[cfg(unix)]
fn managed_signal_set() -> std::io::Result<libc::sigset_t> {
    let mut signals = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    check_signal_call(unsafe { libc::sigemptyset(&mut signals) })?;
    for signal in MANAGED_SIGNALS {
        check_signal_call(unsafe { libc::sigaddset(&mut signals, signal) })?;
    }
    Ok(signals)
}

#[cfg(unix)]
fn apply_process_signal(signal: i32) {
    let mut signal_set = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    unsafe {
        if libc::sigemptyset(&mut signal_set) != 0
            || libc::sigaddset(&mut signal_set, signal) != 0
            || libc::pthread_sigmask(libc::SIG_UNBLOCK, &signal_set, std::ptr::null_mut()) != 0
            || libc::raise(signal) != 0
        {
            libc::_exit(128 + signal);
        }

        // An ignored signal or a returning inherited handler leaves the
        // broker alive. Re-block it before waiting again so future deliveries
        // cannot escape to an arbitrary process thread.
        if libc::pthread_sigmask(libc::SIG_BLOCK, &signal_set, std::ptr::null_mut()) != 0 {
            libc::_exit(128 + signal);
        }
    }
}

// On macOS an ignored signal, such as `SIGWINCH` by default or one set to
// `SIG_IGN` by the parent process, is discarded when generated, even while
// blocked, unless it has a handler. The handler never runs:
// the signal stays blocked and is taken by `sigwait`. Exec resets it for
// children.
#[cfg(unix)]
fn install_resize_handler() -> std::io::Result<()> {
    extern "C" fn resize_handler(_signal: libc::c_int) {}

    let mut old = unsafe { std::mem::zeroed::<libc::sigaction>() };
    check_signal_call(unsafe { libc::sigaction(libc::SIGWINCH, std::ptr::null(), &mut old) })?;
    if old.sa_sigaction != libc::SIG_DFL && old.sa_sigaction != libc::SIG_IGN {
        return Ok(());
    }
    let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
    action.sa_sigaction = resize_handler as extern "C" fn(libc::c_int) as libc::sighandler_t;
    check_signal_call(unsafe { libc::sigemptyset(&mut action.sa_mask) })?;
    check_signal_call(unsafe { libc::sigaction(libc::SIGWINCH, &action, std::ptr::null_mut()) })
}

#[cfg(unix)]
fn check_signal_call(result: i32) -> std::io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn check_pthread_call(result: i32) -> std::io::Result<()> {
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(result))
    }
}

#[cfg(windows)]
pub(crate) struct ProcessSignals;

#[cfg(windows)]
static PROCESS_SIGNAL_TARGET: std::sync::Mutex<Option<SignalTarget>> = std::sync::Mutex::new(None);

#[cfg(windows)]
impl ProcessSignals {
    fn install_target(signal_target: SignalTarget) -> std::io::Result<Self> {
        use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;

        let mut target = PROCESS_SIGNAL_TARGET
            .lock()
            .map_err(|_| std::io::Error::other("moonrun process signal adapter is poisoned"))?;
        if target.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "moonrun process signal adapter is already installed",
            ));
        }
        *target = Some(signal_target);
        if unsafe { SetConsoleCtrlHandler(Some(console_control_handler), 1) } == 0 {
            *target = None;
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self)
    }
}

#[cfg(windows)]
unsafe extern "system" fn console_control_handler(control: u32) -> i32 {
    let Ok(target) = PROCESS_SIGNAL_TARGET.lock() else {
        return 0;
    };
    target
        .as_ref()
        .is_some_and(|target| target.send(control as i32))
        .into()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::cli::HostSignalPolicy;
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    use std::time::Duration;

    const BLOCKED_SIGNAL_CHILD: &str = "MOONRUN_TEST_BLOCKED_SIGNAL_CHILD";
    const BLOCKED_SIGNAL_PRESERVED: &str = "blocked signal remained pending";
    const TEST_NAME: &str =
        "cli::process_signals::tests::inherited_blocked_signal_remains_pending_after_fallback";

    const RESIZE_CHILD: &str = "MOONRUN_TEST_RESIZE_SIGNAL_CHILD";
    const RESIZE_DROPPED: &str = "unaccepted resize signal was dropped";
    const RESIZE_TEST_NAME: &str =
        "cli::process_signals::tests::unaccepted_resize_signal_is_dropped_by_the_broker";

    #[test]
    fn unaccepted_resize_signal_is_dropped_by_the_broker() {
        if let Some(mode) = std::env::var_os(RESIZE_CHILD) {
            let (sender, _receiver) = moonrun::signal_channel();
            let session = match mode.to_str().unwrap() {
                "run" => None,
                "virtual-session" => Some(CliSession::new(HostSignalPolicy::RootThenShutdown(
                    sender.clone(),
                ))),
                "ipc-session" => Some(CliSession::new(HostSignalPolicy::ShutdownEngine)),
                mode => panic!("unknown mode {mode}"),
            };
            let _process_signals = match &session {
                Some(session) => ProcessSignals::install_session(session.clone()).unwrap(),
                None => ProcessSignals::install(sender).unwrap(),
            };
            let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
            assert_eq!(
                unsafe { libc::sigaction(libc::SIGWINCH, std::ptr::null(), &mut action) },
                0
            );
            assert_ne!(action.sa_sigaction, libc::SIG_DFL);
            assert_eq!(unsafe { libc::kill(libc::getpid(), libc::SIGWINCH) }, 0);
            std::thread::sleep(Duration::from_millis(500));

            // Even though SIGWINCH was blocked on entry, the broker consumed
            // it rather than handing it back, so it keeps waiting for resizes.
            let mut pending = unsafe { std::mem::zeroed::<libc::sigset_t>() };
            assert_eq!(unsafe { libc::sigpending(&mut pending) }, 0);
            assert_eq!(unsafe { libc::sigismember(&pending, libc::SIGWINCH) }, 0);
            // A resize nobody accepts must not shut down the session.
            if let Some(session) = session {
                assert_eq!(
                    session
                        .finish::<()>(Ok(moonrun::RunOutcome::Completed))
                        .unwrap(),
                    moonrun::RunOutcome::Completed
                );
            }
            println!("{RESIZE_DROPPED}");
            return;
        }

        for mode in ["run", "virtual-session", "ipc-session"] {
            let output = blocked_signal_child(RESIZE_TEST_NAME, RESIZE_CHILD, mode, libc::SIGWINCH);
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.contains(RESIZE_DROPPED),
                "resize-signal {mode} child failed with {:?}:\nstdout:\n{}\nstderr:\n{}",
                output.status,
                stdout,
                String::from_utf8_lossy(&output.stderr),
            );
        }
    }

    /// Run `test` in a child process with `mode` in `env` and `signal`
    /// blocked on entry.
    fn blocked_signal_child(
        test: &str,
        env: &str,
        mode: &str,
        signal: i32,
    ) -> std::process::Output {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child.args(["--exact", test, "--nocapture"]).env(env, mode);
        unsafe {
            child.pre_exec(move || {
                let mut blocked = std::mem::zeroed::<libc::sigset_t>();
                if libc::sigemptyset(&mut blocked) != 0
                    || libc::sigaddset(&mut blocked, signal) != 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                let error = libc::pthread_sigmask(libc::SIG_BLOCK, &blocked, std::ptr::null_mut());
                if error == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::from_raw_os_error(error))
                }
            });
        }
        child.output().unwrap()
    }

    #[test]
    fn inherited_blocked_signal_remains_pending_after_fallback() {
        if let Some(mode) = std::env::var_os(BLOCKED_SIGNAL_CHILD) {
            let (sender, _receiver) = moonrun::signal_channel();
            let _process_signals = if mode == "finished-session" {
                let session = CliSession::new(HostSignalPolicy::RootThenShutdown(sender));
                session
                    .finish::<()>(Ok(moonrun::RunOutcome::Completed))
                    .unwrap();
                ProcessSignals::install_session(session).unwrap()
            } else {
                ProcessSignals::install(sender).unwrap()
            };
            assert_eq!(unsafe { libc::kill(libc::getpid(), libc::SIGINT) }, 0);

            // The broker thread is already running. Give the old implementation
            // enough time to consume the signal and incorrectly deliver it.
            std::thread::sleep(Duration::from_secs(1));

            let mut pending = unsafe { std::mem::zeroed::<libc::sigset_t>() };
            assert_eq!(unsafe { libc::sigpending(&mut pending) }, 0);
            assert_eq!(unsafe { libc::sigismember(&pending, libc::SIGINT) }, 1);
            println!("{BLOCKED_SIGNAL_PRESERVED}");
            return;
        }

        for mode in ["run", "finished-session"] {
            let output = blocked_signal_child(TEST_NAME, BLOCKED_SIGNAL_CHILD, mode, libc::SIGINT);
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.contains(BLOCKED_SIGNAL_PRESERVED),
                "blocked-signal {mode} child failed with {:?}:\nstdout:\n{}\nstderr:\n{}",
                output.status,
                stdout,
                String::from_utf8_lossy(&output.stderr),
            );
        }
    }
}
