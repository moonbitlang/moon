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

//! Process control and exit-result ownership above native OS mechanisms.

#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::os::fd::IntoRawFd;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, IntoRawHandle};

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::ported_fns;
use crate::resource::{Resource, ResourceRef};

use super::{HostProcess, Job, native, os};

ported_fns! {
    impl HostProcess {
        #[ported(source = "src/internal/event_loop/process.c", original = "moonbitlang_async_open_pid_handle")]
        /// Return an owned Resource for registration, or a PID-only wait target.
        /// Linux pidfd errors remain available to the ABI's fallback encoding.
        pub(crate) fn open_handle(&self, pid: i32) -> AsyncHostResult<Option<Resource>> {
            if let Some(context) = &self.context {
                #[cfg(unix)]
                {
                    // Validate direct-child authority without consuming completion.
                    drop(context.wait(pid)?);
                    return Ok(None);
                }
                #[cfg(windows)]
                return Ok(Some(Resource::process(crate::process_tree::ProcessHandle::open(
                    context, pid,
                )?)));
            }
            self.with_owned_child_pid(pid, || {
                #[cfg(target_os = "linux")]
                return Ok(Some(Resource::new(os::open_pidfd(pid)?.into_raw_fd())));
                #[cfg(windows)]
                return Ok(Some(Resource::new(
                    os::open_process_for_wait(pid as u32)?.into_raw_handle(),
                )));
                #[cfg(all(unix, not(target_os = "linux")))]
                Ok(None)
            })
        }

        /// Preserve each implementation's consumption point. Virtual completion
        /// is accepted only after the caller receives it; an OS wait may reap
        /// the child before the caller can copy the result.
        pub(crate) fn read_exit(
            &self,
            pid: i32,
            handle: Option<(u64, &Resource)>,
            accept: impl FnOnce(i32) -> AsyncHostResult<()>,
        ) -> AsyncHostResult<()> {
            if let Some(context) = &self.context {
                #[cfg(unix)]
                {
                    if handle.is_some() {
                        return Err(AsyncHostError::Badf);
                    }
                    return context.read_exit(pid, accept);
                }
                #[cfg(windows)]
                {
                    let (_, resource) = handle.ok_or(AsyncHostError::Badf)?;
                    let handle = resource.as_process()?;
                    if !handle.matches(context, pid) {
                        return Err(AsyncHostError::PermissionDenied);
                    }
                    return handle.read_exit(accept);
                }
            }
            accept(self.read_native_exit(pid, handle)?)
        }

        #[ported(source = "src/internal/event_loop/process.c", original = "moonbitlang_async_get_process_result")]
        fn read_native_exit(&self, pid: i32, handle: Option<(u64, &Resource)>) -> AsyncHostResult<i32> {
            #[cfg(unix)]
            let raw = handle
                .map(|(_, resource)| resource.as_fd().map(|fd| fd.as_raw_fd()))
                .transpose()?;
            #[cfg(windows)]
            let (handle_id, raw) = {
                let (id, resource) = handle.ok_or(AsyncHostError::Badf)?;
                (id, resource.as_handle()?.as_raw_handle())
            };

            // Keep authority and handle identity stable through the native wait and
            // successful revocation. A pending result or OS error preserves authority.
            let mut state = self
                .child_authority
                .as_deref()
                .map(|state| state.inner.lock().unwrap());
            if let Some(state) = &state {
                #[cfg(unix)]
                if !state.owned_child_pids.contains(&pid)
                    || handle.is_some_and(|(id, _)| state.process_handle_pids.get(&id) != Some(&pid))
                {
                    return Err(AsyncHostError::PermissionDenied);
                }
                #[cfg(windows)]
                {
                    if state.process_handle_pids.get(&handle_id) != Some(&pid) {
                        return Err(AsyncHostError::PermissionDenied);
                    }
                    if os::process_id(raw)? != pid as u32 {
                        return Err(AsyncHostError::PermissionDenied);
                    }
                }
            }
            #[cfg(target_os = "linux")]
            let status = match raw {
                Some(pidfd) => os::try_wait_pidfd(pidfd)?,
                None => os::try_wait_pid(pid)?,
            };
            #[cfg(all(unix, not(target_os = "linux")))]
            let status = {
                let _ = raw;
                os::try_wait_pid(pid)?
            };
            #[cfg(windows)]
            let status = os::try_wait_handle(raw)?;
            #[cfg(unix)]
            let pending = libc::EAGAIN;
            #[cfg(windows)]
            let pending = windows_sys::Win32::Foundation::ERROR_IO_PENDING as i32;
            let status = status.ok_or(AsyncHostError::Native(pending))?;
            if let Some(state) = &mut state {
                state.owned_child_pids.remove(&pid);
            }
            Ok(native::exit_code(status) as i32)
        }

        #[ported(source = "src/process/unix.c", original = "moonbitlang_async_kill_process")]
        #[cfg(unix)]
        pub(crate) fn kill(&self, pid: i32) -> AsyncHostResult<()> {
            if let Some(context) = &self.context {
                return context.kill(pid);
            }
            self.with_owned_child_pid(pid, || {
                // Preserve the guest ABI's best-effort native signal behavior.
                let _ = os::send_signal(pid, libc::SIGKILL);
                Ok(())
            })
        }

        #[ported(source = "src/process/windows.c", original = "moonbitlang_async_kill_process")]
        #[cfg(windows)]
        pub(crate) fn kill(&self, pid: i32) -> AsyncHostResult<()> {
            if let Some(context) = &self.context {
                return context.kill(pid);
            }
            self.with_owned_child_pid(pid, || {
                let _ = os::terminate_process(pid as u32, 1);
                Ok(())
            })
        }

        #[ported(source = "src/process/unix.c", original = "moonbitlang_async_terminate_process")]
        #[cfg(unix)]
        pub(crate) fn terminate(&self, pid: i32, signal: i32) -> AsyncHostResult<()> {
            if let Some(context) = &self.context {
                return context.terminate(pid, signal);
            }
            self.with_owned_child_pid(pid, || {
                let _ = os::send_signal(pid, signal);
                Ok(())
            })
        }

        #[ported(source = "src/process/windows.c", original = "moonbitlang_async_terminate_process")]
        #[cfg(windows)]
        pub(crate) fn terminate(&self, pid: i32, signal: i32) -> AsyncHostResult<()> {
            if let Some(context) = &self.context {
                return context.terminate(pid, signal);
            }
            self.with_owned_child_pid(pid, || {
                // Native graceful cancellation ignores the guest's signal value.
                let _ = os::send_console_event(
                    pid as u32,
                    windows_sys::Win32::System::Console::CTRL_BREAK_EVENT,
                );
                Ok(())
            })
        }
    }
}

