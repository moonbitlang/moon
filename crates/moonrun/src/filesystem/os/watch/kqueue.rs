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

//! macOS vnode notification mechanisms. User data is an opaque caller token.

use super::RawFile;
use crate::async_sys::ported_fns;
use std::io;

ported_fns! {
    #[ported(source = "src/fs/watch_kqueue.c", original = "moonbitlang_async_kqueue_watcher_create")]
    pub(crate) fn create() -> io::Result<RawFile> {
        let kqueue = unsafe { libc::kqueue() };
        if kqueue < 0 {
            return Err(io::Error::last_os_error());
        }
        let flags = unsafe { libc::fcntl(kqueue, libc::F_GETFD) };
        if flags < 0
            || (flags & libc::FD_CLOEXEC) == 0
                && unsafe { libc::fcntl(kqueue, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0
        {
            let error = io::Error::last_os_error();
            unsafe { libc::close(kqueue) };
            return Err(error);
        }
        Ok(kqueue)
    }

    #[ported(source = "src/fs/watch_kqueue.c", original = "moonbitlang_async_kqueue_watcher_add_file")]
    pub(in crate::filesystem) fn add_file(
        kqueue: RawFile,
        file: RawFile,
        token: usize,
    ) -> io::Result<()> {
        let mut event: libc::kevent = unsafe { std::mem::zeroed() };
        event.ident = file as libc::uintptr_t;
        event.filter = libc::EVFILT_VNODE;
        event.flags = libc::EV_ADD | libc::EV_CLEAR;
        // NOTE_WRITE also covers extension; NOTE_EXTEND can cause a second
        // wakeup for the same write (moonbitlang/async#588).
        event.fflags = libc::NOTE_WRITE;
        event.udata = token as *mut libc::c_void;
        if unsafe {
            libc::kevent(kqueue, &event, 1, std::ptr::null_mut(), 0, std::ptr::null())
        } < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

}

/// Poll for native events without blocking.
///
/// # Safety
/// `events` must point to writable storage for `count` kevent records;
/// `count` must be nonnegative. The pointer is not retained.
pub(in crate::filesystem) unsafe fn fetch_event(
    kqueue: RawFile,
    events: *mut libc::kevent,
    count: i32,
) -> io::Result<i32> {
    let timeout = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let ret = unsafe { libc::kevent(kqueue, std::ptr::null(), 0, events, count, &timeout) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret)
    }
}
