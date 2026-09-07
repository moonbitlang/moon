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

use std::cell::Cell;
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

/// Stateful UTF-16 code-unit output used by MoonBit's legacy character ABI.
#[derive(Default)]
pub(crate) struct Utf16Writer {
    dangling_high_half: Cell<Option<u32>>,
}

impl Utf16Writer {
    pub(crate) fn write_stdout(&self, stdio: &Stdio, value: u32) -> io::Result<()> {
        if (0xd800..=0xdbff).contains(&value) {
            if self
                .dangling_high_half
                .replace(Some(value - 0xd800))
                .is_some()
            {
                stdio.with_stdout(|stdout| write!(stdout, "\u{fffd}"))?;
            }
            return Ok(());
        }

        let value = if (0xdc00..=0xdfff).contains(&value) {
            self.dangling_high_half
                .take()
                .map_or(0xfffd, |high| 0x10000 + (high << 10) + (value - 0xdc00))
        } else {
            value
        };
        let value = char::from_u32(value)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid character"))?;
        stdio.with_stdout(|stdout| write!(stdout, "{value}"))
    }
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

    pub(crate) fn read_utf8_char(&self) -> io::Result<Option<char>> {
        self.with_stdin(|stdin| {
            let mut buffer = [0; 4];
            if stdin.read(&mut buffer[..1])? == 0 {
                return Ok(None);
            }

            let length = match buffer[0] {
                0..=0x7f => 1,
                0xc0..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf7 => 4,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid UTF-8 first byte",
                    ));
                }
            };
            if length > 1 {
                stdin.read_exact(&mut buffer[1..length])?;
            }

            std::str::from_utf8(&buffer[..length])
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
                .chars()
                .next()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no character found"))
                .map(Some)
        })
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
