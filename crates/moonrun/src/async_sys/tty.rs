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

#[cfg(all(test, unix))]
mod unix_tests;

#[cfg(windows)]
mod windows;

/// The Resources behind `tty/open`, in the order the guest receives them. On
/// Unix one descriptor serves as both input and output, so the guest can
/// register it on the event bus.
pub(crate) struct TtyResources {
    #[cfg(unix)]
    pub(crate) terminal: Resource,
    #[cfg(windows)]
    pub(crate) input: Resource,
    #[cfg(windows)]
    pub(crate) output: Resource,
}

impl TtyResources {
    /// The number of Handles `tty/open` writes.
    #[cfg(unix)]
    pub(crate) const COUNT: usize = 1;
    #[cfg(windows)]
    pub(crate) const COUNT: usize = 2;

    pub(crate) fn into_resources(self) -> [Resource; Self::COUNT] {
        #[cfg(unix)]
        return [self.terminal];
        #[cfg(windows)]
        return [self.input, self.output];
    }
}

#[cfg(unix)]
mod unix {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};

    use super::TtyResources;
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

    /// The path of the controlling terminal.
    #[cfg(target_os = "linux")]
    pub(super) fn tty_path() -> AsyncHostResult<CString> {
        Ok(CString::new("/dev/tty").unwrap())
    }

    /// The path of the controlling terminal. kqueue cannot poll `/dev/tty`,
    /// so find the device behind it. Native reads `kp_eproc.e_tdev` through
    /// `sysctl`; `proc_pidinfo` reports the same device without `kinfo_proc`,
    /// which `libc` does not define.
    #[cfg(target_os = "macos")]
    pub(super) fn tty_path() -> AsyncHostResult<CString> {
        unsafe extern "C" {
            fn devname(dev: libc::dev_t, kind: libc::mode_t) -> *mut libc::c_char;
        }

        let mut info = unsafe { std::mem::zeroed::<libc::proc_bsdinfo>() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        let ret = unsafe {
            libc::proc_pidinfo(
                libc::getpid(),
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                size,
            )
        };
        // 0 on failure, with errno set.
        if ret <= 0 {
            return Err(last_error());
        }
        if ret != size {
            return Err(AsyncHostError::Native(libc::EIO));
        }
        // `NODEV`
        if info.e_tdev == u32::MAX {
            return Err(AsyncHostError::Native(libc::ENXIO));
        }
        let name = unsafe { devname(info.e_tdev as libc::dev_t, libc::S_IFCHR) };
        if name.is_null() {
            return Err(AsyncHostError::Native(libc::ENXIO));
        }
        let name = unsafe { std::ffi::CStr::from_ptr(name) };
        let mut path = b"/dev/".to_vec();
        path.extend_from_slice(name.to_bytes());
        Ok(CString::new(path).unwrap())
    }

    pub(super) fn open() -> AsyncHostResult<TtyResources> {
        let path = tty_path()?;
        let terminal = owned(unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
            )
        })?;
        Ok(TtyResources {
            terminal: Resource::new(terminal.into_raw_fd()),
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
