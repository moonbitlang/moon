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

const EVENT_COUNT: usize = 1024;
const EVENT_SIZE: usize = std::mem::size_of::<libc::kevent>();

ported_fns! {
    #[ported(
        source = "src/fs/watch_kqueue.c",
        original = "moonbitlang_async_kqueue_watcher_create"
    )]
    pub(crate) fn create() -> AsyncHostResult<RawFd> {
        let kqueue = unsafe { libc::kqueue() };
        if kqueue < 0 {
            return Err(last_native_error());
        }
        let flags = unsafe { libc::fcntl(kqueue, libc::F_GETFD) };
        if flags < 0
            || (flags & libc::FD_CLOEXEC) == 0
                && unsafe { libc::fcntl(kqueue, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0
        {
            unsafe { libc::close(kqueue) };
            return Err(last_native_error());
        }
        Ok(kqueue)
    }

    #[ported(
        source = "src/fs/watch_kqueue.c",
        original = "moonbitlang_async_kqueue_watcher_buffer_size"
    )]
    pub(crate) fn buffer_size() -> u32 {
        u32::try_from(EVENT_COUNT * EVENT_SIZE).expect("kqueue watcher buffer size fits u32")
    }

    #[ported(
        source = "src/fs/watch_kqueue.c",
        original = "moonbitlang_async_kqueue_watcher_add_file"
    )]
    pub(crate) fn add_file(
        kqueue: RawFd,
        file: RawFd,
        is_dir: bool,
        file_handle: u64,
    ) -> AsyncHostResult<()> {
        let mut event = empty_kevent();
        event.ident = file as libc::uintptr_t;
        event.filter = libc::EVFILT_VNODE;
        event.flags = libc::EV_ADD | libc::EV_CLEAR;
        event.fflags = libc::NOTE_WRITE | if is_dir { 0 } else { libc::NOTE_EXTEND };
        event.udata = usize::try_from(file_handle)
            .map_err(|_| AsyncHostError::Fault)? as *mut libc::c_void;
        if unsafe {
            libc::kevent(
                kqueue,
                &event,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        } < 0
        {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/fs/watch_kqueue.c",
        original = "moonbitlang_async_kqueue_watcher_fetch_event"
    )]
    pub(crate) fn fetch_event(
        kqueue: RawFd,
        buffer: &mut [u8],
        len: u32,
    ) -> AsyncHostResult<i32> {
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let buffer = buffer.get_mut(..len).ok_or(AsyncHostError::Fault)?;
        let event_count = buffer.len() / EVENT_SIZE;
        let event_count = i32::try_from(event_count).map_err(|_| AsyncHostError::Fault)?;
        let timeout = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        let ret = unsafe {
            libc::kevent(
                kqueue,
                std::ptr::null(),
                0,
                buffer.as_mut_ptr().cast(),
                event_count,
                &timeout,
            )
        };
        if ret < 0 {
            Err(last_native_error())
        } else {
            Ok(ret)
        }
    }

    #[ported(
        source = "src/fs/watch_kqueue.c",
        original = "moonbitlang_async_kqueue_watcher_event_get_fd"
    )]
    pub(crate) fn event_get_fd(buffer: &[u8], index: u32) -> AsyncHostResult<u64> {
        let event = event_at(buffer, index)?;
        let handle = event.udata as usize;
        if handle == 0 {
            Ok(event.ident as u64)
        } else {
            Ok(handle as u64)
        }
    }

    #[ported(
        source = "src/fs/watch_kqueue.c",
        original = "moonbitlang_async_kqueue_watcher_event_has_modify"
    )]
    pub(crate) fn event_has_modify(buffer: &[u8], index: u32) -> AsyncHostResult<bool> {
        Ok(event_at(buffer, index)?.fflags & (libc::NOTE_WRITE | libc::NOTE_EXTEND) != 0)
    }
}

fn event_at(buffer: &[u8], index: u32) -> AsyncHostResult<libc::kevent> {
    let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
    let offset = index.checked_mul(EVENT_SIZE).ok_or(AsyncHostError::Fault)?;
    let end = offset
        .checked_add(EVENT_SIZE)
        .ok_or(AsyncHostError::Fault)?;
    if end > buffer.len() {
        return Err(AsyncHostError::Fault);
    }
    Ok(unsafe { std::ptr::read_unaligned(buffer.as_ptr().add(offset).cast()) })
}

fn empty_kevent() -> libc::kevent {
    unsafe { std::mem::zeroed() }
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}
