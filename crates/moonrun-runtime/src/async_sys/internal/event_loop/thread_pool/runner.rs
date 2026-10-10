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

use super::sleep::run_sleep_job;
use super::types::{Job, JobPayload};
use crate::async_host::AsyncHostError;

pub(crate) fn run_host_job(job: &mut Job) {
    job.set_ret(0);

    let result = match job.payload_mut() {
        JobPayload::Failed { errno } => Err(AsyncHostError::Native(*errno)),
        JobPayload::Sleep { duration_ms } => {
            run_sleep_job(*duration_ms);
            Ok(0)
        }
        JobPayload::Filesystem(job) => job.run(),
        JobPayload::Network(job) => job.run(),
        JobPayload::Process(job) => job.run(),
        JobPayload::Sqlite(job) => job.run(),
        #[cfg(unix)]
        JobPayload::Signal(job) => job.run(),
    };

    // A normal host call can still return a domain-specific status in `ret`.
    // For example, getaddrinfo returns nonzero EAI_* values with `err == 0`.
    match result {
        Ok(ret) => job.set_ret(ret),
        Err(error) => job.set_err(error.errno()),
    }
}
