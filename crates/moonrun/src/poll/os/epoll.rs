// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use crate::async_sys::ported_fns;
use crate::filesystem::RawFile as RawFd;
use std::io;
use std::os::fd::{FromRawFd, OwnedFd};

use super::{
    EVENT_BUFFER_SIZE, PollEvent, PollInstance, READ_EVENT, WRITE_EVENT, last_native_error,
};

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_bus_create"
    )]
    pub(in crate::poll) fn poll_create() -> io::Result<PollInstance> {
        let fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if fd < 0 {
            Err(last_native_error())
        } else {
            Ok(PollInstance {
                fd: unsafe { OwnedFd::from_raw_fd(fd) },
                raw_events: vec![
                    libc::epoll_event { events: 0, u64: 0 };
                    EVENT_BUFFER_SIZE
                ]
                .into_boxed_slice(),
                event_count: 0,
            })
        }
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_bus_destroy"
    )]
    pub(in crate::poll) fn poll_destroy(instance: PollInstance) {
        drop(instance);
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_bus_register"
    )]
    pub(in crate::poll) fn poll_register(
        instance: &PollInstance,
        fd: RawFd,
        read_only: bool,
        user_data: usize,
    ) -> io::Result<bool> {
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
            u64: user_data as u64,
        };
        if unsafe { libc::epoll_ctl(instance.raw_fd(), libc::EPOLL_CTL_ADD, fd, &mut event) } < 0 {
            let errno = super::last_errno();
            if errno == libc::EPERM {
                Ok(false)
            } else {
                Err(io::Error::from_raw_os_error(errno))
            }
        } else {
            Ok(true)
        }
    }

    pub(in crate::poll) fn poll_register_legacy(
        instance: &PollInstance,
        fd: RawFd,
        read_only: bool,
        user_data: usize,
    ) -> io::Result<()> {
        // Keep the pre-#536 syscall shape for event_bus/register compatibility.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags >= 0 && (flags & libc::O_NONBLOCK) == 0 {
            unsafe {
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }

        let mut events = (libc::EPOLLIN | libc::EPOLLET | libc::EPOLLRDHUP) as u32;
        if !read_only {
            events |= libc::EPOLLOUT as u32;
        }
        let mut event = libc::epoll_event {
            events,
            u64: user_data as u64,
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
    pub(in crate::poll) fn poll_wait(instance: &mut PollInstance, timeout: i32) -> io::Result<i32> {
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
        instance.event_count = count as usize;
        Ok(count)
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_list_get"
    )]
    pub(in crate::poll) fn event_list_get(instance: &PollInstance, index: usize) -> Option<&PollEvent> {
        instance.raw_events.get(..instance.event_count)?.get(index)
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_get_fd"
    )]
    pub(in crate::poll) fn event_get_fd(event: &PollEvent) -> u64 {
        event.u64
    }

    #[ported(
        source = "src/internal/event_loop/epoll.c",
        original = "moonbitlang_async_event_get_events"
    )]
    pub(in crate::poll) fn event_get_events(event: &PollEvent) -> i32 {
        epoll_result_events(event.events)
    }
}

// Host completion sources coalesce wakeups. Level triggering preserves
// readiness when the guest fetches only part of the pending notifications.
pub(in crate::poll) fn poll_register_completion_source(
    instance: &PollInstance,
    fd: RawFd,
    user_data: usize,
) -> io::Result<()> {
    let mut event = libc::epoll_event {
        events: libc::EPOLLIN as u32,
        u64: user_data as u64,
    };
    if unsafe { libc::epoll_ctl(instance.raw_fd(), libc::EPOLL_CTL_ADD, fd, &mut event) } < 0 {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

pub(in crate::poll) fn poll_unregister(instance: &PollInstance, fd: RawFd) -> io::Result<()> {
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
            Err(io::Error::from_raw_os_error(errno))
        }
    } else {
        Ok(())
    }
}

fn epoll_event_mask(events: i32) -> io::Result<u32> {
    let mut mask = match events {
        READ_EVENT => libc::EPOLLIN,
        WRITE_EVENT => libc::EPOLLOUT,
        events if events == (READ_EVENT | WRITE_EVENT) => libc::EPOLLIN | libc::EPOLLOUT,
        _ => return Err(io::Error::from_raw_os_error(libc::EINVAL)),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;

    #[test]
    fn nonpollable_fd_is_reported_as_unsupported() {
        let poll = poll_create().unwrap();
        let null = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/null")
            .unwrap();

        assert!(!poll_register(&poll, null.as_raw_fd(), false, 1).unwrap());
    }

    #[test]
    fn legacy_registration_preserves_nonpollable_error() {
        let poll = poll_create().unwrap();
        let null = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/null")
            .unwrap();

        assert_eq!(
            poll_register_legacy(&poll, null.as_raw_fd(), false, 1)
                .unwrap_err()
                .raw_os_error(),
            Some(libc::EPERM)
        );
    }
}
