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

mod fs;
mod jobs;
mod runner;
mod sleep;
mod socket;
mod types;
mod worker;

pub(crate) use jobs::{
    errno_is_cancelled, get_file_size_result, get_platform, getaddrinfo_job_result, job_get_err,
    job_get_ret, make_access_job, make_bind_job, make_chmod_job, make_file_kind_by_path_job,
    make_file_size_job, make_file_time_by_path_job, make_file_time_job, make_flock_job,
    make_fsync_job, make_getaddrinfo_job, make_mkdir_job, make_open_job, make_read_job,
    make_readdir_job, make_remove_job, make_rename_job, make_rmdir_job, make_sleep_job,
    make_symlink_job, make_write_job, open_job_get_dev_id, open_job_get_fd, open_job_get_file_id,
    open_job_get_kind, open_job_result, open_job_result_mut, take_open_job_result,
};
pub(crate) use runner::{get_file_time_result, get_read_result, run_host_job};
#[cfg(all(test, unix))]
pub(crate) use types::JobPayload;
pub(crate) use types::{
    FileTimeResult, HostHandle, Job, OpenJobResource, OpenJobResult, Resource, ResourceClass,
    ResourceRef, ResourceTable,
};
pub(crate) use worker::{
    HostWorkerHandle, HostWorkerJob, WorkerCompletionId, cancel_worker, free_worker, spawn_worker,
    wake_worker, worker_enter_idle,
};

#[cfg(test)]
pub(crate) fn ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    let mut symbols = Vec::new();
    symbols.extend_from_slice(jobs::PORTED_SYMBOLS);
    symbols.extend_from_slice(worker::PORTED_SYMBOLS);
    symbols
}

#[cfg(test)]
fn job_executor_ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    let mut symbols = Vec::new();
    symbols.extend_from_slice(fs::PORTED_SYMBOLS);
    symbols.extend_from_slice(sleep::PORTED_SYMBOLS);
    symbols.extend_from_slice(socket::PORTED_SYMBOLS);
    symbols
}

#[cfg(test)]
mod tests {
    #[test]
    fn job_executors_reference_native_worker_symbols() {
        let async_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../third_party/moonbitlang_async");
        for symbol in super::job_executor_ported_symbols() {
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
