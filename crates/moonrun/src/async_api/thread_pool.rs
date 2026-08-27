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
use crate::async_sys::internal::event_loop::thread_pool;
use crate::filesystem::Job as FilesystemJob;
use crate::guest_memory::GuestMemory;
use crate::process::{Job as ProcessJob, SpawnOptions};
use crate::resource::{ResourceClass, ResourceRef};

use super::context::ImportContext;
use super::os_string::read_guest as read_guest_os_string;
use super::provenance::ported_imports;

fn filesystem_job_or_failed(job: AsyncHostResult<FilesystemJob>) -> thread_pool::Job {
    job.map(Into::into)
        .unwrap_or_else(|error| thread_pool::make_failed_job(error.errno()))
}

ported_imports! {
pub(super) fn free_job(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<()> {
    context.host.free_job(job)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn job_get_ret(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<i32> {
    context.host.job_get_ret(job).map(|value| value as i32)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn job_get_err(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<i32> {
    context.host.job_get_err(job)
}

pub(super) fn run_job(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<()> {
    context.host.run_job(job)
}

pub(super) fn init_thread_pool(context: &mut ImportContext<'_, '_>, poll: u64) -> AsyncHostResult<u64> {
    context.host.init_thread_pool(poll)
}

pub(super) fn destroy_thread_pool(context: &mut ImportContext<'_, '_>) {
    context.host.destroy_thread_pool();
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn spawn_worker(
    context: &mut ImportContext<'_, '_>,
    completion_id: i32,
    job: u64,
) -> AsyncHostResult<u64> {
    context.host.spawn_worker(completion_id, job)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn free_worker(context: &mut ImportContext<'_, '_>, worker: u64) -> AsyncHostResult<()> {
    context.host.free_worker(worker)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn wake_worker(
    context: &mut ImportContext<'_, '_>,
    worker: u64,
    completion_id: i32,
    job: u64,
) -> AsyncHostResult<()> {
    context.host.wake_worker(worker, completion_id, job)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn worker_enter_idle(context: &mut ImportContext<'_, '_>, worker: u64) -> AsyncHostResult<()> {
    context.host.worker_enter_idle(worker)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn cancel_worker(context: &mut ImportContext<'_, '_>, worker: u64) -> AsyncHostResult<i32> {
    context.host.cancel_worker(worker)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
#[cfg(unix)]
pub(super) fn fetch_completion(
    context: &mut ImportContext<'_, '_>,
    source_fd: u64,
    dst: u32,
    max_jobs: u32,
) -> i32 {
    match context.with_host_and_memory_mut(|host, memory| {
        host.fetch_completion(memory, source_fd, dst, max_jobs)
    })
    {
        Ok(bytes) => bytes,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_sleep_job(
    context: &mut ImportContext<'_, '_>,
    duration_ms: i32,
) -> AsyncHostResult<u64> {
    context.host
        .insert_job(thread_pool::make_sleep_job(duration_ms))
}

#[compat(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_make_open_job",
    upstream_pr = 527,
    replacement = "thread_pool/make_open_stat_job",
    api_only = true
)]
#[allow(clippy::too_many_arguments)]
pub(super) fn make_open_job_legacy(
    context: &mut ImportContext<'_, '_>,
    path_ptr: u32,
    path_len: u32,
    access: i32,
    create_mode: i32,
    append: i32,
    sync: i32,
    mode: i32,
) -> AsyncHostResult<u64> {
    let filename = read_guest_os_string(context, path_ptr, path_len)?;

    context.host.insert_job(
        FilesystemJob::open_legacy(filename, access, create_mode, append != 0, sync, mode),
    )
}

// Unlike the native ABI, the wasm maker does not receive the eventual output
// buffer. The worker produces a Rust-owned PackedStat and the wasm completion
// callback copies it into current Guest Memory through get_stat_result.
#[ported(
    source = "src/internal/event_loop/fs.c",
    original = "moonbitlang_async_make_open_job"
)]
#[allow(clippy::too_many_arguments)]
pub(super) fn make_open_stat_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: u32,
    path_len: u32,
    access: i32,
    create_mode: i32,
    append: i32,
    sync: i32,
    mode: i32,
    stat_request: i32,
    stat_result_len: u32,
) -> AsyncHostResult<u64> {
    let filename = read_guest_os_string(context, path_ptr, path_len)?;
    context.host.insert_job(filesystem_job_or_failed(FilesystemJob::open(
        filename,
        access,
        create_mode,
        append != 0,
        sync,
        mode,
        stat_request as u32,
        stat_result_len,
    )))
}

#[ported(
    source = "src/internal/event_loop/fs.c",
    original = "moonbitlang_async_make_fstatx_job"
)]
pub(super) fn make_fstatx_job(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    stat_request: i32,
    stat_result_len: u32,
) -> AsyncHostResult<u64> {
    let file = context.host.acquire_resource(fd)?;
    context
        .host
        .insert_job(filesystem_job_or_failed(FilesystemJob::fstatx(
            file,
            stat_request as u32,
            stat_result_len,
        )))
}

#[ported(
    source = "src/internal/event_loop/fs.c",
    original = "moonbitlang_async_make_statx_job"
)]
#[allow(clippy::too_many_arguments)]
pub(super) fn make_statx_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: u32,
    path_len: u32,
    stat_request: i32,
    stat_result_len: u32,
    parent: u64,
    follow_symlink: i32,
) -> AsyncHostResult<u64> {
    let parent = if parent == context.host.invalid_fd() {
        None
    } else {
        Some(
            context
                .host
                .acquire_resource_of_class(parent, ResourceClass::File)?,
        )
    };
    let path = read_guest_os_string(context, path_ptr, path_len)?;
    context
        .host
        .insert_job(filesystem_job_or_failed(FilesystemJob::statx(
            parent,
            path,
            stat_request as u32,
            stat_result_len,
            follow_symlink != 0,
        )))
}

pub(super) fn get_stat_result(
    context: &mut ImportContext<'_, '_>,
    job: u64,
    dst: u32,
    dst_len: u32,
) -> AsyncHostResult<()> {
    context.with_host_and_memory_mut(|host, memory| {
        host.get_stat_result(memory, job, dst, dst_len)
    })
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_read_job(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    len: u32,
    position: i64,
) -> AsyncHostResult<u64> {
    let file = context
        .host
        .acquire_resource_of_class(fd, ResourceClass::File)?;
    context
        .host
        .insert_job(FilesystemJob::read(file, len, position))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_write_job(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    ptr: u32,
    offset: u32,
    len: u32,
    position: i64,
) -> AsyncHostResult<u64> {
    let file = context
        .host
        .acquire_resource_of_class(fd, ResourceClass::File)?;
    let ptr = ptr.checked_add(offset).ok_or(AsyncHostError::Fault)?;
    let data = context.with_memory_mut(|memory| Ok(memory.read_exact(ptr, len)?.to_vec()))?;

    context
        .host
        .insert_job(FilesystemJob::write(file, data, position))
}

pub(super) fn get_read_result(
    context: &mut ImportContext<'_, '_>,
    job: u64,
    dst: u32,
    offset: u32,
    len: u32,
) -> AsyncHostResult<()> {
    context.with_host_and_memory_mut(|host, memory| {
        host.get_read_result(memory, job, dst, offset, len)
    })
}

#[compat(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_make_file_kind_by_path_job",
    upstream_pr = 527,
    replacement = "thread_pool/make_statx_job with STAT_FILE_KIND",
    api_only = true
)]
pub(super) fn make_file_kind_by_path_job(
    context: &mut ImportContext<'_, '_>,
    parent: u64,
    path_ptr: u32,
    path_len: u32,
    follow_symlink: i32,
) -> AsyncHostResult<u64> {
    let parent = if parent == context.host.invalid_fd() {
        None
    } else {
        Some(
            context
                .host
                .acquire_resource_of_class(parent, ResourceClass::File)?,
        )
    };
    let path = read_guest_os_string(context, path_ptr, path_len)?;

    context.host.insert_job(
        FilesystemJob::file_kind_by_path(parent, path, follow_symlink != 0),
    )
}

#[compat(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_make_file_size_job",
    upstream_pr = 527,
    replacement = "thread_pool/make_fstatx_job with STAT_FILE_SIZE",
    api_only = true
)]
pub(super) fn make_file_size_job(context: &mut ImportContext<'_, '_>, fd: u64) -> AsyncHostResult<u64> {
    let file = context
        .host
        .acquire_resource_of_class(fd, ResourceClass::File)?;
    context.host.insert_job(FilesystemJob::file_size(file))
}

#[compat(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_get_file_size_result",
    upstream_pr = 527,
    replacement = "thread_pool/get_stat_result",
    api_only = true
)]
pub(super) fn get_file_size_result(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<i64> {
    context.host.get_file_size_result(job)
}

#[compat(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_make_file_time_job",
    upstream_pr = 527,
    replacement = "thread_pool/make_fstatx_job with timestamp properties",
    api_only = true
)]
pub(super) fn make_file_time_job(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
) -> AsyncHostResult<u64> {
    let file = context
        .host
        .acquire_resource_of_class(fd, ResourceClass::File)?;
    context.host.insert_job(FilesystemJob::file_time(file))
}

