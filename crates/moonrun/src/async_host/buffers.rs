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

//! Guest-visible buffer registrations and exclusive operation leases.
//!
//! Publishing owned bytes creates a Handle; leasing transfers the bytes of an
//! existing Handle to an operation. These are separate operations on the same
//! object, not different buffer kinds. Request-private storage stays with Jobs
//! and I/OResults until their owners choose to publish or copy a result.
//!
//! This module controls access and validates returned leases. Job and I/OResult
//! lifecycles decide when access may resume, including cancellation and teardown.

use std::cell::RefCell;
use std::rc::Rc;

use super::{AsyncHostError, AsyncHostResult, HandleKey, handle_from_key, key_from_handle};
use crate::async_sys::ported_fns;
use crate::runtime::{Handles, HostKeys, HostResourceKind as HandleKind, null_handle};

enum HostCBuffer {
    Available(Box<[u8]>),
    // A readdir Job temporarily owns the buffer. Keeping the slot reserved
    // prevents guest calls from racing the worker without sharing the bytes.
    Leased,
}

#[derive(Debug)]
pub(crate) struct CBufferLease {
    key: HandleKey,
    buffer: Box<[u8]>,
}

impl CBufferLease {
    pub(crate) fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buffer
    }
}

#[cfg(windows)]
enum HostWindowsWatcherBuffer {
    Available(crate::filesystem::watch::EventBuffer),
    Leased,
}

#[cfg(windows)]
#[derive(Debug)]
pub(super) struct WindowsWatcherBufferLease {
    key: HandleKey,
    buffer: crate::filesystem::watch::EventBuffer,
}

#[cfg(windows)]
impl WindowsWatcherBufferLease {
    pub(super) fn buffer_mut(&mut self) -> &mut crate::filesystem::watch::EventBuffer {
        &mut self.buffer
    }
}

pub(crate) struct Buffers {
    c_buffers: RefCell<Handles<HostCBuffer>>,
    #[cfg(windows)]
    windows_watcher_buffers: RefCell<Handles<HostWindowsWatcherBuffer>>,
}

impl Buffers {
    pub(super) fn new(keys: Rc<RefCell<HostKeys>>) -> Self {
        Self {
            c_buffers: RefCell::new(Handles::new(keys.clone(), HandleKind::CBuffer)),
            #[cfg(windows)]
            windows_watcher_buffers: RefCell::new(Handles::new(
                keys.clone(),
                HandleKind::WindowsWatcherBuffer,
            )),
        }
    }

    /// Publish owned bytes under a new guest Handle. The producer decides when
    /// its result is ready and who will release the Handle.
    pub(crate) fn publish(&self, buffer: Box<[u8]>) -> u64 {
        let key = self
            .c_buffers
            .borrow_mut()
            .insert(HostCBuffer::Available(buffer));
        handle_from_key(key)
    }

    pub(crate) fn free(&self, handle: u64) -> AsyncHostResult<()> {
        if handle == null_handle() {
            return Ok(());
        }
        self.c_buffers
            .borrow_mut()
            .remove(key_from_handle(handle))
            .map(|_| ())
            .ok_or(AsyncHostError::Badf)
    }

