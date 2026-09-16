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

mod pipe;
#[cfg(unix)]
mod unix;

pub(crate) use pipe::PipeCompletionNotifier;
#[cfg(unix)]
pub(crate) use unix::{CancellationRetryNotifier, ThreadPoolCompletionNotifier};

use super::thread_pool::WorkerCompletionId;
use crate::async_host::AsyncHostResult;
#[cfg(unix)]
use std::sync::Arc;

// A Worker chooses exactly one completion destination at spawn and retains it
// for its lifetime. Cancellation checks the same destination used for delivery.
pub(crate) enum WorkerCompletionDestination {
    // Retain the Unix source so retry checks can validate its identity.
    #[cfg(unix)]
    Pool(Arc<ThreadPoolCompletionNotifier>),
    #[cfg(windows)]
    Pool(super::poll::CompletionPort),
    // A supplied pipe carries only finished Job IDs.
    Pipe(PipeCompletionNotifier),
    // Lifecycle tests observe or pause delivery after result publication.
    #[cfg(test)]
    Test(Box<dyn Fn(WorkerCompletionId) + Send + Sync>),
}

impl WorkerCompletionDestination {
    /// Called after the Job result and finished status have been published.
    /// Supplied-pipe delivery is outside Job cancellation; Worker teardown
    /// can interrupt a full pipe. Reuse the Worker's state snapshot for the
    /// first write, and check again only when delivery needs to wait or retry.
    /// Teardown can race either check with a write.
    pub(crate) fn notify_finished(
        &self,
        completion_id: WorkerCompletionId,
        stopping: bool,
        is_stopping: impl Fn() -> bool,
    ) -> AsyncHostResult<()> {
        match self {
            #[cfg(unix)]
            Self::Pool(source) => source.notify(completion_id.as_i32()),
            #[cfg(windows)]
            Self::Pool(port) => {
                super::poll::post_thread_pool_completion(port, completion_id.as_i32())
            }
            Self::Pipe(pipe) if !stopping => pipe.notify(completion_id.as_i32(), &is_stopping),
            Self::Pipe(_) => Ok(()),
            #[cfg(test)]
            Self::Test(notify) => {
                notify(completion_id);
                Ok(())
            }
        }
    }
}
