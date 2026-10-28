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

//! Terminal read Jobs, following `src/internal/event_loop/tty.c` on async's tty branch.

#[cfg(windows)]
mod windows;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::resource::Resource;

/// `TTY_READ_RESIZED` in the native runtime: the wait ended because the
/// window was resized, and no input was consumed.
pub(super) const TTY_READ_RESIZED: i64 = -1;

#[cfg(unix)]
mod unix {
    use std::os::fd::{AsRawFd, RawFd};

    use super::TTY_READ_RESIZED;
    use crate::async_host::{AsyncHostError, AsyncHostResult};
    use crate::async_sys::internal::event_loop::thread_pool::with_cancellable_region;
    use crate::resource::Resource;

    const WAIT_INPUT: i32 = 1;
    const WAIT_RESIZE: i32 = 2;

    fn last_errno() -> i32 {
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno())
    }

    #[cfg(target_os = "linux")]
    fn wait(input: RawFd, wakeup: RawFd) -> Result<i32, i32> {
        let mut fds = [
            libc::pollfd {
                fd: input,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: wakeup,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        if unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) } < 0 {
            return Err(last_errno());
        }
        let mut ready = 0;
        if fds[0].revents != 0 {
            ready |= WAIT_INPUT;
        }
        if fds[1].revents != 0 {
            ready |= WAIT_RESIZE;
        }
        Ok(ready)
    }

    // `poll` does not support devices on macOS.
    #[cfg(target_os = "macos")]
    fn wait(input: RawFd, wakeup: RawFd) -> Result<i32, i32> {
        if input >= libc::FD_SETSIZE as RawFd || wakeup >= libc::FD_SETSIZE as RawFd {
            return Err(libc::EINVAL);
        }
        let mut readfds = unsafe { std::mem::zeroed::<libc::fd_set>() };
        unsafe {
            libc::FD_ZERO(&mut readfds);
            libc::FD_SET(input, &mut readfds);
            libc::FD_SET(wakeup, &mut readfds);
        }
        let nfds = input.max(wakeup) + 1;
        let ret = unsafe {
            libc::select(
                nfds,
                &mut readfds,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if ret < 0 {
            return Err(last_errno());
        }
        let mut ready = 0;
        if unsafe { libc::FD_ISSET(input, &readfds) } {
            ready |= WAIT_INPUT;
        }
        if unsafe { libc::FD_ISSET(wakeup, &readfds) } {
            ready |= WAIT_RESIZE;
        }
        Ok(ready)
    }

    #[cfg(target_os = "linux")]
    fn drain_resize_source(wakeup: RawFd) {
        let mut info = std::mem::MaybeUninit::<libc::signalfd_siginfo>::uninit();
        let size = std::mem::size_of::<libc::signalfd_siginfo>();
        while unsafe { libc::read(wakeup, info.as_mut_ptr().cast(), size) } == size as isize {}
    }

    #[cfg(target_os = "macos")]
    fn drain_resize_source(wakeup: RawFd) {
        let mut event = std::mem::MaybeUninit::<libc::kevent>::uninit();
        let zero = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        while unsafe { libc::kevent(wakeup, std::ptr::null(), 0, event.as_mut_ptr(), 1, &zero) } > 0
        {
        }
    }

    /// Wait for terminal input or a resize, then read the input.
    ///
    /// A cancellation interrupts the wait or the read with `EINTR`; retrying
    /// re-enters the cancellable region, which then acknowledges it.
    pub(in crate::filesystem::job) fn read(
        input: &Resource,
        wakeup: &Resource,
        len: u32,
        result: &mut Option<Vec<u8>>,
    ) -> AsyncHostResult<i64> {
        let input = input.as_fd()?.as_raw_fd();
        let wakeup = wakeup.as_fd()?.as_raw_fd();
        let mut buf = vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?];
        loop {
            match with_cancellable_region(|| Ok(wait(input, wakeup)))? {
                Err(libc::EINTR) => continue,
                Err(errno) => return Err(AsyncHostError::Native(errno)),
                Ok(ready) if ready & WAIT_INPUT == 0 => {
                    drain_resize_source(wakeup);
                    *result = Some(Vec::new());
                    return Ok(TTY_READ_RESIZED);
                }
                Ok(_) => {}
            }
            let n = with_cancellable_region(|| {
                Ok(unsafe { libc::read(input, buf.as_mut_ptr().cast(), buf.len()) })
            })?;
            if n < 0 {
                match last_errno() {
                    libc::EINTR => continue,
                    errno => return Err(AsyncHostError::Native(errno)),
                }
            }
            buf.truncate(n as usize);
            *result = Some(buf);
            return Ok(n as i64);
        }
    }
}

#[cfg(unix)]
pub(super) fn read(
    input: &Resource,
    wakeup: &Resource,
    len: u32,
    result: &mut Option<Vec<u8>>,
) -> AsyncHostResult<i64> {
    unix::read(input, wakeup, len, result)
}

// The native runtime reads the Windows console through
// `read_console_input` instead.
#[cfg(windows)]
pub(super) fn read(
    _input: &Resource,
    _wakeup: &Resource,
    _len: u32,
    _result: &mut Option<Vec<u8>>,
) -> AsyncHostResult<i64> {
    Err(AsyncHostError::Native(
        windows_sys::Win32::Foundation::ERROR_NOT_SUPPORTED as i32,
    ))
}

/// Read console input records as UTF-16. Return `TTY_READ_RESIZED` for a
/// resize without input; a resize read along with input sets `resized`.
///
/// Like native, Unix reports this operation as unavailable.
#[cfg(unix)]
pub(super) fn read_console_input(
    _input: &Resource,
    _wakeup: &Resource,
    _len: u32,
    _result: &mut Option<Vec<u16>>,
    _resized: &mut bool,
) -> AsyncHostResult<i64> {
    Err(AsyncHostError::Native(libc::ENOSYS))
}

#[cfg(windows)]
pub(super) use windows::read_console_input;
