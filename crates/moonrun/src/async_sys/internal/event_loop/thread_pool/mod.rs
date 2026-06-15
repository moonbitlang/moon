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
mod process;
mod runner;
mod sleep;
mod types;
mod worker;

pub(crate) use jobs::{
    errno_is_cancelled, get_file_size_result, get_platform, job_get_err, job_get_ret,
    make_access_job, make_chmod_job, make_file_kind_by_path_job, make_file_size_job,
    make_file_time_by_path_job, make_file_time_job, make_flock_job, make_fsync_job, make_mkdir_job,
    make_open_job, make_read_job, make_readdir_job, make_remove_job, make_rename_job,
    make_rmdir_job, make_sleep_job, make_symlink_job, make_write_job, open_job_get_dev_id,
    open_job_get_fd, open_job_get_file_id, open_job_get_kind, open_job_result,
};
pub(crate) use process::{
    HostProcess, HostProcessTable, make_wait_for_process_job_from_handle, spawn_process,
};
pub(crate) use runner::{complete_guest_job, run_host_job};
#[cfg(test)]
pub(crate) use types::JobPayload;
pub(crate) use types::{
    FileTimeResult, GuestBuffer, HostFile, HostFileTable, HostHandle, Job, OpenJobResult,
};
pub(crate) use worker::{
    HostWorkerHandle, HostWorkerJob, cancel_worker, free_worker, spawn_worker, wake_worker,
    worker_enter_idle,
};

#[cfg(test)]
pub(crate) fn ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    let mut symbols = Vec::new();
    symbols.extend_from_slice(jobs::PORTED_SYMBOLS);
    symbols.extend_from_slice(worker::PORTED_SYMBOLS);
    symbols
}