    pub(crate) fn with_buffer<T>(
        &self,
        handle: u64,
        f: impl FnOnce(&[u8]) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        // A c_buffer handle always names a whole host-owned buffer entry.
        // Callers that need a subrange must pass explicit offset/length
        // arguments; never reinterpret raw or interior pointers as handles.
        let key = key_from_handle(handle);
        let buffers = self.c_buffers.borrow();
        match buffers.get(key).ok_or(AsyncHostError::Badf)? {
            HostCBuffer::Available(buffer) => f(buffer),
            HostCBuffer::Leased => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn with_buffer_mut<T>(
        &self,
        handle: u64,
        f: impl FnOnce(&mut [u8]) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let key = key_from_handle(handle);
        let mut buffers = self.c_buffers.borrow_mut();
        match buffers.get_mut(key).ok_or(AsyncHostError::Badf)? {
            HostCBuffer::Available(buffer) => f(buffer),
            HostCBuffer::Leased => Err(AsyncHostError::Badf),
        }
    }

    /// Transfer a published buffer to an operation, preserving its Handle.
    /// The operation's owner must return it once access is safe again.
    pub(crate) fn lease(&self, handle: u64) -> AsyncHostResult<CBufferLease> {
        let key = key_from_handle(handle);
        let mut buffers = self.c_buffers.borrow_mut();
        let entry = buffers.get_mut(key).ok_or(AsyncHostError::Badf)?;
        match std::mem::replace(entry, HostCBuffer::Leased) {
            HostCBuffer::Available(buffer) => Ok(CBufferLease { key, buffer }),
            HostCBuffer::Leased => Err(AsyncHostError::Badf),
        }
    }

    pub(super) fn restore(&self, buffer: CBufferLease) {
        let CBufferLease { key, buffer } = buffer;
        if let Some(entry) = self.c_buffers.borrow_mut().get_mut(key)
            && matches!(entry, HostCBuffer::Leased)
        {
            *entry = HostCBuffer::Available(buffer);
        }
    }

    #[cfg(windows)]
    pub(crate) fn allocate_windows_watcher(&self) -> u64 {
        let key =
            self.windows_watcher_buffers
                .borrow_mut()
                .insert(HostWindowsWatcherBuffer::Available(
                    crate::filesystem::watch::EventBuffer::new(),
                ));
        handle_from_key(key)
    }

    #[cfg(windows)]
    pub(crate) fn free_windows_watcher(&self, handle: u64) -> AsyncHostResult<()> {
        if handle == null_handle() {
            return Ok(());
        }
        self.windows_watcher_buffers
            .borrow_mut()
            .remove(key_from_handle(handle))
            .map(|_| ())
            .ok_or(AsyncHostError::Badf)
    }

    #[cfg(windows)]
    pub(crate) fn with_windows_watcher<T>(
        &self,
        handle: u64,
        f: impl FnOnce(&crate::filesystem::watch::EventBuffer) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let key = key_from_handle(handle);
        let buffers = self.windows_watcher_buffers.borrow();
        match buffers.get(key).ok_or(AsyncHostError::Badf)? {
            HostWindowsWatcherBuffer::Available(buffer) => f(buffer),
            HostWindowsWatcherBuffer::Leased => Err(AsyncHostError::Badf),
        }
    }

    #[cfg(windows)]
    pub(super) fn lease_windows_watcher(
        &self,
        handle: u64,
    ) -> AsyncHostResult<WindowsWatcherBufferLease> {
        let key = key_from_handle(handle);
        let mut buffers = self.windows_watcher_buffers.borrow_mut();
        let entry = buffers.get_mut(key).ok_or(AsyncHostError::Badf)?;
        match std::mem::replace(entry, HostWindowsWatcherBuffer::Leased) {
            HostWindowsWatcherBuffer::Available(buffer) => {
                Ok(WindowsWatcherBufferLease { key, buffer })
            }
            HostWindowsWatcherBuffer::Leased => Err(AsyncHostError::Badf),
        }
    }

    #[cfg(windows)]
    pub(super) fn restore_windows_watcher(&self, buffer: WindowsWatcherBufferLease) {
        let WindowsWatcherBufferLease { key, buffer } = buffer;
        if let Some(entry) = self.windows_watcher_buffers.borrow_mut().get_mut(key)
            && matches!(entry, HostWindowsWatcherBuffer::Leased)
        {
            *entry = HostWindowsWatcherBuffer::Available(buffer);
        }
    }

    pub(super) fn append_leaks(&self, leaks: &mut Vec<String>) {
        {
            let c_buffers = self.c_buffers.borrow();
            if !c_buffers.is_empty() {
                leaks.push(format!("c_buffers={}", c_buffers.len()));
            }
        }
        #[cfg(windows)]
        {
            let buffers = self.windows_watcher_buffers.borrow();
            if !buffers.is_empty() {
                leaks.push(format!("windows_watcher_buffers={}", buffers.len()));
            }
        }
    }
}

ported_fns! {
impl Buffers {
    #[ported(
        source = "src/internal/c_buffer/stub.c",
        original = "moonbitlang_async_blit_to_c"
    )]
    pub(crate) fn blit_to_c(
        &self,
        dst: u64,
        dst_offset: u32,
        src: &[u8],
        src_offset: u32,
        len: u32,
    ) -> AsyncHostResult<()> {
        self.with_buffer_mut(dst, |dst| {
            let src = checked_range(src, src_offset, len)?;
            let dst = checked_range_mut(dst, dst_offset, len)?;
            dst.copy_from_slice(src);
            Ok(())
        })
    }

    #[ported(
        source = "src/internal/c_buffer/stub.c",
        original = "moonbitlang_async_blit_from_c"
    )]
    pub(crate) fn blit_from_c(
        &self,
        src: u64,
        src_offset: u32,
        dst: &mut [u8],
        dst_offset: u32,
        len: u32,
    ) -> AsyncHostResult<()> {
        self.with_buffer(src, |src| {
            let src = checked_range(src, src_offset, len)?;
            let dst = checked_range_mut(dst, dst_offset, len)?;
            dst.copy_from_slice(src);
            Ok(())
        })
    }

    #[ported(
        source = "src/internal/c_buffer/stub.c",
        original = "moonbitlang_async_c_buffer_get"
    )]
    pub(crate) fn get(&self, handle: u64, index: u32) -> AsyncHostResult<u8> {
        self.with_buffer(handle, |buffer| {
            let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
            buffer.get(index).copied().ok_or(AsyncHostError::Fault)
        })
    }

    #[ported(
        source = "src/internal/c_buffer/stub.c",
        original = "moonbitlang_async_strlen"
    )]
    pub(crate) fn strlen(&self, handle: u64) -> AsyncHostResult<u32> {
        self.with_buffer(handle, |buffer| {
            let len = buffer
                .iter()
                .position(|byte| *byte == 0)
                .ok_or(AsyncHostError::Fault)?;
            u32::try_from(len).map_err(|_| AsyncHostError::Fault)
        })
    }

    #[ported(
        source = "src/internal/c_buffer/stub.c",
        original = "moonbitlang_async_make_c_buffer"
    )]
    pub(crate) fn allocate(&self, size: u32) -> AsyncHostResult<u64> {
        let size = usize::try_from(size).map_err(|_| AsyncHostError::Fault)?;
        Ok(self.publish(vec![0; size].into_boxed_slice()))
    }
}
}

