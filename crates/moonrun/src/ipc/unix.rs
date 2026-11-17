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

//! Unix connection admission, descriptor transfer, and interruptible reply I/O.

use rustix::event::{PollFd, PollFlags, poll};
use rustix::net::{
    self, RecvAncillaryBuffer, RecvAncillaryMessage, RecvFlags, ReturnFlags, SendAncillaryBuffer,
    SendAncillaryMessage, SendFlags,
};
use std::fs::File;
use std::io::{self, IoSlice, IoSliceMut, Write};
use std::mem::MaybeUninit;
use std::net::Shutdown;
use std::os::fd::AsFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

pub type Connection = UnixStream;
pub(crate) struct Listener {
    listener: UnixListener,
    interrupted: UnixStream,
    // The listener thread retains this through delegate teardown. Close the
    // socket before removing its private namespace.
    _directory: tempfile::TempDir,
}
pub(crate) struct Wake(UnixStream);
impl Wake {
    pub(crate) fn close(&self) {
        let _ = self.0.shutdown(Shutdown::Both);
    }
}
impl Listener {
    pub(crate) fn bind() -> io::Result<(Self, Wake, PathBuf)> {
        let directory = tempfile::Builder::new().prefix("moonrun-").tempdir()?;
        let endpoint = directory.path().join("ipc");
        let listener = UnixListener::bind(&endpoint)?;
        let (wake, interrupted) = UnixStream::pair()?;
        Ok((
            Self {
                listener,
                interrupted,
                _directory: directory,
            },
            Wake(wake),
            endpoint,
        ))
    }
    pub(crate) fn accept(&mut self) -> io::Result<Connection> {
        let mut events = [
            PollFd::new(&self.listener, PollFlags::IN),
            PollFd::new(&self.interrupted, PollFlags::IN),
        ];
        rustix::io::retry_on_intr(|| poll(&mut events, None))?;
        if !events[1].revents().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "IPC listener closed",
            ));
        }
        self.listener.accept().map(|(stream, _)| stream)
    }
}

/// Final replies have a total send deadline, including partial writes. On
/// Darwin, shutdown(Both) after a prior shutdown(Read) may report ENOTCONN
/// without interrupting a writer. A non-reading client must never retain an
/// execution thread indefinitely after the Run has finished.
pub(crate) fn write_all_deadline(mut stream: &Connection, frame: &[u8]) -> io::Result<()> {
    // The reader has already joined; this only changes the private control
    // socket, never the transferred stdio or another execution's resources.
    stream.set_nonblocking(true)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut remaining = frame;
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

pub(crate) fn send_stdio(stream: &UnixStream, files: &[File; 3]) -> io::Result<()> {
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

pub(crate) fn receive_stdio(stream: &UnixStream) -> io::Result<[File; 3]> {
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
