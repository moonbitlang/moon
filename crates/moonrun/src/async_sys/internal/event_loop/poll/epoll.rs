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
use std::os::fd::{FromRawFd, OwnedFd};

use super::{
    EVENT_BUFFER_SIZE, PollEvent, PollInstance, READ_EVENT, WRITE_EVENT, last_native_error,
};

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_bus_create"
    )]
    pub(crate) fn poll_create() -> AsyncHostResult<PollInstance> {
        let fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if fd < 0 {
            Err(last_native_error())
        } else {
            Ok(PollInstance {
                fd: unsafe { OwnedFd::from_raw_fd(fd) },
                raw_events: vec![
                    libc::epoll_event { events: 0, u64: 0 };
                    EVENT_BUFFER_SIZE
                ],
                events: Vec::with_capacity(EVENT_BUFFER_SIZE),
            })
        }
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_bus_destroy"
    )]
    pub(crate) fn poll_destroy(instance: PollInstance) {
        drop(instance);
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_bus_register"
    )]
    pub(crate) fn poll_register(
        instance: &PollInstance,
        fd: RawFd,
        read_only: bool,
    ) -> AsyncHostResult<()> {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags >= 0 && (flags & libc::O_NONBLOCK) == 0 {
            unsafe {
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }
        let events = if read_only {
            READ_EVENT
        } else {
            READ_EVENT | WRITE_EVENT
        };
        let mut event = libc::epoll_event {
            events: epoll_event_mask(events)?,
            u64: fd as u64,
        };
        if unsafe { libc::epoll_ctl(instance.raw_fd(), libc::EPOLL_CTL_ADD, fd, &mut event) } < 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_bus_wait"
    )]
    pub(crate) fn poll_wait(instance: &mut PollInstance, timeout: i32) -> AsyncHostResult<i32> {
        let count = unsafe {
            libc::epoll_wait(
                instance.raw_fd(),
                instance.raw_events.as_mut_ptr(),
                EVENT_BUFFER_SIZE as i32,
                timeout,
            )
        };
        if count < 0 {
            return Err(last_native_error());
        }
        instance.events.clear();
        instance.events.extend(
            instance
            .raw_events
            .iter()
            .take(count as usize)
            .map(|event| PollEvent {
                fd: event.u64 as RawFd,
                events: epoll_result_events(event.events),
            }),
        );
        Ok(count)
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_list_get"
    )]
    pub(crate) fn event_list_get(instance: &PollInstance, index: i32) -> AsyncHostResult<&PollEvent> {
        let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
        instance.events.get(index).ok_or(AsyncHostError::Fault)
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_get_fd"
    )]
    pub(crate) fn event_get_fd(event: &PollEvent) -> RawFd {
        event.fd
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_get_events"
    )]
    pub(crate) fn event_get_events(event: &PollEvent) -> i32 {
        event.events
    }
}

pub(crate) fn poll_unregister(instance: &PollInstance, fd: RawFd) -> AsyncHostResult<()> {
    if unsafe {
        libc::epoll_ctl(
            instance.raw_fd(),
            libc::EPOLL_CTL_DEL,
            fd,
            std::ptr::null_mut(),
        )
    } < 0
    {
        let errno = super::last_errno();
        if errno == libc::ENOENT {
            Ok(())
        } else {
            Err(AsyncHostError::Native(errno))
        }
    } else {
        Ok(())
    }
}

fn epoll_event_mask(events: i32) -> AsyncHostResult<u32> {
    let mut mask = match events {
        READ_EVENT => libc::EPOLLIN,
        WRITE_EVENT => libc::EPOLLOUT,
        events if events == (READ_EVENT | WRITE_EVENT) => libc::EPOLLIN | libc::EPOLLOUT,
        _ => return Err(AsyncHostError::Inval),
    };
    mask |= libc::EPOLLET;
    mask |= libc::EPOLLRDHUP;
    Ok(mask as u32)
}

fn epoll_result_events(events: u32) -> i32 {
    if (events & (libc::EPOLLERR | libc::EPOLLHUP | libc::EPOLLRDHUP) as u32) != 0 {
        return READ_EVENT | WRITE_EVENT;
    }

    let mut result = 0;
    if (events & libc::EPOLLIN as u32) != 0 {
        result |= READ_EVENT;
    }
    if (events & libc::EPOLLOUT as u32) != 0 {
        result |= WRITE_EVENT;
    }
    result
}
