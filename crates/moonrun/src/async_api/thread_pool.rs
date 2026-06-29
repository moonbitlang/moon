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
use crate::async_sys::internal::event_loop::thread_pool;

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
    worker_id: i32,
    waiting_worker_id: u64,
) -> AsyncHostResult<u64> {
    context.host.spawn_worker(worker_id, waiting_worker_id)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn free_worker(context: &mut ImportContext<'_, '_>, worker: u64) -> AsyncHostResult<()> {
    context.host.free_worker(worker)
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn wake_worker(
    context: &mut ImportContext<'_, '_>,
    worker: u64,
    job_id: i32,
    job: u64,
) -> AsyncHostResult<()> {
    context.host.wake_worker(worker, job_id, job)
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
    context.host
        .insert_job(thread_pool::make_read_job(fd, len, position))
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
    let offset_ptr = ptr.checked_add(offset).ok_or(AsyncHostError::Fault)?;
    let data =
        context.with_memory_mut(|memory| Ok(memory.read_exact(offset_ptr, len)?.to_vec()))?;

    context.host
        .insert_job(thread_pool::make_write_job(fd, data, position))
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
    context.host.insert_job(thread_pool::make_file_size_job(fd))
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
    context.host
        .insert_job(thread_pool::make_file_time_job(fd))
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
    context.host
        .insert_job(thread_pool::make_fsync_job(fd, only_data != 0))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_flock_job(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    exclusive: i32,
) -> AsyncHostResult<u64> {
    context.host
        .insert_job(thread_pool::make_flock_job(fd, exclusive != 0))
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
    let addr = context.with_memory_mut(|memory| Ok(memory.read_exact(addr, addr_len)?.to_vec()))?;
    context
        .host
        .insert_job(thread_pool::make_bind_job(socket, addr))
}

#[ported(source = "src/internal/event_loop/thread_pool.c")]
pub(super) fn make_getaddrinfo_job(
    context: &mut ImportContext<'_, '_>,
    host: i32,
    host_len: i32,
) -> AsyncHostResult<u64> {
    let host = read_guest_os_string(context, host, host_len)?;
    context
        .host
        .insert_job(thread_pool::make_getaddrinfo_job(host))
}

pub(super) fn get_getaddrinfo_result(
    context: &mut ImportContext<'_, '_>,
    job: u64,
) -> AsyncHostResult<u64> {
    context.host.get_getaddrinfo_result(job)
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

            let path = char::decode_utf16(units.iter().copied())
                .map(Result::unwrap)
                .collect::<String>();
            Ok(OsString::from_vec(path.into_bytes()))
        }

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;

            Ok(OsString::from_wide(units))
        }
    })
}

}
