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

//! Windows I/OResult ownership, submission, and completion retirement.

use super::buffers::WindowsWatcherBufferLease;
use super::{
    AsyncHost, AsyncHostError, AsyncHostResult, HandleKey, HandleKind, HostHandle, handle_from_key,
    key_from_handle,
};
use crate::guest_memory::GuestMemory;
use crate::network::AcceptBuffer;
use crate::poll::CompletionPort;
use crate::resource::{Resource, ResourceClass, ResourceRef};
use crate::runtime::{Handles, HostKeys};
use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::os::windows::io::{AsRawHandle, AsRawSocket, FromRawHandle, OwnedHandle, RawHandle};
use std::rc::Rc;
use std::sync::{Arc, Weak};

pub(super) struct IoResultTable {
    io_results: Handles<Box<HostIoResult>>,
    io_results_by_overlapped: HashMap<OverlappedAddr, HostHandle>,
    registrations: HashMap<usize, (Weak<Resource>, CompletionPort)>,
}

impl IoResultTable {
    pub(super) fn new(keys: Rc<RefCell<HostKeys>>) -> Self {
        Self {
            io_results: Handles::new(keys, HandleKind::IoResult),
            io_results_by_overlapped: HashMap::new(),
            registrations: HashMap::new(),
        }
    }

    pub(super) fn register(&mut self, resource: &ResourceRef, port: CompletionPort) {
        self.registrations
            .retain(|_, (resource, _)| resource.strong_count() != 0);
        self.registrations.insert(
            Arc::as_ptr(resource) as usize,
            (Arc::downgrade(resource), port),
        );
    }

    fn completion_port(&self, resource: &ResourceRef) -> Option<CompletionPort> {
        self.registrations
            .get(&(Arc::as_ptr(resource) as usize))
            .filter(|(registered, _)| registered.ptr_eq(&Arc::downgrade(resource)))
            .map(|(_, port)| port.clone())
    }

    // Resolve native pointers exactly once, before the guest can free or reuse
    // a result. Event accessors subsequently read only these generational keys.
    pub(super) fn complete(
        &mut self,
        overlapped: *mut windows_sys::Win32::System::IO::OVERLAPPED,
    ) -> Option<HandleKey> {
        let address = OverlappedAddr::from_ptr(overlapped);
        let handle = *self.io_results_by_overlapped.get(&address)?;
        let key = key_from_handle(handle);
        self.io_results.get_mut(key)?.complete_pending();
        Some(key)
    }

    pub(super) fn event_result(&self, key: HandleKey) -> AsyncHostResult<HostHandle> {
        self.io_results.get(key).ok_or(AsyncHostError::Badf)?;
        Ok(handle_from_key(key))
    }

    pub(super) fn shutdown(&mut self) {
        // Retire guest reachability first, then cancel all operations before
        // waiting for any one of them. Requests retain both Resources and ports.
        let mut results = self.io_results.take_all();
        self.io_results_by_overlapped.clear();
        for result in &mut results {
            if let Err(error) = result.io.request_cancel() {
                tracing::error!(?error, "failed to cancel Windows I/O during shutdown");
            }
        }
        while let Some(port) = results
            .iter()
            .find(|result| result.io.is_pending() && result.io.completion_port.is_some())
            .and_then(|result| result.io.completion_port.clone())
        {
            match port.wait_for_io() {
                Ok(overlapped) => {
                    for result in &mut results {
                        if result.io.overlapped_ptr() == overlapped {
                            result.io.clear_pending();
                            break;
                        }
                    }
                }
                Err(error) => {
                    tracing::error!(?error, "failed to drain Windows I/O completion port");
                    break;
                }
            }
        }
        for mut result in results {
            if result.io.is_pending() && result.io.completion_port.is_none() {
                // Each request has its own event; another operation on the same
                // file cannot satisfy this wait.
                let file = &result.io.pending.as_ref().unwrap().resource;
                let raw = raw_overlapped_handle(file).expect("submitted Resource");
                let mut bytes = 0;
                unsafe {
                    windows_sys::Win32::System::IO::GetOverlappedResult(
                        raw,
                        result.io.overlapped_ptr(),
                        &mut bytes,
                        1,
                    );
                }
                if result.io.native_completed() {
                    result.io.clear_pending();
                }
            }
            if result.io.is_pending() {
                // An OS wait failure gives no permission to free memory still
                // borrowed by Windows. Retain this allocation, including its
                // Resource/event/port owners, on this exceptional shutdown path.
                tracing::error!(
                    "retaining a Windows I/O request whose completion could not be confirmed"
                );
                std::mem::forget(result);
            }
        }
        self.registrations.clear();
    }

