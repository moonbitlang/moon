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

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::fd_util::stub::RawFd;
use crate::async_sys::ported_fns;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

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
                fd: std::sync::Arc::new(unsafe { OwnedFd::from_raw_fd(fd) }),
                raw_events: KeventBuffer(
                    vec![empty_kevent(); EVENT_BUFFER_SIZE].into_boxed_slice(),
                ),
                event_count: 0,
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
        fd_handle: u64,
    ) -> AsyncHostResult<i32> {
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags >= 0 && (flags & libc::O_NONBLOCK) == 0 {
            unsafe {
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }

        let flags = libc::EV_ADD | libc::EV_CLEAR | libc::EV_RECEIPT;
        let events = [
            new_kevent(
                fd as libc::uintptr_t,
                libc::EVFILT_READ,
                flags,
                0,
                0,
                Some(fd_handle),
            )?,
            new_kevent(
                fd as libc::uintptr_t,
                libc::EVFILT_WRITE,
                flags,
                0,
                0,
                Some(fd_handle),
            )?,
        ];
        let mut receipts = [empty_kevent(); 2];
        let result = unsafe {
            libc::kevent(
                instance.raw_fd(),
                events.as_ptr(),
                if read_only { 1 } else { 2 },
                receipts.as_mut_ptr(),
                receipts.len() as i32,
                std::ptr::null(),
            )
        };
        if result < 0 {
            return Err(last_native_error());
        }
        Ok(i32::from(
            receipts[..result as usize]
                .iter()
                .any(|receipt| receipt.data == 0),
        ))
    }

    pub(crate) fn poll_register_legacy(
        instance: &PollInstance,
        fd: RawFd,
        read_only: bool,
        fd_handle: u64,
    ) -> AsyncHostResult<()> {
        // Keep the pre-#536 syscall shape for event_bus/register compatibility.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags >= 0 && (flags & libc::O_NONBLOCK) == 0 {
            unsafe {
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }

        let flags = libc::EV_ADD | libc::EV_CLEAR;
        let events = [
            new_kevent(
                fd as libc::uintptr_t,
                libc::EVFILT_READ,
                flags,
                0,
                0,
                Some(fd_handle),
            )?,
            new_kevent(
                fd as libc::uintptr_t,
                libc::EVFILT_WRITE,
                flags,
                0,
                0,
                Some(fd_handle),
            )?,
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
        let event = new_kevent(
            pid as libc::uintptr_t,
            libc::EVFILT_PROC,
            flags,
            fflags,
            0,
            None,
        )?;
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
        instance.event_count = count as usize;
        Ok(count)
    }

    #[ported(
        source = "src/internal/event_loop/kqueue.c",
        original = "moonbitlang_async_event_list_get"
    )]
    pub(crate) fn event_list_get(instance: &PollInstance, index: u32) -> AsyncHostResult<&PollEvent> {
        let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
        if index >= instance.event_count {
            return Err(AsyncHostError::Fault);
        }
        instance
            .raw_events
            .0
            .get(index)
            .ok_or(AsyncHostError::Fault)
    }

    #[ported(
        source = "src/internal/event_loop/kqueue.c",
        original = "moonbitlang_async_event_get_fd"
    )]
    pub(crate) fn event_get_fd(event: &PollEvent) -> u64 {
        let fd_handle = event.udata as usize;
        // Resource filters carry the guest Resource Handle in udata. Process
        // filters have no Resource Handle and preserve native's ident result.
        if fd_handle == 0 {
            event.ident as u64
        } else {
            fd_handle as u64
        }
    }

    #[ported(
        source = "src/internal/event_loop/kqueue.c",
        original = "moonbitlang_async_event_get_events"
    )]
    pub(crate) fn event_get_events(event: &PollEvent) -> i32 {
        kqueue_result_events(event)
    }
}

pub(crate) fn event_get_pid(event: &PollEvent) -> RawFd {
    event.ident as RawFd
}

