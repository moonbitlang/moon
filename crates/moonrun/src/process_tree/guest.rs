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

//! Guest-visible process control. Integer PIDs never fall back to the OS.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{Table, VirtualPid, WaitClaim};
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::event_loop::thread_pool::{
    CancellationOutcome, JobCancellation, JobCancellationOverride,
};
use crate::{RunOutcome, RunTermination};

#[derive(Clone)]
pub(crate) struct ProcessContext {
    pub(super) pid: VirtualPid,
    pub(super) table: Table,
}

impl std::fmt::Debug for ProcessContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessContext")
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

impl ProcessContext {
    pub(crate) fn pid(&self) -> i32 {
        self.pid.get()
    }

    pub(crate) fn kill(&self, pid: i32) -> AsyncHostResult<()> {
        let mut entries = self.table.lock().unwrap();
        let entry = entries
            .get_mut(&VirtualPid(pid))
            .ok_or(AsyncHostError::PermissionDenied)?;
        if pid != self.pid.get() && entry.info.parent != Some(self.pid) {
            return Err(AsyncHostError::PermissionDenied);
        }
        #[cfg(unix)]
        let outcome = RunTermination::KilledBySignal(libc::SIGKILL);
        #[cfg(windows)]
        let outcome = RunTermination::Exit(1);
        if entry.terminator.terminate(outcome) {
            entry.info.state = super::ProcessState::Terminating;
        }
        Ok(())
    }

    pub(crate) fn wait(&self, pid: i32) -> AsyncHostResult<ProcessWait> {
        let pid = VirtualPid(pid);
        let entries = self.table.lock().unwrap();
        if entries
            .get(&pid)
            .is_none_or(|entry| entry.info.parent != Some(self.pid))
        {
            return Err(AsyncHostError::PermissionDenied);
        }
        Ok(ProcessWait {
            pid,
            cancel: Arc::new(WaitCancellation {
                context: self.clone(),
                completion: Arc::clone(&entries[&pid].completion),
                cancelled: AtomicBool::new(false),
            }),
            claim: None,
        })
    }
}

pub(crate) struct ProcessWait {
    pid: VirtualPid,
    cancel: Arc<WaitCancellation>,
    claim: Option<WaitClaim>,
}

impl std::fmt::Debug for ProcessWait {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessWait")
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

impl ProcessWait {
    pub(crate) fn cancellation(&self) -> JobCancellation {
        self.cancel.clone()
    }

    pub(crate) fn run(&mut self) -> AsyncHostResult<i64> {
        if self.claim.is_some() {
            return Err(AsyncHostError::Inval);
        }
        let table = &self.cancel.context.table;
        let claim =
            WaitClaim::new(Arc::clone(table), self.pid).map_err(|_| AsyncHostError::Inval)?;
        let mut result = self.cancel.completion.result.lock().unwrap();
        loop {
            if self.cancel.cancelled.load(Ordering::Relaxed) {
                // Release the lock before dropping the claim.
                drop(result);
                #[cfg(unix)]
                return Err(AsyncHostError::Native(libc::EINTR));
                #[cfg(windows)]
                return Err(AsyncHostError::Native(
                    windows_sys::Win32::Foundation::ERROR_OPERATION_ABORTED as i32,
                ));
            }
            if let Some(result) = &*result {
                let status = match result {
                    Ok(RunOutcome::Completed) => 0,
                    Ok(RunOutcome::Exited(code)) => i64::from(*code),
                    Ok(RunOutcome::KilledBySignal(signal)) => -i64::from(*signal),
                    Err(_) => 1,
                };
                self.claim = Some(claim);
                return Ok(status);
            }
            result = self.cancel.completion.changed.wait(result).unwrap();
        }
    }