fn checked_range(src: &[u8], offset: u32, len: u32) -> AsyncHostResult<&[u8]> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let end = offset.checked_add(len).ok_or(AsyncHostError::Fault)?;
    src.get(offset..end).ok_or(AsyncHostError::Fault)
}

fn checked_range_mut(src: &mut [u8], offset: u32, len: u32) -> AsyncHostResult<&mut [u8]> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let end = offset.checked_add(len).ok_or(AsyncHostError::Fault)?;
    src.get_mut(offset..end).ok_or(AsyncHostError::Fault)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_buffer_returns_explicitly_after_worker_handoff() {
        let buffers = Buffers::new(Rc::default());
        let handle = buffers.publish(b"abcd".to_vec().into_boxed_slice());
        let mut lease = buffers.lease(handle).unwrap();
        assert_eq!(buffers.get(handle, 0), Err(AsyncHostError::Badf));
        assert_eq!(
            buffers.blit_to_c(handle, 0, b"x", 0, 1),
            Err(AsyncHostError::Badf)
        );
        assert!(matches!(buffers.lease(handle), Err(AsyncHostError::Badf)));

        // The operation can move to a Worker without retaining the Handle table.
        let lease = std::thread::spawn(move || {
            lease.as_mut_slice()[0] = b'x';
            lease
        })
        .join()
        .unwrap();
        // The completed operation still owns the bytes until its owner returns them.
        assert_eq!(buffers.get(handle, 0), Err(AsyncHostError::Badf));
        buffers.restore(lease);
        assert_eq!(buffers.get(handle, 0), Ok(b'x'));
        buffers.blit_to_c(handle, 1, b"y", 0, 1).unwrap();

        // Reuse follows the same protocol after the previous return.
        let mut lease = buffers.lease(handle).unwrap();
        assert_eq!(lease.as_mut_slice(), b"xycd");
        lease.as_mut_slice()[2] = b'z';
        buffers.restore(lease);
        assert_eq!(buffers.get(handle, 2), Ok(b'z'));
        buffers.free(handle).unwrap();
    }

    #[test]
    fn a_late_return_cannot_reach_a_replacement_buffer() {
        let buffers = Buffers::new(Rc::default());
        let old = buffers.publish(b"old".to_vec().into_boxed_slice());
        let lease = buffers.lease(old).unwrap();
        buffers.free(old).unwrap();
        let replacement = buffers.publish(b"new".to_vec().into_boxed_slice());
        let replacement_lease = buffers.lease(replacement).unwrap();

        buffers.restore(lease);
        assert_eq!(buffers.get(old, 0), Err(AsyncHostError::Badf));
        assert_eq!(buffers.get(replacement, 0), Err(AsyncHostError::Badf));
        buffers.restore(replacement_lease);
        buffers
            .with_buffer(replacement, |buffer| {
                assert_eq!(buffer, b"new");
                Ok(())
            })
            .unwrap();
        buffers.free(replacement).unwrap();
    }

    #[test]
    fn buffer_lease_outlives_registration_table() {
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let buffers = Buffers::new(keys.clone());
        let handle = buffers.publish(b"abcd".to_vec().into_boxed_slice());
        let mut lease = buffers.lease(handle).unwrap();
        drop(buffers);

        assert_eq!(keys.borrow().kind(key_from_handle(handle)), None);
        lease.as_mut_slice()[0] = b'x';
        drop(lease);
    }

    #[test]
    fn c_buffer_access_rejects_interior_raw_pointer() {
        let buffers = Buffers::new(Rc::default());
        let handle = buffers.publish(b"abcd".to_vec().into_boxed_slice());
        let interior_ptr = buffers
            .with_buffer(handle, |buffer| {
                // `c_buffer` values are slot-map handles, not addresses into
                // host-owned buffers.
                Ok((buffer.as_ptr() as u64) + 1)
            })
            .unwrap();

        assert_eq!(
            buffers.with_buffer(interior_ptr, |_| Ok(())).unwrap_err(),
            AsyncHostError::Badf
        );
        assert_eq!(
            buffers
                .with_buffer_mut(interior_ptr, |_| Ok(()))
                .unwrap_err(),
            AsyncHostError::Badf
        );
    }

    #[test]
    fn blit_to_c_copies_between_offsets() {
        let buffers = Buffers::new(Rc::default());
        let dst = buffers.publish(b"abcdef".to_vec().into_boxed_slice());

        buffers.blit_to_c(dst, 2, b"XYZ123", 1, 3).unwrap();

        buffers
            .with_buffer(dst, |bytes| {
                assert_eq!(bytes, b"abYZ1f");
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn blit_from_c_copies_between_offsets() {
        let buffers = Buffers::new(Rc::default());
        let src = buffers.publish(b"XYZ123".to_vec().into_boxed_slice());
        let mut dst = *b"abcdef";

        buffers.blit_from_c(src, 1, &mut dst, 2, 3).unwrap();

        assert_eq!(&dst, b"abYZ1f");
    }

    #[test]
    fn strlen_stops_at_first_nul() {
        let buffers = Buffers::new(Rc::default());
        let buffer = buffers.publish(b"abc\0def".to_vec().into_boxed_slice());
        assert_eq!(buffers.strlen(buffer).unwrap(), 3);
    }
}