// Host completion sources coalesce wakeups. Omit EV_CLEAR so readiness
// persists when the guest fetches only part of the pending notifications.
pub(crate) fn poll_register_completion_source(
    instance: &PollInstance,
    fd: RawFd,
    fd_handle: u64,
) -> AsyncHostResult<()> {
    let change = new_kevent(
        fd as libc::uintptr_t,
        libc::EVFILT_READ,
        libc::EV_ADD,
        0,
        0,
        Some(fd_handle),
    )?;
    if unsafe {
        libc::kevent(
            instance.raw_fd(),
            &change,
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

pub(crate) fn poll_unregister(instance: &PollInstance, fd: RawFd) -> AsyncHostResult<()> {
    for filter in [libc::EVFILT_READ, libc::EVFILT_WRITE] {
        let event = new_kevent(fd as libc::uintptr_t, filter, libc::EV_DELETE, 0, 0, None)?;
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
    if event.filter == libc::EVFILT_PROC || event.filter == libc::EVFILT_USER {
        return PROCESS_EVENT;
    }
    if (event.flags & libc::EV_ERROR) != 0 {
        return READ_EVENT | WRITE_EVENT;
    }
    0
}

// One-shot virtual process events use a separate kqueue filter namespace.
// The parent poll owns the Waker; the tree holds only a weak observation.
// An in-flight notification retains this exact descriptor until kevent returns,
// even if the parent closes its poll meanwhile.
#[derive(Debug)]
pub(crate) struct ProcessNotification {
    queue: std::sync::Arc<OwnedFd>,
    pid: i32,
}

impl std::task::Wake for ProcessNotification {
    fn wake(self: std::sync::Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &std::sync::Arc<Self>) {
        if let Err(error) =
            change_process_notification(self.queue.as_raw_fd(), self.pid, 0, libc::NOTE_TRIGGER)
        {
            // A retried trigger may find its one-shot event already delivered.
            if error != AsyncHostError::Native(libc::ENOENT) {
                tracing::error!(
                    ?error,
                    pid = self.pid,
                    "failed to notify a virtual process exit"
                );
            }
        }
    }
}

impl Drop for ProcessNotification {
    fn drop(&mut self) {
        // Revalidation may report ready before the event is collected. Remove
        // that abandoned filter too, instead of retaining it until poll close.
        if let Err(error) =
            change_process_notification(self.queue.as_raw_fd(), self.pid, libc::EV_DELETE, 0)
            && error != AsyncHostError::Native(libc::ENOENT)
        {
            tracing::error!(
                ?error,
                pid = self.pid,
                "failed to remove a virtual process event"
            );
        }
    }
}

pub(crate) fn register_process_notification(
    instance: &PollInstance,
    pid: i32,
) -> AsyncHostResult<ProcessNotification> {
    change_process_notification(instance.raw_fd(), pid, libc::EV_ADD | libc::EV_ONESHOT, 0)?;
    Ok(ProcessNotification {
        queue: std::sync::Arc::clone(&instance.fd),
        pid,
    })
}

fn change_process_notification(
    fd: RawFd,
    pid: i32,
    flags: u16,
    fflags: u32,
) -> AsyncHostResult<()> {
    let event = new_kevent(pid as _, libc::EVFILT_USER, flags, fflags, 0, None)?;
    loop {
        if unsafe { libc::kevent(fd, &event, 1, std::ptr::null_mut(), 0, std::ptr::null()) } >= 0 {
            return Ok(());
        }
        let errno = last_errno();
        if errno != libc::EINTR {
            return Err(AsyncHostError::Native(errno));
        }
    }
}

fn new_kevent(
    ident: libc::uintptr_t,
    filter: i16,
    flags: u16,
    fflags: u32,
    data: libc::intptr_t,
    fd_handle: Option<u64>,
) -> AsyncHostResult<libc::kevent> {
    let mut event = empty_kevent();
    event.ident = ident;
    event.filter = filter;
    event.flags = flags;
    event.fflags = fflags;
    event.data = data;
    event.udata = fd_handle
        .map(|handle| usize::try_from(handle).map_err(|_| AsyncHostError::Fault))
        .transpose()?
        .map_or(std::ptr::null_mut(), |handle| handle as *mut libc::c_void);
    Ok(event)
}

fn empty_kevent() -> libc::kevent {
    unsafe { std::mem::zeroed() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::sync::Arc;
    use std::task::Wake;

    #[test]
    fn virtual_process_events_share_io_batches_without_losing_completions() {
        let mut poll = poll_create().unwrap();
        let (read, mut write) = std::io::pipe().unwrap();
        poll_register(&poll, read.as_raw_fd(), true, 77).unwrap();
        write.write_all(b"R").unwrap();
        let pids: BTreeSet<_> = (1..=EVENT_BUFFER_SIZE as i32 + 1).collect();
        let notifications: Vec<_> = pids
            .iter()
            .map(|pid| Arc::new(register_process_notification(&poll, *pid).unwrap()))
            .collect();
        for notification in &notifications {
            notification.wake_by_ref();
        }
        let mut received = BTreeSet::new();
        let mut io_events = 0;
        // More notifications than fit in a batch must remain queued by kqueue.
        for _ in 0..3 {
            let count = poll_wait(&mut poll, 0).unwrap();
            for index in 0..count as u32 {
                let event = event_list_get(&poll, index).unwrap();
                match event_get_events(event) {
                    PROCESS_EVENT => assert!(received.insert(event_get_pid(event))),
                    READ_EVENT => {
                        assert_eq!(event_get_fd(event), 77);
                        io_events += 1;
                    }
                    other => panic!("unexpected event {other}"),
                }
            }
        }
        assert_eq!(received, pids);
        assert_eq!(io_events, 1);
        assert_eq!(poll_wait(&mut poll, 0), Ok(0));
    }

    #[test]
    fn abandoned_registration_removes_its_pending_event() {
        let mut poll = poll_create().unwrap();
        let notification = Arc::new(register_process_notification(&poll, 42).unwrap());
        notification.wake_by_ref();
        drop(notification);
        assert_eq!(poll_wait(&mut poll, 0), Ok(0));
        let notification = Arc::new(register_process_notification(&poll, 42).unwrap());
        notification.wake_by_ref();
        assert_eq!(poll_wait(&mut poll, 0), Ok(1));
        assert_eq!(event_get_pid(event_list_get(&poll, 0).unwrap()), 42);
    }

    #[test]
    fn in_flight_notification_pins_only_its_original_queue() {
        let poll = poll_create().unwrap();
        let queue = Arc::downgrade(&poll.fd);
        let notification = Arc::new(register_process_notification(&poll, 42).unwrap());
        drop(poll);
        assert!(queue.upgrade().is_some());
        let mut replacement = poll_create().unwrap();
        notification.wake_by_ref();
        assert_eq!(poll_wait(&mut replacement, 0), Ok(0));
        drop(notification);
        assert!(queue.upgrade().is_none());
    }

    #[test]
    fn nonpollable_fd_is_reported_as_unsupported() {
        let poll = poll_create().unwrap();
        let null = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/null")
            .unwrap();

        assert_eq!(poll_register(&poll, null.as_raw_fd(), false, 1).unwrap(), 0);
    }

    #[test]
    fn legacy_registration_preserves_nonpollable_error() {
        let poll = poll_create().unwrap();
        let null = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/null")
            .unwrap();

        assert!(poll_register_legacy(&poll, null.as_raw_fd(), false, 1).is_err());
    }
}