#[compat(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_make_file_time_by_path_job",
    upstream_pr = 527,
    replacement = "thread_pool/make_statx_job with timestamp properties",
    api_only = true
)]
pub(super) fn make_file_time_by_path_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: u32,
    path_len: u32,
    follow_symlink: i32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;

    context.host.insert_job(
        FilesystemJob::file_time_by_path(path, follow_symlink != 0),
    )
}

pub(super) fn get_file_time_result(
    context: &mut ImportContext<'_, '_>,
    job: u64,
    out: u32,
) -> AsyncHostResult<()> {
    context.with_host_and_memory_mut(|host, memory| {
        host.get_file_time_result(memory, job, out)
    })
}

#[ported(source = "src/internal/event_loop/fs.c")]
pub(super) fn make_access_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: u32,
    path_len: u32,
    access: i32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;

    context
        .host
        .insert_job(FilesystemJob::access(path, access))
}

#[ported(source = "src/internal/event_loop/fs.c")]
pub(super) fn make_chmod_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: u32,
    path_len: u32,
    mode: i32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;

    context
        .host
        .insert_job(FilesystemJob::chmod(path, mode))
}

#[ported(source = "src/internal/event_loop/fs.c")]
pub(super) fn make_fsync_job(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    only_data: i32,
) -> AsyncHostResult<u64> {
    let file = context
        .host
        .acquire_resource_of_class(fd, ResourceClass::File)?;
    context
        .host
        .insert_job(FilesystemJob::fsync(file, only_data != 0))
}

