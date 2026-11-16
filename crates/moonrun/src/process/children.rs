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

//! Child authority, control, wait completion, and handle provenance.
//! Guest identity and command recognition are not inputs to these operations.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use super::{Job, os};
use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(windows)]
use crate::async_sys::internal::fd_util::stub::RawFd;
use crate::process_tree::ExecutionContext;
use crate::resource::ResourceRef;

#[derive(Clone)]
pub(crate) struct ChildProcesses {
    execution: Option<ExecutionContext>,
    child_authority: Option<Arc<ChildAuthorityState>>,
}

#[derive(Default)]
struct ChildAuthorityState {
    // PID authority and stable-handle provenance must change atomically.
    inner: Mutex<ChildAuthorityStateInner>,
}

#[derive(Default)]
struct ChildAuthorityStateInner {
    owned_child_pids: HashSet<i32>,
    process_handle_pids: HashMap<u64, i32>,
}

impl ChildProcesses {
    pub(super) fn new(execution: Option<ExecutionContext>, restricted: bool) -> Self {
        let child_authority = restricted.then(|| Arc::new(ChildAuthorityState::default()));
        Self {
            execution,
            child_authority,
        }
    }

    pub(crate) fn managed(&self) -> Option<&ExecutionContext> {
        self.execution.as_ref()
    }

    pub(crate) fn kill(&self, pid: i32) -> AsyncHostResult<()> {
        if let Some(context) = self.managed() {
            return context.kill(pid);
        }
        self.with_owned_child_pid(pid, || {
            let _ = crate::async_sys::process::kill_process(pid);
            Ok(())
        })
    }

    pub(crate) fn terminate(&self, pid: i32, signal: i32) -> AsyncHostResult<()> {
        if let Some(context) = self.managed() {
            return context.terminate(pid, signal);
        }
        self.with_owned_child_pid(pid, || {
            let _ = crate::async_sys::process::terminate_process(pid, signal);
            Ok(())
        })
    }

    // Native-handle paths must never reinterpret a virtual PID as an
    // operating-system process identity.
    fn require_native(&self) -> AsyncHostResult<()> {
        if self.managed().is_none() {
            return Ok(());
        }
        #[cfg(unix)]
        return Err(AsyncHostError::Native(libc::ENOTSUP));
        #[cfg(windows)]
        return Err(AsyncHostError::Native(
            windows_sys::Win32::Foundation::ERROR_NOT_SUPPORTED as i32,
        ));
    }

    pub(crate) fn wait_job(
        &self,
        handle: Option<ResourceRef>,
        tracked_pid: Option<i32>,
        pid: i32,
    ) -> AsyncHostResult<Job> {
        if let Some(context) = self.managed() {
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
        // reaping, so the worker defers the final reap to the child owner.
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
        self.require_native()?;
        let Some(state) = self.child_authority.as_deref() else {
            return f();
        };
        let state = state.inner.lock().unwrap();
        if !state.owned_child_pids.contains(&pid) {
            return Err(AsyncHostError::PermissionDenied);
        }
        f()
    }

    #[cfg(unix)]
    pub(crate) fn finish_owned_child<T>(
        &self,
        pid: i32,
        handle: Option<u64>,
        f: impl FnOnce() -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        self.require_native()?;
        let Some(state) = self.child_authority.as_deref() else {
            return f();
        };
        let mut state = state.inner.lock().unwrap();
        if !state.owned_child_pids.contains(&pid)
            || handle.is_some_and(|handle| state.process_handle_pids.get(&handle) != Some(&pid))
        {
            return Err(AsyncHostError::PermissionDenied);
        }
        let result = f()?;
        state.owned_child_pids.remove(&pid);
        Ok(result)
    }

    #[cfg(windows)]
    pub(crate) fn finish_process_handle<T>(
        &self,
        pid: i32,
        handle: u64,
        raw_handle: RawFd,
        f: impl FnOnce() -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        self.require_native()?;
        let Some(state) = self.child_authority.as_deref() else {
            return f();
        };
        let mut state = state.inner.lock().unwrap();
        if state.process_handle_pids.get(&handle) != Some(&pid) {
            return Err(AsyncHostError::PermissionDenied);
        }
        if os::process_id(raw_handle)? != pid as u32 {
            return Err(AsyncHostError::PermissionDenied);
        }
        let result = f()?;
        state.owned_child_pids.remove(&pid);
        Ok(result)
    }

    pub(crate) fn process_handle_pid(&self, handle: u64) -> AsyncHostResult<Option<i32>> {
        if self.managed().is_some() {
            return Ok(None);
        }
        let Some(state) = self.child_authority.as_deref() else {
            return Ok(None);
        };
        state
            .inner
            .lock()
            .unwrap()
            .process_handle_pids
            .get(&handle)
            .copied()
            .map(Some)
            .ok_or(AsyncHostError::PermissionDenied)
    }

    pub(crate) fn track_process_handle(&self, handle: u64, pid: i32) {
        if self.managed().is_some() {
            return;
        }
        if let Some(state) = self.child_authority.as_deref() {
            state
                .inner
                .lock()
                .unwrap()
                .process_handle_pids
                .insert(handle, pid);
        }
    }

    pub(crate) fn untrack_process_handle(&self, handle: u64) {
        if let Some(state) = self.child_authority.as_deref() {
            let mut state = state.inner.lock().unwrap();
            if let Some(pid) = state.process_handle_pids.remove(&handle)
                && !state
                    .process_handle_pids
                    .values()
                    .any(|tracked_pid| *tracked_pid == pid)
            {
                state.owned_child_pids.remove(&pid);
            }
        }
    }

    pub(super) fn check_wait(
        &self,
        has_handle: bool,
        tracked_pid: Option<i32>,
        pid: i32,
    ) -> AsyncHostResult<()> {
        if self.child_authority.is_none() {
            return Ok(());
        }
        self.ensure_owned_child_pid(pid)?;
        match (has_handle, tracked_pid) {
            (false, None) => Ok(()),
            (true, Some(tracked_pid)) if tracked_pid == pid => Ok(()),
            _ => Err(AsyncHostError::PermissionDenied),
        }
    }

    fn ensure_owned_child_pid(&self, pid: i32) -> AsyncHostResult<()> {
        let Some(state) = self.child_authority.as_deref() else {
            return Ok(());
        };
        if state.inner.lock().unwrap().owned_child_pids.contains(&pid) {
            Ok(())
        } else {
            Err(AsyncHostError::PermissionDenied)
        }
    }

    pub(crate) fn track_spawned_child(&self, pid: i32) {
        if let Some(state) = self.child_authority.as_deref() {
            state.inner.lock().unwrap().owned_child_pids.insert(pid);
        }
    }

    pub(super) fn finish_waited_child(
        &self,
        pid: i32,
        #[cfg(unix)] defer_reap: bool,
    ) -> AsyncHostResult<()> {
        let Some(state) = self.child_authority.as_deref() else {
            #[cfg(unix)]
            if defer_reap {
                os::reap(pid)?;
            }
            return Ok(());
        };
        let mut state = state.inner.lock().unwrap();
        #[cfg(unix)]
        if defer_reap {
            os::reap(pid)?;
        }
        state.owned_child_pids.remove(&pid);
        Ok(())
    }

    pub(super) fn revoke_child_if_unreferenced(&self, pid: i32) {
        let Some(state) = self.child_authority.as_deref() else {
            return;
        };
        let mut state = state.inner.lock().unwrap();
        if !state
            .process_handle_pids
            .values()
            .any(|tracked_pid| *tracked_pid == pid)
        {
            state.owned_child_pids.remove(&pid);
        }
    }
}
