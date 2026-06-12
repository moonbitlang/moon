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

use std::{sync::mpsc, thread::JoinHandle};

use crate::async_sys::ported_fns;

use super::types::Job;
use super::wakeup::{WorkerThreadId, WorkerWakeup, cancel_running_worker};

#[allow(dead_code)]
pub(crate) struct Worker {
    id: Option<WorkerThreadId>,
    job_id: i32,
    job: Option<Job>,
    waiting: bool,
    wakeup: WorkerWakeup,
}

impl Worker {
    #[allow(dead_code)]
    pub(crate) fn new(init_job_id: i32, init_job: Job) -> Self {
        Self {
            id: None,
            job_id: init_job_id,
            job: Some(init_job),
            waiting: false,
            wakeup: WorkerWakeup::new(),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn wake(&mut self, job_id: i32, job: Option<Job>) {
        self.job_id = job_id;
        self.job = job;
        self.wakeup.wake(self.id, &mut self.waiting);
    }

    #[allow(dead_code)]
    pub(crate) fn enter_idle(&mut self) {
        self.job = None;
    }

    #[allow(dead_code)]
    pub(crate) fn mark_waiting(&mut self) {
        self.waiting = true;
    }

    #[allow(dead_code)]
    pub(crate) fn wait_for_wake(&mut self) {
        self.wakeup.wait(&mut self.waiting);
    }

    #[allow(dead_code)]
    pub(crate) fn cancel(&self) -> i32 {
        if self.waiting {
            return 1;
        }
        cancel_running_worker(self.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HostWorkerJob {
    pub(crate) job_id: i32,
    pub(crate) job_handle: i32,
}

enum HostWorkerCommand {
    Run(HostWorkerJob),
    Shutdown,
}

pub(crate) struct HostWorkerHandle {
    sender: mpsc::Sender<HostWorkerCommand>,
    thread: Option<JoinHandle<()>>,
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
            .finish()
    }
}

impl HostWorkerHandle {
    pub(crate) fn spawn(mut f: impl FnMut(HostWorkerJob) + Send + 'static) -> Self {
        let (command_sender, command_receiver) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            while let Ok(command) = command_receiver.recv() {
                match command {
                    HostWorkerCommand::Run(job) => f(job),
                    HostWorkerCommand::Shutdown => break,
                }
            }
        });
        Self {
            sender: command_sender,
            thread: Some(thread),
        }
    }

    pub(crate) fn run(&self, job_id: i32, job_handle: i32) -> Result<(), ()> {
        self.sender
            .send(HostWorkerCommand::Run(HostWorkerJob { job_id, job_handle }))
            .map_err(|_| ())
    }

    pub(crate) fn join(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = self.sender.send(HostWorkerCommand::Shutdown);
            let _ = thread.join();
        }
    }
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_spawn_worker"
    )]
    #[allow(dead_code)]
    pub(crate) fn spawn_worker(init_job_id: i32, init_job: Job) -> Worker {
        Worker::new(init_job_id, init_job)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_wake_worker"
    )]
    #[allow(dead_code)]
    pub(crate) fn wake_worker(worker: &mut Worker, job_id: i32, job: Job) {
        worker.wake(job_id, Some(job));
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_worker_enter_idle"
    )]
    #[allow(dead_code)]
    pub(crate) fn worker_enter_idle(worker: &mut Worker) {
        worker.enter_idle();
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_cancel_worker"
    )]
    #[allow(dead_code)]
    pub(crate) fn cancel_worker(worker: &Worker) -> i32 {
        worker.cancel()
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "moonbitlang_async_free_worker"
    )]
    #[allow(dead_code)]
    pub(crate) fn free_worker(_worker: Worker) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_sys::internal::event_loop::thread_pool::make_sleep_job;

    #[test]
    fn worker_wake_replaces_job_and_leaves_waiting_state() {
        let mut worker = Worker::new(1, make_sleep_job(0));
        worker.mark_waiting();

        worker.wake(2, Some(make_sleep_job(0)));

        assert_eq!(worker.job_id, 2);
        assert!(worker.job.is_some());
        assert!(!worker.waiting);
    }

    #[test]
    fn worker_enter_idle_clears_current_job() {
        let mut worker = Worker::new(1, make_sleep_job(0));

        worker.enter_idle();

        assert!(worker.job.is_none());
    }
}
