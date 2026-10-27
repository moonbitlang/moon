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

#[cfg(unix)]
use crate::async_host::AsyncHostError;
use crate::async_host::AsyncHostResult;

#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use super::PollInstance;

/// A host-only wake source, retained by its poll and any in-flight sender.
/// It is used only after a Run requests termination; the import adapter consumes
/// that outcome before guest code can observe the internal event.
#[derive(Debug)]
pub(crate) struct PollWaker {
    #[cfg(unix)]
    _recv: OwnedFd,
    #[cfg(unix)]
    send: OwnedFd,
    #[cfg(windows)]
    port: super::CompletionPort,
}

impl PollWaker {
    pub(crate) fn new(poll: &PollInstance) -> AsyncHostResult<Self> {
        #[cfg(unix)]
        {
            let fds = crate::async_sys::internal::fd_util::stub::pipe(true, true)?;
            // SAFETY: pipe returned two fresh descriptors whose ownership moves here.
            let recv = unsafe { OwnedFd::from_raw_fd(fds[0]) };
            let send = unsafe { OwnedFd::from_raw_fd(fds[1]) };
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

    pub(crate) fn wake(&self) -> AsyncHostResult<()> {
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
            return Err(AsyncHostError::Native(if written == 0 {
                libc::EIO
            } else {
                errno
            }));
        }
        #[cfg(windows)]
        {
            super::post_thread_pool_completion(&self.port, 0)
        }
    }
}
