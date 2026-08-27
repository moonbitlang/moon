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

//! Standard-stream behavior selected for one Moonrun Runtime.

use std::io::{self, Read, Write};
use std::time::Duration;

#[cfg(unix)]
pub(crate) type RawStdio = std::os::fd::RawFd;
#[cfg(windows)]
pub(crate) type RawStdio = std::os::windows::io::RawHandle;

#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stdio {
    Stdin,
    Stdout,
    Stderr,
}

impl Stdio {
    pub(crate) const ALL: [Self; 3] = [Self::Stdin, Self::Stdout, Self::Stderr];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChildStdio {
    #[cfg(unix)]
    Inherit,
    #[cfg(windows)]
    Handle(RawStdio),
}

/// Runtime-owned selection of the guest-visible standard streams.
///
/// Ambient deliberately observes the process standard streams at the same
/// points as the historical callers: the Handle namespace snapshots them at
/// construction, child defaults are resolved at spawn, and synchronous I/O
/// observes them for each operation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum StdioBindings {
    #[default]
    Ambient,
}

impl StdioBindings {
    pub(crate) fn with_stdin<T, E>(
        &self,
        f: impl FnOnce(&mut dyn Read) -> Result<T, E>,
    ) -> Result<T, E> {
        match self {
            Self::Ambient => f(&mut io::stdin().lock()),
        }
    }

    pub(crate) fn with_stdout<T, E>(
        &self,
        f: impl FnOnce(&mut dyn Write) -> Result<T, E>,
    ) -> Result<T, E> {
        match self {
            Self::Ambient => f(&mut io::stdout().lock()),
        }
    }

    pub(crate) fn with_stderr<T, E>(
        &self,
        f: impl FnOnce(&mut dyn Write) -> Result<T, E>,
    ) -> Result<T, E> {
        match self {
            Self::Ambient => f(&mut io::stderr().lock()),
        }
    }

    /// Observe one ambient raw handle without taking ownership of it.
    pub(crate) fn raw(&self, stream: Stdio) -> io::Result<RawStdio> {
        match self {
            #[cfg(unix)]
            Self::Ambient => Ok(stream as RawStdio),
            #[cfg(windows)]
            Self::Ambient => ambient_windows_raw(stream),
        }
    }

    pub(crate) fn raw_handles(&self) -> [Option<RawStdio>; 3] {
        Stdio::ALL.map(|stream| self.raw(stream).ok())
    }

    pub(crate) fn child(&self, stream: Stdio) -> io::Result<ChildStdio> {
        match self {
            // Unix historically leaves an absent spawn entry untouched so
            // posix_spawn inherits the child's descriptor 0, 1, or 2.
            #[cfg(unix)]
            Self::Ambient => {
                let _ = stream;
                Ok(ChildStdio::Inherit)
            }
            // Windows requires concrete STARTUPINFO handles and historically
            // observes them immediately before each spawn.
            #[cfg(windows)]
            Self::Ambient => self.raw(stream).map(ChildStdio::Handle),
        }
    }

    #[cfg(unix)]
    pub(crate) fn poll_stdin(&self, timeout: Option<Duration>) -> io::Result<bool> {
        let timeout_ms = match timeout {
            Some(duration) => i32::try_from(duration.as_millis().min(i32::MAX as u128))
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?,
            None => -1,
        };
        let mut pollfd = libc::pollfd {
            fd: self.raw(Stdio::Stdin)?,
            events: libc::POLLIN,
            revents: 0,
        };
        loop {
            // SAFETY: `pollfd` is a valid single-element array for this call.
            let ready_count = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };
            if ready_count >= 0 {
                return Ok(ready_count > 0
                    && (pollfd.revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR)) != 0);
            }
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            // Sandbox profiles can reject poll while reads remain available.
            if error.raw_os_error() == Some(libc::EPERM) {
                return Ok(true);
            }
            return Err(error);
        }
    }

    #[cfg(windows)]
    pub(crate) fn poll_stdin(&self, timeout: Option<Duration>) -> io::Result<bool> {
        use windows_sys::Win32::Foundation::{WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
        use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};

        let timeout_ms = match timeout {
            Some(duration) => u32::try_from(duration.as_millis().min(u32::MAX as u128))
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?,
            None => INFINITE,
        };
        let handle = self.raw(Stdio::Stdin)?;
        // SAFETY: the Ambient binding just obtained this non-owning process
        // handle from GetStdHandle and does not close it.
        match unsafe { WaitForSingleObject(handle, timeout_ms) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            WAIT_FAILED => Err(io::Error::last_os_error()),
            _ => Ok(true),
        }
    }
}

#[cfg(windows)]
fn ambient_windows_raw(stream: Stdio) -> io::Result<RawStdio> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    let id = match stream {
        Stdio::Stdin => STD_INPUT_HANDLE,
        Stdio::Stdout => STD_OUTPUT_HANDLE,
        Stdio::Stderr => STD_ERROR_HANDLE,
    };
    // SAFETY: this only observes a process standard handle; ownership remains
    // with the process.
    let raw = unsafe { GetStdHandle(id) };
    if raw.is_null() || raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_stdio_defaults_to_ambient() {
        assert_eq!(StdioBindings::default(), StdioBindings::Ambient);
    }

    #[cfg(unix)]
    #[test]
    fn ambient_child_stdio_preserves_native_inheritance() {
        for stream in Stdio::ALL {
            assert_eq!(
                StdioBindings::Ambient.child(stream).unwrap(),
                ChildStdio::Inherit
            );
        }
    }
}
