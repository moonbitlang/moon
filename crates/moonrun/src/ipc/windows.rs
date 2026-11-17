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

//! Local named pipes with cancellable overlapped I/O and owned handle transfer.
//!
//! No operation releases its buffer or OVERLAPPED until Windows has completed
//! it, including after cancellation. Clones share cancellation and ownership;
//! closing both directions drops the handle after the last pending I/O drains.

use std::fs::File;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::windows::{
    ffi::OsStrExt,
    io::{AsRawHandle, FromRawHandle, OwnedHandle},
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows_sys::Win32::{
    Foundation::*,
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        },
        *,
    },
    Storage::FileSystem::*,
    System::{Console::*, IO::*, Pipes::*, Threading::*},
};

#[derive(Clone)]
struct Event(Arc<OwnedHandle>);
impl Event {
    fn new() -> io::Result<Self> {
        // SAFETY: no attributes/name; the returned event is exclusively owned.
        let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        Ok(Self(Arc::new(owned(handle)?)))
    }
    fn raw(&self) -> HANDLE {
        self.0.as_raw_handle()
    }
    fn set(&self) {
        unsafe {
            SetEvent(self.raw());
        }
    }
}

fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: callers pass only newly acquired, valid owning handles.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}

/// Keep each pending operation and its caller-owned buffer alive through its
/// completion. The cancellation event is sticky, so shutdown also covers the
/// race between checking admission and submitting ReadFile/WriteFile.
fn operation(
    handle: &OwnedHandle,
    canceled: &Event,
    timeout: Option<Duration>,
    start: impl FnOnce(*mut OVERLAPPED) -> io::Result<Option<u32>>,
) -> io::Result<u32> {
    // SAFETY: these events and the pipe remain owned throughout the operation.
    if unsafe { WaitForSingleObject(canceled.raw(), 0) } == WAIT_OBJECT_0 {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "IPC connection closed",
        ));
    }
    let event = Event::new()?;
    let mut overlapped = OVERLAPPED {
        hEvent: event.raw(),
        ..unsafe { std::mem::zeroed() }
    };
    if let Some(count) = start(&mut overlapped)? {
        return Ok(count);
    }
    let millis = timeout.map_or(INFINITE, |timeout| {
        timeout.as_millis().max(1).min(u128::from(INFINITE - 1)) as u32
    });
    let handles = [event.raw(), canceled.raw()];
    let wait = unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, millis) };
    let mut count = 0;
    if wait == WAIT_OBJECT_0 {
        if unsafe { GetOverlappedResult(handle.as_raw_handle(), &overlapped, &mut count, 0) } != 0 {
            return Ok(count);
        }
        return Err(io::Error::last_os_error());
    }
    let error = match wait {
        WAIT_TIMEOUT => io::Error::new(io::ErrorKind::TimedOut, "IPC I/O deadline exceeded"),
        WAIT_FAILED => io::Error::last_os_error(),
        _ => io::Error::new(io::ErrorKind::ConnectionAborted, "IPC connection closed"),
    };
    // SAFETY: CancelIoEx only requests cancellation. Always drain the exact
    // operation before releasing overlapped/event or the caller's buffer.
    unsafe {
        CancelIoEx(handle.as_raw_handle(), &overlapped);
        GetOverlappedResult(handle.as_raw_handle(), &overlapped, &mut count, 1);
    }
    Err(error)
}

struct Peer {
    process: OwnedHandle,
    pid: u32,
}
struct Inner {
    handle: Mutex<Option<Arc<OwnedHandle>>>,
    read_closed: Event,
    write_closed: Event,
    read_timeout: Mutex<Option<Duration>>,
    peer: Option<Peer>,
}