    pub(super) fn leak_summary(&self) -> Option<String> {
        (!self.io_results.is_empty()).then(|| format!("io_results={}", self.io_results.len()))
    }

    pub(super) fn has_pending_io_for_resource(&self, file: &Resource) -> bool {
        self.io_results
            .values()
            .any(|result| result.io.protects_pending_resource(file))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct OverlappedAddr(usize);

impl OverlappedAddr {
    fn from_ptr(ptr: *mut windows_sys::Win32::System::IO::OVERLAPPED) -> Self {
        Self(ptr as usize)
    }
}

const IO_RESULT_READ_EVENT: i32 = 1;

const IO_RESULT_WRITE_EVENT: i32 = 2;

enum HostIoOperation {
    File(Box<[u8]>),
    Socket {
        buffer: Box<[u8]>,
        flags: u32,
    },
    SocketWithAddr {
        buffer: Box<[u8]>,
        flags: u32,
        addr_buffer: Box<[u8]>,
        // WSARecvFrom may write through lpFromlen after submission returns.
        addr_len: i32,
    },
    Connect(Box<[u8]>),
    Accept {
        buffer: AcceptBuffer,
        peer_addr: OnceCell<AsyncHostResult<Box<[u8]>>>,
    },
    // The I/OResult can outlive the lease returned after completion or cancel.
    ReadDirChanges(Option<WindowsWatcherBufferLease>),
}

impl HostIoOperation {
    // Called only on successful native completion; getters consume this result
    // without re-entering the provider parser.
    fn complete_success(&mut self) {
        if let Self::Accept { buffer, peer_addr } = self {
            peer_addr
                .set(
                    unsafe { buffer.parse_peer_addr() }
                        .map(Box::from)
                        .map_err(Into::into),
                )
                .expect("accept output is produced once per submission");
        }
    }

    fn validate_resource(&self, resource: &Resource) -> AsyncHostResult<()> {
        let class = resource.resource_class();
        match self {
            Self::File(_) | Self::ReadDirChanges(_) if class == ResourceClass::File => Ok(()),
            Self::Socket { .. } if class.is_socket() => Ok(()),
            Self::SocketWithAddr { .. } if class == ResourceClass::UdpSocket => Ok(()),
            _ => Err(AsyncHostError::Inval),
        }
    }
}

// Boxed before submission. Neither the operation nor its native output fields
// may move while Windows retains pointers into them. Read operations own output
// storage; writes own a copy of guest input, never a retained guest pointer.
struct HostIoResult {
    operation: HostIoOperation,
    event: i32,
    io: OverlappedIo,
}

// Common submission and retirement state is independent of operation storage,
// so submission can borrow both without moving the operation out of its box.
struct OverlappedIo {
    overlapped: windows_sys::Win32::System::IO::OVERLAPPED,
    completion_event: Option<OwnedHandle>,
    completion_port: Option<CompletionPort>,
    pending: Option<PendingIo>,
}

impl Default for OverlappedIo {
    fn default() -> Self {
        Self {
            overlapped: unsafe { std::mem::zeroed() },
            completion_event: None,
            completion_port: None,
            pending: None,
        }
    }
}

// Present from a successful asynchronous submission until its completion is
// consumed. AcceptEx also borrows the pre-created accepted socket.
struct PendingIo {
    resource: ResourceRef,
    close_guard: Option<ResourceRef>,
}

impl HostIoResult {
    // Called when a pending request's completion is observed. Fresh requests
    // also have a zero native status, but have no output to finalize.
    fn complete_pending(&mut self) {
        if !self.io.is_pending() {
            return;
        }
        self.io.clear_pending();
        if self.io.overlapped.Internal == windows_sys::Win32::Foundation::STATUS_SUCCESS as usize {
            self.operation.complete_success();
        }
    }

    fn cancel_pending(&mut self) -> AsyncHostResult<i32> {
        self.io.request_cancel()?;
        let Some(pending) = &self.io.pending else {
            return Ok(0);
        };
        if self.io.completion_port.is_some() {
            // Cancellation does not retire the packet's pointer identity.
            // The guest must still poll before reusing/freeing this result.
            return Ok(1);
        }
        let raw = raw_overlapped_handle(&pending.resource)?;
        let mut bytes = 0;
        unsafe {
            windows_sys::Win32::System::IO::GetOverlappedResult(
                raw,
                self.io.overlapped_ptr(),
                &mut bytes,
                0,
            );
        }
        if self.io.native_completed() {
            self.complete_pending();
            Ok(0)
        } else {
            Ok(1)
        }
    }

    fn for_file_read(len: u32, position: i64) -> AsyncHostResult<Self> {
        let buffer =
            vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?].into_boxed_slice();
        Ok(Self::for_file(IO_RESULT_READ_EVENT, buffer, position))
    }

    fn for_file_write(buffer: Box<[u8]>, position: i64) -> Self {
        Self::for_file(IO_RESULT_WRITE_EVENT, buffer, position)
    }

    fn for_file(event: i32, buffer: Box<[u8]>, position: i64) -> Self {
        let mut io = OverlappedIo::default();
        io.overlapped.Anonymous.Anonymous.Offset = position as u32;
        io.overlapped.Anonymous.Anonymous.OffsetHigh = (position >> 32) as u32;
        Self {
            operation: HostIoOperation::File(buffer),
            event,
            io,
        }
    }

    fn for_socket_read(len: u32, flags: i32) -> AsyncHostResult<Self> {
        let buffer =
            vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?].into_boxed_slice();
        Ok(Self::for_socket(IO_RESULT_READ_EVENT, buffer, flags))
    }

