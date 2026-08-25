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

use crate::async_host::{AsyncHostError, AsyncHostResult, write_u16};
use crate::async_sys::fs::dir;
use crate::async_sys::fs::stub;
#[cfg(target_os = "linux")]
use crate::async_sys::fs::watch_inotify;
#[cfg(target_os = "macos")]
use crate::async_sys::fs::watch_kqueue;
#[cfg(windows)]
use crate::async_sys::fs::watch_windows;

use super::context::ImportContext;
use super::os_string::encode_guest_units;
use super::provenance::ported_imports;

ported_imports! {
pub(super) fn get_tmp_path_len(context: &mut ImportContext<'_, '_>) -> i32 {
    match context
        .host
        .temp_dir()
        .and_then(|path| encode_guest_units(&path))
        .and_then(|units| i32::try_from(units.len()).map_err(|_| AsyncHostError::Fault))
    {
        Ok(len) => len,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(source = "src/fs/stub.c")]
pub(super) fn get_tmp_path(context: &mut ImportContext<'_, '_>, ptr: u32, len: u32) -> i32 {
    let result = (|| {
        let path = context.host.temp_dir()?;
        let units = encode_guest_units(&path)?;
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        if len != units.len() {
            return Err(AsyncHostError::Inval);
        }
        context.with_memory_mut(|memory| write_u16(memory, ptr, &units))
    })();
    zero_or_minus_one(context, result)
}

pub(super) fn get_tmp_path_buffer(context: &mut ImportContext<'_, '_>) -> AsyncHostResult<u64> {
    let path = context.host.temp_dir()?;
    Ok(context
        .host
        .insert_c_buffer(stub::tmp_path_buffer(path.as_os_str())?))
}

#[ported(source = "src/internal/fd_util/stub.c")]
pub(super) fn close_fd(context: &mut ImportContext<'_, '_>, fd: u64) -> i32 {
    zero_or_minus_one(context, context.host.close_fd(fd))
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_buffer_min_size(_context: &mut ImportContext<'_, '_>) -> u32 {
    dir::buffer_min_size()
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_entry_length(
    context: &mut ImportContext<'_, '_>,
    buf: u64,
    offset: u32,
) -> AsyncHostResult<u32> {
    context.host
        .with_c_buffer(buf, |buf| dir::entry_length(buf, 0, offset))
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_entry_name_len(
    context: &mut ImportContext<'_, '_>,
    buf: u64,
    offset: u32,
) -> AsyncHostResult<u32> {
    context.host
        .with_c_buffer(buf, |buf| dir::entry_name_len(buf, 0, offset))
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_entry_name_offset(
    context: &mut ImportContext<'_, '_>,
    buf: u64,
    offset: u32,
) -> AsyncHostResult<u32> {
    context.host
        .with_c_buffer(buf, |buf| dir::entry_name_offset(buf, 0, offset))
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_entry_is_dir(
    context: &mut ImportContext<'_, '_>,
    buf: u64,
    offset: u32,
) -> AsyncHostResult<i32> {
    context.host
        .with_c_buffer(buf, |buf| dir::entry_is_dir(buf, 0, offset))
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_entry_is_hidden(
    context: &mut ImportContext<'_, '_>,
    buf: u64,
    offset: u32,
) -> AsyncHostResult<i32> {
    context.host
        .with_c_buffer(buf, |buf| dir::entry_is_hidden(buf, 0, offset))
        .map(|value| if value { 1 } else { 0 })
}

#[ported(source = "src/fs/dir.c")]
pub(super) fn dir_entry_file_id(
    context: &mut ImportContext<'_, '_>,
    buf: u64,
    offset: u32,
) -> AsyncHostResult<u64> {
    context.host
        .with_c_buffer(buf, |buf| dir::entry_file_id(buf, 0, offset))
}

#[ported(
    source = "src/fs/watch_inotify.c",
    original = "moonbitlang_async_inotify_create"
)]
#[cfg(target_os = "linux")]
pub(super) fn inotify_create(context: &mut ImportContext<'_, '_>) -> u64 {
    match watch_inotify::create() {
        Ok(fd) => context.host.insert_file_resource(fd),
        Err(error) => {
            context.host.record_error(error);
            context.host.invalid_fd()
        }
    }
}

#[ported(
    source = "src/fs/watch_inotify.c",
    original = "moonbitlang_async_inotify_remove_file"
)]
#[cfg(target_os = "linux")]
pub(super) fn inotify_remove_file(
    context: &mut ImportContext<'_, '_>,
    watcher: u64,
    wd: u64,
) -> i32 {
    use std::os::fd::AsRawFd;

    let result = (|| {
        let wd = i32::try_from(wd).map_err(|_| AsyncHostError::Inval)?;
        context.host.with_resource(watcher, |watcher| {
            watch_inotify::remove_file(watcher.as_fd()?.as_raw_fd(), wd)
        })
    })();
    zero_or_minus_one(context, result)
}

#[ported(
    source = "src/fs/watch_inotify.c",
    original = "moonbitlang_async_inotify_event_buffer_size"
)]
#[cfg(target_os = "linux")]
pub(super) fn inotify_event_buffer_size(_context: &mut ImportContext<'_, '_>) -> u32 {
    watch_inotify::event_buffer_size()
}

#[ported(
    source = "src/fs/watch_inotify.c",
    original = "moonbitlang_async_inotify_fetch_event"
)]
#[cfg(target_os = "linux")]
pub(super) fn inotify_fetch_event(
    context: &mut ImportContext<'_, '_>,
    watcher: u64,
    buffer: u64,
    len: u32,
) -> i32 {
    use std::os::fd::AsRawFd;

    let result = (|| {
        let watcher = context.host.acquire_resource(watcher)?;
        context.host.with_c_buffer_mut(buffer, |buffer| {
            watch_inotify::fetch_event(watcher.as_fd()?.as_raw_fd(), buffer, len)
        })
    })();
    match result {
        Ok(n) => n,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(
    source = "src/fs/watch_inotify.c",
    original = "moonbitlang_async_inotify_event_get_size"
)]
#[cfg(target_os = "linux")]
pub(super) fn inotify_event_get_size(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
    offset: u32,
) -> AsyncHostResult<u32> {
    context
        .host
        .with_c_buffer(buffer, |buffer| watch_inotify::event_get_size(buffer, offset))
}

#[ported(
    source = "src/fs/watch_inotify.c",
    original = "moonbitlang_async_inotify_event_get_wd"
)]
#[cfg(target_os = "linux")]
pub(super) fn inotify_event_get_wd(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
    offset: u32,
) -> AsyncHostResult<u64> {
    context.host.with_c_buffer(buffer, |buffer| {
        let wd = watch_inotify::event_get_wd(buffer, offset)?;
        Ok(i64::from(wd) as u64)
    })
}