/// An owned local named-pipe connection. Clones share directional shutdown.
#[derive(Clone)]
pub struct Connection(Arc<Inner>);
impl Connection {
    fn new(handle: OwnedHandle, peer: Option<Peer>) -> io::Result<Self> {
        Ok(Self(Arc::new(Inner {
            handle: Mutex::new(Some(Arc::new(handle))),
            read_closed: Event::new()?,
            write_closed: Event::new()?,
            read_timeout: Mutex::new(None),
            peer,
        })))
    }
    fn handle(&self) -> io::Result<Arc<OwnedHandle>> {
        self.0
            .handle
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "IPC connection closed"))
    }
    pub fn connect(endpoint: impl AsRef<Path>) -> io::Result<Self> {
        let name = endpoint.as_ref().as_os_str();
        // Never accept a UNC server: this capability is strictly local.
        if !name.to_string_lossy().starts_with(r"\\.\pipe\moonrun-") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid local IPC endpoint",
            ));
        }
        let wide: Vec<_> = name.encode_wide().chain(Some(0)).collect();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            // Do not grant the server impersonation rights to the client.
            let handle = unsafe {
                CreateFileW(
                    wide.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                    std::ptr::null_mut(),
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                return Self::new(owned(handle)?, None);
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_PIPE_BUSY as i32) {
                return Err(error);
            }
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return Err(error);
            };
            // Only retry connection admission, never a submitted invocation.
            if unsafe { WaitNamedPipeW(wide.as_ptr(), left.as_millis().max(1) as u32) } == 0 {
                return Err(io::Error::last_os_error());
            }
        }
    }
    pub(crate) fn try_clone(&self) -> io::Result<Self> {
        Ok(self.clone())
    }
    pub(crate) fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        *self.0.read_timeout.lock().unwrap() = timeout;
        Ok(())
    }
    // Internal local cancellation, not a Unix-compatible write-half close.
    pub(crate) fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        if matches!(how, Shutdown::Read | Shutdown::Both) {
            self.0.read_closed.set();
        }
        if matches!(how, Shutdown::Write | Shutdown::Both) {
            self.0.write_closed.set();
        }
        if how == Shutdown::Both {
            self.0.handle.lock().unwrap().take();
        }
        Ok(())
    }
    fn write_with_timeout(&self, bytes: &[u8], timeout: Duration) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let handle = self.handle()?;
        operation(&handle, &self.0.write_closed, Some(timeout), |overlapped| {
            let mut count = 0;
            // SAFETY: operation drains before bytes or overlapped can be freed.
            if unsafe {
                WriteFile(
                    handle.as_raw_handle(),
                    bytes.as_ptr(),
                    bytes.len().min(u32::MAX as usize) as u32,
                    std::ptr::null_mut(),
                    overlapped,
                )
            } != 0
            {
                if unsafe { GetOverlappedResult(handle.as_raw_handle(), overlapped, &mut count, 0) }
                    == 0
                {
                    return Err(io::Error::last_os_error());
                }
                return Ok(Some(count));
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_IO_PENDING as i32) {
                Ok(None)
            } else {
                Err(error)
            }
        })
        .map(|count| count as usize)
    }
}
impl Read for &Connection {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let handle = self.handle()?;
        let timeout = *self.0.read_timeout.lock().unwrap();
        let result = operation(&handle, &self.0.read_closed, timeout, |overlapped| {
            let mut count = 0;
            // SAFETY: operation drains before bytes or overlapped can be freed.
            if unsafe {
                ReadFile(
                    handle.as_raw_handle(),
                    bytes.as_mut_ptr(),
                    bytes.len().min(u32::MAX as usize) as u32,
                    std::ptr::null_mut(),
                    overlapped,
                )
            } != 0
            {
                if unsafe { GetOverlappedResult(handle.as_raw_handle(), overlapped, &mut count, 0) }
                    == 0
                {
                    return Err(io::Error::last_os_error());
                }
                return Ok(Some(count));
            }
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_IO_PENDING as i32) {
                Ok(None)
            } else {
                Err(error)
            }
        });
        match result {
            Err(error) if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) => Ok(0),
            result => result.map(|count| count as usize),
        }
    }
}
impl Write for &Connection {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.write_with_timeout(bytes, Duration::from_secs(2))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn write_all_deadline(stream: &Connection, mut frame: &[u8]) -> io::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !frame.is_empty() {
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "IPC reply deadline exceeded")
            })?;
        let count = stream.write_with_timeout(frame, left)?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "IPC reply connection closed",
            ));
        }
        frame = &frame[count..];
    }
    Ok(())
}

