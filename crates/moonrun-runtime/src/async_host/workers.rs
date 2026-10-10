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

//! Host Worker lifecycle for one MoonBit event loop.
//!
//! MoonBit schedules Jobs and observes completion notifications. This module
//! owns the corresponding host Worker registry, completed Jobs, and teardown;
//! `async_sys` retains only the native-shaped single-Worker primitives.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use super::{AsyncHostError, AsyncHostResult, HandleKey};
use crate::async_sys::internal::event_loop::thread_pool::{
    self, CancellationOutcome, CompletedJob, Worker, WorkerCompletionDestination, WorkerJob,
};
use crate::runtime::{Handles, HostKeys, HostResourceKind};

pub(super) struct InstanceWorkers {
    pub(super) workers: RefCell<Handles<Worker>>,
    completed_sender: mpsc::Sender<CompletedJob>,
    completed: mpsc::Receiver<CompletedJob>,
}

impl std::fmt::Debug for InstanceWorkers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstanceWorkers")
            .field("workers", &self.workers.borrow().len())
            .finish_non_exhaustive()
    }
}

impl InstanceWorkers {
    pub(super) fn new(keys: Rc<RefCell<HostKeys>>) -> Self {
        let (completed_sender, completed) = mpsc::channel();
        Self {
            workers: RefCell::new(Handles::new(keys, HostResourceKind::Worker)),
            completed_sender,
            completed,
        }
    }

    pub(super) fn spawn(
        &self,
        first_job: WorkerJob,
        run_job: impl FnMut(&mut WorkerJob) + Send + 'static,
        completion: WorkerCompletionDestination,
    ) -> HandleKey {
        let handle = thread_pool::spawn_worker(
            first_job,
            run_job,
            self.completed_sender.clone(),
            completion,
        );
        self.workers.borrow_mut().insert(handle)
    }

    pub(super) fn take_pending(&self, worker: HandleKey) -> AsyncHostResult<Option<WorkerJob>> {
        let workers = self.workers.borrow();
        let worker = workers.get(worker).ok_or(AsyncHostError::Badf)?;
        Ok(thread_pool::worker_enter_idle(worker))
    }

    pub(super) fn cancel(&self, worker: HandleKey) -> AsyncHostResult<CancellationOutcome> {
        let workers = self.workers.borrow();
        let worker = workers.get(worker).ok_or(AsyncHostError::Badf)?;
        thread_pool::cancel_worker(worker)
    }

    pub(super) fn cancel_with_retry(
        &self,
        worker: HandleKey,
        #[cfg(unix)] notifier: std::sync::Arc<
            crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier,
        >,
    ) -> AsyncHostResult<CancellationOutcome> {
        let workers = self.workers.borrow();
        let worker = workers.get(worker).ok_or(AsyncHostError::Badf)?;
        thread_pool::cancel_worker_with_retry(
            worker,
            #[cfg(unix)]
            notifier,
        )
    }

    pub(super) fn try_recv_completed(&self) -> Result<CompletedJob, mpsc::TryRecvError> {
        self.completed.try_recv()
    }

    pub(super) fn len(&self) -> usize {
        self.workers.borrow().len()
    }

    pub(super) fn destroy(&self) -> Vec<WorkerJob> {
        let workers = self.workers.borrow_mut().take_all();

        // Cancellation must fan out before any join: one slow Worker must not
        // prevent the remaining Workers from receiving their stop request.
        // FIXME: after the guest stops polling, a cancellation signal arriving
        // before a blocking syscall may still need a retry to let join finish.
        // Define cancellation retry ownership for Run teardown outside
        // native free_worker; neither join nor repeated signals can forcibly
        // stop noncooperative computation.
        for worker in &workers {
            let _ = thread_pool::cancel_worker(worker);
        }
        workers
            .into_iter()
            .filter_map(thread_pool::free_worker)
            .collect()
    }
}