#[ported(source = "src/internal/event_loop/fs.c")]
pub(super) fn make_flock_job(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    exclusive: i32,
) -> AsyncHostResult<u64> {
    let file = context
        .host
        .acquire_resource_of_class(fd, ResourceClass::File)?;
    context
        .host
        .insert_job(FilesystemJob::flock(file, exclusive != 0))
}

#[ported(source = "src/internal/event_loop/fs.c")]
pub(super) fn make_remove_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: u32,
    path_len: u32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;
    context.host.insert_job(FilesystemJob::remove(path))
}

#[ported(source = "src/internal/event_loop/fs.c")]
pub(super) fn make_rename_job(
    context: &mut ImportContext<'_, '_>,
    old_path_ptr: u32,
    old_path_len: u32,
    new_path_ptr: u32,
    new_path_len: u32,
    replace: i32,
) -> AsyncHostResult<u64> {
    let old_path = read_guest_os_string(context, old_path_ptr, old_path_len)?;
    let new_path = read_guest_os_string(context, new_path_ptr, new_path_len)?;

    context
        .host
        .insert_job(FilesystemJob::rename(old_path, new_path, replace != 0))
}

#[ported(source = "src/internal/event_loop/fs.c")]
pub(super) fn make_symlink_job(
    context: &mut ImportContext<'_, '_>,
    target_ptr: u32,
    target_len: u32,
    path_ptr: u32,
    path_len: u32,
    force_symlink: i32,
) -> AsyncHostResult<u64> {
    let target = read_guest_os_string(context, target_ptr, target_len)?;
    let path = read_guest_os_string(context, path_ptr, path_len)?;

    context.host.insert_job(
        FilesystemJob::symlink(target, path, force_symlink != 0),
    )
}

