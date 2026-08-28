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

#[cfg(unix)]
pub(crate) type RawStdio = std::os::fd::RawFd;
#[cfg(windows)]
pub(crate) type RawStdio = std::os::windows::io::RawHandle;

#[repr(usize)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StdioStream {
    Stdin,
    Stdout,
    Stderr,
}

impl StdioStream {
    pub(crate) const ALL: [Self; 3] = [Self::Stdin, Self::Stdout, Self::Stderr];
}

/// Runtime-owned selection of the guest-visible standard streams.
///
/// Ambient deliberately observes the process standard streams at the same
/// points as the historical callers: the Handle namespace snapshots them at
/// construction, child defaults are resolved at spawn, and synchronous I/O
/// observes them for each operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stdio {
    Ambient,
}

impl Stdio {
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
    pub(crate) fn raw(&self, stream: StdioStream) -> io::Result<RawStdio> {
        match self {
            Self::Ambient => ambient_raw(stream),
        }
    }

    pub(crate) fn raw_handles(&self) -> [Option<RawStdio>; 3] {
        StdioStream::ALL.map(|stream| self.raw(stream).ok())
    }
}

#[cfg(unix)]
fn ambient_raw(stream: StdioStream) -> io::Result<RawStdio> {
    Ok(stream as RawStdio)
}

#[cfg(windows)]
fn ambient_raw(stream: StdioStream) -> io::Result<RawStdio> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    let id = match stream {
        StdioStream::Stdin => STD_INPUT_HANDLE,
        StdioStream::Stdout => STD_OUTPUT_HANDLE,
        StdioStream::Stderr => STD_ERROR_HANDLE,
    };
    // SAFETY: this only observes a process standard handle; ownership remains
    // with the process.
    let raw = unsafe { GetStdHandle(id) };
    if raw.is_null() || raw == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(raw)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn ambient_raw_handles_are_native_stdio_descriptors() {
        for (stream, expected) in StdioStream::ALL.into_iter().zip([0, 1, 2]) {
            assert_eq!(Stdio::Ambient.raw(stream).unwrap(), expected);
        }
    }
}
