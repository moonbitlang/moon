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

use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::JoinHandle;

use crate::async_host::{AsyncHostError, AsyncHostResult, HandleKey};
use crate::async_sys::ported_fns;
#[cfg(windows)]
use crate::resource::ResourceRef;
#[cfg(unix)]
use std::os::unix::thread::JoinHandleExt;

use super::super::PipeCompletionNotifier;
#[cfg(unix)]
use super::JobCancellation;
use super::cancellation::WorkerCancellation;
use super::{CancellationOutcome, Job};
#[cfg(unix)]
use crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier;

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
    pub(crate) job_key: HandleKey,
    pub(crate) job: Job,
    #[cfg(windows)]
    cancel: Option<ResourceRef>,
}

impl HostWorkerJob {
    pub(crate) fn new(completion_id: WorkerCompletionId, job_key: HandleKey, job: Job) -> Self {
        #[cfg(windows)]
        let cancel = job.cancellation_resource();
        Self {
            completion_id,
            job_key,
            job,
            #[cfg(windows)]
            cancel,
        }
    }
}

#[derive(Debug)]
pub(crate) struct HostWorkerJobResult {
    pub(crate) job_key: HandleKey,
    pub(crate) job: Job,
}

#[derive(Debug, Clone)]
enum WorkerCancellationTarget {
    Thread,
    #[cfg(unix)]
    Override(JobCancellation),
    #[cfg(windows)]
    Resource(ResourceRef),
}

#[derive(Debug)]
// This slot lives in the Worker's shared allocation. Keeping its Job inline
// avoids another allocation and indirection for the queued payload.
#[allow(clippy::large_enum_variant)]
enum Admission {
    Open(Option<HostWorkerJob>),
    Closed,
}

#[derive(Debug)]
enum Activity {
    // No Job is executing; notification delivery may still be in progress.
    Idle,
    Running(WorkerCancellationTarget),
}

#[derive(Debug)]
struct HostWorkerState {
    // Admission and execution are independent: closing returns queued work
    // while preserving the active operation's cancellation target.
    admission: Admission,
    activity: Activity,
}

// A Worker chooses exactly one completion destination at spawn and retains it
// for its lifetime. Cancellation checks the same destination used for delivery.
pub(crate) enum WorkerCompletionDestination {
    // Async's Completion Source also supports cancellation retry notifications.
    #[cfg(unix)]
    Default(Arc<ThreadPoolCompletionNotifier>),
    #[cfg(windows)]
    Default(super::super::poll::CompletionPort),
    // A supplied pipe carries only finished Job IDs.
    Pipe(PipeCompletionNotifier),
    // Lifecycle tests can observe or pause notification after real publication.
    #[cfg(test)]
    Test(Box<dyn Fn(WorkerCompletionId) + Send + Sync>),
}

struct HostWorkerShared {
    completion: WorkerCompletionDestination,
    cancellation: WorkerCancellation,
    state: Mutex<HostWorkerState>,
    wakeup: Condvar,
}

// MoonBit owns the pool scheduler. Each host worker handle owns one long-lived
// OS thread and follows thread_pool.c's worker state machine: run current job,
// publish completion, wait until MoonBit either assigns another job or parks it.
// Joining consumes the handle, so every usable handle still owns its thread.
pub(crate) struct HostWorkerHandle {
    shared: Arc<HostWorkerShared>,
    thread: JoinHandle<()>,
}

impl std::fmt::Debug for HostWorkerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostWorkerHandle")
            .field("alive", &!self.thread.is_finished())
            .field("state", &self.shared.state.lock().ok())
            .finish()
    }
}

#[cfg(unix)]
fn init_worker_signal_handler() {
    static INIT: std::sync::Once = std::sync::Once::new();

    INIT.call_once(|| unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
        let mut action = std::mem::zeroed::<libc::sigaction>();
        let signal_handler: extern "C" fn(i32) = super::cancellation::cancellation_signal_handler;
        action.sa_sigaction = signal_handler as usize;
        libc::sigemptyset(&mut action.sa_mask);
        action.sa_flags = 0;
        libc::sigaction(libc::SIGUSR2, &action, std::ptr::null_mut());
    });
}