#[ported(source = "src/internal/event_loop/fs.c")]
pub(super) fn make_mkdir_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: u32,
    path_len: u32,
    mode: i32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;

    context
        .host
        .insert_job(FilesystemJob::mkdir(path, mode))
}

#[ported(source = "src/internal/event_loop/fs.c")]
pub(super) fn make_rmdir_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: u32,
    path_len: u32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;
    context.host.insert_job(FilesystemJob::rmdir(path))
}

#[ported(source = "src/internal/event_loop/fs.c")]
pub(super) fn make_readdir_job(
    context: &mut ImportContext<'_, '_>,
    dir: u64,
    buf: u64,
    len: u32,
    restart: i32,
) -> AsyncHostResult<u64> {
    let dir = context
        .host
        .acquire_resource_of_class(dir, ResourceClass::File)?;
    let buffer = context.host.lease_c_buffer(buf)?;
    context.host.insert_job(
        FilesystemJob::readdir(dir, buffer, len, restart != 0),
    )
}

#[ported(
    source = "src/internal/event_loop/fs.c",
    original = "moonbitlang_async_make_inotify_add_watch_job"
)]
#[cfg(target_os = "linux")]
pub(super) fn make_inotify_add_watch_job(
    context: &mut ImportContext<'_, '_>,
    inotify: u64,
    path_ptr: u32,
    path_len: u32,
    is_dir: i32,
) -> AsyncHostResult<u64> {
    let inotify = context
        .host
        .acquire_resource_of_class(inotify, ResourceClass::File)?;
    let path = read_guest_os_string(context, path_ptr, path_len)?;
    context.host.insert_job(
        FilesystemJob::inotify_add_watch(inotify, path, is_dir != 0),
    )
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_bind_job(
    context: &mut ImportContext<'_, '_>,
    socket: u64,
    addr: u32,
    addr_len: u32,
) -> AsyncHostResult<u64> {
    let socket = context.host.acquire_socket_resource(socket)?;
    let addr = context.with_memory_mut(|memory| Ok(memory.read_exact(addr, addr_len)?.to_vec()))?;
    context.host.make_bind_job(socket, addr)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_getaddrinfo_job(
    context: &mut ImportContext<'_, '_>,
    host: u32,
    host_len: u32,
) -> AsyncHostResult<u64> {
    let host = read_guest_os_string(context, host, host_len)?;
    context.host.make_getaddrinfo_job(host)
}

#[compat(
    source = "src/internal/event_loop/thread_pool.wasm.mbt",
    original = "thread_pool/make_spawn_job/unix",
    upstream_pr = 546,
    replacement = "thread_pool/spawn_job/unix + thread_pool/spawn_job/set_cwd",
    api_only = true
)]
#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
pub(super) fn make_spawn_job_unix(
    context: &mut ImportContext<'_, '_>,
    path: u32,
    path_len: u32,
    args: u64,
    env: u64,
    inherited_env_entry_count: u32,
    stdin: u64,
    stdout: u64,
    stderr: u64,
    cwd: u32,
    cwd_len: u32,
    has_cwd: i32,
) -> AsyncHostResult<u64> {
    let (args, env) = context.host.take_legacy_process_spawn_inputs(
        args,
        env,
        inherited_env_entry_count,
    )?;
    let path = read_guest_os_string(context, path, path_len)?;
    let cwd = if has_cwd == 0 {
        None
    } else {
        Some(read_guest_os_string(context, cwd, cwd_len)?)
    };
    let stdin = optional_resource(context, stdin)?;
    let stdout = optional_resource(context, stdout)?;
    let stderr = optional_resource(context, stderr)?;
    let options = SpawnOptions {
        child_signal_mask: context.host.thread_pool_child_signal_mask()?,
    };
    context
        .host
        .insert_job(ProcessJob::spawn_unix(
            path, args, env, stdin, stdout, stderr, cwd, options,
        ))
}