#[ported(
    source = "src/fs/watch_inotify.c",
    original = "moonbitlang_async_inotify_event_has_relevant_event"
)]
#[cfg(target_os = "linux")]
pub(super) fn inotify_event_has_relevant_event(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
    offset: u32,
) -> AsyncHostResult<i32> {
    context.host.with_c_buffer(buffer, |buffer| {
        watch_inotify::event_has_relevant_event(buffer, offset).map(i32::from)
    })
}

#[ported(
    source = "src/fs/watch_inotify.c",
    original = "moonbitlang_async_inotify_event_has_overflow"
)]
#[cfg(target_os = "linux")]
pub(super) fn inotify_event_has_overflow(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
    offset: u32,
) -> AsyncHostResult<i32> {
    context.host.with_c_buffer(buffer, |buffer| {
        watch_inotify::event_has_overflow(buffer, offset).map(i32::from)
    })
}

#[ported(
    source = "src/fs/watch_inotify.c",
    original = "moonbitlang_async_inotify_event_has_ignore"
)]
#[cfg(target_os = "linux")]
pub(super) fn inotify_event_has_ignore(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
    offset: u32,
) -> AsyncHostResult<i32> {
    context.host.with_c_buffer(buffer, |buffer| {
        watch_inotify::event_has_ignore(buffer, offset).map(i32::from)
    })
}

#[ported(
    source = "src/fs/watch_kqueue.c",
    original = "moonbitlang_async_kqueue_watcher_create"
)]
#[cfg(target_os = "macos")]
pub(super) fn kqueue_watcher_create(context: &mut ImportContext<'_, '_>) -> u64 {
    match watch_kqueue::create() {
        Ok(fd) => context.host.insert_file_resource(fd),
        Err(error) => {
            context.host.record_error(error);
            context.host.invalid_fd()
        }
    }
}

#[ported(
    source = "src/fs/watch_kqueue.c",
    original = "moonbitlang_async_kqueue_watcher_buffer_size"
)]
#[cfg(target_os = "macos")]
pub(super) fn kqueue_watcher_buffer_size(_context: &mut ImportContext<'_, '_>) -> u32 {
    watch_kqueue::buffer_size()
}

#[ported(
    source = "src/fs/watch_kqueue.c",
    original = "moonbitlang_async_kqueue_watcher_add_file"
)]
#[cfg(target_os = "macos")]
pub(super) fn kqueue_watcher_add_file(
    context: &mut ImportContext<'_, '_>,
    kqueue: u64,
    file: u64,
    is_dir: i32,
) -> i32 {
    zero_or_minus_one(
        context,
        context
            .host
            .kqueue_watcher_add_file(kqueue, file, is_dir != 0),
    )
}