impl HostWorkerHandle {
    pub(crate) fn spawn(
        init_job: HostWorkerJob,
        mut run_job: impl FnMut(&mut HostWorkerJob) + Send + 'static,
        completed: mpsc::Sender<HostWorkerJobResult>,
        completion: WorkerCompletionDestination,
    ) -> Self {
        #[cfg(unix)]
        init_worker_signal_handler();
        let shared = Arc::new(HostWorkerShared {
            completion,
            cancellation: WorkerCancellation::new(),
            state: Mutex::new(HostWorkerState {
                admission: Admission::Open(Some(init_job)),
                activity: Activity::Idle,
            }),
            wakeup: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        #[cfg(unix)]
        let parent_signal_mask = crate::async_sys::signal::block_worker_thread_signals().ok();
        let thread = std::thread::spawn(move || {
            #[cfg(unix)]
            {
                let _ = crate::async_sys::signal::unblock_worker_cancellation_signal();
            }

            loop {
                let job = {
                    let mut state = worker_shared.state.lock().unwrap();
                    while matches!(state.admission, Admission::Open(None)) {
                        state = worker_shared.wakeup.wait(state).unwrap();
                    }
                    let job = match &mut state.admission {
                        Admission::Open(job) => job.take(),
                        Admission::Closed => None,
                    };
                    if let Some(job) = &job {
                        // The payload moves onto the executing thread. Retain
                        // only the target needed to cancel that operation.
                        #[cfg(unix)]
                        let target = job.job.cancellation_override().map_or(
                            WorkerCancellationTarget::Thread,
                            WorkerCancellationTarget::Override,
                        );
                        #[cfg(windows)]
                        let target = job.cancel.clone().map_or(
                            WorkerCancellationTarget::Thread,
                            WorkerCancellationTarget::Resource,
                        );
                        state.activity = Activity::Running(target);
                    }
                    job
                };
                let Some(mut job) = job else {
                    break;
                };
                worker_shared
                    .cancellation
                    .run(job.completion_id, || run_job(&mut job));
                let completion_id = job.completion_id;
                let _ = completed.send(HostWorkerJobResult {
                    job_key: job.job_key,
                    job: job.job,
                });

                let terminating = {
                    let mut state = worker_shared.state.lock().unwrap();
                    state.activity = Activity::Idle;
                    // Publish Job payload -> Waiting -> notify. A pending
                    // cancellation-retry event must never expose an unfinished
                    // payload as a completed Job.
                    worker_shared.cancellation.finish();
                    if matches!(state.admission, Admission::Open(Some(_))) {
                        // An early wake from a direct caller advances cancellation
                        // to the next Job. Async's guest accepts the previous
                        // completion before waking us again.
                        worker_shared.cancellation.start();
                    }
                    matches!(state.admission, Admission::Closed)
                };
                match &worker_shared.completion {
                    #[cfg(unix)]
                    WorkerCompletionDestination::Default(notifier) => {
                        let _ = notifier.notify(completion_id.as_i32());
                    }
                    #[cfg(windows)]
                    WorkerCompletionDestination::Default(port) => {
                        let _ = super::super::poll::post_thread_pool_completion(
                            port,
                            completion_id.as_i32(),
                        );
                    }
                    #[cfg(test)]
                    WorkerCompletionDestination::Test(notify) => notify(completion_id),
                    WorkerCompletionDestination::Pipe(pipe) => {
                        // Delivery is outside the Job's cancellation scope;
                        // only Worker teardown interrupts a full pipe.
                        // Reuse the state read above for the first write; the
                        // notifier checks again only if delivery needs to wait
                        // or retry. Teardown can race either check with a write.
                        if !terminating {
                            let _ = pipe.notify(completion_id.as_i32(), &|| {
                                matches!(
                                    worker_shared.state.lock().unwrap().admission,
                                    Admission::Closed
                                )
                            });
                        }
                    }
                }
                if terminating {
                    break;
                }
            }
        });
        #[cfg(unix)]
        {
            if let Some(parent_signal_mask) = parent_signal_mask {
                let _ =
                    crate::async_sys::signal::restore_thread_pool_signal_mask(&parent_signal_mask);
            }
        }
        Self { shared, thread }
    }

    /// Return any displaced queued Job, or the submitted Job if stopping.
    pub(crate) fn submit(&self, job: HostWorkerJob) -> Option<HostWorkerJob> {
        let mut state = self.shared.state.lock().unwrap();
        let previous_job = match &mut state.admission {
            Admission::Open(pending) => pending.replace(job),
            Admission::Closed => return Some(job),
        };
        if matches!(state.activity, Activity::Idle) {
            self.shared.cancellation.start();
        }
        self.shared.wakeup.notify_one();
        previous_job
    }

    /// Close admission and return any queued Job; the active Job finishes normally.
    fn request_stop(&self) -> Option<HostWorkerJob> {
        let mut state = self.shared.state.lock().unwrap();
        let pending = match std::mem::replace(&mut state.admission, Admission::Closed) {
            Admission::Open(pending) => pending,
            Admission::Closed => None,
        };
        self.shared.wakeup.notify_one();
        pending
    }

    pub(crate) fn enter_idle(&self) -> Option<HostWorkerJob> {
        let mut state = self.shared.state.lock().unwrap();
        match &mut state.admission {
            Admission::Open(pending) => pending.take(),
            Admission::Closed => None,
        }
    }

    fn cancellation_target(&self) -> WorkerCancellationTarget {
        let state = self.shared.state.lock().unwrap();
        match (&state.activity, &state.admission) {
            (Activity::Running(target), _) => target.clone(),
            // Unix overrides support cancellation before execution. Their
            // queued Job is still present, so no second copy needs updating.
            #[cfg(unix)]
            (Activity::Idle, Admission::Open(Some(job))) => job.job.cancellation_override().map_or(
                WorkerCancellationTarget::Thread,
                WorkerCancellationTarget::Override,
            ),
            // Windows queues target the thread until the operation starts;
            // CancelSynchronousIo can return RetryLater during that transition.
            _ => WorkerCancellationTarget::Thread,
        }
    }

    pub(crate) fn cancel(&self) -> AsyncHostResult<CancellationOutcome> {
        #[cfg(unix)]
        self.shared.cancellation.disable_retry();
        self.cancel_inner()
    }

    /// The host validates registration lifetime; the Worker retains transport
    /// ownership. Reinitializing the pool must not redirect an old Worker's retries.
    #[cfg(unix)]
    pub(crate) fn check_completion_source(
        &self,
        source: &Arc<ThreadPoolCompletionNotifier>,
    ) -> AsyncHostResult<()> {
        match &self.shared.completion {
            WorkerCompletionDestination::Default(bound) if Arc::ptr_eq(bound, source) => Ok(()),
            WorkerCompletionDestination::Default(_) => Err(AsyncHostError::Badf),
            _ => Err(AsyncHostError::Inval),
        }
    }

    /// Inspect or cancel the current Job, not an earlier completion ID.
    /// The guest must accept completion before assigning the next Job; an
    /// early wake can advance this state before the old notification is read.
    pub(crate) fn cancel_with_retry(&self) -> AsyncHostResult<CancellationOutcome> {
        // Retry IDs belong to async's default completion source. A supplied
        // pipe carries finished Jobs only and cannot route those retries.
        let WorkerCompletionDestination::Default(_source) = &self.shared.completion else {
            return Err(AsyncHostError::Inval);
        };
        if self.shared.cancellation.is_waiting() {
            return Ok(CancellationOutcome::JobFinished);
        }
        #[cfg(unix)]
        self.shared.cancellation.enable_retry(_source);
        self.cancel_inner()
    }

    fn cancel_inner(&self) -> AsyncHostResult<CancellationOutcome> {
        if !self.shared.cancellation.request() {
            return Ok(CancellationOutcome::NeedWait);
        }
        #[cfg(unix)]
        {
            if let WorkerCancellationTarget::Override(cancel) = self.cancellation_target() {
                return cancel.cancel();
            }
            unsafe {
                libc::pthread_kill(self.thread.as_pthread_t(), libc::SIGUSR2);
            }
            Ok(if self.shared.cancellation.retry_enabled() {
                CancellationOutcome::NeedWait
            } else {
                CancellationOutcome::RetryLater
            })
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
            use windows_sys::Win32::System::IO::CancelSynchronousIo;

            if let WorkerCancellationTarget::Resource(cancel) = self.cancellation_target() {
                crate::process::cancel_wait(&cancel)?;
                return Ok(CancellationOutcome::NeedWait);
            }
            if unsafe { CancelSynchronousIo(self.thread.as_raw_handle()) } != 0 {
                Ok(CancellationOutcome::NeedWait)
            } else {
                let error = unsafe { GetLastError() };
                if error == ERROR_NOT_FOUND {
                    Ok(CancellationOutcome::RetryLater)
                } else {
                    Err(AsyncHostError::Native(error as i32))
                }
            }
        }
    }

    pub(crate) fn join(self) -> Option<HostWorkerJob> {
        let pending = self.request_stop();
        let _ = self.thread.join();
        pending
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
        completed: mpsc::Sender<HostWorkerJobResult>,
        completion: WorkerCompletionDestination,
    ) -> HostWorkerHandle {
        HostWorkerHandle::spawn(init_job, run_job, completed, completion)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_wake_worker"
    )]
    pub(crate) fn wake_worker(
        worker: &HostWorkerHandle,
        job: HostWorkerJob,
    ) -> Option<HostWorkerJob> {
        worker.submit(job)
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
    pub(crate) fn cancel_worker_with_retry(
        worker: &HostWorkerHandle,
    ) -> AsyncHostResult<CancellationOutcome> {
        worker.cancel_with_retry()
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_free_worker"
    )]
    pub(crate) fn free_worker(worker: HostWorkerHandle) -> Option<HostWorkerJob> {
        worker.join()
    }
}

pub(crate) fn cancel_worker(worker: &HostWorkerHandle) -> AsyncHostResult<CancellationOutcome> {
    worker.cancel()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_sys::internal::event_loop::thread_pool::make_sleep_job;
    #[cfg(windows)]
    use crate::process::Job as ProcessJob;
    use slotmap::KeyData;
    use std::sync::mpsc;

    fn spawn_worker(
        job: HostWorkerJob,
        run: impl FnMut(&mut HostWorkerJob) + Send + 'static,
        complete: impl Fn(HostWorkerJobResult) + Send + Sync + 'static,
    ) -> HostWorkerHandle {
        // Observe the real result channel after notification, so tests exercise
        // the same publication order as production.
        let (completed, results) = mpsc::channel();
        let results = Mutex::new(results);
        super::spawn_worker(
            job,
            run,
            completed,
            WorkerCompletionDestination::Test(Box::new(move |_| {
                complete(results.lock().unwrap().try_recv().unwrap())
            })),
        )
    }

    fn make_job_key(value: u64) -> HandleKey {
        KeyData::from_ffi(value).into()
    }

    fn worker_job_summary(job: &HostWorkerJob) -> (WorkerCompletionId, HandleKey) {
        (job.completion_id, job.job_key)
    }

    fn make_worker_job(completion_id: i32, job_key: u64) -> HostWorkerJob {
        HostWorkerJob::new(
            WorkerCompletionId::from_abi(completion_id),
            make_job_key(job_key),
            make_sleep_job(0),
        )
    }

    #[test]
    fn host_worker_runs_initial_job_then_waits_for_wake() {
        let (sender, receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            make_worker_job(7, 11),
            move |job| {
                sender.send(worker_job_summary(job)).unwrap();
            },
            move |job| completion_sender.send(job.job_key).unwrap(),
        );

        assert_eq!(
            receiver.recv().unwrap(),
            (WorkerCompletionId::from_abi(7), make_job_key(11))
        );
        assert_eq!(completion_receiver.recv().unwrap(), make_job_key(11));
        assert_eq!(cancel_worker(&worker), Ok(CancellationOutcome::NeedWait));

        assert!(wake_worker(&worker, make_worker_job(13, 17)).is_none());
        assert_eq!(
            receiver.recv().unwrap(),
            (WorkerCompletionId::from_abi(13), make_job_key(17))
        );
        assert_eq!(completion_receiver.recv().unwrap(), make_job_key(17));
        assert!(free_worker(worker).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn terminating_worker_publishes_job_without_writing_completion() {
        use crate::async_sys::internal::fd_util;
        use crate::resource::Resource;
        use std::os::fd::AsRawFd;
        use std::time::Duration;

        let [reader, writer] = fd_util::stub::pipe(true, true).unwrap();
        let reader = Resource::new(reader);
        let writer =
            PipeCompletionNotifier::new(Arc::new(Resource::async_pipe_writer(writer))).unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let (completed_tx, completed_rx) = mpsc::channel();
        let worker = super::spawn_worker(
            make_worker_job(7, 11),
            move |_| {
                started_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
            },
            completed_tx,
            WorkerCompletionDestination::Pipe(writer),
        );

        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        // Teardown starts while the Job is running. Its result must still be
        // published, but the Worker must skip notification once it sees this.
        assert!(worker.request_stop().is_none());
        resume_tx.send(()).unwrap();
        assert_eq!(
            completed_rx
                .recv_timeout(Duration::from_secs(3))
                .unwrap()
                .job_key,
            make_job_key(11)
        );
        assert!(free_worker(worker).is_none());

        let mut bytes = [0u8; 4];
        let read = unsafe {
            libc::read(
                reader.as_file().unwrap().as_raw_fd(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        assert_eq!(read, 0, "expected EOF without a completion record");
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_and_completion_do_not_block_on_a_full_pipe() {
        use super::super::with_cancellable_region;
        use std::os::fd::{FromRawFd, OwnedFd};
        use std::time::{Duration, Instant};

        let (notifier, recv) = ThreadPoolCompletionNotifier::new().unwrap();
        let _recv = unsafe { OwnedFd::from_raw_fd(recv) };
        let notifier = Arc::new(notifier);
        notifier.fill_with_completions(17);
        let completion = Arc::clone(&notifier);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let worker = super::spawn_worker(
            make_worker_job(23, 29),
            move |job| {
                with_cancellable_region(|| {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    // Exercise the real handler synchronously as well as the
                    // cancellation request below, without depending on delivery timing.
                    assert_eq!(unsafe { libc::raise(libc::SIGUSR2) }, 0);
                    Ok(())
                })
                .unwrap();
                job.job.set_ret(73);
            },
            result_tx,
            WorkerCompletionDestination::Default(completion),
        );
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            cancel_worker_with_retry(&worker),
            Ok(CancellationOutcome::NeedWait)
        );
        release_tx.send(()).unwrap();
        let (joined_tx, joined_rx) = mpsc::channel();
        let join = std::thread::spawn(move || {
            assert!(free_worker(worker).is_none());
            joined_tx.send(()).unwrap();
        });

        let joined_without_fetch = joined_rx.recv_timeout(Duration::from_secs(1)).is_ok();
        if !joined_without_fetch {
            // Unblock the old implementation so this regression fails cleanly
            // instead of leaving a Worker stuck inside the signal handler.
            let deadline = Instant::now() + Duration::from_secs(5);
            while !join.is_finished() && Instant::now() < deadline {
                notifier.fetch(&mut [0; 4096]).unwrap();
                std::thread::yield_now();
            }
            assert!(
                join.is_finished(),
                "Worker did not exit even after draining"
            );
        }
        join.join().unwrap();
        assert_eq!(result_rx.try_recv().unwrap().job.ret(), 73);
        assert!(
            joined_without_fetch,
            "Worker join depended on guest draining the full pipe"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_retry_interrupts_a_syscall_started_after_the_first_signal() {
        use super::super::with_cancellable_region;
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        use std::time::{Duration, Instant};

        let (notifier, recv) = ThreadPoolCompletionNotifier::new().unwrap();
        let _recv = unsafe { OwnedFd::from_raw_fd(recv) };
        let notifier = Arc::new(notifier);
        let completion = Arc::clone(&notifier);
        let fds = crate::async_sys::internal::fd_util::stub::pipe(false, false).unwrap();
        let read = unsafe { OwnedFd::from_raw_fd(fds[0]) };
        let write = unsafe { OwnedFd::from_raw_fd(fds[1]) };
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let worker = super::spawn_worker(
            make_worker_job(17, 19),
            move |job| {
                let (ret, errno) = with_cancellable_region(|| {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    let mut byte = 0u8;
                    let ret = unsafe { libc::read(read.as_raw_fd(), (&raw mut byte).cast(), 1) };
                    let errno = std::io::Error::last_os_error().raw_os_error().unwrap();
                    Ok((ret, errno))
                })
                .unwrap();
                job.job.set_ret(if ret < 0 {
                    i64::from(-errno)
                } else {
                    ret as i64
                });
            },
            result_tx,
            WorkerCompletionDestination::Default(completion),
        );
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            cancel_worker_with_retry(&worker),
            Ok(CancellationOutcome::NeedWait)
        );

        let mut poll = libc::pollfd {
            fd: recv,
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(unsafe { libc::poll(&mut poll, 1, 5000) }, 1);
        let mut bytes = [0u8; 4];
        assert_eq!(notifier.fetch(&mut bytes).unwrap(), 4);
        assert_eq!(i32::from_ne_bytes(bytes), 17);
        assert!(
            result_rx.try_recv().is_err(),
            "a retry is not job completion"
        );
        assert_eq!(
            cancel_worker_with_retry(&worker),
            Ok(CancellationOutcome::NeedWait)
        );
        release_tx.send(()).unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut finished = false;
        let mut result_at_completion = None;
        while Instant::now() < deadline {
            if unsafe { libc::poll(&mut poll, 1, 100) } == 1
                && notifier.fetch(&mut bytes).unwrap() == 4
            {
                assert_eq!(i32::from_ne_bytes(bytes), 17);
                if cancel_worker_with_retry(&worker).unwrap() == CancellationOutcome::JobFinished {
                    result_at_completion = result_rx.try_recv().ok().map(|result| result.job.ret());
                    finished = true;
                    break;
                }
            }
        }
        // Release the read even on failure, so a regression cannot strand it.
        drop(write);
        assert!(free_worker(worker).is_none());
        assert!(
            finished,
            "retry notification must eventually interrupt the read"
        );
        assert_eq!(
            result_at_completion,
            Some(i64::from(-libc::EINTR)),
            "payload must be published before completion is accepted"
        );
    }

    #[cfg(unix)]
    #[test]
    fn switching_to_legacy_cancellation_preserves_a_published_retry() {
        use super::super::with_cancellable_region;
        use std::os::fd::{FromRawFd, OwnedFd};
        use std::time::Duration;

        let (notifier, recv) = ThreadPoolCompletionNotifier::new().unwrap();
        let _recv = unsafe { OwnedFd::from_raw_fd(recv) };
        let notifier = Arc::new(notifier);
        let completion = Arc::clone(&notifier);
        let (started_tx, started_rx) = mpsc::channel();
        let (signal_tx, signal_rx) = mpsc::channel();
        let (handled_tx, handled_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let (completed, _results) = mpsc::channel();
        let worker = super::spawn_worker(
            make_worker_job(23, 29),
            move |_| {
                with_cancellable_region(|| {
                    started_tx.send(()).unwrap();
                    for _ in 0..2 {
                        signal_rx.recv().unwrap();
                        // Complete the real handler before acknowledging each
                        // phase, independently of pthread_kill delivery timing.
                        assert_eq!(unsafe { libc::raise(libc::SIGUSR2) }, 0);
                        handled_tx.send(()).unwrap();
                    }
                    Ok(())
                })
                .unwrap();
                finish_rx.recv().unwrap();
            },
            completed,
            WorkerCompletionDestination::Default(completion),
        );
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            cancel_worker_with_retry(&worker),
            Ok(CancellationOutcome::NeedWait)
        );
        signal_tx.send(()).unwrap();
        handled_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        assert_eq!(cancel_worker(&worker), Ok(CancellationOutcome::RetryLater));
        signal_tx.send(()).unwrap();
        handled_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut bytes = [0; 4];
        assert_eq!(notifier.fetch(&mut bytes).unwrap(), 4);
        assert_eq!(i32::from_ne_bytes(bytes), 23);
        assert_eq!(notifier.fetch(&mut bytes).unwrap(), 0);

        finish_tx.send(()).unwrap();
        assert!(free_worker(worker).is_none());
        assert_eq!(notifier.fetch(&mut bytes).unwrap(), 4);
        assert_eq!(i32::from_ne_bytes(bytes), 23);
    }

    #[cfg(unix)]
    #[test]
    fn legacy_cancellation_does_not_publish_retry_events() {
        use super::super::with_cancellable_region;
        use std::os::fd::{FromRawFd, OwnedFd};
        use std::time::Duration;

        let (notifier, recv) = ThreadPoolCompletionNotifier::new().unwrap();
        let _recv = unsafe { OwnedFd::from_raw_fd(recv) };
        let notifier = Arc::new(notifier);
        let completion = Arc::clone(&notifier);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (ack_tx, ack_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let (completed, _results) = mpsc::channel();
        let worker = super::spawn_worker(
            make_worker_job(23, 29),
            move |_| {
                with_cancellable_region(|| {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                })
                .unwrap();
                assert!(with_cancellable_region(|| Ok(())).is_err());
                ack_tx.send(()).unwrap();
                finish_rx.recv().unwrap();
            },
            completed,
            WorkerCompletionDestination::Default(completion),
        );
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(cancel_worker(&worker), Ok(CancellationOutcome::RetryLater));
        release_tx.send(()).unwrap();
        ack_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(cancel_worker(&worker), Ok(CancellationOutcome::NeedWait));
        let mut bytes = [0u8; 4];
        assert_eq!(notifier.fetch(&mut bytes).unwrap(), 0);
        finish_tx.send(()).unwrap();
        assert!(free_worker(worker).is_none());
        assert_eq!(notifier.fetch(&mut bytes).unwrap(), 4);
        assert_eq!(i32::from_ne_bytes(bytes), 23);
    }

    #[test]
    fn worker_enter_idle_parks_until_next_wake() {
        let (sender, receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            make_worker_job(1, 2),
            move |job| {
                sender.send(worker_job_summary(job)).unwrap();
            },
            move |job| completion_sender.send(job.job_key).unwrap(),
        );

        assert_eq!(receiver.recv().unwrap().0, WorkerCompletionId::from_abi(1));
        assert_eq!(completion_receiver.recv().unwrap(), make_job_key(2));

        assert!(worker_enter_idle(&worker).is_none());
        assert!(wake_worker(&worker, make_worker_job(3, 4)).is_none());

        assert_eq!(receiver.recv().unwrap().0, WorkerCompletionId::from_abi(3));
        assert_eq!(completion_receiver.recv().unwrap(), make_job_key(4));
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
            move |job| completion_sender.send(job.job_key).unwrap(),
        );

        assert_eq!(
            started_receiver.recv().unwrap().0,
            WorkerCompletionId::from_abi(1)
        );
        assert!(wake_worker(&worker, make_worker_job(3, 4)).is_none());
        release_sender.send(()).unwrap();

        assert_eq!(completion_receiver.recv().unwrap(), make_job_key(2));
        assert_eq!(
            started_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap()
                .0,
            WorkerCompletionId::from_abi(3)
        );
        assert_eq!(completion_receiver.recv().unwrap(), make_job_key(4));
        assert!(free_worker(worker).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn running_job_cancellation_does_not_target_queued_job() {
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            make_worker_job(1, 2),
            move |job| {
                started_sender.send(job.completion_id).unwrap();
                if job.completion_id == WorkerCompletionId::from_abi(1) {
                    release_receiver.recv().unwrap();
                }
            },
            move |job| completion_sender.send(job.job_key).unwrap(),
        );

        assert_eq!(
            started_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            WorkerCompletionId::from_abi(1)
        );
        let queued_job = HostWorkerJob::new(
            WorkerCompletionId::from_abi(3),
            make_job_key(4),
            ProcessJob::wait_for_process(None, None, 0).unwrap().into(),
        );
        assert!(queued_job.cancel.is_some());
        assert!(wake_worker(&worker, queued_job).is_none());
        assert!(matches!(
            worker.cancellation_target(),
            WorkerCancellationTarget::Thread
        ));

        release_sender.send(()).unwrap();
        assert_eq!(
            completion_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            make_job_key(2)
        );
        assert_eq!(
            started_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            WorkerCompletionId::from_abi(3)
        );
        assert_eq!(
            completion_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            make_job_key(4)
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
    fn worker_continues_after_completion() {
        let (started_sender, started_receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            make_worker_job(1, 2),
            move |job| {
                started_sender.send(worker_job_summary(job)).unwrap();
            },
            move |job| completion_sender.send(job.job_key).unwrap(),
        );

        assert_eq!(
            started_receiver.recv().unwrap(),
            (WorkerCompletionId::from_abi(1), make_job_key(2))
        );
        assert_eq!(completion_receiver.recv().unwrap(), make_job_key(2));

        assert!(wake_worker(&worker, make_worker_job(3, 4)).is_none());
        assert_eq!(
            started_receiver.recv().unwrap(),
            (WorkerCompletionId::from_abi(3), make_job_key(4))
        );
        assert_eq!(completion_receiver.recv().unwrap(), make_job_key(4));
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
            move |job| completion_sender.send(job.job_key).unwrap(),
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
        assert_eq!(completion_receiver.recv().unwrap(), make_job_key(2));
        assert!(wake_worker(&worker, make_worker_job(5, 6)).is_none());
        assert_eq!(
            started_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            (WorkerCompletionId::from_abi(5), make_job_key(6))
        );
        release_sender.send(()).unwrap();
        assert_eq!(completion_receiver.recv().unwrap(), make_job_key(6));
        assert!(free_worker(worker).is_none());
    }

    #[test]
    fn worker_protocol_preserves_jobs_across_admission_sequences() {
        #[derive(Clone, Copy, Debug)]
        enum Operation {
            Submit,
            Withdraw,
            Stop,
        }
        let operations = [Operation::Submit, Operation::Withdraw, Operation::Stop];
        for first in operations {
            for second in operations {
                for third in operations {
                    let sequence = [first, second, third];
                    let (started, running) = mpsc::channel();
                    let (release, resume) = mpsc::channel();
                    let (completed, results) = mpsc::channel();
                    let worker = spawn_worker(
                        make_worker_job(1, 1),
                        move |job| {
                            if job.job_key == make_job_key(1) {
                                started.send(()).unwrap();
                                resume.recv().unwrap();
                            }
                        },
                        move |job| completed.send(job.job_key).unwrap(),
                    );
                    running.recv().unwrap();

                    let mut submitted = Vec::new();
                    let mut returned = Vec::new();
                    for (index, operation) in sequence.into_iter().enumerate() {
                        let job = match operation {
                            Operation::Submit => {
                                let key = index as u64 + 2;
                                submitted.push(make_job_key(key));
                                wake_worker(&worker, make_worker_job(index as i32 + 2, key))
                            }
                            Operation::Withdraw => worker_enter_idle(&worker),
                            Operation::Stop => worker.request_stop(),
                        };
                        returned.extend(job.map(|job| job.job_key));
                    }
                    returned.extend(worker.request_stop().map(|job| job.job_key));
                    release.send(()).unwrap();
                    let unrun = free_worker(worker);

                    // Each submitted Job must come back exactly once, regardless
                    // of replacement, withdrawal, or when admission closed.
                    submitted.sort();
                    returned.sort();
                    assert_eq!(returned, submitted, "{sequence:?}");
                    assert!(unrun.is_none(), "{sequence:?}");
                    assert_eq!(
                        results.into_iter().collect::<Vec<_>>(),
                        [make_job_key(1)],
                        "only the active Job should execute: {sequence:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn stopping_worker_returns_pending_job_and_rejects_submission() {
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            make_worker_job(1, 2),
            move |_| {
                started_sender.send(()).unwrap();
                release_receiver.recv().unwrap();
            },
            move |job| completion_sender.send(job.job_key).unwrap(),
        );

        started_receiver.recv().unwrap();
        let displaced = wake_worker(&worker, make_worker_job(3, 4));
        let pending = worker.request_stop();
        let rejected = wake_worker(&worker, make_worker_job(5, 6));
        let stopped_again = worker.request_stop();

        // Let the active Job finish and join before checking admission, so a
        // failed assertion cannot leave its thread waiting for this test.
        release_sender.send(()).unwrap();
        let remaining = free_worker(worker);

        assert!(displaced.is_none());
        assert_eq!(
            pending.as_ref().map(worker_job_summary),
            Some((WorkerCompletionId::from_abi(3), make_job_key(4)))
        );
        assert_eq!(
            rejected.as_ref().map(worker_job_summary),
            Some((WorkerCompletionId::from_abi(5), make_job_key(6)))
        );
        assert!(stopped_again.is_none());
        assert!(remaining.is_none());
        assert_eq!(
            completion_receiver.into_iter().collect::<Vec<_>>(),
            [make_job_key(2)]
        );
    }

    #[test]
    fn stop_during_job_exits_after_completion() {
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let (completion_sender, completion_receiver) = mpsc::channel();
        let worker = spawn_worker(
            make_worker_job(21, 34),
            move |job| {
                started_sender.send(worker_job_summary(job)).unwrap();
                release_receiver.recv().unwrap();
            },
            move |job| completion_sender.send(job.job_key).unwrap(),
        );

        assert_eq!(
            started_receiver.recv().unwrap().0,
            WorkerCompletionId::from_abi(21)
        );
        assert!(worker.request_stop().is_none());
        release_sender.send(()).unwrap();
        assert_eq!(completion_receiver.recv().unwrap(), make_job_key(34));

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while !worker.thread.is_finished() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(worker.thread.is_finished());
        assert!(free_worker(worker).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn queued_job_cancellation_targets_the_worker_thread() {
        let queued_job = HostWorkerJob::new(
            WorkerCompletionId::from_abi(3),
            make_job_key(4),
            ProcessJob::wait_for_process(None, None, 0).unwrap().into(),
        );
        assert!(queued_job.cancel.is_some());
        let worker = HostWorkerHandle {
            shared: Arc::new(HostWorkerShared {
                completion: WorkerCompletionDestination::Test(Box::new(|_| {})),
                cancellation: WorkerCancellation::new(),
                state: Mutex::new(HostWorkerState {
                    admission: Admission::Open(Some(queued_job)),
                    activity: Activity::Idle,
                }),
                wakeup: Condvar::new(),
            }),
            thread: std::thread::spawn(|| {}),
        };

        assert!(matches!(
            worker.cancellation_target(),
            WorkerCancellationTarget::Thread
        ));
        assert!(free_worker(worker).is_some());
    }
}
