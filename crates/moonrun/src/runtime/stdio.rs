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
use std::io::{self, Read, Write};
use std::sync::{
    Arc, Mutex,
    mpsc::{Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError},
};
use std::time::{Duration, Instant};

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
#[derive(Clone, Debug, Default)]
pub(crate) enum Stdio {
    #[default]
    Ambient,
    Channel {
        stdin: Arc<Mutex<ChannelInput>>,
        stdout: SyncSender<Vec<u8>>,
        stderr: SyncSender<Vec<u8>>,
    },
}

/// Buffered input for the channel's byte-block protocol.
#[derive(Debug)]
pub(crate) struct ChannelInput {
    receiver: Receiver<Vec<u8>>,
    bytes: Vec<u8>,
    offset: usize,
    eof: bool,
}

fn check_worker_cancellation() -> io::Result<()> {
    crate::async_sys::internal::event_loop::thread_pool::with_cancellable_region(|| Ok(()))
        .map_err(|error| io::Error::from_raw_os_error(error.errno()))
}

impl ChannelInput {
    fn ready(&mut self, timeout: Option<Duration>) -> bool {
        let deadline = timeout.map(|duration| Instant::now() + duration);
        while self.offset == self.bytes.len() && !self.eof {
            let received = match deadline {
                None => self
                    .receiver
                    .recv()
                    .map_err(|_| RecvTimeoutError::Disconnected),
                Some(deadline) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        match self.receiver.try_recv() {
                            Ok(bytes) => Ok(bytes),
                            Err(TryRecvError::Empty) => Err(RecvTimeoutError::Timeout),
                            Err(TryRecvError::Disconnected) => Err(RecvTimeoutError::Disconnected),
                        }
                    } else {
                        self.receiver.recv_timeout(remaining)
                    }
                }
            };
            match received {
                Ok(bytes) => {
                    self.bytes = bytes;
                    self.offset = 0;
                }
                Err(RecvTimeoutError::Disconnected) => self.eof = true,
                Err(RecvTimeoutError::Timeout) => return false,
            }
        }
        true
    }
}

impl Read for ChannelInput {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        loop {
            check_worker_cancellation()?;
            if self.ready(Some(Duration::from_millis(10))) {
                break;
            }
        }
        let count = buffer.len().min(self.bytes.len() - self.offset);
        buffer[..count].copy_from_slice(&self.bytes[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}

struct ChannelOutput<'a>(&'a SyncSender<Vec<u8>>);

impl Write for ChannelOutput<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let count = bytes.len().min(64 * 1024);
        let mut block = bytes[..count].to_vec();
        loop {
            check_worker_cancellation()?;
            match self.0.try_send(block) {
                Ok(()) => return Ok(count),
                Err(TrySendError::Disconnected(_)) => {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "stdio receiver closed",
                    ));
                }
                Err(TrySendError::Full(bytes)) => {
                    block = bytes;
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
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
    pub(crate) fn channel(
        stdin: Receiver<Vec<u8>>,
        stdout: SyncSender<Vec<u8>>,
        stderr: SyncSender<Vec<u8>>,
    ) -> Self {
        Self::Channel {
            stdin: Arc::new(Mutex::new(ChannelInput {
                receiver: stdin,
                bytes: Vec::new(),
                offset: 0,
                eof: false,
            })),
            stdout,
            stderr,
        }
    }

    pub(crate) fn channel_stdin_ready(&self, timeout: Option<Duration>) -> Option<bool> {
        match self {
            Self::Channel { stdin, .. } => {
                if timeout == Some(Duration::ZERO) {
                    match stdin.try_lock() {
                        Ok(mut input) => Some(input.ready(timeout)),
                        Err(std::sync::TryLockError::WouldBlock) => Some(false),
                        Err(std::sync::TryLockError::Poisoned(_)) => panic!("stdio input poisoned"),
                    }
                } else {
                    Some(stdin.lock().unwrap().ready(timeout))
                }
            }
            Self::Ambient => None,
        }
    }

    pub(crate) fn with_stdin<T, E>(
        &self,
        f: impl FnOnce(&mut dyn Read) -> Result<T, E>,
    ) -> Result<T, E> {
        match self {
            Self::Ambient => f(&mut io::stdin().lock()),
            Self::Channel { stdin, .. } => f(&mut *stdin.lock().unwrap()),
        }
    }

    pub(crate) fn with_stdout<T, E>(
        &self,
        f: impl FnOnce(&mut dyn Write) -> Result<T, E>,
    ) -> Result<T, E> {
        match self {
            Self::Ambient => f(&mut io::stdout().lock()),
            Self::Channel { stdout, .. } => f(&mut ChannelOutput(stdout)),
        }
    }

    pub(crate) fn with_stderr<T, E>(
        &self,
        f: impl FnOnce(&mut dyn Write) -> Result<T, E>,
    ) -> Result<T, E> {
        match self {
            Self::Ambient => f(&mut io::stderr().lock()),
            Self::Channel { stderr, .. } => f(&mut ChannelOutput(stderr)),
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
            Self::Channel { .. } => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "channel stdio has no native handle",
            )),
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

#[cfg(test)]
mod channel_tests {
    use super::*;
    use std::sync::mpsc::sync_channel;

    #[test]
    fn input_preserves_bytes_across_blocks_and_reports_eof() {
        let (sender, input) = sync_channel(3);
        let (output, _) = sync_channel(1);
        let (error, _) = sync_channel(1);
        let stdio = Stdio::channel(input, output, error);
        sender.send(Vec::new()).unwrap();
        sender.send(vec![1, 2]).unwrap();
        sender.send(vec![3]).unwrap();
        drop(sender);
        let mut bytes = [0; 3];
        stdio
            .with_stdin(|input| input.read_exact(&mut bytes))
            .unwrap();
        assert_eq!(bytes, [1, 2, 3]);
        assert_eq!(stdio.with_stdin(|input| input.read(&mut bytes)).unwrap(), 0);
        assert!(stdio.raw(StdioStream::Stdin).is_err());
    }

    #[test]
    fn output_backpressure_resumes_after_consumption_and_detects_closed_receiver() {
        let (_, input) = sync_channel(1);
        let (output, receiver) = sync_channel(1);
        let (error, _) = sync_channel(1);
        let stdio = Stdio::channel(input, output, error);
        stdio
            .with_stdout(|output| output.write_all(b"first"))
            .unwrap();
        let writer = stdio.clone();
        let (done, finished) = sync_channel(1);
        let thread = std::thread::spawn(move || {
            done.send(writer.with_stdout(|output| output.write_all(b"second")))
                .unwrap();
        });
        assert!(finished.recv_timeout(Duration::from_millis(30)).is_err());
        assert_eq!(receiver.recv().unwrap(), b"first");
        finished
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();
        thread.join().unwrap();
        assert_eq!(receiver.recv().unwrap(), b"second");
        drop(receiver);
        assert_eq!(
            stdio
                .with_stdout(|output| output.write(b"closed"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}
