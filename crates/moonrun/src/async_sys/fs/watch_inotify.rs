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
use crate::async_sys::internal::fd_util::stub::RawFd;
use crate::async_sys::ported_fns;

const EVENT_HEADER_SIZE: usize = std::mem::size_of::<libc::inotify_event>();
const NAME_MAX: usize = 255;

ported_fns! {
    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_create"
    )]
    pub(crate) fn create() -> AsyncHostResult<RawFd> {
        let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if fd < 0 {
            Err(last_native_error())
        } else {
            Ok(fd)
        }
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_remove_file"
    )]
    pub(crate) fn remove_file(inotify: RawFd, wd: i32) -> AsyncHostResult<()> {
        if unsafe { libc::inotify_rm_watch(inotify, wd) } < 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_event_buffer_size"
    )]
    pub(crate) fn event_buffer_size() -> u32 {
        let min_size = EVENT_HEADER_SIZE + NAME_MAX + 1;
        u32::try_from(4096usize.max(min_size)).expect("inotify event buffer size fits u32")
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_fetch_event"
    )]
    pub(crate) fn fetch_event(
        inotify: RawFd,
        buffer: &mut [u8],
        len: u32,
    ) -> AsyncHostResult<i32> {
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let buffer = buffer.get_mut(..len).ok_or(AsyncHostError::Fault)?;
        let ret = unsafe { libc::read(inotify, buffer.as_mut_ptr().cast(), buffer.len()) };
        if ret > 0 {
            i32::try_from(ret).map_err(|_| AsyncHostError::Fault)
        } else if ret < 0 && last_errno() == libc::EAGAIN {
            Ok(0)
        } else if ret < 0 {
            Err(last_native_error())
        } else {
            Ok(0)
        }
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_event_get_size"
    )]
    pub(crate) fn event_get_size(buffer: &[u8], offset: u32) -> AsyncHostResult<u32> {
        let event = event_at(buffer, offset)?;
        let size = EVENT_HEADER_SIZE
            .checked_add(event.len as usize)
            .ok_or(AsyncHostError::Fault)?;
        let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
        if offset.checked_add(size).ok_or(AsyncHostError::Fault)? > buffer.len() {
            return Err(AsyncHostError::Fault);
        }
        u32::try_from(size).map_err(|_| AsyncHostError::Fault)
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_event_get_wd"
    )]
    pub(crate) fn event_get_wd(buffer: &[u8], offset: u32) -> AsyncHostResult<i32> {
        Ok(event_at(buffer, offset)?.wd)
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_event_has_relevant_event"
    )]
    pub(crate) fn event_has_relevant_event(
        buffer: &[u8],
        offset: u32,
    ) -> AsyncHostResult<bool> {
        let mask = event_at(buffer, offset)?.mask;
        Ok(mask
            & (libc::IN_CREATE
                | libc::IN_DELETE
                | libc::IN_MODIFY
                | libc::IN_MOVED_FROM
                | libc::IN_MOVED_TO)
            != 0)
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_event_has_overflow"
    )]
    pub(crate) fn event_has_overflow(buffer: &[u8], offset: u32) -> AsyncHostResult<bool> {
        Ok(event_at(buffer, offset)?.mask & libc::IN_Q_OVERFLOW != 0)
    }

    #[ported(
        source = "src/fs/watch_inotify.c",
        original = "moonbitlang_async_inotify_event_has_ignore"
    )]
    pub(crate) fn event_has_ignore(buffer: &[u8], offset: u32) -> AsyncHostResult<bool> {
        Ok(event_at(buffer, offset)?.mask & libc::IN_IGNORED != 0)
    }
}

fn event_at(buffer: &[u8], offset: u32) -> AsyncHostResult<libc::inotify_event> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let end = offset
        .checked_add(EVENT_HEADER_SIZE)
        .ok_or(AsyncHostError::Fault)?;
    if end > buffer.len() {
        return Err(AsyncHostError::Fault);
    }
    Ok(unsafe { std::ptr::read_unaligned(buffer.as_ptr().add(offset).cast()) })
}

fn last_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_else(|| AsyncHostError::Inval.errno())
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(last_errno())
}
