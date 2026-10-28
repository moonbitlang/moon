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

// Use the same status encoding as native process waits and the CLI adapter.
pub(super) fn exit_status(result: &Result<RunOutcome, super::ProcessError>) -> i32 {
    match result {
        Ok(RunOutcome::Completed) => 0,
        Ok(RunOutcome::Exited(code)) => *code,
        #[cfg(unix)]
        Ok(RunOutcome::KilledBySignal(signal)) => -*signal,
        #[cfg(windows)]
        Ok(RunOutcome::KilledBySignal(_)) => 0xC000_013A_u32 as i32,
        Err(_) => 1,
    }
}

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

    #[cfg(unix)]
    pub(crate) fn read_exit<T>(
        &self,
        pid: i32,
        accept: impl FnOnce(i32) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let mut entries = self.table.lock().unwrap();
        let entry = entries
            .get(&VirtualPid(pid))
            .ok_or(AsyncHostError::PermissionDenied)?;
        if entry.info.parent != Some(self.pid) || entry.waiting {
            return Err(AsyncHostError::PermissionDenied);
        }
        let result_guard = entry.completion.result.lock().unwrap();
        let Some(result) = result_guard.as_ref() else {
            return Err(AsyncHostError::Native(libc::EAGAIN));
        };
        let status = exit_status(result);
        let accepted = accept(status)?;
        drop(result_guard);
        // The completion mutex must be released before removing its owner.
        drop(entries.remove(&VirtualPid(pid)));
        Ok(accepted)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn register_exit(
        &self,
        pid: i32,
        poll: &crate::async_sys::internal::event_loop::poll::PollInstance,
    ) -> AsyncHostResult<i32> {
        let entries = self.table.lock().unwrap();
        let entry = entries
            .get(&VirtualPid(pid))
            .ok_or(AsyncHostError::PermissionDenied)?;
        if entry.info.parent != Some(self.pid) {
            return Err(AsyncHostError::PermissionDenied);
        }
        // Serialize registration with publication so an exit cannot fall
        // between checking completion and installing its kqueue notification.
        let result = entry.completion.result.lock().unwrap();
        if result.is_some() {
            return Ok(0);
        }
        let notification =
            crate::async_sys::internal::event_loop::poll::register_process_notification(poll, pid)?;
        let mut notifications = entry.completion.notifications.lock().unwrap();
        notifications.retain(|existing| existing.is_live());
        if !notifications
            .iter()
            .any(|existing| existing.same_queue(&notification))
        {
            notifications.push(notification);
        }
        Ok(1)
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

    pub(crate) fn terminate(&self, pid: i32, signal: i32) -> AsyncHostResult<()> {
        #[cfg(unix)]
        {
            if signal == libc::SIGKILL {
                return self.kill(pid);
            }
            // Only the signals with guest cancellation/default termination
            // semantics are implemented. In particular, stop/continue must
            // never accidentally turn into termination.
            if ![0, libc::SIGINT, libc::SIGTERM, libc::SIGHUP].contains(&signal) {
                return Err(AsyncHostError::Inval);
            }
        }
        // Match the native Windows adapter: graceful cancellation targets a
        // process group with Ctrl-Break, independently of the signal argument.
        #[cfg(windows)]
        let signal = {
            let _ = signal;
            windows_sys::Win32::System::Console::CTRL_BREAK_EVENT as i32
        };
        let mut entries = self.table.lock().unwrap();
        let entry = entries
            .get_mut(&VirtualPid(pid))
            .ok_or(AsyncHostError::PermissionDenied)?;
        if pid != self.pid.get() && entry.info.parent != Some(self.pid) {
            return Err(AsyncHostError::PermissionDenied);
        }
        #[cfg(unix)]
        if signal == 0 {
            return Ok(());
        }
        match entry.signals.send(signal) {
            Ok(true) | Err(crate::SignalSendError::Disconnected) => return Ok(()),
            Ok(false) => {}
            Err(_) => return Err(AsyncHostError::Io),
        }
        // An unhandled cancellation signal takes its default action on this
        // Run only. Accepted signals leave the caller's grace period in charge.
        if entry
            .terminator
            .terminate(RunTermination::KilledBySignal(signal))
        {
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
            consume_on_read: true,
        })
    }
}

