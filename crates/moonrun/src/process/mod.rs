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

//! Wasm-backend-neutral process operations and Moon artifact preparation.
//!
//! [`HostProcess`] is the Runtime-owned interface used by the async host. Policy,
//! working-directory configuration, and child provenance stay above the
//! native workers and [`os`] process mechanisms. [`ChildProcesses`] owns child
//! control independently of the identity returned by [`HostProcess::pid`].
//! [`prepare_moon`] runs host-selected helpers and owns their artifact handoff.

mod children;
mod intercept;
mod job;
mod lookup;
mod moon;
pub(crate) mod native;
pub(crate) mod os;
mod prepare_artifact;
mod spawn;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use crate::async_host::AsyncHostResult;
use crate::policy::{Policy, ProcessPolicy};
use crate::runtime::{Stdio, WorkingDirectory};

pub(crate) use children::ChildProcesses;
pub(crate) use intercept::CommandInterception;
pub(crate) use job::Job;
pub(crate) use os::SpawnOptions;
pub(crate) use prepare_artifact::{Preparation, prepare_moon};

/// Resolved guest behavior. Each field supplies only its own operation's inputs.
#[derive(Clone, Debug, Default)]
pub(crate) struct ProcessConfig {
    pub(crate) identity: GuestIdentity,
    pub(crate) managed_children: Option<crate::process_tree::ExecutionContext>,
    pub(crate) interception: CommandInterception,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GuestIdentity {
    #[default]
    Native,
    Virtual(crate::VirtualPid),
}

/// Process operations and child ownership shared by guest and worker threads.
#[derive(Clone)]
pub(crate) struct HostProcess {
    ipc_endpoint: Option<std::path::PathBuf>,
    policy: Option<ProcessPolicy>,
    policy_inheritance: Option<Policy>,
    working_directory: Arc<WorkingDirectory>,
    stdio: Arc<Stdio>,
    pub(crate) children: ChildProcesses,
    identity: GuestIdentity,
    interception: CommandInterception,
    environment: Arc<crate::runtime::Env>,
}

impl HostProcess {
    pub(crate) fn new(
        policy: Option<Policy>,
        working_directory: Arc<WorkingDirectory>,
        stdio: Arc<Stdio>,
        config: ProcessConfig,
        environment: Arc<crate::runtime::Env>,
    ) -> Self {
        let children = ChildProcesses::new(config.managed_children, policy.is_some());
        Self {
            ipc_endpoint: None,
            policy: policy.as_ref().map(Policy::process),
            policy_inheritance: policy,
            working_directory,
            stdio,
            children,
            identity: config.identity,
            interception: config.interception,
            environment,
        }
    }

    pub(crate) fn with_ipc_endpoint(mut self, endpoint: Option<std::path::PathBuf>) -> Self {
        self.ipc_endpoint = endpoint;
        self
    }

    pub(crate) fn pid(&self) -> i32 {
        match self.identity {
            GuestIdentity::Native => crate::async_sys::internal::env_util::stub::get_pid() as i32,
            GuestIdentity::Virtual(pid) => pid.get(),
        }
    }

    /// Interpret and authorize a job once, before handing owned execution
    /// inputs to an OS primitive or the Engine's virtual child admission.
    pub(crate) fn prepare_job(&self, job: &mut Job) -> AsyncHostResult<()> {
        job.prepare(self)
    }

    pub(crate) fn finish_job(&self, job: &Job, ret: i64, err: i32) -> AsyncHostResult<()> {
        job.finish(&self.children, ret, err)
    }

    pub(crate) fn revoke_unclaimed_spawn(&self, job: &Job, ret: i64, err: i32) {
        job.revoke_unclaimed_spawn(&self.children, ret, err)
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
}

#[cfg(windows)]
pub(crate) fn cancel_wait(cancel: &crate::resource::ResourceRef) -> AsyncHostResult<()> {
    native::cancel_wait_for_process(cancel)
}
