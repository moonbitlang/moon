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

use std::ffi::OsString;

use crate::async_host::{AsyncHostError, AsyncHostResult, GuestMemory, read_u16};
use crate::async_sys::internal::event_loop::thread_pool::{self, ResourceClass, ResourceRef};

use super::context::ImportContext;
use super::provenance::ported_imports;

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
    dst: i32,
    max_jobs: i32,
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

#[ported(source = "src/internal/event_loop/thread_pool.c")]
#[allow(clippy::too_many_arguments)]
pub(super) fn make_open_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: i32,
    path_len: i32,
    access: i32,
    create_mode: i32,
    append: i32,
    sync: i32,
    mode: i32,
) -> AsyncHostResult<u64> {
    let filename = read_guest_os_string(context, path_ptr, path_len)?;

    context.host.insert_job(thread_pool::make_open_job(
        filename,
        access,
        create_mode,
        append != 0,
        sync,
        mode,
    ))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_read_job(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    len: i32,
    position: i64,
) -> AsyncHostResult<u64> {
    let file = context.host.resource_of_class(fd, ResourceClass::File)?;
    context.host
        .insert_job(thread_pool::make_read_job(file, len, position))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_write_job(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    ptr: i32,
    offset: i32,
    len: i32,
    position: i64,
) -> AsyncHostResult<u64> {
    let file = context.host.resource_of_class(fd, ResourceClass::File)?;
    let offset_ptr = ptr.checked_add(offset).ok_or(AsyncHostError::Fault)?;
    let data =
        context.with_memory_mut(|memory| Ok(memory.read_exact(offset_ptr, len)?.to_vec()))?;

    context.host
        .insert_job(thread_pool::make_write_job(file, data, position))
}

pub(super) fn get_read_result(
    context: &mut ImportContext<'_, '_>,
    job: u64,
    dst: i32,
    offset: i32,
    len: i32,
) -> AsyncHostResult<()> {
    context.with_host_and_memory_mut(|host, memory| {
        host.get_read_result(memory, job, dst, offset, len)
    })
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_file_kind_by_path_job(
    context: &mut ImportContext<'_, '_>,
    parent: u64,
    path_ptr: i32,
    path_len: i32,
    follow_symlink: i32,
) -> AsyncHostResult<u64> {
    let parent = if parent == context.host.invalid_fd() {
        None
    } else {
        Some(
            context
                .host
                .resource_of_class(parent, ResourceClass::File)?,
        )
    };
    let path = read_guest_os_string(context, path_ptr, path_len)?;

    context.host
        .insert_job(thread_pool::make_file_kind_by_path_job(
            parent,
            path,
            follow_symlink != 0,
        ))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_file_size_job(context: &mut ImportContext<'_, '_>, fd: u64) -> AsyncHostResult<u64> {
    let file = context.host.resource_of_class(fd, ResourceClass::File)?;
    context.host.insert_job(thread_pool::make_file_size_job(file))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn get_file_size_result(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<i64> {
    context.host.get_file_size_result(job)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_file_time_job(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
) -> AsyncHostResult<u64> {
    let file = context.host.resource_of_class(fd, ResourceClass::File)?;
    context.host
        .insert_job(thread_pool::make_file_time_job(file))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_file_time_by_path_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: i32,
    path_len: i32,
    follow_symlink: i32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;

    context.host
        .insert_job(thread_pool::make_file_time_by_path_job(
            path,
            follow_symlink != 0,
        ))
}

pub(super) fn get_file_time_result(
    context: &mut ImportContext<'_, '_>,
    job: u64,
    out: i32,
) -> AsyncHostResult<()> {
    context.with_host_and_memory_mut(|host, memory| {
        host.get_file_time_result(memory, job, out)
    })
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_access_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: i32,
    path_len: i32,
    access: i32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;

    context.host
        .insert_job(thread_pool::make_access_job(path, access))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_chmod_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: i32,
    path_len: i32,
    mode: i32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;

    context.host
        .insert_job(thread_pool::make_chmod_job(path, mode))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_fsync_job(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    only_data: i32,
) -> AsyncHostResult<u64> {
    let file = context.host.resource_of_class(fd, ResourceClass::File)?;
    context.host
        .insert_job(thread_pool::make_fsync_job(file, only_data != 0))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_flock_job(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    exclusive: i32,
) -> AsyncHostResult<u64> {
    let file = context.host.resource_of_class(fd, ResourceClass::File)?;
    context.host
        .insert_job(thread_pool::make_flock_job(file, exclusive != 0))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_remove_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: i32,
    path_len: i32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;
    context.host.insert_job(thread_pool::make_remove_job(path))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_rename_job(
    context: &mut ImportContext<'_, '_>,
    old_path_ptr: i32,
    old_path_len: i32,
    new_path_ptr: i32,
    new_path_len: i32,
    replace: i32,
) -> AsyncHostResult<u64> {
    let old_path = read_guest_os_string(context, old_path_ptr, old_path_len)?;
    let new_path = read_guest_os_string(context, new_path_ptr, new_path_len)?;

    context.host.insert_job(thread_pool::make_rename_job(
        old_path,
        new_path,
        replace != 0,
    ))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_symlink_job(
    context: &mut ImportContext<'_, '_>,
    target_ptr: i32,
    target_len: i32,
    path_ptr: i32,
    path_len: i32,
    force_symlink: i32,
) -> AsyncHostResult<u64> {
    let target = read_guest_os_string(context, target_ptr, target_len)?;
    let path = read_guest_os_string(context, path_ptr, path_len)?;

    context.host.insert_job(thread_pool::make_symlink_job(
        target,
        path,
        force_symlink != 0,
    ))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_mkdir_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: i32,
    path_len: i32,
    mode: i32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;

    context.host
        .insert_job(thread_pool::make_mkdir_job(path, mode))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_rmdir_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: i32,
    path_len: i32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;
    context.host.insert_job(thread_pool::make_rmdir_job(path))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_readdir_job(
    context: &mut ImportContext<'_, '_>,
    dir: u64,
    buf: u64,
    len: i32,
    restart: i32,
) -> AsyncHostResult<u64> {
    let dir = context.host.resource_of_class(dir, ResourceClass::File)?;
    let buffer = context.host.c_buffer(buf)?;
    context.host
        .insert_job(thread_pool::make_readdir_job(
            dir,
            buffer,
            len,
            restart != 0,
        ))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_bind_job(
    context: &mut ImportContext<'_, '_>,
    socket: u64,
    addr: i32,
    addr_len: i32,
) -> AsyncHostResult<u64> {
    let socket = context.host.socket_resource(socket)?;
    let addr = context.with_memory_mut(|memory| Ok(memory.read_exact(addr, addr_len)?.to_vec()))?;
    match context.host.policy().bind_socket(&addr) {
        Ok(()) => context.host.insert_job(thread_pool::make_bind_job(socket, addr)),
        Err(error) => context.host.insert_failed_job(error),
    }
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_getaddrinfo_job(
    context: &mut ImportContext<'_, '_>,
    host: i32,
    host_len: i32,
) -> AsyncHostResult<u64> {
    let host = read_guest_os_string(context, host, host_len)?;
    match context.host.policy().resolve_dns(&host) {
        Ok(()) => context.host.insert_job(thread_pool::make_getaddrinfo_job(host)),
        Err(error) => context.host.insert_failed_job(error),
    }
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
#[cfg(unix)]
#[allow(clippy::too_many_arguments)]
pub(super) fn make_spawn_job_unix(
    context: &mut ImportContext<'_, '_>,
    path: i32,
    path_len: i32,
    args: u64,
    env: u64,
    inherited_env_entry_count: i32,
    stdin: u64,
    stdout: u64,
    stderr: u64,
    cwd: i32,
    cwd_len: i32,
    has_cwd: i32,
) -> AsyncHostResult<u64> {
    let _ = inherited_env_entry_count;
    let args = context.host.clone_process_argv(args)?;
    let env = context.host.clone_process_env(env)?;
    let path = read_guest_os_string(context, path, path_len)?;
    let cwd = if has_cwd == 0 {
        None
    } else {
        Some(read_guest_os_string(context, cwd, cwd_len)?)
    };
    let stdin = optional_resource(context, stdin)?;
    let stdout = optional_resource(context, stdout)?;
    let stderr = optional_resource(context, stderr)?;
    let options = thread_pool::SpawnOptions {
        child_signal_mask: context.host.thread_pool_child_signal_mask()?,
    };
    context.host.insert_job(thread_pool::make_spawn_job_unix(
        path,
        args,
        env,
        stdin,
        stdout,
        stderr,
        cwd,
        options,
    ))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
pub(super) fn make_spawn_job_windows(
    context: &mut ImportContext<'_, '_>,
    command_line: i32,
    command_line_len: i32,
    env: u64,
    stdin: u64,
    stdout: u64,
    stderr: u64,
    cwd: i32,
    cwd_len: i32,
    has_cwd: i32,
    no_console_window: i32,
    is_orphan: i32,
) -> AsyncHostResult<u64> {
    let env = context.host.clone_process_env(env)?;
    let command_line = read_guest_os_string(context, command_line, command_line_len)?;
    let cwd = if has_cwd == 0 {
        None
    } else {
        Some(read_guest_os_string(context, cwd, cwd_len)?)
    };
    let stdin = optional_resource(context, stdin)?;
    let stdout = optional_resource(context, stdout)?;
    let stderr = optional_resource(context, stderr)?;
    let options = thread_pool::SpawnOptions {
        no_console_window: no_console_window != 0,
        is_orphan: is_orphan != 0,
    };
    context.host.insert_job(thread_pool::make_spawn_job_windows(
        command_line,
        env,
        stdin,
        stdout,
        stderr,
        cwd,
        options,
    ))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn get_spawn_job_result_handle(
    context: &mut ImportContext<'_, '_>,
    job: u64,
) -> AsyncHostResult<u64> {
    context.host.get_spawn_job_result_handle(job)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_wait_for_process_job(
    context: &mut ImportContext<'_, '_>,
    handle: u64,
    pid: i32,
) -> AsyncHostResult<u64> {
    let tracked_pid = context.host.process_handle_pid(handle)?;
    let handle = optional_resource(context, handle)?;
    #[cfg(unix)]
    let defer_reap = context.host.policy().has_process_policy();
    context
        .host
        .insert_job(thread_pool::make_wait_for_process_job(
            handle,
            tracked_pid,
            pid,
            #[cfg(unix)]
            defer_reap,
        )?)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
#[cfg(unix)]
pub(super) fn make_sigwait_job(
    context: &mut ImportContext<'_, '_>,
    signals: i32,
    signals_len: i32,
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
        context.host.resource(handle).map(Some)
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
    offset: i32,
    len: i32,
) -> AsyncHostResult<Vec<i32>> {
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let byte_len = len
        .checked_mul(std::mem::size_of::<i32>())
        .ok_or(AsyncHostError::Fault)?;
    let bytes = memory.read_exact(
        offset,
        i32::try_from(byte_len).map_err(|_| AsyncHostError::Fault)?,
    )?;
    Ok(bytes
        .chunks_exact(std::mem::size_of::<i32>())
        .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()))
        .collect())
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_realpath_job(
    context: &mut ImportContext<'_, '_>,
    path_ptr: i32,
    path_len: i32,
) -> AsyncHostResult<u64> {
    let path = read_guest_os_string(context, path_ptr, path_len)?;
    context
        .host
        .insert_job(thread_pool::make_realpath_job(path))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn get_realpath_result(
    context: &mut ImportContext<'_, '_>,
    job: u64,
) -> AsyncHostResult<u64> {
    context.host.get_realpath_result(job)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn open_job_get_fd(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<u64> {
    context.host.open_job_get_fd(job)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn open_job_get_kind(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<i32> {
    context.host.open_job_get_kind(job)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn open_job_get_dev_id(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<u64> {
    context.host.open_job_get_dev_id(job)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn open_job_get_file_id(context: &mut ImportContext<'_, '_>, job: u64) -> AsyncHostResult<u64> {
    context.host.open_job_get_file_id(job)
}

fn read_guest_os_string(context: &mut ImportContext<'_, '_>, ptr: i32, len: i32) -> AsyncHostResult<OsString> {
    // Async OsString imports pass MoonBit String data, so `len` is UTF-16 code
    // units. Do not treat this as UTF-8 bytes or a native C string.
    context.with_memory_mut(|memory| {
        let units = read_u16(memory, ptr, len)?;

        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;

            let path = char::decode_utf16(units)
                .map(Result::unwrap)
                .collect::<String>();
            Ok(OsString::from_vec(path.into_bytes()))
        }

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;

            Ok(OsString::from_wide(&units))
        }
    })
}

}
