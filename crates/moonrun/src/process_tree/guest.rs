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

use super::{VirtualPid, WaitClaim};
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
    pub(super) engine: crate::Engine,
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

    pub(crate) fn spawn_moon(
        &self,
        executable: &std::path::Path,
        args: &[std::ffi::OsString],
        options: crate::RunOptions,
        #[cfg(windows)] no_console_window: bool,
    ) -> AsyncHostResult<super::SpawnedProcess> {
        let executable = executable.to_owned();
        let args = args.to_vec();
        self.engine
            .spawn_execution(
                Some(self.pid),
                options,
                crate::signal_channel().1,
                move |engine, options, signals, termination| {
                    engine.execute_moon_in_scope(
                        &executable,
                        &args,
                        options,
                        signals,
                        termination,
                        #[cfg(windows)]
                        no_console_window,
                    )
                },
            )
            .map_err(|error| match error {
                super::ProcessError::Spawn(error) => error
                    .raw_os_error()
                    .map_or(AsyncHostError::Io, AsyncHostError::Native),
                super::ProcessError::ParentNotRunning(_) | super::ProcessError::NotFound(_) => {
                    AsyncHostError::PermissionDenied
                }
                _ => AsyncHostError::Io,
            })
    }

    #[cfg(unix)]
    pub(crate) fn read_exit<T>(
        &self,
        pid: i32,
        accept: impl FnOnce(i32) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let mut entries = self.engine.processes.0.lock().unwrap();
        let entry = entries
            .entries
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
        drop(entries.entries.remove(&VirtualPid(pid)));
        Ok(accepted)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn observe_exit(
        &self,
        pid: i32,
        create_wake: impl FnOnce() -> AsyncHostResult<Arc<std::task::Waker>>,
    ) -> AsyncHostResult<i32> {
        let entries = self.engine.processes.0.lock().unwrap();
        let entry = entries
            .entries
            .get(&VirtualPid(pid))
            .ok_or(AsyncHostError::PermissionDenied)?;
        if entry.info.parent != Some(self.pid) || entry.waiting {
            return Err(AsyncHostError::PermissionDenied);
        }
        // A supervisor can take the result only while entry.waiting is true.
        // With the table locked and that claim excluded, None means pending,
        // even on repeat registration. Exited alone does not imply TLS cleanup
        // and join have finished.
        let result = entry.completion.result.lock().unwrap();
        if result.is_some() {
            return Ok(0);
        }
        // Construct the Run-owned notifier only for an authorized, pending
        // child. The caller must retain a strong Arc and construction must not
        // re-enter the tree. Holding the result lock through registration
        // excludes publication and lost wakeups.
        let wake = Arc::downgrade(&create_wake()?);
        let mut notifications = entry.completion.notifications.lock().unwrap();
        notifications.retain(|notification| notification.strong_count() != 0);
        if !notifications
            .iter()
            .any(|notification| notification.ptr_eq(&wake))
        {
            notifications.push(wake);
        }
        Ok(1)
    }

    pub(crate) fn kill(&self, pid: i32) -> AsyncHostResult<()> {
        let entries = self.engine.processes.0.lock().unwrap();
        let entry = entries
            .entries
            .get(&VirtualPid(pid))
            .ok_or(AsyncHostError::PermissionDenied)?;
        if pid != self.pid.get() && entry.info.parent != Some(self.pid) {
            return Err(AsyncHostError::PermissionDenied);
        }
        #[cfg(unix)]
        let outcome = RunTermination::KilledBySignal(libc::SIGKILL);
        #[cfg(windows)]
        let outcome = RunTermination::Exit(1);
        entry.terminator.terminate(outcome);
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
        let entries = self.engine.processes.0.lock().unwrap();
        let entry = entries
            .entries
            .get(&VirtualPid(pid))
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
        entry
            .terminator
            .terminate(RunTermination::KilledBySignal(signal));
        Ok(())
    }

    pub(crate) fn wait(&self, pid: i32) -> AsyncHostResult<ProcessWait> {
        let pid = VirtualPid(pid);
        let entries = self.engine.processes.0.lock().unwrap();
        if entries
            .entries
            .get(&pid)
            .is_none_or(|entry| entry.info.parent != Some(self.pid))
        {
            return Err(AsyncHostError::PermissionDenied);
        }
        Ok(ProcessWait {
            pid,
            cancel: Arc::new(WaitCancellation {
                context: self.clone(),
                completion: Arc::clone(&entries.entries[&pid].completion),
                cancelled: AtomicBool::new(false),
            }),
            claim: None,
            claim_on_run: true,
            completed: false,
        })
    }

    fn reap_unclaimed(&self, pid: VirtualPid, completion: &Arc<super::ProcessCompletion>) {
        let mut entries = self.engine.processes.0.lock().unwrap();
        // Handle observations do not own an exclusive wait claim. Preserve
        // another waiter's result, and remove only the observed entry.
        if entries
            .entries
            .get(&pid)
            .is_some_and(|entry| !entry.waiting && Arc::ptr_eq(&entry.completion, completion))
        {
            entries.entries.remove(&pid);
        }
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
    pub(crate) fn from_spawn(owner: &ProcessContext, spawned: super::SpawnedProcess) -> Self {
        Self {
            owner: owner.clone(),
            pid: spawned.pid.get(),
            completion: spawned.completion,
        }
    }

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
            && Arc::ptr_eq(&self.owner.engine.processes.0, &owner.engine.processes.0)
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
            claim_on_run: false,
            completed: false,
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
        self.owner
            .reap_unclaimed(VirtualPid(self.pid), &self.completion);
        Ok(accepted)
    }
}

