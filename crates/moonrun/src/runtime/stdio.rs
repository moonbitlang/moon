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

//! Standard-stream behavior selected for one Moonrun Runtime.

use std::cell::Cell;
use std::fs::File;
use std::io::{self, Read, Write};
use std::sync::Mutex;

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
#[derive(Debug, Default)]
pub(crate) enum Stdio {
    #[default]
    Ambient,
    // None retains that ambient stream; Some binds an owned file or pipe.
    Bound([Option<Mutex<File>>; 3]),
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
    pub(crate) fn into_files(self) -> io::Result<[File; 3]> {
        let files = match self {
            Self::Ambient => [None, None, None],
            Self::Bound(files) => files,
        };
        let mut files = files.into_iter();
        let [stdin, stdout, stderr] = StdioStream::ALL.map(|stream| match files.next().unwrap() {
            Some(file) => file
                .into_inner()
                .map_err(|_| io::Error::other("stdio lock poisoned")),
            None => clone_ambient(stream),
        });
        Ok([stdin?, stdout?, stderr?])
    }

    pub(crate) fn owned(stdin: File, stdout: File, stderr: File) -> Self {
        Self::bound([Some(stdin), Some(stdout), Some(stderr)])
    }

    pub(crate) fn bound(files: [Option<File>; 3]) -> Self {
        Self::Bound(files.map(|file| file.map(Mutex::new)))
    }

    pub(crate) fn with_stdin<T, E>(
        &self,
        f: impl FnOnce(&mut dyn Read) -> Result<T, E>,
    ) -> Result<T, E> {
        match self {
            Self::Bound([Some(file), _, _]) => f(&mut *file.lock().unwrap()),
            _ => f(&mut io::stdin().lock()),
        }
    }

    pub(crate) fn with_stdout<T, E>(
        &self,
        f: impl FnOnce(&mut dyn Write) -> Result<T, E>,
    ) -> Result<T, E> {
        match self {
            Self::Bound([_, Some(file), _]) => f(&mut *file.lock().unwrap()),
            _ => f(&mut io::stdout().lock()),
        }
    }

    pub(crate) fn with_stderr<T, E>(
        &self,
        f: impl FnOnce(&mut dyn Write) -> Result<T, E>,
    ) -> Result<T, E> {
        match self {
            Self::Bound([_, _, Some(file)]) => f(&mut *file.lock().unwrap()),
            _ => f(&mut io::stderr().lock()),
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
            Self::Bound(files) if files[stream as usize].is_some() => {
                let file = files[stream as usize].as_ref().unwrap().lock().unwrap();
                #[cfg(unix)]
                {
                    use std::os::fd::AsRawFd;
                    Ok(file.as_raw_fd())
                }
                #[cfg(windows)]
                {
                    use std::os::windows::io::AsRawHandle;
                    Ok(file.as_raw_handle())
                }
            }
            _ => ambient_raw(stream),
        }
    }

    pub(crate) fn raw_handles(&self) -> [Option<RawStdio>; 3] {
        StdioStream::ALL.map(|stream| self.raw(stream).ok())
    }
}

fn clone_ambient(stream: StdioStream) -> io::Result<File> {
    #[cfg(unix)]
    {
        use std::os::fd::AsFd;
        let fd = match stream {
            StdioStream::Stdin => io::stdin().as_fd().try_clone_to_owned()?,
            StdioStream::Stdout => io::stdout().as_fd().try_clone_to_owned()?,
            StdioStream::Stderr => io::stderr().as_fd().try_clone_to_owned()?,
        };
        Ok(File::from(fd))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsHandle;
        let handle = match stream {
            StdioStream::Stdin => io::stdin().as_handle().try_clone_to_owned()?,
            StdioStream::Stdout => io::stdout().as_handle().try_clone_to_owned()?,
            StdioStream::Stderr => io::stderr().as_handle().try_clone_to_owned()?,
        };
        Ok(File::from(handle))
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
