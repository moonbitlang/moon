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

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::fd_util;
use crate::guest_memory::GuestMemory;

use super::fs::{
    run_access_job, run_chmod_job, run_file_kind_by_path_job, run_file_size_job,
    run_file_time_by_path_job, run_file_time_job, run_flock_job, run_fsync_job, run_mkdir_job,
    run_open_job, run_read_job, run_readdir_job, run_realpath_job, run_remove_job, run_rename_job,
    run_rmdir_job, run_symlink_job, run_write_job,
};
#[cfg(unix)]
use super::process::run_spawn_job_unix;
#[cfg(windows)]
use super::process::run_spawn_job_windows;
use super::process::run_wait_for_process_job;
#[cfg(unix)]
use super::signal::run_sigwait_job;
use super::sleep::run_sleep_job;
use super::socket::{run_bind_job, run_getaddrinfo_job};
use super::stat::{run_fstatx_job, run_statx_job};
use super::types::{Job, JobPayload};

pub(crate) fn run_host_job(job: &mut Job) {
    job.set_ret(0);

    let result = match job.payload_mut() {
        JobPayload::Failed { errno } => Err(AsyncHostError::Native(*errno)),
        JobPayload::Sleep { duration_ms } => {
            run_sleep_job(*duration_ms);
            Ok(0)
        }
        JobPayload::Open {
            filename,
            access,
            create_mode,
            append,
            sync,
            mode,
            request,
            result,
        } => run_open_job(
            result,
            std::mem::take(filename),
            *access,
            *create_mode,
            *append,
            *sync,
            *mode,
            *request,
        ),
        JobPayload::Fstatx {
            file,
            request,
            result,
        } => match file.take() {
            Some(file) => run_fstatx_job(&file, *request, result),
            None => Err(AsyncHostError::Badf),
        },
        JobPayload::Statx {
            parent,
            path,
            request,
            follow_symlink,
            result,
        } => {
            let parent = parent.take();
            run_statx_job(
                parent.as_deref(),
                std::mem::take(path),
                *request,
                *follow_symlink,
                result,
            )
        }
        JobPayload::Read {
            file,
            len,
            position,
            result,
        } => match file.take() {
            Some(file) => run_read_job(&file, *len, *position, result),
            None => Err(AsyncHostError::Badf),
        },
        JobPayload::Write {
            file,
            data,
            position,
        } => match file.take() {
            Some(file) => run_write_job(&file, data, *position),
            None => Err(AsyncHostError::Badf),
        },
        JobPayload::FileKindByPath {
            parent,
            path,
            follow_symlink,
        } => {
            let parent = parent.take();
            run_file_kind_by_path_job(parent.as_deref(), std::mem::take(path), *follow_symlink)
        }
        JobPayload::FileSize { file, result } => match file.take() {
            Some(file) => run_file_size_job(&file, result),
            None => Err(AsyncHostError::Badf),
        },
        JobPayload::FileTime { file, result, .. } => match file.take() {
            Some(file) => run_file_time_job(&file, result),
            None => Err(AsyncHostError::Badf),
        },
        JobPayload::FileTimeByPath {
            path,
            follow_symlink,
            result,
            ..
        } => run_file_time_by_path_job(std::mem::take(path), *follow_symlink, result),
        JobPayload::Access { path, access } => run_access_job(std::mem::take(path), *access),
        JobPayload::Chmod { path, mode } => run_chmod_job(std::mem::take(path), *mode),
        JobPayload::Fsync { file, only_data } => match file.take() {
            Some(file) => run_fsync_job(&file, *only_data),
            None => Err(AsyncHostError::Badf),
        },
        JobPayload::Flock { file, exclusive } => match file.take() {
            Some(file) => run_flock_job(&file, *exclusive),
            None => Err(AsyncHostError::Badf),
        },
        JobPayload::Remove { path } => run_remove_job(std::mem::take(path)),
        JobPayload::Rename {
            old_path,
            new_path,
            replace,
        } => run_rename_job(std::mem::take(old_path), std::mem::take(new_path), *replace),
        JobPayload::Symlink {
            target,
            path,
            force_symlink,
        } => run_symlink_job(std::mem::take(target), std::mem::take(path), *force_symlink),
        JobPayload::Mkdir { path, mode } => run_mkdir_job(std::mem::take(path), *mode),
        JobPayload::Rmdir { path } => run_rmdir_job(std::mem::take(path)),
        JobPayload::Readdir {
            dir,
            buffer,
            len,
            restart,
        } => match (dir.take(), buffer.as_mut()) {
            (Some(dir), Some(buffer)) => {
                run_readdir_job(&dir, buffer.as_mut_slice(), *len, *restart)
            }
            _ => Err(AsyncHostError::Badf),
        },
        JobPayload::Bind { socket, addr } => match socket.take() {
            Some(socket) => run_bind_job(&socket, addr),
            None => Err(AsyncHostError::Badf),
        },
        JobPayload::GetAddrInfo { host, result } => run_getaddrinfo_job(host.clone(), result),
        JobPayload::Realpath { path, result, .. } => run_realpath_job(std::mem::take(path), result),
        #[cfg(unix)]
        JobPayload::SpawnUnix {
            path,
            args,
            env,
            options,
            stdio,
            cwd,
            result,
        } => run_spawn_job_unix(
            std::mem::take(path),
            std::mem::take(args),
            std::mem::take(env),
            std::mem::take(stdio),
            cwd.take(),
            *options,
            result,
        ),
        #[cfg(windows)]
        JobPayload::SpawnWindows {
            command_line,
            env,
            options,
            stdio,
            cwd,
            result,
        } => run_spawn_job_windows(
            std::mem::take(command_line),
            std::mem::take(env),
            std::mem::take(stdio),
            cwd.take(),
            *options,
            result,
        ),
        JobPayload::WaitForProcess {
            handle,
            tracked_pid: _,
            pid,
            #[cfg(unix)]
            defer_reap,
            #[cfg(windows)]
            cancel,
        } => run_wait_for_process_job(
            handle.take(),
            *pid,
            #[cfg(unix)]
            *defer_reap,
            #[cfg(windows)]
            cancel.take(),
        ),
        #[cfg(unix)]
        JobPayload::Sigwait { signals, notifier } => run_sigwait_job(signals, notifier),
    };

    match result {
        Ok(ret) => job.set_ret(ret),
        Err(error) => job.set_err(error.errno()),
    }
}