#[ported(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_make_spawn_job"
)]
#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_job_unix(
    context: &mut ImportContext<'_, '_>,
    path: u32,
    path_len: u32,
    args: u64,
    env: u64,
    stdin: u64,
    stdout: u64,
    stderr: u64,
) -> AsyncHostResult<u64> {
    let (args, env) = context
        .host
        .take_process_spawn_inputs(args, env)?;
    let path = read_guest_os_string(context, path, path_len)?;
    let stdin = optional_resource(context, stdin)?;
    let stdout = optional_resource(context, stdout)?;
    let stderr = optional_resource(context, stderr)?;
    let options = SpawnOptions {
        child_signal_mask: context.host.thread_pool_child_signal_mask()?,
    };
    context
        .host
        .insert_job(ProcessJob::spawn_unix(
            path, args, env, stdin, stdout, stderr, None, options,
        ))
}

#[ported(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_spawn_job_set_cwd"
)]
pub(super) fn spawn_job_set_cwd(
    context: &mut ImportContext<'_, '_>,
    job: u64,
    cwd: u32,
    cwd_len: u32,
) -> AsyncHostResult<()> {
    let cwd = read_guest_os_string(context, cwd, cwd_len)?;
    context.host.spawn_job_set_cwd(job, cwd)
}

#[compat(
    source = "src/internal/event_loop/thread_pool.wasm.mbt",
    original = "thread_pool/make_spawn_job/windows",
    upstream_pr = 546,
    replacement = "thread_pool/spawn_job/windows + spawn-job setters",
    api_only = true
)]
#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
pub(super) fn make_spawn_job_windows(
    context: &mut ImportContext<'_, '_>,
    command_line: u32,
    command_line_len: u32,
    env: u64,
    stdin: u64,
    stdout: u64,
    stderr: u64,
    cwd: u32,
    cwd_len: u32,
    has_cwd: i32,
    no_console_window: i32,
    is_orphan: i32,
) -> AsyncHostResult<u64> {
    let env = context.host.take_process_env(env)?;
    let command_line = read_guest_os_string(context, command_line, command_line_len)?;
    let cwd = if has_cwd == 0 {
        None
    } else {
        Some(read_guest_os_string(context, cwd, cwd_len)?)
    };
    let stdin = optional_resource(context, stdin)?;
    let stdout = optional_resource(context, stdout)?;
    let stderr = optional_resource(context, stderr)?;
    let options = SpawnOptions {
        no_console_window: no_console_window != 0,
        is_orphan: is_orphan != 0,
    };
    context
        .host
        .insert_job(ProcessJob::spawn_windows(
            command_line,
            env,
            stdin,
            stdout,
            stderr,
            cwd,
            options,
        ))
}

#[ported(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_make_spawn_job"
)]
#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_job_windows(
    context: &mut ImportContext<'_, '_>,
    command_line: u32,
    command_line_len: u32,
    env: u64,
    stdin: u64,
    stdout: u64,
    stderr: u64,
    is_orphan: i32,
) -> AsyncHostResult<u64> {
    let env = context.host.take_process_env_builder(env)?;
    let command_line = read_guest_os_string(context, command_line, command_line_len)?;
    let stdin = optional_resource(context, stdin)?;
    let stdout = optional_resource(context, stdout)?;
    let stderr = optional_resource(context, stderr)?;
    let options = SpawnOptions {
        no_console_window: false,
        is_orphan: is_orphan != 0,
    };
    context
        .host
        .insert_job(ProcessJob::spawn_windows(
            command_line,
            env,
            stdin,
            stdout,
            stderr,
            None,
            options,
        ))
}

#[ported(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_spawn_job_set_no_console_window"
)]
#[cfg(windows)]
pub(super) fn spawn_job_set_no_console_window(
    context: &mut ImportContext<'_, '_>,
    job: u64,
) -> AsyncHostResult<()> {
    context.host.spawn_job_set_no_console_window(job)
}

#[ported(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_get_spawn_job_result_handle"
)]
pub(super) fn spawn_job_get_result_handle(
    context: &mut ImportContext<'_, '_>,
    job: u64,
) -> AsyncHostResult<u64> {
    context.host.get_spawn_job_result_handle(job)
}

