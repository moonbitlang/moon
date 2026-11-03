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

//! Bounded framing and owned Unix descriptor transfer. No execution policy.

use std::fs::File;
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::mem::MaybeUninit;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;

use rustix::net::{
    self, RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, ReturnFlags, SendAncillaryBuffer,
    SendAncillaryMessage, SendFlags,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub(super) const VERSION: u32 = 2;
const MAX_FRAME: usize = 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub version: u32,
    pub invocation: super::Invocation,
}

#[derive(Serialize, Deserialize)]
pub(super) enum Control {
    Signal(i32),
    Exit(i32),
    KilledBySignal(i32),
}

#[derive(Serialize, Deserialize)]
pub(super) enum Reply {
    Completed,
    Exited(i32),
    KilledBySignal(i32),
    Failed(String),
}

pub(super) fn encode(value: &impl Serialize) -> io::Result<Vec<u8>> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "IPC frame exceeds size limit",
        ));
    }
    Ok(bytes)
}

pub(super) fn write(stream: &UnixStream, value: &impl Serialize) -> io::Result<()> {
    write_frame(stream, &encode(value)?)
}

pub(super) fn write_frame(mut stream: &UnixStream, bytes: &[u8]) -> io::Result<()> {
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(bytes)
}

/// Final replies have a total send deadline, including partial writes. On
/// Darwin, shutdown(Both) after a prior shutdown(Read) may report ENOTCONN
/// without interrupting a writer. A non-reading client must never retain an
/// execution thread indefinitely after the Run has finished.
pub(super) fn write_reply(mut stream: &UnixStream, reply: &Reply) -> io::Result<()> {
    let bytes = encode(reply)?;
    let mut frame = Vec::with_capacity(4 + bytes.len());
    frame.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    frame.extend_from_slice(&bytes);
    // The reader has already joined; this only changes the private control
    // socket, never the transferred stdio or another execution's resources.
    stream.set_nonblocking(true)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut remaining = frame.as_slice();
    while !remaining.is_empty() {
        let duration = deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "IPC reply deadline exceeded")
            })?;
        match stream.write(remaining) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "IPC reply connection closed",
                ));
            }
            Ok(size) => remaining = &remaining[size..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                use rustix::event::{PollFd, PollFlags, Timespec, poll};
                let timeout = Timespec::try_from(duration).map_err(io::Error::other)?;
                match poll(&mut [PollFd::new(&stream, PollFlags::OUT)], Some(&timeout)) {
                    Ok(_) | Err(rustix::io::Errno::INTR) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub(super) fn read<T: DeserializeOwned>(mut stream: &UnixStream) -> io::Result<T> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC frame exceeds size limit",
        ));
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub(super) fn send_stdio(stream: &UnixStream, files: &[File; 3]) -> io::Result<()> {
    let fds = files.each_ref().map(AsFd::as_fd);
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(3))];
    let mut ancillary = SendAncillaryBuffer::new(&mut space);
    assert!(ancillary.push(SendAncillaryMessage::ScmRights(&fds)));
    #[cfg(target_os = "macos")]
    let flags = {
        net::sockopt::set_socket_nosigpipe(stream, true)?;
        SendFlags::empty()
    };
    #[cfg(not(target_os = "macos"))]
    let flags = SendFlags::NOSIGNAL;
    let size = rustix::io::retry_on_intr(|| {
        net::sendmsg(stream, &[IoSlice::new(b"R")], &mut ancillary, flags)
    })?;
    if size != 1 {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "failed to send IPC stdio",
        ));
    }
    Ok(())
}

pub(super) fn receive_stdio(stream: &UnixStream) -> io::Result<[File; 3]> {
    let mut marker = [0];
    // Receive the kernel's full descriptor set before enforcing our count of
    // three. Linux limits SCM_RIGHTS to 253 fds; XNU's UIPC_MAX_CMSG_FD is 512
    // (bsd/kern/uipc_usrreq.c). A one-byte recv consumes one rights-bearing send.
    // In particular, never truncate on Darwin: it can install undisclosed fds,
    // even though rustix now bounds-checks truncated control headers.
    let mut space = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(512))];
    let mut ancillary = RecvAncillaryBuffer::new(&mut space);
    #[cfg(target_os = "linux")]
    let flags = RecvFlags::CMSG_CLOEXEC;
    #[cfg(not(target_os = "linux"))]
    let flags = RecvFlags::empty();
    let received = rustix::io::retry_on_intr(|| {
        net::recvmsg(
            stream,
            &mut [IoSliceMut::new(&mut marker)],
            &mut ancillary,
            flags,
        )
    })?;
    if received.bytes != 1 || marker != *b"R" || received.flags.contains(ReturnFlags::CTRUNC) {
        // Dropping the ancillary buffer closes even descriptors we did not drain.
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid IPC stdio transfer",
        ));
    }
    let mut files = Vec::new();
    for message in ancillary.drain() {
        match message {
            RecvAncillaryMessage::ScmRights(fds) => {
                for fd in fds {
                    // Darwin has no MSG_CMSG_CLOEXEC. Set the received descriptor
                    // flags immediately; never alter shared file status flags.
                    rustix::io::fcntl_setfd(&fd, rustix::io::FdFlags::CLOEXEC)?;
                    files.push(File::from(fd));
                }
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected IPC ancillary message",
                ));
            }
        }
    }
    files.try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC requires exactly three stdio descriptors",
        )
    })
}

// serde's Path serialization requires UTF-8. OsString's Unix representation
// preserves bytes, including paths that cannot be represented as Rust strings.
pub(super) mod path {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    pub fn serialize<S: Serializer>(path: &Path, serializer: S) -> Result<S::Ok, S::Error> {
        path.as_os_str().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
        OsString::deserialize(deserializer).map(PathBuf::from)
    }
}
