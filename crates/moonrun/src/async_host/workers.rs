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

//! Host Worker lifecycle for one MoonBit event loop.
//!
//! MoonBit schedules Jobs and observes completion notifications. This module
//! owns the corresponding host Worker registry, completed Jobs, and teardown;
//! `async_sys` retains only the native-shaped single-Worker primitives.

use std::cell::RefCell;
use std::sync::mpsc;

use slotmap::SecondaryMap;

use super::{AsyncHostError, AsyncHostResult, HandleKey};
use crate::async_sys::internal::event_loop::thread_pool::{
    self, HostWorkerHandle, HostWorkerJob, HostWorkerJobResult, WorkerCompletionId,
};

pub(super) struct InstanceWorkers {
    workers: RefCell<SecondaryMap<HandleKey, HostWorkerHandle>>,
    completed_sender: mpsc::Sender<HostWorkerJobResult>,
    completed: mpsc::Receiver<HostWorkerJobResult>,
}

impl std::fmt::Debug for InstanceWorkers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstanceWorkers")
            .field("workers", &self.workers.borrow().len())
            .finish_non_exhaustive()
    }
}

impl InstanceWorkers {
    pub(super) fn new() -> Self {
        let (completed_sender, completed) = mpsc::channel();
        Self {
            workers: RefCell::new(SecondaryMap::new()),
            completed_sender,
            completed,
        }
    }

    pub(super) fn spawn(
        &self,
        worker: HandleKey,
        init_job: HostWorkerJob,
        run_job: impl FnMut(&mut HostWorkerJob) + Send + 'static,
        mut notify_completion: impl FnMut(WorkerCompletionId) + Send + 'static,
    ) -> AsyncHostResult<()> {
        let mut workers = self.workers.borrow_mut();
        if workers.contains_key(worker) {
            return Err(AsyncHostError::Badf);
        }
        let completed = self.completed_sender.clone();
        let handle = thread_pool::spawn_worker(init_job, run_job, move |result| {
            let completion_id = result.completion_id;
            if completed.send(result).is_ok() {
                notify_completion(completion_id);
            }
        });
        workers.insert(worker, handle);
        Ok(())
    }

    pub(super) fn wake(
        &self,
        worker: HandleKey,
        job: HostWorkerJob,
    ) -> AsyncHostResult<Option<HostWorkerJob>> {
        let workers = self.workers.borrow();
        let worker = workers.get(worker).ok_or(AsyncHostError::Badf)?;
        Ok(thread_pool::wake_worker(worker, job))
    }

    pub(super) fn enter_idle(&self, worker: HandleKey) -> AsyncHostResult<Option<HostWorkerJob>> {
        let workers = self.workers.borrow();
        let worker = workers.get(worker).ok_or(AsyncHostError::Badf)?;
        Ok(thread_pool::worker_enter_idle(worker))
    }

    pub(super) fn cancel(&self, worker: HandleKey) -> AsyncHostResult<i32> {
        let workers = self.workers.borrow();
        let worker = workers.get(worker).ok_or(AsyncHostError::Badf)?;
        cancel_host_worker(worker)
    }

    pub(super) fn free(&self, worker: HandleKey) -> AsyncHostResult<Option<HostWorkerJob>> {
        let worker = self
            .workers
            .borrow_mut()
            .remove(worker)
            .ok_or(AsyncHostError::Badf)?;
        let _ = cancel_host_worker(&worker);
        Ok(thread_pool::free_worker(worker))
    }

    pub(super) fn try_recv_completed(&self) -> Result<HostWorkerJobResult, mpsc::TryRecvError> {
        self.completed.try_recv()
    }

    pub(super) fn len(&self) -> usize {
        self.workers.borrow().len()
    }

