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

//! The controlling terminal, following `src/tty/tty.c` on async's tty branch.

use crate::async_host::AsyncHostResult;
use crate::resource::Resource;

#[cfg(all(test, windows))]
mod tests;

#[cfg(windows)]
mod windows;

/// The controlling terminal on Unix.
#[cfg(unix)]
const TTY_PATH: &str = "/dev/tty";

/// The descriptors behind `tty/open`. On Unix the control descriptor is the
/// input descriptor, so it has no entry of its own.
pub(crate) struct TtyResources {
    pub(crate) input: Resource,
    pub(crate) wakeup: Resource,
    #[cfg(windows)]
    pub(crate) control: Resource,
    pub(crate) output: Resource,
}

#[cfg(unix)]
mod unix {
    use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};

    use super::{TTY_PATH, TtyResources};
    use crate::async_host::{AsyncHostError, AsyncHostResult};
    use crate::resource::Resource;

    fn last_error() -> AsyncHostError {
        AsyncHostError::Native(
            std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or_else(|| AsyncHostError::Inval.errno()),
        )
    }

    fn owned(fd: RawFd) -> AsyncHostResult<OwnedFd> {
        if fd < 0 {
            return Err(last_error());
        }
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    fn dup(fd: RawFd) -> AsyncHostResult<OwnedFd> {
        owned(unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) })
    }

    /// Readable while `SIGWINCH` is pending. This only works while every
    /// thread blocks `SIGWINCH`; the CLI blocks it before creating threads.
    #[cfg(target_os = "linux")]
    fn open_resize_source() -> AsyncHostResult<OwnedFd> {
        let mut set = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        unsafe {
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, libc::SIGWINCH);
        }
        owned(unsafe { libc::signalfd(-1, &set, libc::SFD_CLOEXEC | libc::SFD_NONBLOCK) })
    }

    /// Readable after `SIGWINCH` is generated. kqueue observes the signal
    /// whatever the signal masks are.
    #[cfg(target_os = "macos")]
    fn open_resize_source() -> AsyncHostResult<OwnedFd> {
        let kq = owned(unsafe { libc::kqueue() })?;
        unsafe {
            libc::fcntl(kq.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
        }
        let event = libc::kevent {
            ident: libc::SIGWINCH as usize,
            filter: libc::EVFILT_SIGNAL,
            flags: libc::EV_ADD,
            fflags: 0,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        let ret = unsafe {
            libc::kevent(
                kq.as_raw_fd(),
                &event,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        if ret < 0 {
            return Err(last_error());
        }
        Ok(kq)
    }

    pub(super) fn open() -> AsyncHostResult<TtyResources> {
        let path = std::ffi::CString::new(TTY_PATH).unwrap();
        let input = owned(unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
            )
        })?;
        let output = dup(input.as_raw_fd())?;
        let wakeup = open_resize_source()?;
        // The guest thread too, in case an embedder did not block it.
        let mut winch = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        unsafe {
            libc::sigemptyset(&mut winch);
            libc::sigaddset(&mut winch, libc::SIGWINCH);
            libc::pthread_sigmask(libc::SIG_BLOCK, &winch, std::ptr::null_mut());
        }
        Ok(TtyResources {
            input: Resource::new(input.into_raw_fd()),
            wakeup: Resource::new(wakeup.into_raw_fd()),
            output: Resource::new(output.into_raw_fd()),
        })
    }

    /// The kernel's `struct termios2`, which `TCGETS2` and `TCSETS2` use,
    /// unlike glibc's `struct termios`.
    #[cfg(target_os = "linux")]
    type Termios = libc::termios2;

    /// macOS's `struct termios`, the same on arm64 and x86_64.
    #[cfg(target_os = "macos")]
    type Termios = libc::termios;

    /// The size of a state: the termios above.
    pub(super) const STATE_SIZE: usize = std::mem::size_of::<Termios>();

    #[cfg(target_os = "linux")]
    fn get_termios(fd: BorrowedFd<'_>, termios: &mut Termios) -> libc::c_int {
        unsafe { libc::ioctl(fd.as_raw_fd(), libc::TCGETS2, termios as *mut Termios) }
    }

    #[cfg(target_os = "macos")]
    fn get_termios(fd: BorrowedFd<'_>, termios: &mut Termios) -> libc::c_int {
        unsafe { libc::tcgetattr(fd.as_raw_fd(), termios) }
    }

    #[cfg(target_os = "linux")]
    fn set_termios(fd: BorrowedFd<'_>, termios: &Termios) -> libc::c_int {
        unsafe { libc::ioctl(fd.as_raw_fd(), libc::TCSETS2, termios as *const Termios) }
    }

    #[cfg(target_os = "macos")]
    fn set_termios(fd: BorrowedFd<'_>, termios: &Termios) -> libc::c_int {
        unsafe { libc::tcsetattr(fd.as_raw_fd(), libc::TCSANOW, termios) }
    }

    /// Return the current state as the bytes of `Termios`.
    pub(super) fn get_state(input: BorrowedFd<'_>) -> AsyncHostResult<Vec<u8>> {
        let mut termios = unsafe { std::mem::zeroed::<Termios>() };
        if get_termios(input, &mut termios) < 0 {
            return Err(last_error());
        }
        let bytes = unsafe {
            std::slice::from_raw_parts((&termios as *const Termios).cast::<u8>(), STATE_SIZE)
        };
        Ok(bytes.to_vec())
    }

    /// Apply a state, the bytes of `Termios`.
    pub(super) fn set_state(input: BorrowedFd<'_>, state: &[u8]) -> AsyncHostResult<()> {
        if state.len() != STATE_SIZE {
            return Err(AsyncHostError::Inval);
        }
        let mut termios = unsafe { std::mem::zeroed::<Termios>() };
        unsafe {
            std::ptr::copy_nonoverlapping(
                state.as_ptr(),
                (&mut termios as *mut Termios).cast::<u8>(),
                STATE_SIZE,
            );
        }
        if set_termios(input, &termios) < 0 {
            return Err(last_error());
        }
        Ok(())
    }

    pub(super) fn size(output: BorrowedFd<'_>) -> AsyncHostResult<i64> {
        let mut ws = unsafe { std::mem::zeroed::<libc::winsize>() };
        if unsafe { libc::ioctl(output.as_raw_fd(), libc::TIOCGWINSZ, &mut ws) } < 0 {
            return Err(last_error());
        }
        Ok(((ws.ws_row as i64) << 32) | ws.ws_col as i64)
    }
}

pub(crate) fn open() -> AsyncHostResult<TtyResources> {
    #[cfg(unix)]
    return unix::open();
    #[cfg(windows)]
    return windows::open();
}

/// The size of a state: the kernel `struct termios2` on Linux (44 bytes),
/// `struct termios` on macOS
/// (72 bytes), and the input and output console modes as little-endian `u32`
/// on Windows (8 bytes). The guest interprets these layouts.
pub(crate) fn state_size() -> usize {
    #[cfg(unix)]
    return unix::STATE_SIZE;
    #[cfg(windows)]
    return windows::STATE_SIZE;
}

/// Return the current terminal state, see `state_size`.
pub(crate) fn get_state(
    input: &Resource,
    #[cfg(windows)] output: &Resource,
) -> AsyncHostResult<Vec<u8>> {
    #[cfg(unix)]
    return unix::get_state(input.as_fd()?);
    #[cfg(windows)]
    return windows::get_state(input.as_handle()?, output.as_handle()?);
}

/// Apply a terminal state, see `state_size`.
pub(crate) fn set_state(
    input: &Resource,
    #[cfg(windows)] output: &Resource,
    state: &[u8],
) -> AsyncHostResult<()> {
    #[cfg(unix)]
    return unix::set_state(input.as_fd()?, state);
    #[cfg(windows)]
    return windows::set_state(input.as_handle()?, output.as_handle()?, state);
}

pub(crate) fn size(output: &Resource) -> AsyncHostResult<i64> {
    #[cfg(unix)]
    return unix::size(output.as_fd()?);
    #[cfg(windows)]
    return windows::size(output.as_handle()?);
}