#[cfg(windows)]
#[derive(Debug)]
pub(crate) struct ProcessHandle {
    owner: ProcessContext,
    pid: i32,
    completion: Arc<super::ProcessCompletion>,
}

#[cfg(windows)]
impl ProcessHandle {
    pub(crate) fn open(owner: &ProcessContext, pid: i32) -> AsyncHostResult<Self> {
        let wait = owner.wait(pid)?;
        Ok(Self {
            owner: owner.clone(),
            pid,
            completion: Arc::clone(&wait.cancel.completion),
        })
    }

    pub(crate) fn matches(&self, owner: &ProcessContext, pid: i32) -> bool {
        self.pid == pid
            && self.owner.pid == owner.pid
            && Arc::ptr_eq(&self.owner.table, &owner.table)
    }

    pub(crate) fn wait(&self) -> ProcessWait {
        // Like a Windows process HANDLE, each wait is independent and pins
        // the completion even if another handle or supervisor reaps the PID.
        ProcessWait {
            pid: VirtualPid(self.pid),
            cancel: Arc::new(WaitCancellation {
                context: self.owner.clone(),
                completion: Arc::clone(&self.completion),
                cancelled: AtomicBool::new(false),
            }),
            claim: None,
            consume_on_read: false,
        }
    }

    pub(crate) fn read_exit<T>(
        &self,
        accept: impl FnOnce(i32) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let code = self
            .completion
            .exit_code
            .get()
            .copied()
            .ok_or(AsyncHostError::Native(
                windows_sys::Win32::Foundation::ERROR_IO_PENDING as i32,
            ))?;
        let accepted = accept(code)?;
        let mut entries = self.owner.table.lock().unwrap();
        // An outstanding PID wait still owns the table entry. Open handles
        // retain their own completion and can read it independently.
        if entries
            .get(&VirtualPid(self.pid))
            .is_some_and(|entry| !entry.waiting)
        {
            entries.remove(&VirtualPid(self.pid));
        }
        Ok(accepted)
    }
}