// Restrict the pipe to the current user, rather than the default pipe DACL,
// which also grants Everyone/anonymous read access. Convert* allocates with
// LocalAlloc; keep those allocations owned through their last Win32 use.
struct LocalAllocation(*mut core::ffi::c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn user_security() -> io::Result<Vec<u16>> {
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = owned(token)?;
    let mut needed = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    if needed == 0 {
        return Err(io::Error::last_os_error());
    }
    // TOKEN_USER requires pointer alignment; a byte vector does not promise it.
    let mut buffer = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    let mut text = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let allocation = LocalAllocation(text.cast());
    let mut length = 0;
    unsafe {
        while *text.add(length) != 0 {
            length += 1;
        }
    }
    let sid = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
        .map_err(io::Error::other)?;
    drop(allocation);
    Ok(format!("D:P(A;;GA;;;{sid})")
        .encode_utf16()
        .chain(Some(0))
        .collect())
}

pub(crate) struct Wake(Event);
impl Wake {
    pub(crate) fn close(&self) {
        self.0.set();
    }
}
pub(crate) struct Listener {
    name: Vec<u16>,
    security: Vec<u16>,
    pending: OwnedHandle,
    stop: Event,
}
impl Listener {
    pub(crate) fn bind() -> io::Result<(Self, Wake, PathBuf)> {
        let endpoint = PathBuf::from(format!(r"\\.\pipe\moonrun-{:032x}", rand::random::<u128>()));
        let name: Vec<_> = endpoint.as_os_str().encode_wide().chain(Some(0)).collect();
        let security = user_security()?;
        let pending = Self::create(&name, &security, true)?;
        let stop = Event::new()?;
        Ok((
            Self {
                name,
                security,
                pending,
                stop: stop.clone(),
            },
            Wake(stop),
            endpoint,
        ))
    }
    fn create(name: &[u16], security: &[u16], first: bool) -> io::Result<OwnedHandle> {
        let mut descriptor = std::ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                security.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let descriptor = LocalAllocation(descriptor);
        let attrs = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        // FIRST_PIPE_INSTANCE rejects a pre-existing endpoint; every later
        // instance is created while we still own an existing one. Reject SMB.
        let flags = PIPE_ACCESS_DUPLEX
            | FILE_FLAG_OVERLAPPED
            | if first {
                FILE_FLAG_FIRST_PIPE_INSTANCE
            } else {
                0
            };
        owned(unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                flags,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                65536,
                65536,
                0,
                &attrs,
            )
        })
    }
    pub(crate) fn accept(&mut self) -> io::Result<Connection> {
        loop {
            let connected = operation(&self.pending, &self.stop, None, |overlapped| {
                if unsafe { ConnectNamedPipe(self.pending.as_raw_handle(), overlapped) } != 0 {
                    return Ok(Some(0));
                }
                let error = io::Error::last_os_error();
                match error.raw_os_error().map(|code| code as u32) {
                    Some(ERROR_PIPE_CONNECTED) => Ok(Some(0)),
                    Some(ERROR_IO_PENDING) => Ok(None),
                    _ => Err(error),
                }
            });
            if let Err(error) = &connected
                && !matches!(
                    error.raw_os_error().map(|code| code as u32),
                    Some(ERROR_NO_DATA | ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED)
                )
            {
                return Err(connected.unwrap_err());
            }
            // Keep a listening instance even if this client disappeared during
            // admission. A single client's death must not close other Runs.
            let next = Self::create(&self.name, &self.security, false)?;
            let handle = std::mem::replace(&mut self.pending, next);
            if connected.is_err() {
                continue;
            }
            let mut pid = 0;
            if unsafe { GetNamedPipeClientProcessId(handle.as_raw_handle(), &mut pid) } == 0 {
                continue;
            }
            // Identity comes from the connected pipe, never an untrusted wire
            // PID. Keep the process handle pinned through stdio acquisition.
            let Ok(process) = owned(unsafe { OpenProcess(PROCESS_DUP_HANDLE, 0, pid) }) else {
                continue;
            };
            return Connection::new(handle, Some(Peer { process, pid }));
        }
    }
}

