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

mod jobs;
mod runner;
mod sleep;
mod types;
mod worker;

pub(crate) use jobs::{
    errno_is_cancelled, get_platform, job_get_err, job_get_ret, make_failed_job, make_sleep_job,
};
pub(crate) use runner::run_host_job;
pub(crate) use types::{HostHandle, Job, JobPayload, ResourceTable};
#[cfg(unix)]
pub(crate) use types::{JobCancellation, JobCancellationOverride};
pub(crate) use worker::{
    HostWorkerHandle, HostWorkerJob, HostWorkerJobResult, WorkerCompletionId, cancel_worker,
    free_worker, spawn_worker, wake_worker, worker_enter_idle,
};
#[cfg(windows)]
pub(crate) use worker::{WorkerCancellationTarget, worker_cancellation_target};

#[cfg(test)]
pub(crate) fn ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    let mut symbols = jobs::PORTED_SYMBOLS.to_vec();
    symbols.extend_from_slice(worker::PORTED_SYMBOLS);
    symbols
}

#[cfg(test)]
mod tests {
    #[test]
    fn sleep_job_executor_references_native_worker_symbol() {
        let async_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../third_party/moonbitlang_async");
        for symbol in super::sleep::PORTED_SYMBOLS {
            let source_path = async_root.join(symbol.source);
            let contents = std::fs::read_to_string(&source_path)
                .unwrap_or_else(|error| panic!("failed to read {:?}: {error}", source_path));
            assert!(
                contents.contains(symbol.native_symbol),
                "{:?} does not contain native worker symbol {} for {}::{}",
                source_path,
                symbol.native_symbol,
                symbol.rust_module,
                symbol.rust_symbol
            );
        }
    }
}
