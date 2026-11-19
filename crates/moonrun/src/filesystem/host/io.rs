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

//! Read and write operations in the Host Filesystem domain.
//!
//! Keep authorization next to the native effect. Async Host validates guest
//! memory and owns pending-I/O lifetimes; it does not interpret file policy.

use crate::async_host::AsyncHostResult;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use super::HostFs;
use crate::filesystem::{FsOperation, os};
use crate::resource::Resource;

impl HostFs {
    #[cfg(unix)]
    pub(crate) fn read_immediate(
        &self,
        file: &Resource,
        destination: &mut [u8],
    ) -> AsyncHostResult<usize> {
        self.authorize(FsOperation::ReadResource(file.file_grant()))?;
        Ok(os::io::read(file.as_file()?.as_raw_fd(), destination)?)
    }

    #[cfg(unix)]
    pub(crate) fn write_immediate(&self, file: &Resource, source: &[u8]) -> AsyncHostResult<usize> {
        self.authorize(FsOperation::WriteResource(file.file_grant()))?;
        Ok(os::io::write(file.as_file()?.as_raw_fd(), source)?)
    }

    /// Start a read whose completion is owned by Async Host.
    ///
    /// # Safety
    /// The caller retains the resource, buffer, and pinned OVERLAPPED until I/O
    /// completes, and prevents access to the buffer while it is pending.
    #[cfg(windows)]
    pub(crate) unsafe fn read_overlapped(
        &self,
        file: &Resource,
        buffer: &mut [u8],
        overlapped: *mut windows_sys::Win32::System::IO::OVERLAPPED,
    ) -> AsyncHostResult<usize> {
        u32::try_from(buffer.len()).map_err(|_| crate::async_host::AsyncHostError::Fault)?;
        self.authorize(FsOperation::ReadResource(file.file_grant()))?;
        Ok(unsafe { os::io::read(file.as_file()?.as_raw_handle(), buffer, overlapped)? })
    }

    /// Start a write whose completion is owned by Async Host.
    ///
    /// # Safety
    /// The caller retains the resource, buffer, and pinned OVERLAPPED until I/O
    /// completes, and prevents modification of the buffer while it is pending.
    #[cfg(windows)]
    pub(crate) unsafe fn write_overlapped(
        &self,
        file: &Resource,
        buffer: &[u8],
        overlapped: *mut windows_sys::Win32::System::IO::OVERLAPPED,
    ) -> AsyncHostResult<usize> {
        u32::try_from(buffer.len()).map_err(|_| crate::async_host::AsyncHostError::Fault)?;
        self.authorize(FsOperation::WriteResource(file.file_grant()))?;
        Ok(unsafe { os::io::write(file.as_file()?.as_raw_handle(), buffer, overlapped)? })
    }
}