impl Drop for InstanceWorkers {
    fn drop(&mut self) {
        drop(self.destroy());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_sys::internal::event_loop::thread_pool::{WorkerCompletionId, make_sleep_job};
    use std::time::Duration;

    fn job(completion_id: i32, job_key: HandleKey) -> WorkerJob {
        WorkerJob::new(
            WorkerCompletionId::from_abi(completion_id),
            job_key,
            make_sleep_job(0),
        )
    }

    #[test]
    fn destroying_or_dropping_workers_retires_their_handles() {
        for destroy_explicitly in [true, false] {
            let keys = Rc::new(RefCell::new(HostKeys::default()));
            let workers = InstanceWorkers::new(keys.clone());
            let job_key = keys.borrow_mut().insert(HostResourceKind::Job);
            let worker = workers.spawn(
                job(11, job_key),
                |_| {},
                WorkerCompletionDestination::Test(Box::new(|_| {})),
            );

            if destroy_explicitly {
                drop(workers.destroy());
                assert_eq!(keys.borrow().kind(worker), None);
            }
            drop(workers);
            assert_eq!(keys.borrow().kind(worker), None);
        }
    }

    #[cfg(unix)]
    #[test]
    fn signal_wait_worker_cancellation_is_durable_before_the_job_waits() {
        use std::os::fd::RawFd;
        use std::sync::Arc;

        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let workers = InstanceWorkers::new(keys.clone());
        let job_key = keys.borrow_mut().insert(HostResourceKind::Job);
        let (_sender, receiver) = crate::signal_channel();
        let (notifier, notifier_fd): (_, RawFd) =
            crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier::new().unwrap();
        let wait_job = crate::async_sys::signal::make_sigwait_job(
            &receiver,
            &[libc::SIGINT],
            Arc::new(notifier),
        )
        .unwrap();
        let worker_job = WorkerJob::new(WorkerCompletionId::from_abi(7), job_key, wait_job.into());
        let (completed, completion) = mpsc::channel();
        let (started, worker_started) = mpsc::sync_channel(0);
        let (proceed, worker_may_proceed) = mpsc::sync_channel(0);

        let worker = workers.spawn(
            worker_job,
            move |job| {
                started.send(()).unwrap();
                worker_may_proceed.recv().unwrap();
                thread_pool::run_host_job(&mut job.job);
            },
            WorkerCompletionDestination::Test(Box::new(move |completion_id| {
                completed.send(completion_id).unwrap()
            })),
        );

        worker_started.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(workers.cancel(worker), Ok(CancellationOutcome::RetryLater));
        proceed.send(()).unwrap();

        let first_completion = completion.recv_timeout(Duration::from_secs(1));
        if first_completion.is_err() {
            // Release the old implementation so a failing assertion does not
            // leave its Worker blocked in read(2).
            assert_eq!(workers.cancel(worker), Ok(CancellationOutcome::RetryLater));
            completion.recv_timeout(Duration::from_secs(1)).unwrap();
        }
        let completion_id = first_completion.expect("the first cancellation was lost");
        assert_eq!(completion_id, WorkerCompletionId::from_abi(7));
        let result = workers.try_recv_completed().unwrap();
        assert_eq!(result.job.ret(), 0);
        assert_eq!(result.job.err(), 0);

        drop(workers.destroy());
        assert_eq!(unsafe { libc::close(notifier_fd) }, 0);
    }

    #[test]
    fn destroy_cancels_all_workers_before_joining_any() {
        use std::time::Instant;

        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let workers = InstanceWorkers::new(keys.clone());
        let (first_cancelled, wait_for_first) = mpsc::channel();
        let (second_cancelled, wait_for_second) = mpsc::channel();
        let (started, wait_for_start) = mpsc::channel();
        let (finished, results) = mpsc::channel();
        let timeout = Duration::from_secs(5);

        for (cancelled, other_cancelled) in [
            (first_cancelled, wait_for_second),
            (second_cancelled, wait_for_first),
        ] {
            let job_key = keys.borrow_mut().insert(HostResourceKind::Job);
            let started = started.clone();
            let finished = finished.clone();
            workers.spawn(
                job(11, job_key),
                move |_| {
                    started.send(()).unwrap();
                    let deadline = Instant::now() + timeout;
                    while thread_pool::with_cancellable_region(|| Ok(())).is_ok() {
                        assert!(Instant::now() < deadline, "Worker was not cancelled");
                        std::thread::yield_now();
                    }
                    cancelled.send(()).unwrap();
                    // Neither runner can finish until the other is cancelled.
                    // A timeout makes a sequential cancel-and-join regression
                    // fail without leaving the test process stuck in join.
                    let both_cancelled = other_cancelled.recv_timeout(timeout).is_ok();
                    finished.send(both_cancelled).unwrap();
                },
                WorkerCompletionDestination::Test(Box::new(|_| {})),
            );
        }
        for _ in 0..2 {
            wait_for_start.recv_timeout(timeout).unwrap();
        }

        assert!(workers.destroy().is_empty());
        for _ in 0..2 {
            assert!(results.recv_timeout(timeout).unwrap());
        }
    }

    #[test]
    fn worker_handles_are_scoped_to_one_instance() {
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let first = InstanceWorkers::new(keys.clone());
        let second = InstanceWorkers::new(keys.clone());
        let first_job = keys.borrow_mut().insert(HostResourceKind::Job);
        let second_job = keys.borrow_mut().insert(HostResourceKind::Job);
        let (first_sender, first_receiver) = mpsc::channel();
        let (second_sender, second_receiver) = mpsc::channel();

        let first_worker = first.spawn(
            job(11, first_job),
            |_| {},
            WorkerCompletionDestination::Test(Box::new(move |completion| {
                first_sender.send(completion).unwrap()
            })),
        );
        let second_worker = second.spawn(
            job(22, second_job),
            |_| {},
            WorkerCompletionDestination::Test(Box::new(move |completion| {
                second_sender.send(completion).unwrap()
            })),
        );

        assert_eq!(
            first_receiver.recv_timeout(Duration::from_secs(1)).unwrap(),
            WorkerCompletionId::from_abi(11)
        );
        assert_eq!(
            second_receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap(),
            WorkerCompletionId::from_abi(22)
        );
        assert_eq!(first.try_recv_completed().unwrap().job_key, first_job);
        assert_eq!(second.try_recv_completed().unwrap().job_key, second_job);
        assert_eq!(first.cancel(second_worker), Err(AsyncHostError::Badf));
        assert_eq!(second.cancel(first_worker), Err(AsyncHostError::Badf));
        for (instance, worker) in [(&first, first_worker), (&second, second_worker)] {
            let worker = instance.workers.borrow_mut().remove(worker).unwrap();
            assert!(thread_pool::free_worker(worker).is_none());
        }
    }
}