pub(crate) struct ProcessWait {
    pid: VirtualPid,
    cancel: Arc<WaitCancellation>,
    claim: Option<WaitClaim>,
    claim_on_run: bool,
    completed: bool,
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
        let table = &self.cancel.context.engine.processes.0;
        let claim = if self.claim_on_run {
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
                self.completed = true;
                return Ok(i64::from(status));
            }
            result = self.cancel.completion.changed.wait(result).unwrap();
        }
    }

    /// Reading the completed Job result commits the wait. Freeing an unread
    /// Job (including Runtime teardown) instead leaves the exit for its owner.
    pub(crate) fn consume(&mut self) {
        // An untrusted guest can read ret before running the Job, when err is
        // still zero, or after cancelling it. Only a successful wait may reap.
        if !std::mem::take(&mut self.completed) {
            return;
        }
        if let Some(claim) = self.claim.take() {
            claim.table.lock().unwrap().entries.remove(&claim.pid);
        } else if !self.claim_on_run {
            // Windows handles retain their cached status independently; the
            // guest need not call get_process_result to release this entry.
            self.cancel
                .context
                .reap_unclaimed(self.pid, &self.cancel.completion);
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

    fn test_table(entries: BTreeMap<VirtualPid, Entry>) -> super::super::Table {
        Arc::new(Mutex::new(super::super::TableState {
            entries,
            shutting_down: false,
        }))
    }

    fn test_engine(table: super::super::Table) -> crate::Engine {
        let mut engine = crate::Engine::default();
        engine.processes = super::super::ProcessTable(table);
        engine
    }

    #[cfg(unix)]
    #[test]
    fn signal_probe_and_unsupported_signals_do_not_terminate() {
        let pid = VirtualPid(1);
        let (terminator, receiver) = crate::termination_channel();
        let (signals, _signals) = crate::signal_channel();
        let context = ProcessContext {
            pid,
            engine: test_engine(test_table(BTreeMap::from([(
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

    #[cfg(target_os = "macos")]
    #[test]
    fn completion_observers_survive_races_and_detach_independently() {
        struct Notify(mpsc::Sender<i32>, i32);
        impl std::task::Wake for Notify {
            fn wake(self: Arc<Self>) {
                self.0.send(self.1).unwrap();
            }
        }
        for race in [false, true] {
            let parent = VirtualPid(1);
            let child = VirtualPid(2);
            let completion = Arc::new(ProcessCompletion::default());
            let context = ProcessContext {
                pid: parent,
                engine: test_engine(test_table(BTreeMap::from([(
                    child,
                    Entry {
                        info: ProcessInfo {
                            pid: child,
                            parent: Some(parent),
                            state: ProcessState::Running,
                        },
                        terminator: crate::termination_channel().0,
                        signals: crate::signal_channel().0,
                        completion: completion.clone(),
                        waiting: false,
                    },
                )]))),
            };
            let (send, received) = mpsc::channel();
            let cancelled = Arc::new(std::task::Waker::from(Arc::new(Notify(send.clone(), 1))));
            let retained = Arc::new(std::task::Waker::from(Arc::new(Notify(send.clone(), 2))));
            let concurrent = Arc::new(std::task::Waker::from(Arc::new(Notify(send, 3))));
            assert_eq!(
                context.observe_exit(child.get(), || Ok(cancelled.clone())),
                Ok(1)
            );
            for _ in 0..2 {
                assert_eq!(
                    context.observe_exit(child.get(), || Ok(retained.clone())),
                    Ok(1)
                );
            }
            let claim = WaitClaim::new(context.engine.processes.0.clone(), child).unwrap();
            assert_eq!(
                context.observe_exit(child.get(), || panic!("claimed by supervisor")),
                Err(AsyncHostError::PermissionDenied),
            );
            drop(claim);
            drop(cancelled);
            let pending = if race {
                let barrier = Arc::new(std::sync::Barrier::new(2));
                let start = barrier.clone();
                let finished = completion.clone();
                let publisher = std::thread::spawn(move || {
                    start.wait();
                    finished.publish(Ok(RunOutcome::Exited(23)));
                });
                barrier.wait();
                let pending = context
                    .observe_exit(child.get(), || Ok(concurrent.clone()))
                    .unwrap();
                publisher.join().unwrap();
                pending
            } else {
                let pending = context
                    .observe_exit(child.get(), || Ok(concurrent.clone()))
                    .unwrap();
                completion.publish(Ok(RunOutcome::Exited(23)));
                pending
            };
            let mut notifications: Vec<_> = received.try_iter().collect();
            notifications.sort();
            assert_eq!(
                notifications,
                if pending == 1 { vec![2, 3] } else { vec![2] }
            );
            assert_eq!(
                context.observe_exit(child.get(), || panic!("already complete")),
                Ok(0)
            );
            // Taking the owned result must never turn the process back into
            // an observable pending completion while the table entry remains.
            let claim = WaitClaim::new(context.engine.processes.0.clone(), child).unwrap();
            let result = completion.result.lock().unwrap().take();
            assert_eq!(
                context.observe_exit(child.get(), || panic!("result taken by supervisor")),
                Err(AsyncHostError::PermissionDenied),
            );
            *completion.result.lock().unwrap() = result;
            drop(claim);
            // Observation never consumes the exit, even after its owner closes.
            drop((retained, concurrent));
            assert_eq!(context.read_exit(child.get(), Ok), Ok(23));
            assert_eq!(
                context.observe_exit(child.get(), || panic!("already reaped")),
                Err(AsyncHostError::PermissionDenied),
            );
            assert!(
                context
                    .engine
                    .processes
                    .0
                    .lock()
                    .unwrap()
                    .entries
                    .is_empty()
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn parent_poll_owns_virtual_process_observation() {
        use crate::runtime::{Executable, Runtime};

        for finish in ["event", "ready", "closed"] {
            let child = VirtualPid(2);
            let completion = Arc::new(ProcessCompletion::default());
            let context = ProcessContext {
                pid: VirtualPid(1),
                engine: test_engine(test_table(BTreeMap::from([(
                    child,
                    Entry {
                        info: ProcessInfo {
                            pid: child,
                            parent: Some(VirtualPid(1)),
                            state: ProcessState::Running,
                        },
                        terminator: crate::termination_channel().0,
                        signals: crate::signal_channel().0,
                        completion: completion.clone(),
                        waiting: false,
                    },
                )]))),
            };
            let runtime = Runtime::new(
                crate::runtime::RuntimeConfig {
                    process_context: Some(context.clone()),
                    ..Default::default()
                },
                Executable::unavailable(),
                crate::signal_channel().1,
            )
            .unwrap();
            let host = runtime.async_host();
            let poll = host.poll_create().unwrap();
            assert_eq!(host.poll_register_pid(poll, child.get()), Ok(1));
            let observer = completion.notifications.lock().unwrap()[0].clone();
            assert_eq!(host.poll_register_pid(poll, child.get()), Ok(1));
            assert_eq!(completion.notifications.lock().unwrap().len(), 1);
            assert_eq!(observer.strong_count(), 1);
            let claim = WaitClaim::new(context.engine.processes.0.clone(), child).unwrap();
            assert_eq!(
                host.poll_register_pid(poll, child.get()),
                Err(AsyncHostError::PermissionDenied)
            );
            assert_eq!(observer.strong_count(), 1);
            drop(claim);

            if finish == "closed" {
                host.poll_destroy(poll).unwrap();
                assert_eq!(observer.strong_count(), 0);
                let replacement = host.poll_create().unwrap();
                completion.publish(Ok(RunOutcome::Exited(23)));
                assert_eq!(host.poll_wait(replacement, 0, &Default::default()), Ok(0));
                host.poll_destroy(replacement).unwrap();
            } else {
                completion.publish(Ok(RunOutcome::Exited(23)));
                if finish == "event" {
                    assert_eq!(host.poll_wait(poll, 0, &Default::default()), Ok(1));
                    let event = host.poll_get_event(poll, 0).unwrap();
                    assert_eq!(host.poll_event_pid(event), Ok(child.get()));
                    assert_eq!(host.poll_event_events(event), Ok(4));
                }
                assert_eq!(host.poll_register_pid(poll, child.get()), Ok(0));
                assert_eq!(observer.strong_count(), 0);
                assert_eq!(host.poll_wait(poll, 0, &Default::default()), Ok(0));
                host.poll_destroy(poll).unwrap();
            }
            assert_eq!(context.read_exit(child.get(), Ok), Ok(23));
        }
    }

    #[cfg(windows)]
    #[test]
    fn process_handles_retain_completion_after_reaping() {
        let parent = VirtualPid(1);
        let child = VirtualPid(2);
        let completion = Arc::new(ProcessCompletion::default());
        let owner = ProcessContext {
            pid: parent,
            engine: test_engine(test_table(BTreeMap::from([(
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
            engine: test_engine(Default::default()),
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
        assert_eq!(
            first.read_exit::<()>(|_| Err(AsyncHostError::Fault)),
            Err(AsyncHostError::Fault)
        );
        assert!(
            owner
                .engine
                .processes
                .0
                .lock()
                .unwrap()
                .entries
                .contains_key(&child)
        );
        wait.consume();
        assert!(owner.engine.processes.0.lock().unwrap().entries.is_empty());
        assert_eq!(first.read_exit(Ok), Ok(code));
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

    #[cfg(windows)]
    #[test]
    fn spawn_handle_can_be_published_after_supervisor_reaps_pid() {
        let child = VirtualPid(2);
        let completion = Arc::new(ProcessCompletion::default());
        let owner = ProcessContext {
            pid: VirtualPid(1),
            engine: test_engine(Default::default()),
        };
        // Admission captured this before releasing the table lock. By the time
        // the guest obtains its HANDLE the supervisor may have removed the row.
        let spawned = super::super::SpawnedProcess {
            pid: child,
            completion: completion.clone(),
        };
        completion.publish(Ok(RunOutcome::Exited(23)));
        completion.result.lock().unwrap().take();
        let handle = ProcessHandle::from_spawn(&owner, spawned);
        assert!(handle.matches(&owner, child.get()));
        assert_eq!(handle.wait().run(), Ok(23));
        assert_eq!(handle.read_exit(Ok), Ok(23));
    }

    #[test]
    fn invalid_virtual_spawn_inputs_do_not_admit_a_process() {
        let cwd = tempfile::tempdir().unwrap();
        let tree = test_engine(Default::default());
        let process = crate::process::HostProcess::new(
            None,
            None,
            Arc::new(crate::runtime::WorkingDirectory::Fixed(
                cwd.path().to_owned(),
            )),
            Arc::new(crate::runtime::Stdio::Ambient),
            Some(ProcessContext {
                pid: VirtualPid(1),
                engine: tree.clone(),
            }),
            Arc::new(crate::runtime::Env::owned([]).unwrap()),
        );
        // Use an existing executable so malformed payloads are tested
        // independently of executable-not-found errors. Validation must fail
        // before any virtual child is admitted, even for a non-Moon image.
        #[cfg(windows)]
        let executable = std::env::current_exe().unwrap();
        for (argument, environment) in [("bad\0arg", "A=B"), ("ok", "=invalid")] {
            #[cfg(unix)]
            let mut job = crate::process::Job::spawn_unix(
                "moon".into(),
                vec!["moon".into(), argument.into()],
                vec![environment.into()],
                None,
                None,
                None,
                None,
                crate::process::SpawnOptions {
                    child_signal_mask: unsafe { std::mem::zeroed() },
                },
            );
            #[cfg(windows)]
            let mut job = crate::process::Job::spawn_windows(
                format!("\"{}\" {argument}", executable.display()).into(),
                format!("{environment}\0\0").encode_utf16().collect(),
                None,
                None,
                None,
                None,
                crate::process::SpawnOptions {
                    no_console_window: false,
                    is_orphan: false,
                },
            );
            assert_eq!(process.prepare_job(&mut job), Err(AsyncHostError::Inval));
            assert!(tree.snapshot().is_empty());
        }
    }

    #[test]
    fn handle_wait_consumption_requires_successful_completion() {
        // Exercise the non-exclusive wait used by Windows handles without
        // relying on guest code to follow async's normal call sequence.
        let child = VirtualPid(2);
        let completion = Arc::new(ProcessCompletion::default());
        let owner = ProcessContext {
            pid: VirtualPid(1),
            engine: test_engine(test_table(BTreeMap::from([(
                child,
                Entry {
                    info: ProcessInfo {
                        pid: child,
                        parent: Some(VirtualPid(1)),
                        state: ProcessState::Running,
                    },
                    terminator: crate::termination_channel().0,
                    signals: crate::signal_channel().0,
                    completion: completion.clone(),
                    waiting: false,
                },
            )]))),
        };
        let mut cancelled = owner.wait(child.get()).unwrap();
        cancelled.claim_on_run = false;
        cancelled.consume();
        assert!(
            owner
                .engine
                .processes
                .0
                .lock()
                .unwrap()
                .entries
                .contains_key(&child)
        );
        cancelled.cancellation().cancel().unwrap();
        assert!(cancelled.run().is_err());
        completion.publish(Ok(RunOutcome::Exited(23)));
        cancelled.consume();
        assert!(
            owner
                .engine
                .processes
                .0
                .lock()
                .unwrap()
                .entries
                .contains_key(&child)
        );

        let mut unread = owner.wait(child.get()).unwrap();
        unread.claim_on_run = false;
        // Even an already-exited process must not be reaped by reading an
        // unexecuted Job's default return value.
        unread.consume();
        assert!(
            owner
                .engine
                .processes
                .0
                .lock()
                .unwrap()
                .entries
                .contains_key(&child)
        );
        assert_eq!(unread.run(), Ok(23));
        drop(unread);
        assert!(
            owner
                .engine
                .processes
                .0
                .lock()
                .unwrap()
                .entries
                .contains_key(&child)
        );

        let mut accepted = owner.wait(child.get()).unwrap();
        accepted.claim_on_run = false;
        assert_eq!(accepted.run(), Ok(23));
        let claim = WaitClaim::new(owner.engine.processes.0.clone(), child).unwrap();
        accepted.consume();
        assert!(owner.engine.processes.0.lock().unwrap().entries[&child].waiting);
        drop(claim);
        let mut accepted = owner.wait(child.get()).unwrap();
        accepted.claim_on_run = false;
        assert_eq!(accepted.run(), Ok(23));
        accepted.consume();
        accepted.consume();
        assert!(owner.engine.processes.0.lock().unwrap().entries.is_empty());
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
            let table = test_table(BTreeMap::from([(
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
            )]));
            let context = ProcessContext {
                pid: parent,
                engine: test_engine(Arc::clone(&table)),
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
                while !table.lock().unwrap().entries[&child].waiting {
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
            assert!(!table.lock().unwrap().entries[&child].waiting);
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
            assert!(table.lock().unwrap().entries[&child].waiting);
            drop(unread);
            assert!(!table.lock().unwrap().entries[&child].waiting);
            let mut accepted = context.wait(child.get()).unwrap();
            assert_eq!(accepted.run(), Ok(42));
            accepted.consume();
            accepted.consume();
            assert!(table.lock().unwrap().entries.is_empty());
        }
    }
}