pub(crate) fn send_stdio(mut stream: &Connection, files: &[File; 3]) -> io::Result<()> {
    let mut transfer = [0u8; 28];
    transfer[0] = b'R';
    for (index, file) in files.iter().enumerate() {
        let mut mode = 0;
        let console = unsafe { GetConsoleMode(file.as_raw_handle(), &mut mode) } != 0;
        transfer[1 + index * 9] = u8::from(console);
        transfer[2 + index * 9..10 + index * 9]
            .copy_from_slice(&(file.as_raw_handle() as usize as u64).to_le_bytes());
    }
    stream.write_all(&transfer)?;
    // Source handles remain alive until the host has acquired all three owners.
    let mut ack = [0];
    stream.read_exact(&mut ack)?;
    if ack != *b"R" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC stdio transfer rejected",
        ));
    }
    Ok(())
}

pub(crate) fn receive_stdio(mut stream: &Connection) -> io::Result<[Option<File>; 3]> {
    let peer = stream.0.peer.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "IPC stdio requires the pipe server",
        )
    })?;
    let mut transfer = [0; 28];
    stream.read_exact(&mut transfer)?;
    if transfer[0] != b'R' {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid IPC stdio transfer",
        ));
    }
    let mut files = Vec::new();
    for index in 0..3 {
        let source =
            u64::from_le_bytes(transfer[2 + index * 9..10 + index * 9].try_into().unwrap());
        let handle = match transfer[1 + index * 9] {
            0 => {
                let source = usize::try_from(source).map_err(io::Error::other)? as HANDLE;
                let mut duplicate = std::ptr::null_mut();
                // Never adopt a foreign numeric value as an owned local handle.
                // No CLOSE_SOURCE: client owns originals through acknowledgement.
                if unsafe {
                    DuplicateHandle(
                        peer.process.as_raw_handle(),
                        source,
                        GetCurrentProcess(),
                        &mut duplicate,
                        0,
                        0,
                        DUPLICATE_SAME_ACCESS,
                    )
                } == 0
                {
                    return Err(io::Error::last_os_error());
                }
                let handle = owned(duplicate)?;
                let kind = unsafe { GetFileType(handle.as_raw_handle()) };
                if ![FILE_TYPE_DISK, FILE_TYPE_PIPE, FILE_TYPE_CHAR].contains(&kind) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "IPC stdio is not a file or pipe",
                    ));
                }
                Some(File::from(handle))
            }
            1 => {
                // The MVP shares ordinary inherited console streams through
                // existing host stdio. It never opens a console or converts its
                // encoding. Redirected streams above retain their own owners.
                let mut pids = vec![0; 32];
                loop {
                    let count =
                        unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) };
                    if count == 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if count as usize > pids.len() {
                        pids.resize(count as usize, 0);
                        continue;
                    }
                    if !pids[..count as usize].contains(&peer.pid) {
                        return Err(io::Error::new(
                            io::ErrorKind::Unsupported,
                            "IPC console requires a shared console",
                        ));
                    }
                    break;
                }
                let standard = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE][index];
                let mut mode = 0;
                if unsafe { GetConsoleMode(GetStdHandle(standard), &mut mode) } == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "IPC shared console requires the corresponding host standard stream",
                    ));
                }
                None
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid IPC stdio kind",
                ));
            }
        };
        files.push(handle);
    }
    let files = files
        .try_into()
        .map_err(|_| io::Error::other("IPC requires three streams"))?;
    stream.write_all(b"R")?;
    Ok(files)
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
