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

#[cfg(unix)]
use super::signal::run_sigwait_job;
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
        #[cfg(unix)]
        JobPayload::Sigwait { signals, notifier } => run_sigwait_job(signals, notifier),
    };

    // A normal host call can still return a domain-specific status in `ret`.
    // For example, getaddrinfo returns nonzero EAI_* values with `err == 0`.
    match result {
        Ok(ret) => job.set_ret(ret),
        Err(error) => job.set_err(error.errno()),
    }
}
