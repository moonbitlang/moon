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

//! Runtime-engine-neutral process operations for one moonrun Host.

mod job;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::policy::Policy;

pub(crate) use job::{Job, SpawnOptions};

/// Permission and child ownership state shared by guest and worker threads.
#[derive(Clone)]
pub(crate) struct HostProcess {
    policy: Arc<Policy>,
    state: Option<Arc<ProcessPolicyState>>,
}

#[derive(Default)]
struct ProcessPolicyState {
    // PID authority and stable-handle provenance must change atomically.
    inner: Mutex<ProcessPolicyStateInner>,
}

#[derive(Default)]
struct ProcessPolicyStateInner {
    owned_child_pids: HashSet<i32>,
    process_handle_pids: HashMap<u64, i32>,
}

impl HostProcess {
    pub(crate) fn new(policy: Arc<Policy>) -> Self {
        let state = policy
            .has_process_policy()
            .then(|| Arc::new(ProcessPolicyState::default()));
        Self { policy, state }
    }

    pub(crate) fn has_policy(&self) -> bool {
        self.state.is_some()
    }

    pub(crate) fn check_job(&self, job: &Job) -> AsyncHostResult<()> {
        job.check_policy(self)
    }

    pub(crate) fn finish_job(&self, job: &Job, ret: i64, err: i32) -> AsyncHostResult<()> {
        job.finish(self, ret, err)
    }

    pub(crate) fn revoke_unclaimed_spawn(&self, job: &Job, ret: i64, err: i32) {
        job.revoke_unclaimed_spawn(self, ret, err)
    }

    pub(crate) fn with_owned_child_pid<T>(
        &self,
        pid: i32,
        f: impl FnOnce() -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let Some(state) = self.state.as_deref() else {
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
        let Some(state) = self.state.as_deref() else {
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
        f: impl FnOnce() -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let Some(state) = self.state.as_deref() else {
            return f();
        };
        let mut state = state.inner.lock().unwrap();
        if state.process_handle_pids.get(&handle) != Some(&pid) {
            return Err(AsyncHostError::PermissionDenied);
        }
        let result = f()?;
        state.owned_child_pids.remove(&pid);
        Ok(result)
    }

    pub(crate) fn process_handle_pid(&self, handle: u64) -> AsyncHostResult<Option<i32>> {
        let Some(state) = self.state.as_deref() else {
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
        if let Some(state) = self.state.as_deref() {
            state
                .inner
                .lock()
                .unwrap()
                .process_handle_pids
                .insert(handle, pid);
        }
    }

    pub(crate) fn untrack_process_handle(&self, handle: u64) {
        if let Some(state) = self.state.as_deref() {
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

    #[cfg(test)]
    pub(crate) fn check_owned_child_pid(&self, pid: i32) -> AsyncHostResult<()> {
        self.ensure_owned_child_pid(pid)
    }

    #[cfg(test)]
    pub(crate) fn check_process_handle_pid(&self, handle: u64, pid: i32) -> AsyncHostResult<()> {
        let Some(state) = self.state.as_deref() else {
            return Ok(());
        };
        if state.inner.lock().unwrap().process_handle_pids.get(&handle) == Some(&pid) {
            Ok(())
        } else {
            Err(AsyncHostError::PermissionDenied)
        }
    }

    #[cfg(test)]
    pub(crate) fn track_owned_child(&self, pid: i32) {
        self.track_spawned_child(pid);
    }

    #[cfg(unix)]
    fn check_spawn_unix(
        &self,
        program: &std::ffi::OsStr,
        argv: &[std::ffi::OsString],
    ) -> AsyncHostResult<()> {
        self.policy.spawn_process_unix(program, argv)
    }

    #[cfg(windows)]
    fn check_spawn_windows(&self, command_line: &std::ffi::OsStr) -> AsyncHostResult<()> {
        self.policy.spawn_process_windows(command_line)
    }

    fn check_wait(
        &self,
        has_handle: bool,
        tracked_pid: Option<i32>,
        pid: i32,
    ) -> AsyncHostResult<()> {
        if self.state.is_none() {
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
        let Some(state) = self.state.as_deref() else {
            return Ok(());
        };
        if state.inner.lock().unwrap().owned_child_pids.contains(&pid) {
            Ok(())
        } else {
            Err(AsyncHostError::PermissionDenied)
        }
    }

    fn track_spawned_child(&self, pid: i32) {
        if let Some(state) = self.state.as_deref() {
            state.inner.lock().unwrap().owned_child_pids.insert(pid);
        }
    }

    fn finish_waited_child(&self, pid: i32, #[cfg(unix)] defer_reap: bool) -> AsyncHostResult<()> {
        let Some(state) = self.state.as_deref() else {
            #[cfg(unix)]
            if defer_reap {
                crate::async_sys::process::reap_process(pid)?;
            }
            return Ok(());
        };
        let mut state = state.inner.lock().unwrap();
        #[cfg(unix)]
        if defer_reap {
            crate::async_sys::process::reap_process(pid)?;
        }
        state.owned_child_pids.remove(&pid);
        Ok(())
    }

    fn revoke_child_if_unreferenced(&self, pid: i32) {
        let Some(state) = self.state.as_deref() else {
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

#[cfg(windows)]
pub(crate) fn cancel_wait(cancel: &crate::resource::ResourceRef) -> AsyncHostResult<()> {
    job::cancel_wait(cancel)
}