    fn for_socket_write(buffer: Box<[u8]>, flags: i32) -> Self {
        Self::for_socket(IO_RESULT_WRITE_EVENT, buffer, flags)
    }

    fn for_socket(event: i32, buffer: Box<[u8]>, flags: i32) -> Self {
        Self {
            operation: HostIoOperation::Socket {
                buffer,
                flags: flags as u32,
            },
            event,
            io: OverlappedIo::default(),
        }
    }

    fn for_socket_with_addr_read(
        len: u32,
        flags: i32,
        addr_buffer: Box<[u8]>,
    ) -> AsyncHostResult<Self> {
        let buffer =
            vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?].into_boxed_slice();
        Self::for_socket_with_addr(IO_RESULT_READ_EVENT, buffer, flags, addr_buffer)
    }

    fn for_socket_with_addr_write(
        buffer: Box<[u8]>,
        flags: i32,
        addr_buffer: Box<[u8]>,
    ) -> AsyncHostResult<Self> {
        Self::for_socket_with_addr(IO_RESULT_WRITE_EVENT, buffer, flags, addr_buffer)
    }

    fn for_socket_with_addr(
        event: i32,
        buffer: Box<[u8]>,
        flags: i32,
        addr_buffer: Box<[u8]>,
    ) -> AsyncHostResult<Self> {
        let addr_len = i32::try_from(addr_buffer.len()).map_err(|_| AsyncHostError::Fault)?;
        Ok(Self {
            operation: HostIoOperation::SocketWithAddr {
                buffer,
                flags: flags as u32,
                addr_buffer,
                addr_len,
            },
            event,
            io: OverlappedIo::default(),
        })
    }

    fn for_connect(addr_buffer: Box<[u8]>) -> Self {
        Self {
            operation: HostIoOperation::Connect(addr_buffer),
            event: IO_RESULT_WRITE_EVENT,
            io: OverlappedIo::default(),
        }
    }

    fn for_accept(addr_len: u32) -> AsyncHostResult<Self> {
        Ok(Self {
            operation: HostIoOperation::Accept {
                buffer: AcceptBuffer::new(addr_len).map_err(|_| AsyncHostError::Fault)?,
                peer_addr: OnceCell::new(),
            },
            event: IO_RESULT_READ_EVENT,
            io: OverlappedIo::default(),
        })
    }

    fn for_read_dir_changes(buffer: WindowsWatcherBufferLease) -> Self {
        Self {
            operation: HostIoOperation::ReadDirChanges(Some(buffer)),
            event: IO_RESULT_READ_EVENT,
            io: OverlappedIo::default(),
        }
    }

    fn validate_completed_read(&self) -> AsyncHostResult<()> {
        if self.event != IO_RESULT_READ_EVENT {
            return Err(AsyncHostError::Inval);
        }
        if self.io.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        Ok(())
    }

    fn copy_read_payload(
        buffer: &[u8],
        memory: &mut (impl GuestMemory + ?Sized),
        dst: u32,
        offset: u32,
        len: u32,
    ) -> AsyncHostResult<()> {
        let bytes_transferred = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let data = buffer
            .get(..bytes_transferred)
            .ok_or(AsyncHostError::Fault)?;
        let dst = dst.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        memory.write_exact(dst, data)?;
        Ok(())
    }

    fn copy_read_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        dst: u32,
        offset: u32,
        len: u32,
    ) -> AsyncHostResult<()> {
        self.validate_completed_read()?;
        let buffer: &[u8] = match &self.operation {
            HostIoOperation::File(buffer) | HostIoOperation::Socket { buffer, .. } => buffer,
            // Accept historically exposes an empty generic read result. Preserve
            // its zero-length copy and bounds errors for direct ABI callers.
            HostIoOperation::Accept { .. } => &[],
            _ => return Err(AsyncHostError::Inval),
        };
        Self::copy_read_payload(buffer, memory, dst, offset, len)
    }

    fn copy_read_result_with_addr(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        dst: u32,
        offset: u32,
        len: u32,
        addr: u32,
        addr_len: u32,
    ) -> AsyncHostResult<()> {
        self.validate_completed_read()?;
        let HostIoOperation::SocketWithAddr {
            buffer,
            addr_buffer,
            addr_len: actual_addr_len,
            ..
        } = &self.operation
        else {
            return Err(AsyncHostError::Inval);
        };
        Self::copy_read_payload(buffer, memory, dst, offset, len)?;
        let actual_addr_len =
            usize::try_from(*actual_addr_len).map_err(|_| AsyncHostError::Fault)?;
        let addr_data = addr_buffer
            .get(..actual_addr_len)
            .ok_or(AsyncHostError::Fault)?;
        memory.write_with_capacity(addr, addr_len, addr_data)?;
        Ok(())
    }
}