    pub(super) fn destroy(&self) -> Vec<StoppedWorker> {
        let worker_keys = self
            .workers
            .borrow()
            .iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>();
        let workers = worker_keys
            .into_iter()
            .filter_map(|key| {
                self.workers
                    .borrow_mut()
                    .remove(key)
                    .map(|worker| (key, worker))
            })
            .collect::<Vec<_>>();

        // Cancellation must fan out before any join: one slow Worker must not
        // prevent the remaining Workers from receiving their stop request.
        for (_, worker) in &workers {
            let _ = cancel_host_worker(worker);
        }
        workers
            .into_iter()
            .map(|(key, worker)| StoppedWorker {
                key,
                unrun_job: thread_pool::free_worker(worker),
            })
            .collect()
    }
}

impl Drop for InstanceWorkers {
    fn drop(&mut self) {
        drop(self.destroy());
    }
}

pub(super) struct StoppedWorker {
    pub(super) key: HandleKey,
    pub(super) unrun_job: Option<HostWorkerJob>,
}

fn cancel_host_worker(worker: &HostWorkerHandle) -> AsyncHostResult<i32> {
    #[cfg(windows)]
    {
        match thread_pool::worker_cancellation_target(worker) {
            thread_pool::WorkerCancellationTarget::Resource(cancel) => {
                crate::process::cancel_wait(&cancel)?;
                return Ok(1);
            }
            thread_pool::WorkerCancellationTarget::Thread => {}
        }
    }
    thread_pool::cancel_worker(worker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_sys::internal::event_loop::thread_pool::make_sleep_job;
    use slotmap::KeyData;
    use std::time::Duration;

    fn key(value: u64) -> HandleKey {
        KeyData::from_ffi(value).into()
    }

    fn job(completion_id: i32, job_key: u64) -> HostWorkerJob {
        HostWorkerJob::new(
            WorkerCompletionId::from_abi(completion_id),
            key(job_key),
            make_sleep_job(0),
        )
    }

    #[cfg(unix)]
    #[test]
    fn signal_wait_worker_cancellation_is_durable_before_the_job_waits() {
        use std::os::fd::RawFd;
        use std::sync::Arc;

        let workers = InstanceWorkers::new();
        let worker = key(1);
        let (_sender, receiver) = crate::signal_channel();
        let (notifier, notifier_fd): (_, RawFd) =
            crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier::new().unwrap();
        let wait_job = crate::async_sys::signal::make_sigwait_job(
            &receiver,
            &[libc::SIGINT],
            Arc::new(notifier),
        )
        .unwrap();
        let worker_job =
            HostWorkerJob::new(WorkerCompletionId::from_abi(7), key(2), wait_job.into());
        let (completed, completion) = mpsc::channel();
        let (started, worker_started) = mpsc::sync_channel(0);
        let (proceed, worker_may_proceed) = mpsc::sync_channel(0);

        workers
            .spawn(
                worker,
                worker_job,
                move |job| {
                    started.send(()).unwrap();
                    worker_may_proceed.recv().unwrap();
                    thread_pool::run_host_job(&mut job.job);
                },
                move |completion_id| completed.send(completion_id).unwrap(),
            )
            .unwrap();

        worker_started.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(workers.cancel(worker), Ok(0));
        proceed.send(()).unwrap();

        let first_completion = completion.recv_timeout(Duration::from_secs(1));
        if first_completion.is_err() {
            // Release the old implementation so a failing assertion does not
            // leave its Worker blocked in read(2).
            assert_eq!(workers.cancel(worker), Ok(0));
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
    fn worker_handles_are_scoped_to_one_instance() {
        let first = InstanceWorkers::new();
        let second = InstanceWorkers::new();
        let worker = key(1);
        let (first_sender, first_receiver) = mpsc::channel();
        let (second_sender, second_receiver) = mpsc::channel();

        first
            .spawn(
                worker,
                job(11, 101),
                |_| {},
                move |completion| first_sender.send(completion).unwrap(),
            )
            .unwrap();
        second
            .spawn(
                worker,
                job(22, 202),
                |_| {},
                move |completion| second_sender.send(completion).unwrap(),
            )
            .unwrap();

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
        assert_eq!(first.try_recv_completed().unwrap().job_key, key(101));
        assert_eq!(second.try_recv_completed().unwrap().job_key, key(202));
        assert!(first.free(worker).unwrap().is_none());
        assert!(second.free(worker).unwrap().is_none());
    }
}