pub(crate) struct ProcessWait {
    pid: VirtualPid,
    cancel: Arc<WaitCancellation>,
    claim: Option<WaitClaim>,
    consume_on_read: bool,
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
        let claim = if self.consume_on_read {
            Some(WaitClaim::new(Arc::clone(table), self.pid).map_err(|_| AsyncHostError::Inval)?)
        } else {
            None
        };
        let mut result = self.cancel.completion.result.lock().unwrap();
        loop {
            let status = result.as_ref().map(exit_status);
            #[cfg(windows)]
            let status = status.or_else(|| self.cancel.completion.exit_code.get().copied());
            let cancelled = self.cancel.cancelled.load(Ordering::Relaxed);
            // Match the native Windows wait: the process handle precedes the
            // cancellation event, so an available exit result wins the race.
            #[cfg(windows)]
            let cancelled = cancelled && status.is_none();
            if cancelled {
                // Release the lock before dropping the claim.
                drop(result);
                #[cfg(unix)]
                return Err(AsyncHostError::Native(libc::EINTR));
                #[cfg(windows)]
                return Err(AsyncHostError::Native(
                    windows_sys::Win32::Foundation::ERROR_OPERATION_ABORTED as i32,
                ));
            }
            if let Some(status) = status {
                self.claim = claim;
                return Ok(i64::from(status));
            }
            result = self.cancel.completion.changed.wait(result).unwrap();
        }
    }

    /// Reading the completed Job result commits the wait. Freeing an unread
    /// Job (including Runtime teardown) instead leaves the exit for its owner.
    pub(crate) fn consume(&mut self) {
        if let Some(claim) = self.claim.take()
            && self.consume_on_read
        {
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

    #[cfg(unix)]
    #[test]
    fn signal_probe_and_unsupported_signals_do_not_terminate() {
        let pid = VirtualPid(1);
        let (terminator, receiver) = crate::termination_channel();
        let (signals, _signals) = crate::signal_channel();
        let context = ProcessContext {
            pid,
            table: Arc::new(Mutex::new(BTreeMap::from([(
                pid,
                Entry {
                    info: ProcessInfo {
                        pid,
                        parent: None,
                        state: ProcessState::Running,
                    },
                    terminator,
                    signals,
                    completion: Arc::new(ProcessCompletion::default()),
                    waiting: false,
                },
            )]))),
        };
        assert_eq!(context.terminate(pid.get(), 0), Ok(()));
        for signal in [-1, libc::SIGSTOP, libc::SIGCONT, i32::MAX] {
            assert_eq!(
                context.terminate(pid.get(), signal),
                Err(AsyncHostError::Inval)
            );
        }
        assert!(!receiver.request.is_requested());
        assert_eq!(context.terminate(pid.get(), libc::SIGTERM), Ok(()));
        assert_eq!(
            receiver.close(),
            Some(RunTermination::KilledBySignal(libc::SIGTERM))
        );
        // A signal racing completed teardown must not change the exit result.
        assert_eq!(context.terminate(pid.get(), libc::SIGINT), Ok(()));
    }

    #[cfg(windows)]
    #[test]
    fn process_handles_retain_completion_after_reaping() {
        let parent = VirtualPid(1);
        let child = VirtualPid(2);
        let completion = Arc::new(ProcessCompletion::default());
        let owner = ProcessContext {
            pid: parent,
            table: Arc::new(Mutex::new(BTreeMap::from([(
                child,
                Entry {
                    info: ProcessInfo {
                        pid: child,
                        parent: Some(parent),
                        state: ProcessState::Running,
                    },
                    terminator: crate::termination_channel().0,
                    signals: crate::signal_channel().0,
                    completion: Arc::clone(&completion),
                    waiting: false,
                },
            )]))),
        };
        let first = ProcessHandle::open(&owner, child.get()).unwrap();
        let second = ProcessHandle::open(&owner, child.get()).unwrap();
        assert!(first.matches(&owner, child.get()));
        assert!(!first.matches(&owner, parent.get()));
        let stranger = ProcessContext {
            pid: parent,
            table: Default::default(),
        };
        assert!(!first.matches(&stranger, child.get()));
        assert_eq!(
            first.read_exit(Ok),
            Err(AsyncHostError::Native(
                windows_sys::Win32::Foundation::ERROR_IO_PENDING as i32
            ))
        );
        completion
            .start_reaper(child)
            .unwrap()
            .send(std::thread::spawn(|| {
                Ok(RunOutcome::KilledBySignal(
                    windows_sys::Win32::System::Console::CTRL_BREAK_EVENT as i32,
                ))
            }))
            .unwrap();
        let code = 0xC000_013A_u32 as i32;
        let mut wait = first.wait();
        assert_eq!(wait.run(), Ok(i64::from(code)));
        wait.consume();
        assert_eq!(
            first.read_exit::<()>(|_| Err(AsyncHostError::Fault)),
            Err(AsyncHostError::Fault)
        );
        assert!(owner.table.lock().unwrap().contains_key(&child));
        assert_eq!(first.read_exit(Ok), Ok(code));
        assert!(owner.table.lock().unwrap().is_empty());
        // Model a supervisor taking the owned Run result. Handles still own
        // the status, and completion wins over cancellation on Windows.
        completion.result.lock().unwrap().take();
        let mut again = second.wait();
        again.cancellation().cancel().unwrap();
        assert_eq!(again.run(), Ok(i64::from(code)));
        again.consume();
        assert_eq!(second.read_exit(Ok), Ok(code));
        assert_eq!(first.read_exit(Ok), Ok(code));
    }

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
                    signals: crate::signal_channel().0,
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
            let mut completed_and_cancelled = context.wait(child.get()).unwrap();
            completed_and_cancelled.cancellation().cancel().unwrap();
            // Windows waits put the process before the cancellation event:
            // a completed process wins when both are ready at wait entry.
            #[cfg(windows)]
            assert_eq!(completed_and_cancelled.run(), Ok(42));
            #[cfg(unix)]
            assert_eq!(
                completed_and_cancelled.run(),
                Err(AsyncHostError::Native(libc::EINTR))
            );
            drop(completed_and_cancelled);
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
