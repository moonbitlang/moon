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
    EVENT_BUFFER_SIZE, KeventBuffer, PROCESS_EVENT, PollEvent, PollInstance, READ_EVENT,
    WRITE_EVENT, last_errno, last_native_error,
};

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/kqueue.c",
        original = "moonbitlang_async_event_bus_create"
    )]
    pub(crate) fn poll_create() -> AsyncHostResult<PollInstance> {
        let fd = unsafe { libc::kqueue() };
        if fd < 0 {
            Err(last_native_error())
        } else {
            Ok(PollInstance {
                fd: unsafe { OwnedFd::from_raw_fd(fd) },
                raw_events: KeventBuffer(vec![empty_kevent(); EVENT_BUFFER_SIZE]),
                events: Vec::with_capacity(EVENT_BUFFER_SIZE),
            })
        }
    }

    #[ported(
        source = "src/internal/event_loop/kqueue.c",
        original = "moonbitlang_async_event_bus_destroy"
    )]
    pub(crate) fn poll_destroy(instance: PollInstance) {
        drop(instance);
    }

    #[ported(
        source = "src/internal/event_loop/kqueue.c",
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

        let flags = libc::EV_ADD | libc::EV_CLEAR;
        let events = [
            new_kevent(fd as libc::uintptr_t, libc::EVFILT_READ, flags, 0, 0),
            new_kevent(fd as libc::uintptr_t, libc::EVFILT_WRITE, flags, 0, 0),
        ];
        if unsafe {
            libc::kevent(
                instance.raw_fd(),
                events.as_ptr(),
                if read_only { 1 } else { 2 },
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
        source = "src/internal/event_loop/kqueue.c",
        original = "moonbitlang_async_event_bus_register_pid"
    )]
    pub(crate) fn poll_register_pid(instance: &PollInstance, pid: i32) -> AsyncHostResult<i32> {
        let flags = libc::EV_ADD;
        #[cfg(target_os = "macos")]
        let fflags = libc::NOTE_EXITSTATUS;
        #[cfg(not(target_os = "macos"))]
        let fflags = libc::NOTE_EXIT;
        let event = new_kevent(pid as libc::uintptr_t, libc::EVFILT_PROC, flags, fflags, 0);
        let ret = unsafe {
            libc::kevent(
                instance.raw_fd(),
                &event,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };

        if ret >= 0 {
            Ok(1)
        } else {
            let errno = last_errno();
            if errno == libc::ESRCH {
                Ok(0)
            } else {
                Err(AsyncHostError::Native(errno))
            }
        }
    }

    #[ported(
        source = "src/internal/event_loop/kqueue.c",
        original = "moonbitlang_async_event_bus_wait"
    )]
    pub(crate) fn poll_wait(instance: &mut PollInstance, timeout: i32) -> AsyncHostResult<i32> {
        let timeout_spec = libc::timespec {
            tv_sec: (timeout / 1000) as libc::time_t,
            tv_nsec: ((timeout % 1000) * 1_000_000) as libc::c_long,
        };
        let count = unsafe {
            libc::kevent(
                instance.raw_fd(),
                std::ptr::null(),
                0,
                instance.raw_events.0.as_mut_ptr(),
                EVENT_BUFFER_SIZE as i32,
                if timeout < 0 {
                    std::ptr::null()
                } else {
                    &timeout_spec
                },
            )
        };
        if count < 0 {
            return Err(last_native_error());
        }
        instance.events.clear();
        instance.events.extend(
            instance
                .raw_events
                .0
                .iter()
            .take(count as usize)
            .map(|event| PollEvent {
                fd: event.ident as RawFd,
                events: kqueue_result_events(event),
            }),
        );
        Ok(count)
    }

    #[ported(
        source = "src/internal/event_loop/kqueue.c",
        original = "moonbitlang_async_event_list_get"
    )]
    pub(crate) fn event_list_get(instance: &PollInstance, index: i32) -> AsyncHostResult<&PollEvent> {
        let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
        instance.events.get(index).ok_or(AsyncHostError::Fault)
    }

    #[ported(
        source = "src/internal/event_loop/kqueue.c",
        original = "moonbitlang_async_event_get_fd"
    )]
    pub(crate) fn event_get_fd(event: &PollEvent) -> RawFd {
        event.fd
    }

    #[ported(
        source = "src/internal/event_loop/kqueue.c",
        original = "moonbitlang_async_event_get_events"
    )]
    pub(crate) fn event_get_events(event: &PollEvent) -> i32 {
        event.events
    }
}

pub(crate) fn poll_unregister(instance: &PollInstance, fd: RawFd) -> AsyncHostResult<()> {
    for filter in [libc::EVFILT_READ, libc::EVFILT_WRITE] {
        let event = new_kevent(fd as libc::uintptr_t, filter, libc::EV_DELETE, 0, 0);
        if unsafe {
            libc::kevent(
                instance.raw_fd(),
                &event,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        } < 0
        {
            let errno = last_errno();
            if errno != libc::ENOENT {
                return Err(AsyncHostError::Native(errno));
            }
        }
    }
    Ok(())
}

fn kqueue_result_events(event: &libc::kevent) -> i32 {
    if event.filter == libc::EVFILT_READ {
        return READ_EVENT;
    }
    if event.filter == libc::EVFILT_WRITE {
        return WRITE_EVENT;
    }
    if event.filter == libc::EVFILT_PROC {
        return PROCESS_EVENT;
    }
    if (event.flags & libc::EV_ERROR) != 0 {
        return READ_EVENT | WRITE_EVENT;
    }
    0
}

fn new_kevent(
    ident: libc::uintptr_t,
    filter: i16,
    flags: u16,
    fflags: u32,
    data: libc::intptr_t,
) -> libc::kevent {
    let mut event = empty_kevent();
    event.ident = ident;
    event.filter = filter;
    event.flags = flags;
    event.fflags = fflags;
    event.data = data;
    event.udata = std::ptr::null_mut();
    event
}

fn empty_kevent() -> libc::kevent {
    unsafe { std::mem::zeroed() }
}
