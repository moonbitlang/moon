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

//! Ownership of a supervised OS child. Reaping retires control under the same
//! lock as signal delivery, so an unread completion cannot signal a reused PID.

use std::io;
use std::sync::{Arc, Mutex};

use super::super::{NativeSpawn, os};
use crate::async_host::AsyncHostResult;

pub(crate) struct NativeChild {
    pid: i32,
    control: NativeControl,
}

#[derive(Clone)]
pub(crate) struct NativeControl(Arc<Mutex<State>>);

struct State {
    child: Option<Arc<os::Spawned>>,
    terminating: bool,
}

impl NativeChild {
    pub(crate) fn spawn(spawn: NativeSpawn) -> AsyncHostResult<Self> {
        let child = spawn.spawn()?;
        Ok(Self {
            #[cfg(unix)]
            pid: child.pid,
            #[cfg(windows)]
            pid: child.pid as i32,
            control: NativeControl(Arc::new(Mutex::new(State {
                child: Some(Arc::new(child)),
                terminating: false,
            }))),
        })
    }

    pub(crate) fn pid(&self) -> i32 {
        self.pid
    }

    pub(crate) fn control(&self) -> NativeControl {
        self.control.clone()
    }

    pub(crate) fn wait(self) -> io::Result<std::process::ExitStatus> {
        self.wait_and_reap()
    }

    fn wait_and_reap(&self) -> io::Result<std::process::ExitStatus> {
        #[cfg(unix)]
        {
            // WNOWAIT pins the PID until control can be retired atomically with
            // the final reap. Exit observation leaves the control lock available.
            let status = loop {
                match os::wait_pid(self.pid, true) {
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    result => break result,
                }
            };
            let mut state = self.control.0.lock().unwrap();
            if status
                .as_ref()
                .is_err_and(|error| error.raw_os_error() == Some(libc::ECHILD))
            {
                // External reaping violates ownership; retire control before
                // cleanup can accidentally signal an unpinned identity.
                state.child = None;
            }
            status?;
            let reaped = loop {
                match os::wait_pid(self.pid, false) {
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    result => break result,
                }
            };
            if reaped.is_ok()
                || reaped
                    .as_ref()
                    .is_err_and(|error| error.raw_os_error() == Some(libc::ECHILD))
            {
                state.child = None;
            }
            reaped
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let child = self
                .control
                .0
                .lock()
                .unwrap()
                .child
                .as_ref()
                .unwrap()
                .clone();
            let status = os::wait_handle(child.handle.as_raw_handle())?;
            self.control.0.lock().unwrap().child = None;
            Ok(status)
        }
    }
}

impl Drop for NativeChild {
    fn drop(&mut self) {
        if self.control.0.lock().unwrap().child.is_some() {
            // Failed admission, failed thread creation, and unwind must not
            // leave an unowned child running or unreaped.
            let _ = self.control.kill();
            #[cfg(unix)]
            {
                // Termination is already requested, so cleanup can wait under
                // the control lock without allocating an exit observer.
                let mut state = self.control.0.lock().unwrap();
                loop {
                    match os::wait_pid(self.pid, false) {
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        _ => break,
                    }
                }
                state.child = None;
            }
            #[cfg(windows)]
            let _ = self.wait_and_reap();
        }
    }
}

impl NativeControl {
    pub(crate) fn is_terminating(&self) -> bool {
        self.0.lock().unwrap().terminating
    }

    pub(crate) fn signal(&self, signal: i32) -> io::Result<()> {
        let state = self.0.lock().unwrap();
        let Some(child) = &state.child else {
            return Ok(());
        };
        #[cfg(unix)]
        return os::send_signal(child.pid, signal);
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            if os::try_wait_handle(child.handle.as_raw_handle())?.is_some() {
                return Ok(());
            }
            os::send_console_event(child.pid, signal as u32)
        }
    }

    pub(crate) fn kill(&self) -> io::Result<bool> {
        let mut state = self.0.lock().unwrap();
        let Some(child) = &state.child else {
            return Ok(false);
        };
        if state.terminating {
            return Ok(false);
        }
        #[cfg(unix)]
        os::send_signal(child.pid, libc::SIGKILL)?;
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let handle = child.handle.as_raw_handle();
            if os::try_wait_handle(handle)?.is_some() {
                return Ok(false);
            }
            if let Err(error) = os::terminate_handle(handle, 1) {
                if os::try_wait_handle(handle)?.is_some() {
                    return Ok(false);
                }
                return Err(error);
            }
        }
        state.terminating = true;
        Ok(true)
    }
}