impl HostProcess {
    pub(crate) fn wait_job(
        &self,
        handle: Option<ResourceRef>,
        tracked_pid: Option<i32>,
        pid: i32,
    ) -> AsyncHostResult<Job> {
        if let Some(context) = &self.context {
            #[cfg(unix)]
            if handle.is_some() {
                return Err(AsyncHostError::Badf);
            }
            #[cfg(windows)]
            if let Some(handle) = &handle {
                let handle = handle.as_process()?;
                if !handle.matches(context, pid) {
                    return Err(AsyncHostError::PermissionDenied);
                }
                return Ok(Job::wait_for_virtual_process(handle.wait()));
            }
            return Ok(Job::wait_for_virtual_process(context.wait(pid)?));
        }
        // On Unix, enforced authorization must inspect this provenance before
        // reaping, so the worker defers the final reap to HostProcess.
        #[cfg(unix)]
        let defer_reap_for_authorization = self.child_authority.is_some();
        Job::wait_for_process(
            handle,
            tracked_pid,
            pid,
            #[cfg(unix)]
            defer_reap_for_authorization,
        )
    }

    pub(crate) fn with_owned_child_pid<T>(
        &self,
        pid: i32,
        f: impl FnOnce() -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        self.require_ambient()?;
        let Some(state) = self.child_authority.as_deref() else {
            return f();
        };
        let state = state.inner.lock().unwrap();
        if !state.owned_child_pids.contains(&pid) {
            return Err(AsyncHostError::PermissionDenied);
        }
        f()
    }
}