    /// Reading the completed Job result commits the wait. Freeing an unread
    /// Job (including Runtime teardown) instead leaves the exit for its owner.
    pub(crate) fn consume(&mut self) {
        if let Some(claim) = self.claim.take() {
            claim.table.lock().unwrap().remove(&claim.pid);
        }
    }
}

#[derive(Debug)]
struct WaitCancellation {
    context: ProcessContext,
    completion: Arc<super::ProcessCompletion>,
    cancelled: AtomicBool,
}

impl JobCancellationOverride for WaitCancellation {
    fn cancel(&self) -> AsyncHostResult<CancellationOutcome> {
        // Share the wait predicate lock so cancellation cannot race wait entry.
        // This durable hook works both before dispatch and during Condvar::wait.
        let _result = self.completion.result.lock().unwrap();
        self.cancelled.store(true, Ordering::Relaxed);
        self.completion.changed.notify_all();
        Ok(CancellationOutcome::NeedWait)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Entry, ProcessCompletion, ProcessInfo, ProcessState};
    use super::*;
    use crate::async_sys::internal::event_loop::thread_pool::{
        self, WorkerCompletionDestination, WorkerCompletionId, WorkerJob,
    };
    use std::collections::BTreeMap;
    use std::sync::{Mutex, mpsc};
    use std::time::Duration;

    #[test]
    fn wait_cancellation_and_result_ownership() {
        if std::env::var_os("MOONRUN_WAIT_OWNERSHIP_TEST").is_none() {
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process_tree::guest::tests::wait_cancellation_and_result_ownership",
                    "--nocapture",
                ])
                .env("MOONRUN_WAIT_OWNERSHIP_TEST", "1")
                .timeout(Duration::from_secs(15))
                .assert()
                .success();
            return;
        }
        for before_wait in [true, false] {
            let parent = VirtualPid(1);
            let child = VirtualPid(2);
            let completion = Arc::new(ProcessCompletion::default());
            let table = Arc::new(Mutex::new(BTreeMap::from([(
                child,
                Entry {
                    info: ProcessInfo {
                        pid: child,
                        parent: Some(parent),
                        state: ProcessState::Running,
                    },
                    terminator: crate::termination_channel().0,
                    thread: None,
                    completion: Arc::clone(&completion),
                    waiting: false,
                },
            )])));
            let context = ProcessContext {
                pid: parent,
                table: Arc::clone(&table),
            };
            let wait = context.wait(child.get()).unwrap();
            let (entered, started) = mpsc::channel();
            let (release, proceed) = mpsc::channel();
            let (completed, results) = mpsc::channel();
            let (notified, notification) = mpsc::channel();
            let worker = thread_pool::spawn_worker(
                WorkerJob::new(
                    WorkerCompletionId::from_abi(1),
                    slotmap::KeyData::from_ffi(1).into(),
                    crate::process::Job::wait_for_virtual_process(wait).into(),
                ),
                move |job| {
                    entered.send(()).unwrap();
                    if before_wait {
                        proceed.recv().unwrap();
                    }
                    thread_pool::run_host_job(&mut job.job);
                },
                completed,
                WorkerCompletionDestination::Test(Box::new(move |_| {
                    notified.send(()).unwrap();
                })),
            );
            started.recv_timeout(Duration::from_secs(5)).unwrap();
            if !before_wait {
                while !table.lock().unwrap()[&child].waiting {
                    std::thread::yield_now();
                }
                assert!(matches!(
                    WaitClaim::new(Arc::clone(&table), child),
                    Err(super::super::ProcessError::AlreadyWaiting(_))
                ));
            }
            assert_eq!(
                thread_pool::cancel_worker(&worker).unwrap(),
                CancellationOutcome::NeedWait
            );
            if before_wait {
                release.send(()).unwrap();
            }
            notification.recv_timeout(Duration::from_secs(5)).unwrap();
            let result = results.recv().unwrap();
            assert!(thread_pool::errno_is_cancelled(result.job.err()));
            thread_pool::free_worker(worker);
            drop(result);
            assert!(!table.lock().unwrap()[&child].waiting);
            assert!(completion.result.lock().unwrap().is_none());

            *completion.result.lock().unwrap() = Some(Ok(RunOutcome::Exited(42)));
            let mut unread = context.wait(child.get()).unwrap();
            assert_eq!(unread.run(), Ok(42));
            assert!(table.lock().unwrap()[&child].waiting);
            drop(unread);
            assert!(!table.lock().unwrap()[&child].waiting);
            let mut accepted = context.wait(child.get()).unwrap();
            assert_eq!(accepted.run(), Ok(42));
            accepted.consume();
            accepted.consume();
            assert!(table.lock().unwrap().is_empty());
        }
    }
}
