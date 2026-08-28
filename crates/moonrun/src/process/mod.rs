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

//! Wasm-backend-neutral process operations owned by one moonrun Runtime.
//!
//! [`HostProcess`] is the module interface used by the async host. Policy,
//! working-directory configuration, and child provenance stay above the
//! private [`ambient`] implementation, which owns native operating-system
//! execution. A future execution kind can therefore be added inside this
//! module without exposing platform process details to callers.

mod ambient;
mod job;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(windows)]
use crate::async_sys::internal::fd_util::stub::RawFd;
use crate::policy::{PolicyInheritance, ProcessPolicy};
use crate::resource::ResourceRef;
use crate::runtime::{Stdio, WorkingDirectory};

pub(crate) use job::{Job, SpawnOptions};

/// Process operations and child ownership shared by guest and worker threads.
#[derive(Clone)]
pub(crate) struct HostProcess {
    policy: Option<ProcessPolicy>,
    policy_inheritance: Option<PolicyInheritance>,
    working_directory: Arc<WorkingDirectory>,
    stdio: Arc<Stdio>,
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

impl HostProcess {
    pub(crate) fn new(
        policy: Option<ProcessPolicy>,
        policy_inheritance: Option<PolicyInheritance>,
        working_directory: Arc<WorkingDirectory>,
        stdio: Arc<Stdio>,
    ) -> Self {
        // Enforced process policy needs child provenance to authorize later
        // PID and handle operations. Ambient execution preserves direct OS
        // behavior and therefore does not create authority state.
        let child_authority = policy
            .is_some()
            .then(|| Arc::new(ChildAuthorityState::default()));
        Self {
            policy,
            policy_inheritance,
            working_directory,
            stdio,
            child_authority,
        }
    }

    pub(crate) fn check_job(&self, job: &Job) -> AsyncHostResult<()> {
        job.check_policy(self)
    }

    pub(crate) fn wait_job(
        &self,
        handle: Option<ResourceRef>,
        tracked_pid: Option<i32>,
        pid: i32,
    ) -> AsyncHostResult<Job> {
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

    /// Apply Runtime-owned configuration to an authorized process job.
    ///
    /// A spawn job already contains any cwd explicitly supplied by the guest.
    /// The Runtime Working Directory gets the final chance to adjust that
    /// value before worker execution. Ambient deliberately leaves it alone,
    /// including `None`, so the operating system observes its current
    /// directory at spawn time. Guest-supplied standard streams also remain
    /// authoritative; the Runtime binding resolves only missing child streams
    /// immediately before native execution.
    pub(crate) fn configure_job_for_execution(&self, job: &mut Job) -> AsyncHostResult<()> {
        job.configure_working_directory(&self.working_directory);
        job.configure_stdio(&self.stdio)?;
        // Authorization is complete, so the job may retain the immutable
        // inheritance payload. Direct-moonx recognition, temporary-file I/O,
        // handle duplication, and reserved env replacement stay in the spawn
        // job.
        job.set_policy_inheritance(self.policy_inheritance.clone());
        Ok(())
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
        let Some(state) = self.child_authority.as_deref() else {
            return f();
        };
        let mut state = state.inner.lock().unwrap();
        if state.process_handle_pids.get(&handle) != Some(&pid) {
            return Err(AsyncHostError::PermissionDenied);
        }
        if crate::async_sys::process::process_id_from_handle(raw_handle)? != pid {
            return Err(AsyncHostError::PermissionDenied);
        }
        let result = f()?;
        state.owned_child_pids.remove(&pid);
        Ok(result)
    }

    pub(crate) fn process_handle_pid(&self, handle: u64) -> AsyncHostResult<Option<i32>> {
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

    #[cfg(test)]
    pub(crate) fn check_owned_child_pid(&self, pid: i32) -> AsyncHostResult<()> {
        self.ensure_owned_child_pid(pid)
    }

    #[cfg(test)]
    pub(crate) fn check_process_handle_pid(&self, handle: u64, pid: i32) -> AsyncHostResult<()> {
        let Some(state) = self.child_authority.as_deref() else {
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
        self.policy
            .as_ref()
            .map_or(Ok(()), |policy| policy.allows_unix(program, argv))
    }

    #[cfg(windows)]
    fn check_spawn_windows(&self, command_line: &std::ffi::OsStr) -> AsyncHostResult<()> {
        self.policy
            .as_ref()
            .map_or(Ok(()), |policy| policy.allows_windows(command_line))
    }

    fn check_wait(
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

    fn track_spawned_child(&self, pid: i32) {
        if let Some(state) = self.child_authority.as_deref() {
            state.inner.lock().unwrap().owned_child_pids.insert(pid);
        }
    }

    fn finish_waited_child(&self, pid: i32, #[cfg(unix)] defer_reap: bool) -> AsyncHostResult<()> {
        let Some(state) = self.child_authority.as_deref() else {
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

#[cfg(windows)]
pub(crate) fn cancel_wait(cancel: &crate::resource::ResourceRef) -> AsyncHostResult<()> {
    ambient::cancel_wait_for_process(cancel)
}
