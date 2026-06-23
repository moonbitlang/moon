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

use crate::async_sys::ported_fns;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HostWorkerJob {
    pub(crate) job_id: i32,
    pub(crate) job_handle: u64,
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
        mut run_job: impl FnMut(HostWorkerJob) + Send + 'static,
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
                    let state = worker_shared.state.lock().unwrap();
                    if state.terminating { None } else { state.job }
                };
                let Some(job) = job else {
                    break;
                };
                run_job(job);

                let terminating = {
                    let mut state = worker_shared.state.lock().unwrap();
                    if state.terminating {
                        true
                    } else {
                        state.waiting = true;
                        false
                    }
                };
                complete_job(job);
                if terminating {
                    break;
                }

                let mut state = worker_shared.state.lock().unwrap();
                while state.waiting && !state.terminating {
                    state = worker_shared.wakeup.wait(state).unwrap();
                }
                if state.terminating {
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

    pub(crate) fn wake(&self, job: Option<HostWorkerJob>) {
        let mut state = self.shared.state.lock().unwrap();
        // A missing job is only used by `free_worker`; keep it as an explicit
        // termination state so a wake sent during `run_job` is still observed.
        if job.is_none() {
            state.terminating = true;
        }
        state.job = job;
        state.waiting = false;
        self.shared.wakeup.notify_one();
    }

    pub(crate) fn enter_idle(&self) {
        self.shared.state.lock().unwrap().job = None;
    }

    pub(crate) fn cancel(&self) -> i32 {
        if self.shared.state.lock().unwrap().waiting {
            return 1;
        }

        #[cfg(unix)]
        {
            if let Some(thread_id) = *self.thread_id.lock().unwrap() {
                unsafe {
                    libc::pthread_kill(thread_id, libc::SIGUSR2);
                }
            }
            0
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
            use windows_sys::Win32::System::IO::CancelSynchronousIo;

            let Some(thread) = &self.thread else {
                return -1;
            };
            if unsafe { CancelSynchronousIo(thread.as_raw_handle()) } != 0 {
                1
            } else if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                0
            } else {
                -1
            }
        }
    }

    pub(crate) fn join(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.wake(None);
            let _ = thread.join();
        }
    }
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_spawn_worker"
    )]
    pub(crate) fn spawn_worker(
        init_job: HostWorkerJob,
        run_job: impl FnMut(HostWorkerJob) + Send + 'static,
        complete_job: impl FnMut(HostWorkerJob) + Send + 'static,
    ) -> HostWorkerHandle {
        HostWorkerHandle::spawn(init_job, run_job, complete_job)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_wake_worker"
    )]
    pub(crate) fn wake_worker(worker: &HostWorkerHandle, job: HostWorkerJob) {
        worker.wake(Some(job));
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_worker_enter_idle"
    )]
    pub(crate) fn worker_enter_idle(worker: &HostWorkerHandle) {
        worker.enter_idle();
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_cancel_worker"
    )]
    pub(crate) fn cancel_worker(worker: &HostWorkerHandle) -> i32 {
        worker.cancel()
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_free_worker"
    )]
    pub(crate) fn free_worker(mut worker: HostWorkerHandle) {
        worker.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn host_worker_runs_initial_job_then_waits_for_wake() {
        let (sender, receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            HostWorkerJob {
                job_id: 7,
                job_handle: 11,
            },
            move |job| sender.send(job).unwrap(),
            move |job| completion_sender.send(job).unwrap(),
        );

        assert_eq!(
            receiver.recv().unwrap(),
            HostWorkerJob {
                job_id: 7,
                job_handle: 11
            }
        );
        assert_eq!(
            completion_receiver.recv().unwrap(),
            HostWorkerJob {
                job_id: 7,
                job_handle: 11
            }
        );
        assert_eq!(cancel_worker(&worker), 1);

        wake_worker(
            &worker,
            HostWorkerJob {
                job_id: 13,
                job_handle: 17,
            },
        );
        assert_eq!(
            receiver.recv().unwrap(),
            HostWorkerJob {
                job_id: 13,
                job_handle: 17
            }
        );
        assert_eq!(
            completion_receiver.recv().unwrap(),
            HostWorkerJob {
                job_id: 13,
                job_handle: 17
            }
        );
        free_worker(worker);
    }

    #[test]
    fn worker_enter_idle_parks_until_next_wake() {
        let (sender, receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            HostWorkerJob {
                job_id: 1,
                job_handle: 2,
            },
            move |job| sender.send(job).unwrap(),
            move |job| completion_sender.send(job).unwrap(),
        );

        assert_eq!(receiver.recv().unwrap().job_id, 1);
        assert_eq!(completion_receiver.recv().unwrap().job_id, 1);

        worker_enter_idle(&worker);
        wake_worker(
            &worker,
            HostWorkerJob {
                job_id: 3,
                job_handle: 4,
            },
        );

        assert_eq!(receiver.recv().unwrap().job_id, 3);
        assert_eq!(completion_receiver.recv().unwrap().job_id, 3);
        free_worker(worker);
    }

    #[test]
    fn termination_wake_during_job_exits_after_completion() {
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            HostWorkerJob {
                job_id: 21,
                job_handle: 34,
            },
            move |job| {
                started_sender.send(job).unwrap();
                release_receiver.recv().unwrap();
            },
            move |job| completion_sender.send(job).unwrap(),
        );

        assert_eq!(started_receiver.recv().unwrap().job_id, 21);
        worker.wake(None);
        release_sender.send(()).unwrap();
        assert_eq!(completion_receiver.recv().unwrap().job_id, 21);

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
        free_worker(worker);
    }
}
