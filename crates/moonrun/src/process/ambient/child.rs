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

//! A directly owned OS child. Control and final reaping share one lock so
//! a retained virtual process entry can never signal a recycled native PID.

use std::io;
use std::process::ExitStatus;
use std::sync::Mutex;

#[cfg(windows)]
use crate::resource::Resource;

/// WNOWAIT reserves an identity only while the OS retains an exited child.
/// Do not silently change the embedding application's SIGCHLD disposition.
#[cfg(unix)]
pub(in crate::process) fn ensure_waitable_children() -> io::Result<()> {
    let mut disposition = unsafe { std::mem::zeroed::<libc::sigaction>() };
    if unsafe { libc::sigaction(libc::SIGCHLD, std::ptr::null(), &mut disposition) } < 0 {
        return Err(io::Error::last_os_error());
    }
    if disposition.sa_sigaction == libc::SIG_IGN || disposition.sa_flags & libc::SA_NOCLDWAIT != 0 {
        return Err(io::Error::from_raw_os_error(libc::ENOTSUP));
    }
    Ok(())
}

pub(crate) struct NativeChild {
    pid: i32,
    #[cfg(windows)]
    handle: Resource,
    state: Mutex<State>,
}

struct State {
    active: bool,
    termination_requested: bool,
}

impl NativeChild {
    pub(crate) fn new(pid: i32, #[cfg(windows)] handle: Resource) -> Self {
        Self {
            pid,
            #[cfg(windows)]
            handle,
            state: Mutex::new(State {
                active: true,
                termination_requested: false,
            }),
        }
    }

    pub(crate) fn termination_requested(&self) -> bool {
        self.state.lock().unwrap().termination_requested
    }

    /// Forced termination affects this child only. Completion always comes
    /// from the OS wait, including when natural exit beat this request.
    pub(crate) fn kill(&self, exit_code: u32) -> io::Result<bool> {
        let mut state = self.state.lock().unwrap();
        if !state.active || state.termination_requested {
            return Ok(false);
        }
        #[cfg(unix)]
        {
            let _ = exit_code;
            if unsafe { libc::kill(self.pid, libc::SIGKILL) } < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
            use windows_sys::Win32::System::Threading::{TerminateProcess, WaitForSingleObject};
            let handle = self.raw_handle();
            if unsafe { TerminateProcess(handle, exit_code) } == 0 {
                let error = io::Error::last_os_error();
                // TerminateProcess returns access denied for an exited process.
                // Only the original handle can establish that it already ended.
                match unsafe { WaitForSingleObject(handle, 0) } {
                    WAIT_OBJECT_0 => return Ok(false),
                    WAIT_TIMEOUT => return Err(error),
                    _ => return Err(io::Error::last_os_error()),
                }
            }
        }
        state.termination_requested = true;
        Ok(true)
    }

    pub(crate) fn signal(&self, signal: i32) -> io::Result<()> {
        let state = self.state.lock().unwrap();
        if !state.active {
            // The virtual identity remains waitable after native reaping.
            return Ok(());
        }
        #[cfg(unix)]
        if unsafe { libc::kill(self.pid, signal) } < 0 {
            return Err(io::Error::last_os_error());
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, GenerateConsoleCtrlEvent};
            let _ = signal;
            // CreateProcess started a console group. Keeping its original
            // process HANDLE alive also pins the group leader's PID identity.
            if unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, self.pid as u32) } == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    /// Exactly one Engine reaper calls this. Never hold a control/table lock
    /// while waiting for a running child to finish.
    pub(crate) fn wait(&self) -> io::Result<ExitStatus> {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            let mut info = unsafe { std::mem::zeroed::<libc::siginfo_t>() };
            loop {
                let ret = unsafe {
                    libc::waitid(
                        libc::P_PID,
                        self.pid as libc::id_t,
                        &mut info,
                        libc::WEXITED | libc::WNOWAIT,
                    )
                };
                if ret == 0 {
                    break;
                }
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    // In particular, ECHILD means an external waiter stole our
                    // child: revoke PID control before reporting the violation.
                    self.state.lock().unwrap().active = false;
                    return Err(error);
                }
            }
            let mut state = self.state.lock().unwrap();
            state.active = false;
            loop {
                let mut status = 0;
                if unsafe { libc::waitpid(self.pid, &mut status, 0) } >= 0 {
                    return Ok(ExitStatus::from_raw(status));
                }
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
            use windows_sys::Win32::System::Threading::{
                GetExitCodeProcess, INFINITE, WaitForSingleObject,
            };
            if unsafe { WaitForSingleObject(self.raw_handle(), INFINITE) } != WAIT_OBJECT_0 {
                return Err(io::Error::last_os_error());
            }
            let mut state = self.state.lock().unwrap();
            state.active = false;
            let mut code = 0;
            if unsafe { GetExitCodeProcess(self.raw_handle(), &mut code) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(ExitStatus::from_raw(code))
        }
    }

    #[cfg(windows)]
    fn raw_handle(&self) -> windows_sys::Win32::Foundation::HANDLE {
        use std::os::windows::io::AsRawHandle;
        self.handle
            .as_handle()
            .expect("owned process HANDLE")
            .as_raw_handle()
    }
}
