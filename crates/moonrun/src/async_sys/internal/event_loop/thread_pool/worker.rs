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

use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

#[cfg(windows)]
use crate::async_host::AsyncHostError;
use crate::async_host::{AsyncHostResult, HostJobKey};
use crate::async_sys::ported_fns;

use super::Job;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkerCompletionId(i32);

impl WorkerCompletionId {
    pub(crate) fn from_abi(value: i32) -> Self {
        Self(value)
    }

    pub(crate) fn as_i32(self) -> i32 {
        self.0
    }
}

#[derive(Debug)]
pub(crate) struct HostWorkerJob {
    pub(crate) completion_id: WorkerCompletionId,
    pub(crate) job_key: HostJobKey,
    pub(crate) job: Job,
}

#[derive(Debug)]
struct HostWorkerState {
    job: Option<HostWorkerJob>,
    waiting: bool,
    terminating: bool,
}

#[derive(Debug)]
struct HostWorkerShared {
    state: Mutex<HostWorkerState>,
    wakeup: Condvar,
}

// MoonBit owns the pool scheduler. Each host worker handle owns one long-lived
// OS thread and follows thread_pool.c's worker state machine: run current job,
// publish completion, wait until MoonBit either assigns another job or parks it.
pub(crate) struct HostWorkerHandle {
    shared: Arc<HostWorkerShared>,
    thread: Option<JoinHandle<()>>,
    #[cfg(unix)]
    thread_id: Arc<Mutex<Option<libc::pthread_t>>>,
}

impl std::fmt::Debug for HostWorkerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostWorkerHandle")
            .field(
                "alive",
                &self
                    .thread
                    .as_ref()
                    .is_some_and(|thread| !thread.is_finished()),
            )
            .field("state", &self.shared.state.lock().ok())
            .finish()
    }
}

#[cfg(unix)]
fn init_worker_signal_handler() {
    static INIT: std::sync::Once = std::sync::Once::new();

    extern "C" fn nop_signal_handler(_: i32) {}

    INIT.call_once(|| unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
        let mut action = std::mem::zeroed::<libc::sigaction>();
        action.sa_sigaction = nop_signal_handler as usize;
        libc::sigemptyset(&mut action.sa_mask);
        action.sa_flags = 0;
        libc::sigaction(libc::SIGUSR2, &action, std::ptr::null_mut());
    });
}

