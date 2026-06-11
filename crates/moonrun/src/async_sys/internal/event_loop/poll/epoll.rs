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

use super::{
    EVENT_BUFFER_SIZE, PROCESS_EVENT, PollEvent, PollInstance, READ_EVENT, WRITE_EVENT,
    last_native_error,
};

const PID_MASK: u64 = 1 << 63;

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_poll_create"
    )]
    #[allow(dead_code)]
    pub(crate) fn poll_create() -> AsyncHostResult<PollInstance> {
        let fd = unsafe { libc::epoll_create1(0) };
        if fd < 0 {
            Err(last_native_error())
        } else {
            Ok(PollInstance {
                fd,
                events: Vec::new(),
            })
        }
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_poll_destroy"
    )]
    #[allow(dead_code)]
    pub(crate) fn poll_destroy(instance: PollInstance) {
        drop(instance);
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_poll_register"
    )]
    #[allow(dead_code)]
    pub(crate) fn poll_register(
        instance: &PollInstance,
        fd: RawFd,
        prev_events: i32,
        new_events: i32,
        oneshot: bool,
    ) -> AsyncHostResult<()> {
        let mut event = libc::epoll_event {
            events: epoll_event_mask(prev_events | new_events, oneshot)?,
            u64: fd as u64,
        };
        let op = if prev_events == 0 {
            libc::EPOLL_CTL_ADD
        } else {
            libc::EPOLL_CTL_MOD
        };
        if unsafe { libc::epoll_ctl(instance.fd, op, fd, &mut event) } < 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_support_wait_pid_via_poll"
    )]
    #[allow(dead_code)]
    pub(crate) fn support_wait_pid_via_poll() -> bool {
        let pidfd = unsafe { libc::syscall(libc::SYS_pidfd_open, std::process::id(), 0) };
        if pidfd >= 0 {
            unsafe { libc::close(pidfd as RawFd) };
            true
        } else {
            false
        }
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_poll_register_pid"
    )]
    #[allow(dead_code)]
    pub(crate) fn poll_register_pid(instance: &PollInstance, pid: i32) -> AsyncHostResult<i32> {
        let pidfd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        if pidfd < 0 {
            return Err(last_native_error());
        }

        let pidfd = pidfd as RawFd;
        let mut event = libc::epoll_event {
            events: libc::EPOLLIN as u32,
            u64: PID_MASK | pidfd as u64,
        };
        if unsafe { libc::epoll_ctl(instance.fd, libc::EPOLL_CTL_ADD, pidfd, &mut event) } < 0 {
            let error = last_native_error();
            unsafe { libc::close(pidfd) };
            Err(error)
        } else {
            Ok(pidfd)
        }
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_poll_remove"
    )]
    #[allow(dead_code)]
    pub(crate) fn poll_remove(instance: &PollInstance, fd: RawFd, _events: i32) -> AsyncHostResult<()> {
        if unsafe { libc::epoll_ctl(instance.fd, libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut()) } < 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_poll_remove_pid"
    )]
    #[allow(dead_code)]
    pub(crate) fn poll_remove_pid(instance: &PollInstance, pidfd: i32) -> AsyncHostResult<()> {
        let ret = unsafe { libc::epoll_ctl(instance.fd, libc::EPOLL_CTL_DEL, pidfd, std::ptr::null_mut()) };
        let error = if ret < 0 { Some(last_native_error()) } else { None };
        unsafe { libc::close(pidfd) };
        match error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_poll_wait"
    )]
    #[allow(dead_code)]
    pub(crate) fn poll_wait(instance: &mut PollInstance, timeout: i32) -> AsyncHostResult<i32> {
        let mut events = vec![libc::epoll_event { events: 0, u64: 0 }; EVENT_BUFFER_SIZE];
        let count = unsafe {
            libc::epoll_wait(
                instance.fd,
                events.as_mut_ptr(),
                EVENT_BUFFER_SIZE as i32,
                timeout,
            )
        };
        if count < 0 {
            return Err(last_native_error());
        }
        instance.events = events
            .into_iter()
            .take(count as usize)
            .map(|event| {
                let is_pid = (event.u64 & PID_MASK) != 0;
                PollEvent {
                    fd: (event.u64 & !PID_MASK) as RawFd,
                    events: if is_pid {
                        PROCESS_EVENT
                    } else {
                        epoll_result_events(event.events)
                    },
                }
            })
            .collect();
        Ok(count)
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_list_get"
    )]
    #[allow(dead_code)]
    pub(crate) fn event_list_get(instance: &PollInstance, index: i32) -> AsyncHostResult<&PollEvent> {
        let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
        instance.events.get(index).ok_or(AsyncHostError::Fault)
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_get_fd"
    )]
    #[allow(dead_code)]
    pub(crate) fn event_get_fd(event: &PollEvent) -> RawFd {
        event.fd
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_get_events"
    )]
    #[allow(dead_code)]
    pub(crate) fn event_get_events(event: &PollEvent) -> i32 {
        event.events
    }
}

fn epoll_event_mask(events: i32, oneshot: bool) -> AsyncHostResult<u32> {
    let mut mask = match events {
        0 => 0,
        READ_EVENT => libc::EPOLLIN,
        WRITE_EVENT => libc::EPOLLOUT,
        events if events == (READ_EVENT | WRITE_EVENT) => libc::EPOLLIN | libc::EPOLLOUT,
        _ => return Err(AsyncHostError::Inval),
    };
    if oneshot {
        mask |= libc::EPOLLONESHOT;
    }
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