pub(crate) fn get_read_result(
    job: &Job,
    memory: &mut (impl GuestMemory + ?Sized),
    dst: u32,
    offset: u32,
    len: u32,
) -> AsyncHostResult<()> {
    if job.err() != 0 {
        return Ok(());
    }
    let JobPayload::Read {
        result: Some(result),
        ..
    } = job.payload()
    else {
        return Err(AsyncHostError::Badf);
    };
    let dst = dst.checked_add(offset).ok_or(AsyncHostError::Fault)?;
    Ok(memory.write_with_capacity(dst, len, result)?)
}

pub(crate) fn get_file_time_result(
    job: &Job,
    memory: &mut (impl GuestMemory + ?Sized),
    dst: u32,
) -> AsyncHostResult<()> {
    if job.err() != 0 {
        return Ok(());
    }
    let result = match job.payload() {
        JobPayload::FileTime {
            result: Some(result),
            ..
        }
        | JobPayload::FileTimeByPath {
            result: Some(result),
            ..
        } => result,
        _ => return Err(AsyncHostError::Badf),
    };

    let file_time = result.as_native();
    let mut record = [0; 48];
    record[0..8].copy_from_slice(&fd_util::stub::get_atime_sec(file_time).to_le_bytes());
    record[8..12].copy_from_slice(&fd_util::stub::get_atime_nsec(file_time).to_le_bytes());
    record[16..24].copy_from_slice(&fd_util::stub::get_mtime_sec(file_time).to_le_bytes());
    record[24..28].copy_from_slice(&fd_util::stub::get_mtime_nsec(file_time).to_le_bytes());
    record[32..40].copy_from_slice(&fd_util::stub::get_ctime_sec(file_time).to_le_bytes());
    record[40..44].copy_from_slice(&fd_util::stub::get_ctime_nsec(file_time).to_le_bytes());
    Ok(memory.write_exact(dst, &record)?)
}

pub(crate) fn get_stat_result(
    job: &Job,
    memory: &mut (impl GuestMemory + ?Sized),
    dst: u32,
    dst_len: u32,
) -> AsyncHostResult<()> {
    if job.err() != 0 {
        return Ok(());
    }
    let result = super::jobs::stat_job_result(job)?;
    Ok(memory.write_with_capacity(dst, dst_len, result.as_bytes())?)
}
