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

use std::io;

#[cfg(unix)]
use std::os::fd::{AsRawFd, OwnedFd};

use super::PollInstance;

/// A host-only wake source, retained by its poll and any in-flight sender.
/// On Unix, both pipe ends stay here, outside the guest's Resource Handle table.
/// No code reads the byte: readiness stays pending until Run teardown.
/// It is used only after a Run requests termination; the import adapter consumes
/// that outcome before guest code can observe the internal event.
#[derive(Debug)]
pub(in crate::poll) struct PollWaker {
    #[cfg(unix)]
    _recv: OwnedFd,
    #[cfg(unix)]
    send: OwnedFd,
    #[cfg(windows)]
    port: super::CompletionPort,
}

impl PollWaker {
    pub(in crate::poll) fn new(poll: &PollInstance) -> io::Result<Self> {
        #[cfg(unix)]
        {
            let [recv, send] = crate::filesystem::pipe(true, true)?.map(OwnedFd::from);
            super::poll_register_completion_source(poll, recv.as_raw_fd(), 0)?;
            Ok(Self { _recv: recv, send })
        }
        #[cfg(windows)]
        {
            Ok(Self {
                port: super::CompletionPort::from_poll(poll),
            })
        }
    }

    pub(in crate::poll) fn wake(&self) -> io::Result<()> {
        #[cfg(unix)]
        loop {
            // Keep the byte readable until teardown. Readiness must survive a
            // request arriving before poll_wait; both pipe ends remain owned.
            let written = unsafe { libc::write(self.send.as_raw_fd(), b"x".as_ptr().cast(), 1) };
            if written == 1 {
                return Ok(());
            }
            let errno = super::last_errno();
            if written < 0 && errno == libc::EINTR {
                continue;
            }
            if written < 0 && (errno == libc::EAGAIN || errno == libc::EWOULDBLOCK) {
                return Ok(());
            }
            return Err(io::Error::from_raw_os_error(if written == 0 {
                libc::EIO
            } else {
                errno
            }));
        }
        #[cfg(windows)]
        {
            self.port.post(0, usize::MAX)
        }
    }
}