impl OverlappedIo {
    fn overlapped_ptr(&mut self) -> *mut windows_sys::Win32::System::IO::OVERLAPPED {
        &mut self.overlapped
    }

    fn overlapped_addr(&mut self) -> OverlappedAddr {
        OverlappedAddr::from_ptr(self.overlapped_ptr())
    }

    #[cfg(test)]
    fn pending_resource_identity(&self) -> Option<isize> {
        self.pending
            .as_ref()
            .map(|pending| pending.resource.raw_identity())
    }

    fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    fn protects_pending_resource(&self, file: &Resource) -> bool {
        self.pending.as_ref().is_some_and(|pending| {
            std::ptr::eq(pending.resource.as_ref(), file)
                || pending
                    .close_guard
                    .as_ref()
                    .is_some_and(|guard| std::ptr::eq(guard.as_ref(), file))
        })
    }

    fn mark_pending(&mut self, file: ResourceRef) -> AsyncHostResult<()> {
        if self.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        self.pending = Some(PendingIo {
            resource: file,
            close_guard: None,
        });
        Ok(())
    }

    fn mark_pending_with_close_guard(
        &mut self,
        file: ResourceRef,
        close_guard: ResourceRef,
    ) -> AsyncHostResult<()> {
        if self.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        self.pending = Some(PendingIo {
            resource: file,
            close_guard: Some(close_guard),
        });
        Ok(())
    }

    fn clear_pending(&mut self) {
        self.pending = None;
    }

    fn validate_pending_resource(&self, file: &Resource) -> AsyncHostResult<()> {
        if let Some(pending) = &self.pending
            && !std::ptr::eq(pending.resource.as_ref(), file)
        {
            return Err(AsyncHostError::Badf);
        }
        Ok(())
    }

    fn request_cancel(&mut self) -> AsyncHostResult<()> {
        use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
        use windows_sys::Win32::System::IO::CancelIoEx;
        let Some(pending) = &self.pending else {
            return Ok(());
        };
        let raw = raw_overlapped_handle(&pending.resource)?;
        if unsafe { CancelIoEx(raw, self.overlapped_ptr()) } == 0 {
            let errno = last_errno();
            if errno != ERROR_NOT_FOUND as i32 {
                return Err(AsyncHostError::Native(errno));
            }
        }
        Ok(())
    }

    fn native_completed(&self) -> bool {
        // Windows owns Internal while I/O is pending. This is the native
        // HasOverlappedIoCompleted check, including terminal error results.
        (unsafe { std::ptr::read_volatile(&self.overlapped.Internal) })
            != windows_sys::Win32::Foundation::STATUS_PENDING as usize
    }

    fn prepare_submission(&mut self, port: Option<CompletionPort>) -> AsyncHostResult<()> {
        // IOCP requests wait on their retained port. Only unregistered requests
        // need a private event to distinguish concurrent I/O on the same file.
        if port.is_none() && self.completion_event.is_none() {
            let event = unsafe {
                windows_sys::Win32::System::Threading::CreateEventW(
                    std::ptr::null(),
                    1,
                    0,
                    std::ptr::null(),
                )
            };
            if event.is_null() {
                return Err(last_native_error());
            }
            self.overlapped.hEvent = event;
            self.completion_event = Some(unsafe { OwnedHandle::from_raw_handle(event) });
        }
        // Offsets belong to file requests; reset only native completion state.
        self.overlapped.Internal = 0;
        self.overlapped.InternalHigh = 0;
        if let Some(event) = &self.completion_event
            && unsafe { windows_sys::Win32::System::Threading::ResetEvent(event.as_raw_handle()) }
                == 0
        {
            return Err(last_native_error());
        }
        self.completion_port = port;
        Ok(())
    }
}