// The legacy wasm wrapper passes SpawnJob by value. Its nested Job is a
// two-field valtype, so MoonBit lowers both the JobHandle and its optional
// copy-output closure into this import. Spawn jobs never install that closure;
// validate the representation invariant while this guest ABI remains in use.
pub(super) fn get_spawn_job_result_handle_legacy(
    context: &mut ImportContext<'_, '_>,
    job: u64,
    copy_output: i32,
) -> AsyncHostResult<u64> {
    if copy_output != 0 {
        return Err(AsyncHostError::Inval);
    }
    spawn_job_get_result_handle(context, job)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_wait_for_process_job(
    context: &mut ImportContext<'_, '_>,
    handle: u64,
    pid: i32,
) -> AsyncHostResult<u64> {
    context.host.make_wait_for_process_job(handle, pid)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
#[cfg(unix)]
pub(super) fn make_sigwait_job(
    context: &mut ImportContext<'_, '_>,
    signals: u32,
    signals_len: u32,
) -> AsyncHostResult<u64> {
    let signals =
        context.with_memory_mut(|memory| read_i32_array(memory, signals, signals_len))?;
    let notifier = context.host.thread_pool_notifier()?;
    context
        .host
        .insert_job(thread_pool::make_sigwait_job(signals, notifier))
}

fn optional_resource(
    context: &mut ImportContext<'_, '_>,
    handle: u64,
) -> AsyncHostResult<Option<ResourceRef>> {
    if handle == crate::async_host::INVALID_HOST_HANDLE || handle == context.host.invalid_fd() {
        Ok(None)
    } else {
        context.host.acquire_resource(handle).map(Some)
    }
}

pub(super) fn get_getaddrinfo_result(
    context: &mut ImportContext<'_, '_>,
    job: u64,
) -> AsyncHostResult<u64> {
    context.host.get_getaddrinfo_result(job)
}

#[cfg(unix)]
fn read_i32_array(
    memory: &(impl GuestMemory + ?Sized),
    offset: u32,
    len: u32,
) -> AsyncHostResult<Vec<i32>> {
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let byte_len = len
        .checked_mul(std::mem::size_of::<i32>())
        .ok_or(AsyncHostError::Fault)?;
    let bytes = memory.read_exact(
        offset,
        u32::try_from(byte_len).map_err(|_| AsyncHostError::Fault)?,
    )?;
    Ok(bytes
        .chunks_exact(std::mem::size_of::<i32>())
        .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()))
        .collect())
}

#[ported(source = "src/internal/event_loop/fs.c")]
pub(super) fn make_realpath_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: u32,
    path_len: u32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;
    context
        .host
        .insert_job(FilesystemJob::realpath(path))
}

#[ported(source = "src/internal/event_loop/fs.c")]
pub(super) fn get_realpath_result(
    context: &mut ImportContext<'_, '_>,
    job: u64,
) -> AsyncHostResult<u64> {
    context.host.get_realpath_result(job)
}

#[ported(source = "src/internal/event_loop/fs.c")]
pub(super) fn open_job_get_fd(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<u64> {
    context.host.open_job_get_fd(job)
}

#[compat(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_open_job_get_kind",
    upstream_pr = 527,
    replacement = "thread_pool/get_stat_result with STAT_FILE_KIND",
    api_only = true
)]
pub(super) fn open_job_get_kind(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<i32> {
    context.host.open_job_get_kind(job)
}

#[compat(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_open_job_get_dev_id",
    upstream_pr = 527,
    replacement = "thread_pool/get_stat_result with STAT_DEVICE_ID",
    api_only = true
)]
pub(super) fn open_job_get_dev_id(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<u64> {
    context.host.open_job_get_dev_id(job)
}

#[compat(
    source = "src/internal/event_loop/thread_pool.c",
    original = "moonbitlang_async_open_job_get_file_id",
    upstream_pr = 527,
    replacement = "thread_pool/get_stat_result with STAT_FILE_ID",
    api_only = true
)]
pub(super) fn open_job_get_file_id(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<u64> {
    context.host.open_job_get_file_id(job)
}

}