impl HostWorkerHandle {
    pub(crate) fn spawn(
        init_job: HostWorkerJob,
        mut run_job: impl FnMut(&mut HostWorkerJob) + Send + 'static,
        mut complete_job: impl FnMut(HostWorkerJob) + Send + 'static,
    ) -> Self {
        #[cfg(unix)]
        init_worker_signal_handler();

        let shared = Arc::new(HostWorkerShared {
            state: Mutex::new(HostWorkerState {
                job: Some(init_job),
                waiting: false,
                terminating: false,
            }),
            wakeup: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        #[cfg(unix)]
        let thread_id = Arc::new(Mutex::new(None));
        #[cfg(unix)]
        let worker_thread_id = Arc::clone(&thread_id);
        let thread = std::thread::spawn(move || {
            #[cfg(unix)]
            {
                *worker_thread_id.lock().unwrap() = Some(unsafe { libc::pthread_self() });
            }

            loop {
                let job = {
                    let mut state = worker_shared.state.lock().unwrap();
                    while state.job.is_none() && !state.terminating {
                        state.waiting = true;
                        state = worker_shared.wakeup.wait(state).unwrap();
                    }
                    (!state.terminating).then(|| state.job.take()).flatten()
                };
                let Some(mut job) = job else {
                    break;
                };
                run_job(&mut job);

                let terminating = {
                    let mut state = worker_shared.state.lock().unwrap();
                    if !state.terminating && state.job.is_none() {
                        state.waiting = true;
                    }
                    state.terminating
                };
                complete_job(job);
                if terminating {
                    break;
                }
            }

            #[cfg(unix)]
            {
                *worker_thread_id.lock().unwrap() = None;
            }
        });
        Self {
            shared,
            thread: Some(thread),
            #[cfg(unix)]
            thread_id,
        }
    }

    pub(crate) fn wake(&self, job: Option<HostWorkerJob>) -> Option<HostWorkerJob> {
        let mut state = self.shared.state.lock().unwrap();
        // A missing job is only used by `free_worker`; keep it as an explicit
        // termination state so a wake sent during `run_job` is still observed.
        if job.is_none() {
            state.terminating = true;
        }
        let previous_job = state.job.take();
        state.job = job;
        state.waiting = false;
        self.shared.wakeup.notify_one();
        previous_job
    }

    pub(crate) fn enter_idle(&self) -> Option<HostWorkerJob> {
        self.shared.state.lock().unwrap().job.take()
    }

    pub(crate) fn cancel(&self) -> AsyncHostResult<i32> {
        if self.shared.state.lock().unwrap().waiting {
            return Ok(1);
        }

        #[cfg(unix)]
        {
            if let Some(thread_id) = *self.thread_id.lock().unwrap() {
                unsafe {
                    libc::pthread_kill(thread_id, libc::SIGUSR2);
                }
            }
            Ok(0)
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
            use windows_sys::Win32::System::IO::CancelSynchronousIo;

            let Some(thread) = &self.thread else {
                return Err(AsyncHostError::Badf);
            };
            if unsafe { CancelSynchronousIo(thread.as_raw_handle()) } != 0 {
                Ok(1)
            } else {
                let error = unsafe { GetLastError() };
                if error == ERROR_NOT_FOUND {
                    Ok(0)
                } else {
                    Err(AsyncHostError::Native(error as i32))
                }
            }
        }
    }

    pub(crate) fn join(&mut self) -> Option<HostWorkerJob> {
        let previous_job = if self.thread.is_some() {
            self.wake(None)
        } else {
            None
        };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        previous_job
    }
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_spawn_worker"
    )]
    pub(crate) fn spawn_worker(
        init_job: HostWorkerJob,
        run_job: impl FnMut(&mut HostWorkerJob) + Send + 'static,
        complete_job: impl FnMut(HostWorkerJob) + Send + 'static,
    ) -> HostWorkerHandle {
        HostWorkerHandle::spawn(init_job, run_job, complete_job)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_wake_worker"
    )]
    pub(crate) fn wake_worker(
        worker: &HostWorkerHandle,
        job: HostWorkerJob,
    ) -> Option<HostWorkerJob> {
        worker.wake(Some(job))
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_worker_enter_idle"
    )]
    pub(crate) fn worker_enter_idle(worker: &HostWorkerHandle) -> Option<HostWorkerJob> {
        worker.enter_idle()
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_cancel_worker"
    )]
    pub(crate) fn cancel_worker(worker: &HostWorkerHandle) -> AsyncHostResult<i32> {
        worker.cancel()
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_free_worker"
    )]
    pub(crate) fn free_worker(mut worker: HostWorkerHandle) -> Option<HostWorkerJob> {
        worker.join()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_sys::internal::event_loop::thread_pool::make_sleep_job;
    use slotmap::KeyData;
    use std::sync::mpsc;

    fn make_job_key(value: u64) -> HostJobKey {
        KeyData::from_ffi(value).into()
    }

    fn worker_job_summary(job: &HostWorkerJob) -> (WorkerCompletionId, HostJobKey) {
        (job.completion_id, job.job_key)
    }

    fn make_worker_job(completion_id: i32, job_key: u64) -> HostWorkerJob {
        HostWorkerJob {
            completion_id: WorkerCompletionId::from_abi(completion_id),
            job_key: make_job_key(job_key),
            job: make_sleep_job(0),
        }
    }

    #[test]
    fn host_worker_runs_initial_job_then_waits_for_wake() {
        let (sender, receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            make_worker_job(7, 11),
            move |job| sender.send(worker_job_summary(job)).unwrap(),
            move |job| completion_sender.send(worker_job_summary(&job)).unwrap(),
        );

        assert_eq!(
            receiver.recv().unwrap(),
            (WorkerCompletionId::from_abi(7), make_job_key(11))
        );
        assert_eq!(
            completion_receiver.recv().unwrap(),
            (WorkerCompletionId::from_abi(7), make_job_key(11))
        );
        assert_eq!(cancel_worker(&worker), Ok(1));

        assert!(wake_worker(&worker, make_worker_job(13, 17)).is_none());
        assert_eq!(
            receiver.recv().unwrap(),
            (WorkerCompletionId::from_abi(13), make_job_key(17))
        );
        assert_eq!(
            completion_receiver.recv().unwrap(),
            (WorkerCompletionId::from_abi(13), make_job_key(17))
        );
        assert!(free_worker(worker).is_none());
    }

    #[test]
    fn worker_enter_idle_parks_until_next_wake() {
        let (sender, receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            make_worker_job(1, 2),
            move |job| sender.send(worker_job_summary(job)).unwrap(),
            move |job| completion_sender.send(worker_job_summary(&job)).unwrap(),
        );

        assert_eq!(receiver.recv().unwrap().0, WorkerCompletionId::from_abi(1));
        assert_eq!(
            completion_receiver.recv().unwrap().0,
            WorkerCompletionId::from_abi(1)
        );

        assert!(worker_enter_idle(&worker).is_none());
        assert!(wake_worker(&worker, make_worker_job(3, 4)).is_none());

        assert_eq!(receiver.recv().unwrap().0, WorkerCompletionId::from_abi(3));
        assert_eq!(
            completion_receiver.recv().unwrap().0,
            WorkerCompletionId::from_abi(3)
        );
        assert!(free_worker(worker).is_none());
    }

    #[test]
    fn wake_during_running_job_is_not_lost() {
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            make_worker_job(1, 2),
            move |job| {
                started_sender.send(worker_job_summary(job)).unwrap();
                if job.completion_id == WorkerCompletionId::from_abi(1) {
                    release_receiver.recv().unwrap();
                }
            },
            move |job| completion_sender.send(worker_job_summary(&job)).unwrap(),
        );

        assert_eq!(
            started_receiver.recv().unwrap().0,
            WorkerCompletionId::from_abi(1)
        );
        assert!(wake_worker(&worker, make_worker_job(3, 4)).is_none());
        release_sender.send(()).unwrap();

        assert_eq!(
            completion_receiver.recv().unwrap().0,
            WorkerCompletionId::from_abi(1)
        );
        assert_eq!(
            started_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap()
                .0,
            WorkerCompletionId::from_abi(3)
        );
        assert_eq!(
            completion_receiver.recv().unwrap().0,
            WorkerCompletionId::from_abi(3)
        );
        assert!(free_worker(worker).is_none());
    }

    #[test]
    fn completion_receives_job_mutated_by_runner() {
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            make_worker_job(1, 2),
            move |job| job.job.set_ret(123),
            move |job| completion_sender.send(job.job.ret()).unwrap(),
        );

        assert_eq!(completion_receiver.recv().unwrap(), 123);
        assert!(free_worker(worker).is_none());
    }

    #[test]
    fn worker_enter_idle_returns_queued_job() {
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            make_worker_job(1, 2),
            move |job| {
                started_sender.send(worker_job_summary(job)).unwrap();
                release_receiver.recv().unwrap();
            },
            move |job| completion_sender.send(worker_job_summary(&job)).unwrap(),
        );

        assert_eq!(
            started_receiver.recv().unwrap(),
            (WorkerCompletionId::from_abi(1), make_job_key(2))
        );
        assert!(wake_worker(&worker, make_worker_job(3, 4)).is_none());
        let displaced = worker_enter_idle(&worker).unwrap();
        assert_eq!(
            worker_job_summary(&displaced),
            (WorkerCompletionId::from_abi(3), make_job_key(4))
        );

        release_sender.send(()).unwrap();
        assert_eq!(
            completion_receiver.recv().unwrap(),
            (WorkerCompletionId::from_abi(1), make_job_key(2))
        );
        assert!(wake_worker(&worker, make_worker_job(5, 6)).is_none());
        assert_eq!(
            started_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            (WorkerCompletionId::from_abi(5), make_job_key(6))
        );
        release_sender.send(()).unwrap();
        assert_eq!(
            completion_receiver.recv().unwrap(),
            (WorkerCompletionId::from_abi(5), make_job_key(6))
        );
        assert!(free_worker(worker).is_none());
    }

    #[test]
    fn termination_wake_during_job_exits_after_completion() {
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            make_worker_job(21, 34),
            move |job| {
                started_sender.send(worker_job_summary(job)).unwrap();
                release_receiver.recv().unwrap();
            },
            move |job| completion_sender.send(worker_job_summary(&job)).unwrap(),
        );

        assert_eq!(
            started_receiver.recv().unwrap().0,
            WorkerCompletionId::from_abi(21)
        );
        assert!(worker.wake(None).is_none());
        release_sender.send(()).unwrap();
        assert_eq!(
            completion_receiver.recv().unwrap().0,
            WorkerCompletionId::from_abi(21)
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while worker
            .thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
            && std::time::Instant::now() < deadline
        {
            std::thread::yield_now();
        }
        assert!(
            worker
                .thread
                .as_ref()
                .is_some_and(|thread| thread.is_finished())
        );
        assert!(free_worker(worker).is_none());
    }
}