impl Drop for IoResultTable {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl AsyncHost {
    fn read_guest_slice(
        memory: &mut (impl GuestMemory + ?Sized),
        ptr: u32,
        offset: u32,
        len: u32,
    ) -> AsyncHostResult<Box<[u8]>> {
        let ptr = ptr.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        Ok(memory.read_exact(ptr, len)?.into())
    }

    pub(crate) fn make_file_read_io_result(&self, len: u32, position: i64) -> AsyncHostResult<u64> {
        self.insert_io_result(HostIoResult::for_file_read(len, position)?)
    }

    pub(crate) fn make_file_write_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        src: u32,
        offset: u32,
        len: u32,
        position: i64,
    ) -> AsyncHostResult<u64> {
        let buffer = Self::read_guest_slice(memory, src, offset, len)?;
        self.insert_io_result(HostIoResult::for_file_write(buffer, position))
    }

    pub(crate) fn make_socket_read_io_result(&self, len: u32, flags: i32) -> AsyncHostResult<u64> {
        self.insert_io_result(HostIoResult::for_socket_read(len, flags)?)
    }

    pub(crate) fn make_socket_write_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        src: u32,
        offset: u32,
        len: u32,
        flags: i32,
    ) -> AsyncHostResult<u64> {
        let buffer = Self::read_guest_slice(memory, src, offset, len)?;
        self.insert_io_result(HostIoResult::for_socket_write(buffer, flags))
    }

    pub(crate) fn make_socket_with_addr_read_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        len: u32,
        flags: i32,
        addr: u32,
        addr_len: u32,
    ) -> AsyncHostResult<u64> {
        let addr_buffer = memory.read_exact(addr, addr_len)?.into();
        self.insert_io_result(HostIoResult::for_socket_with_addr_read(
            len,
            flags,
            addr_buffer,
        )?)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn make_socket_with_addr_write_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        src: u32,
        offset: u32,
        len: u32,
        flags: i32,
        addr: u32,
        addr_len: u32,
    ) -> AsyncHostResult<u64> {
        let buffer = Self::read_guest_slice(memory, src, offset, len)?;
        let addr_buffer = memory.read_exact(addr, addr_len)?.into();
        self.insert_io_result(HostIoResult::for_socket_with_addr_write(
            buffer,
            flags,
            addr_buffer,
        )?)
    }

    pub(crate) fn make_connect_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        addr: u32,
        addr_len: u32,
    ) -> AsyncHostResult<u64> {
        let addr_buffer = memory.read_exact(addr, addr_len)?.into();
        self.insert_io_result(HostIoResult::for_connect(addr_buffer))
    }

    pub(crate) fn make_accept_io_result(&self, addr_len: u32) -> AsyncHostResult<u64> {
        self.insert_io_result(HostIoResult::for_accept(addr_len)?)
    }

    pub(crate) fn make_read_dir_changes_io_result(
        &self,
        buffer_handle: u64,
    ) -> AsyncHostResult<u64> {
        let buffer = self.buffers().lease_windows_watcher(buffer_handle)?;
        self.insert_io_result(HostIoResult::for_read_dir_changes(buffer))
    }

    fn insert_io_result(&self, result: HostIoResult) -> AsyncHostResult<u64> {
        let mut result = Box::new(result);
        let overlapped = result.io.overlapped_addr();
        let mut table = self.io_results.borrow_mut();
        let key = table.io_results.insert(result);
        let handle = handle_from_key(key);
        table.io_results_by_overlapped.insert(overlapped, handle);
        Ok(handle)
    }

    pub(crate) fn free_io_result(&self, handle: u64) -> AsyncHostResult<()> {
        let key = key_from_handle(handle);
        let mut io_results = self.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(key)
            .ok_or(AsyncHostError::Badf)?;
        if result.io.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        let mut result = io_results
            .io_results
            .remove(key)
            .ok_or(AsyncHostError::Badf)?;
        let overlapped = result.io.overlapped_addr();
        io_results.io_results_by_overlapped.remove(&overlapped);
        let buffer = match &mut result.operation {
            HostIoOperation::ReadDirChanges(buffer) => buffer.take(),
            _ => None,
        };
        drop(io_results);
        if let Some(buffer) = buffer {
            self.buffers().restore_windows_watcher(buffer);
        }
        Ok(())
    }

    pub(crate) fn io_result_get_event(&self, handle: u64) -> AsyncHostResult<i32> {
        let key = key_from_handle(handle);
        let io_results = self.io_results.borrow();
        let result = io_results.io_results.get(key).ok_or(AsyncHostError::Badf)?;
        Ok(result.event)
    }

    pub(crate) fn cancel_io_result(
        &self,
        result_handle: u64,
        fd_handle: HostHandle,
    ) -> AsyncHostResult<i32> {
        let handles = self.resources.borrow();
        let file = handles.resource(fd_handle)?;
        let result_key = key_from_handle(result_handle);
        let mut io_results = self.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        result.io.validate_pending_resource(file)?;
        let status = result.cancel_pending();
        let buffer = if !result.io.is_pending() {
            match &mut result.operation {
                HostIoOperation::ReadDirChanges(buffer) => buffer.take(),
                _ => None,
            }
        } else {
            None
        };
        drop(io_results);
        drop(handles);
        if let Some(buffer) = buffer {
            self.buffers().restore_windows_watcher(buffer);
        }
        status
    }

    pub(crate) fn io_result_get_status(
        &self,
        result_handle: u64,
        fd_handle: HostHandle,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::System::IO::GetOverlappedResult;

        let handles = self.resources.borrow();
        let file = handles.resource(fd_handle)?;
        let raw_handle = raw_overlapped_handle(file)?;
        let result_key = key_from_handle(result_handle);
        let mut io_results = self.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        result.io.validate_pending_resource(file)?;
        if result.io.is_pending() && result.io.completion_port.is_some() {
            return Err(AsyncHostError::Native(
                windows_sys::Win32::Foundation::ERROR_IO_INCOMPLETE as i32,
            ));
        }
        let mut bytes_transferred = 0;
        let status = if unsafe {
            GetOverlappedResult(
                raw_handle,
                result.io.overlapped_ptr(),
                &mut bytes_transferred,
                0,
            )
        } == 0
        {
            let error = last_native_error();
            if matches!(
                error,
                AsyncHostError::Native(errno)
                    if errno == windows_sys::Win32::Foundation::ERROR_IO_INCOMPLETE as i32
            ) {
                return Err(error);
            }
            if result.io.native_completed() {
                result.complete_pending();
            } else {
                return Err(error);
            }
            Err(error)
        } else {
            result.complete_pending();
            i32::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)
        };
        let buffer = if let HostIoOperation::ReadDirChanges(buffer) = &mut result.operation {
            let completed_len = status
                .as_ref()
                .ok()
                .and_then(|len| usize::try_from(*len).ok())
                .unwrap_or(0);
            buffer
                .as_mut()
                .ok_or(AsyncHostError::Inval)?
                .buffer_mut()
                .complete_read(completed_len)?;
            buffer.take()
        } else {
            None
        };
        drop(io_results);
        drop(handles);
        if let Some(buffer) = buffer {
            self.buffers().restore_windows_watcher(buffer);
        }
        status
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn io_result_copy_read(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        result_handle: u64,
        dst: u32,
        offset: u32,
        len: u32,
    ) -> AsyncHostResult<()> {
        let key = key_from_handle(result_handle);
        let io_results = self.io_results.borrow();
        let result = io_results.io_results.get(key).ok_or(AsyncHostError::Badf)?;
        result.copy_read_result(memory, dst, offset, len)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn io_result_copy_read_with_addr(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        result_handle: u64,
        dst: u32,
        offset: u32,
        len: u32,
        addr: u32,
        addr_len: u32,
    ) -> AsyncHostResult<()> {
        let key = key_from_handle(result_handle);
        let io_results = self.io_results.borrow();
        let result = io_results.io_results.get(key).ok_or(AsyncHostError::Badf)?;
        result.copy_read_result_with_addr(memory, dst, offset, len, addr, addr_len)
    }

    pub(crate) fn read_io_result(
        &self,
        fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::{ERROR_HANDLE_EOF, ERROR_IO_PENDING};

        // The accepted resource class depends on the IO result kind, so resolve
        // both while their tables are borrowed.
        let handles = self.resources.borrow();
        let result_key = key_from_handle(result_handle);
        let mut io_results = self.io_results.borrow_mut();
        let file = handles.resource_ref(fd_handle)?;
        let port = io_results.completion_port(file);
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.io.is_pending() || result.event != IO_RESULT_READ_EVENT {
            return Err(AsyncHostError::Inval);
        }
        result.operation.validate_resource(file)?;
        result.io.prepare_submission(port)?;
        let overlapped = result.io.overlapped_ptr();
        // The domain borrows storage; this table owns it and the Resource until
        // completion. The address length can be written after submission; flags
        // are only borrowed for the call.
        let outcome = match &mut result.operation {
            HostIoOperation::File(buffer) => unsafe {
                self.filesystem.read_overlapped(file, buffer, overlapped)
            },
            HostIoOperation::Socket { buffer, flags } => unsafe {
                self.network
                    .recv_overlapped(file, buffer, flags, overlapped)
            },
            HostIoOperation::SocketWithAddr {
                buffer,
                flags,
                addr_buffer,
                addr_len,
            } => unsafe {
                self.network.recv_from_overlapped(
                    file,
                    buffer,
                    flags,
                    addr_buffer,
                    std::ptr::from_mut(addr_len),
                    overlapped,
                )
            },
            _ => return Err(AsyncHostError::Inval),
        };
        match outcome {
            Ok(bytes) => i32::try_from(bytes).map_err(|_| AsyncHostError::Fault),
            Err(AsyncHostError::Native(errno)) if errno == ERROR_HANDLE_EOF as i32 => Ok(0),
            Err(error) => {
                if matches!(error, AsyncHostError::Native(errno) if errno == ERROR_IO_PENDING as i32)
                {
                    result.io.mark_pending(Arc::clone(file))?;
                }
                Err(error)
            }
        }
    }

    pub(crate) fn read_dir_changes_io_result(
        &self,
        fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::ERROR_IO_PENDING;

        let handles = self.resources.borrow();
        let result_key = key_from_handle(result_handle);
        let mut io_results = self.io_results.borrow_mut();
        let file = handles.resource_ref(fd_handle)?;
        let port = io_results.completion_port(file);
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.io.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        let HostIoOperation::ReadDirChanges(buffer) = &mut result.operation else {
            return Err(AsyncHostError::Inval);
        };
        if file.resource_class() != ResourceClass::File {
            return Err(AsyncHostError::Inval);
        }
        result.io.prepare_submission(port)?;
        let overlapped = result.io.overlapped_ptr();
        let buffer = buffer.as_mut().map(WindowsWatcherBufferLease::buffer_mut);
        // The result keeps the leased buffer, OVERLAPPED, and Resource alive
        // until completion; filesystem owns watch authorization and submission.
        unsafe { crate::filesystem::watch::submit(&self.filesystem, file, buffer, overlapped)? };

        // Directory change notification reports successful submission, not
        // synchronous completion. Match native async by
        // exposing the operation as pending even though the call returned TRUE.
        result.io.mark_pending(Arc::clone(file))?;
        Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
    }

    pub(crate) fn write_io_result(
        &self,
        fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::ERROR_IO_PENDING;

        // The accepted resource class depends on the IO result kind, so resolve
        // both while their tables are borrowed.
        let handles = self.resources.borrow();
        let result_key = key_from_handle(result_handle);
        let mut io_results = self.io_results.borrow_mut();
        let file = handles.resource_ref(fd_handle)?;
        let port = io_results.completion_port(file);
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.io.is_pending() || result.event != IO_RESULT_WRITE_EVENT {
            return Err(AsyncHostError::Inval);
        }
        result.operation.validate_resource(file)?;
        result.io.prepare_submission(port)?;
        let overlapped = result.io.overlapped_ptr();
        let outcome = match &result.operation {
            HostIoOperation::File(buffer) => unsafe {
                self.filesystem.write_overlapped(file, buffer, overlapped)
            },
            HostIoOperation::Socket { buffer, flags } => unsafe {
                self.network
                    .send_overlapped(file, buffer, *flags, overlapped)
            },
            HostIoOperation::SocketWithAddr {
                buffer,
                flags,
                addr_buffer,
                ..
            } => unsafe {
                self.network
                    .send_to_overlapped(file, buffer, *flags, addr_buffer, overlapped)
            },
            _ => return Err(AsyncHostError::Inval),
        };
        match outcome {
            Ok(bytes) => i32::try_from(bytes).map_err(|_| AsyncHostError::Fault),
            Err(error) => {
                if matches!(error, AsyncHostError::Native(errno) if errno == ERROR_IO_PENDING as i32)
                {
                    result.io.mark_pending(Arc::clone(file))?;
                }
                Err(error)
            }
        }
    }

    pub(crate) fn connect_io_result(
        &self,
        fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        let handles = self.resources.borrow();
        let file = handles.acquire_resource_of_class(fd_handle, ResourceClass::TcpSocket)?;
        let result_key = key_from_handle(result_handle);
        let mut io_results = self.io_results.borrow_mut();
        let port = io_results.completion_port(&file);
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.io.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        let HostIoOperation::Connect(addr_buffer) = &result.operation else {
            return Err(AsyncHostError::Inval);
        };
        result.io.prepare_submission(port)?;
        let overlapped = result.io.overlapped_ptr();
        match unsafe {
            self.network
                .connect_overlapped(&file, addr_buffer, overlapped)
        } {
            Ok(()) => Ok(1),
            Err(error) => {
                if matches!(error, AsyncHostError::Native(errno)
                    if errno == windows_sys::Win32::Foundation::ERROR_IO_PENDING as i32)
                {
                    result.io.mark_pending(file)?;
                }
                Err(error)
            }
        }
    }

    pub(crate) fn setup_connected_socket(&self, fd_handle: HostHandle) -> AsyncHostResult<()> {
        self.with_resource_of_class(fd_handle, ResourceClass::TcpSocket, |file| {
            self.network.setup_connected_socket(file)
        })
    }

    pub(crate) fn accept_io_result(
        &self,
        server_fd_handle: HostHandle,
        conn_fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        let handles = self.resources.borrow();
        let server_file =
            handles.acquire_resource_of_class(server_fd_handle, ResourceClass::TcpSocket)?;
        let conn_file =
            handles.acquire_resource_of_class(conn_fd_handle, ResourceClass::TcpSocket)?;
        let result_key = key_from_handle(result_handle);
        let mut io_results = self.io_results.borrow_mut();
        let port = io_results.completion_port(&server_file);
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.io.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        let HostIoOperation::Accept { buffer, peer_addr } = &mut result.operation else {
            return Err(AsyncHostError::Inval);
        };
        // A rejected resubmission must not expose an earlier request's output.
        peer_addr.take();
        result.io.prepare_submission(port)?;
        let overlapped = result.io.overlapped_ptr();
        match unsafe {
            self.network
                .accept_overlapped(&server_file, &conn_file, buffer, overlapped)
        } {
            Ok(()) => {
                result.operation.complete_success();
                Ok(1)
            }
            Err(error) => {
                if matches!(error, AsyncHostError::Native(errno)
                    if errno == windows_sys::Win32::Foundation::ERROR_IO_PENDING as i32)
                {
                    result
                        .io
                        .mark_pending_with_close_guard(server_file, conn_file)?;
                }
                Err(error)
            }
        }
    }

    pub(crate) fn get_accept_peer_addr(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        result_handle: u64,
        dst: u32,
        dst_len: u32,
    ) -> AsyncHostResult<()> {
        let key = key_from_handle(result_handle);
        let io_results = self.io_results.borrow();
        let result = io_results.io_results.get(key).ok_or(AsyncHostError::Badf)?;
        let HostIoOperation::Accept { peer_addr, .. } = &result.operation else {
            return Err(AsyncHostError::Inval);
        };
        let addr = peer_addr
            .get()
            .ok_or(AsyncHostError::Inval)?
            .as_deref()
            .map_err(|error| *error)?;
        memory.write_with_capacity(dst, dst_len, addr)?;
        Ok(())
    }

    pub(crate) fn setup_accepted_socket(
        &self,
        listen_fd_handle: HostHandle,
        accept_fd_handle: HostHandle,
    ) -> AsyncHostResult<()> {
        let handles = self.resources.borrow();
        let listen_file = handles.resource_of_class(listen_fd_handle, ResourceClass::TcpSocket)?;
        let accept_file = handles.resource_of_class(accept_fd_handle, ResourceClass::TcpSocket)?;
        self.network.setup_accepted_socket(listen_file, accept_file)
    }
}

fn last_errno() -> i32 {
    unsafe { windows_sys::Win32::Foundation::GetLastError() as i32 }
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(last_errno())
}

fn raw_overlapped_handle(file: &Resource) -> AsyncHostResult<RawHandle> {
    if file.resource_class().is_socket() {
        // The Windows overlapped-I/O functions used by the native async host
        // accept sockets through their HANDLE parameter. Keep that ABI cast at
        // this adapter seam rather than representing sockets as file handles.
        Ok(file.as_socket()?.as_raw_socket() as RawHandle)
    } else {
        Ok(file.as_file()?.as_raw_handle())
    }
}

#[cfg(test)]
mod tests;
