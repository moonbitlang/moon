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

use moonrun::SignalSender;

#[cfg(unix)]
pub(crate) struct ProcessSignals;

#[cfg(unix)]
const MANAGED_SIGNALS: [i32; 3] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];

#[cfg(unix)]
impl ProcessSignals {
    pub(crate) fn install(sender: SignalSender) -> std::io::Result<Self> {
        let signals = managed_signal_set()?;
        let mut old_mask = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        check_pthread_call(unsafe {
            libc::pthread_sigmask(libc::SIG_BLOCK, &signals, &mut old_mask)
        })?;
        // macOS represents pthread_t as a raw pointer, which is not Send.
        // Its value may still be passed to pthread_kill from another thread.
        let installing_thread = unsafe { libc::pthread_self() } as usize;

        let mut inherited_blocked_signals = 0_u32;
        for signal in MANAGED_SIGNALS {
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
                    if sender.send(signal) != Ok(true) {
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
static PROCESS_SIGNAL_TARGET: std::sync::Mutex<Option<SignalSender>> = std::sync::Mutex::new(None);

#[cfg(windows)]
impl ProcessSignals {
    pub(crate) fn install(sender: SignalSender) -> std::io::Result<Self> {
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
        *target = Some(sender);
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
        .is_some_and(|sender| sender.send(control as i32) == Ok(true))
        .into()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    use std::time::Duration;

    const BLOCKED_SIGNAL_CHILD: &str = "MOONRUN_TEST_BLOCKED_SIGNAL_CHILD";
    const BLOCKED_SIGNAL_PRESERVED: &str = "blocked signal remained pending";
    const TEST_NAME: &str =
        "cli::process_signals::tests::inherited_blocked_signal_remains_pending_after_fallback";

    #[test]
    fn inherited_blocked_signal_remains_pending_after_fallback() {
        if std::env::var_os(BLOCKED_SIGNAL_CHILD).is_some() {
            let (sender, _receiver) = moonrun::signal_channel();
            let _process_signals = ProcessSignals::install(sender).unwrap();
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

        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args(["--exact", TEST_NAME, "--nocapture"])
            .env(BLOCKED_SIGNAL_CHILD, "1");
        unsafe {
            child.pre_exec(|| {
                let mut blocked = std::mem::zeroed::<libc::sigset_t>();
                if libc::sigemptyset(&mut blocked) != 0
                    || libc::sigaddset(&mut blocked, libc::SIGINT) != 0
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

        let output = child.output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains(BLOCKED_SIGNAL_PRESERVED),
            "blocked-signal child failed with {:?}:\nstdout:\n{}\nstderr:\n{}",
            output.status,
            stdout,
            String::from_utf8_lossy(&output.stderr),
        );
    }
}