#[ported(
    source = "src/fs/watch_kqueue.c",
    original = "moonbitlang_async_kqueue_watcher_fetch_event"
)]
#[cfg(target_os = "macos")]
pub(super) fn kqueue_watcher_fetch_event(
    context: &mut ImportContext<'_, '_>,
    kqueue: u64,
    buffer: u64,
    len: u32,
) -> i32 {
    use std::os::fd::AsRawFd;

    let result = (|| {
        let kqueue = context.host.acquire_resource(kqueue)?;
        context.host.with_c_buffer_mut(buffer, |buffer| {
            watch_kqueue::fetch_event(kqueue.as_fd()?.as_raw_fd(), buffer, len)
        })
    })();
    match result {
        Ok(n) => n,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(
    source = "src/fs/watch_kqueue.c",
    original = "moonbitlang_async_kqueue_watcher_event_get_fd"
)]
#[cfg(target_os = "macos")]
pub(super) fn kqueue_watcher_event_get_fd(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
    index: u32,
) -> AsyncHostResult<u64> {
    context
        .host
        .with_c_buffer(buffer, |buffer| watch_kqueue::event_get_fd(buffer, index))
}

#[ported(
    source = "src/fs/watch_kqueue.c",
    original = "moonbitlang_async_kqueue_watcher_event_has_modify"
)]
#[cfg(target_os = "macos")]
pub(super) fn kqueue_watcher_event_has_modify(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
    index: u32,
) -> AsyncHostResult<i32> {
    context.host.with_c_buffer(buffer, |buffer| {
        watch_kqueue::event_has_modify(buffer, index).map(i32::from)
    })
}

#[cfg(windows)]
pub(super) fn windows_watcher_buffer_new(context: &mut ImportContext<'_, '_>) -> u64 {
    context.host.insert_windows_watcher_buffer()
}

#[cfg(windows)]
pub(super) fn windows_watcher_buffer_free(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
) -> AsyncHostResult<()> {
    context.host.free_windows_watcher_buffer(buffer)
}

#[ported(
    source = "src/fs/watch_windows.c",
    original = "moonbitlang_async_watcher_event_get_size"
)]
#[cfg(windows)]
pub(super) fn watcher_event_get_size(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
    offset: u32,
) -> AsyncHostResult<u32> {
    context
        .host
        .with_windows_watcher_buffer(buffer, |buffer| {
            watch_windows::event_get_size(buffer, offset)
        })
}

#[ported(
    source = "src/fs/watch_windows.c",
    original = "moonbitlang_async_watcher_event_is_modify_event"
)]
#[cfg(windows)]
pub(super) fn watcher_event_is_modify(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
    offset: u32,
) -> AsyncHostResult<i32> {
    context.host.with_windows_watcher_buffer(buffer, |buffer| {
        watch_windows::event_is_modify(buffer, offset).map(i32::from)
    })
}

#[cfg(windows)]
pub(super) fn watcher_event_path_len(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
    offset: u32,
) -> AsyncHostResult<u32> {
    context
        .host
        .with_windows_watcher_buffer(buffer, |buffer| {
            watch_windows::event_path_len(buffer, offset)
        })
}

#[cfg(windows)]
pub(super) fn watcher_event_path_copy(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
    offset: u32,
    out: u32,
    out_len: u32,
) -> AsyncHostResult<()> {
    let path = context
        .host
        .with_windows_watcher_buffer(buffer, |buffer| {
            watch_windows::event_path_units(buffer, offset)
        })?;
    if usize::try_from(out_len).map_err(|_| AsyncHostError::Fault)? != path.len() {
        return Err(AsyncHostError::Inval);
    }
    context.with_memory_mut(|memory| write_u16(memory, out, &path))
}

#[cfg(windows)]
pub(super) fn watcher_event_has_file_ids(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
) -> AsyncHostResult<i32> {
    context.host.with_windows_watcher_buffer(buffer, |buffer| {
        watch_windows::event_has_file_ids(buffer).map(i32::from)
    })
}

#[cfg(windows)]
pub(super) fn watcher_event_dirty_file_id(
    context: &mut ImportContext<'_, '_>,
    buffer: u64,
    offset: u32,
) -> AsyncHostResult<u64> {
    context.host.with_windows_watcher_buffer(buffer, |buffer| {
        watch_windows::event_dirty_file_id(buffer, offset)
    })
}

#[ported(source = "src/fs/stub.c")]
pub(super) fn errno_is_lock_violation(_context: &mut ImportContext<'_, '_>, errno: i32) -> i32 {
    if stub::errno_is_lock_violation(errno) {
        1
    } else {
        0
    }
}

#[ported(source = "src/fs/stub.c")]
pub(super) fn try_lock_file(context: &mut ImportContext<'_, '_>, fd: u64, exclusive: i32) -> i32 {
    zero_or_minus_one(context, context.host.try_lock_file(fd, exclusive != 0))
}

#[ported(source = "src/fs/stub.c")]
pub(super) fn unlock_file(context: &mut ImportContext<'_, '_>, fd: u64) -> i32 {
    zero_or_minus_one(context, context.host.unlock_file(fd))
}

fn zero_or_minus_one(context: &ImportContext<'_, '_>, result: AsyncHostResult<()>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

}
