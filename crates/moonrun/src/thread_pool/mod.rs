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

//! Shared Job execution, Worker ownership, cancellation, and completion delivery.

mod cancellation;
mod completion;
mod jobs;
mod registry;
mod runner;
mod sleep;
mod types;
mod worker;

pub(crate) use cancellation::with_cancellable_region;
#[cfg(unix)]
use completion::CancellationRetryNotifier;
pub(crate) use completion::PipeCompletionNotifier;
#[cfg(unix)]
pub(crate) use completion::ThreadPoolCompletionNotifier;
pub(crate) use jobs::make_failed_job;
pub(crate) use jobs::make_sleep_job;
pub(crate) use jobs::{errno_is_cancelled, get_platform};
pub(crate) use jobs::{job_get_err, job_get_ret};
pub(crate) use registry::InstanceWorkers;
pub(crate) use runner::run_host_job;
pub(crate) use types::{CancellationOutcome, HostHandle, Job, JobPayload};
pub(crate) use types::{JobCancellation, JobCancellationOverride};
pub(crate) use worker::{CompletedJob, WorkerCompletionDestination, WorkerCompletionId, WorkerJob};

#[cfg(test)]
pub(crate) fn ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    let mut symbols = jobs::PORTED_SYMBOLS.to_vec();
    symbols.extend_from_slice(worker::Worker::PORTED_SYMBOLS);
    symbols
}

#[cfg(test)]
mod tests {
    #[test]
    fn sleep_job_executor_references_native_worker_symbol() {
        for symbol in super::sleep::PORTED_SYMBOLS {
            symbol.assert_source_exists();
        }
    }
}
