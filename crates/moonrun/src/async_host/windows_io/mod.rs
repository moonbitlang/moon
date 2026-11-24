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

use super::{
    AsyncHost, AsyncHostError, AsyncHostResult, HandleKey, HandleKind, HostHandle,
    WindowsWatcherBufferLease, handle_from_key, key_from_handle,
};
use crate::async_sys::internal::event_loop::poll::{self, CompletionPort, PollInstance};
use crate::async_sys::socket::RawSocket;
use crate::filesystem::{FsOperation, FsTarget};
use crate::guest_memory::GuestMemory;
use crate::resource::{Resource, ResourceClass, ResourceRef};
use crate::runtime::{Handles, HostKeys};
use std::cell::RefCell;
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

    pub(super) fn register(&mut self, resource: &ResourceRef, poll: &PollInstance) {
        self.registrations
            .retain(|_, (resource, _)| resource.strong_count() != 0);
        self.registrations.insert(
            Arc::as_ptr(resource) as usize,
            (Arc::downgrade(resource), CompletionPort::from_poll(poll)),
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
    pub(super) fn complete_batch(
        &mut self,
        poll: &PollInstance,
        count: i32,
    ) -> Vec<Option<HandleKey>> {
        (0..count as u32)
            .map(|index| {
                let event = poll::event_list_get(poll, index).expect("dequeued event");
                let address = OverlappedAddr::from_ptr(poll::event_get_io_result(event));
                let handle = *self.io_results_by_overlapped.get(&address)?;
                let key = key_from_handle(handle);
                self.io_results.get_mut(key)?.clear_pending();
                Some(key)
            })
            .collect()
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
            if let Err(error) = result.request_cancel() {
                tracing::error!(?error, "failed to cancel Windows I/O during shutdown");
            }
        }
        while let Some(port) = results
            .iter()
            .find(|result| result.is_pending() && result.completion_port.is_some())
            .and_then(|result| result.completion_port.clone())
        {
            match port.wait_for_io() {
                Ok(overlapped) => {
                    for result in &mut results {
                        if result.overlapped_ptr() == overlapped {
                            result.clear_pending();
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
            if result.is_pending() && result.completion_port.is_none() {
                // Each request has its own event; another operation on the same
                // file cannot satisfy this wait.
                let file = &result.pending.as_ref().unwrap().resource;
                let raw = raw_overlapped_handle(file).expect("submitted Resource");
                let mut bytes = 0;
                unsafe {
                    windows_sys::Win32::System::IO::GetOverlappedResult(
                        raw,
                        result.overlapped_ptr(),
                        &mut bytes,
                        1,
                    );
                }
                if result.native_completed() {
                    result.clear_pending();
                }
            }
            if result.is_pending() {
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
            .any(|result| result.protects_pending_resource(file))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostIoKind {
    File,
    Socket,
    SocketWithAddr,
    Connect,
    Accept,
    ReadDirChanges,
}

impl HostIoKind {
    fn validate_resource(self, resource: &Resource) -> AsyncHostResult<()> {
        let class = resource.resource_class();
        match self {
            Self::File | Self::ReadDirChanges if class == ResourceClass::File => Ok(()),
            Self::Socket if class.is_socket() => Ok(()),
            Self::SocketWithAddr if class == ResourceClass::UdpSocket => Ok(()),
            _ => Err(AsyncHostError::Inval),
        }
    }
}

struct HostIoResult {
    overlapped: windows_sys::Win32::System::IO::OVERLAPPED,
    kind: HostIoKind,
    completion_event: Option<OwnedHandle>,
    completion_port: Option<CompletionPort>,
    event: i32,
    // Native async retains MoonBit objects until free_io_result. The wasm host
    // cannot retain guest pointers, so IOResults keep only host-owned buffers:
    // read constructors allocate output capacity, and write constructors copy
    // the input payload before any overlapped operation can outlive the import.
    buffer: Vec<u8>,
    // Directory change notification writes into a stable host watcher buffer.
    // Lease it while the OVERLAPPED operation is pending, then restore the
    // bytes and selected event layout before the guest parses the batch.
    read_dir_changes_buffer: Option<WindowsWatcherBufferLease>,
    socket_flags: u32,
    addr_buffer: Vec<u8>,
    // WSARecvFrom may complete asynchronously and write through lpFromlen later.
    // Keep that storage with the overlapped result, not on the submitter stack.
    addr_len: i32,
    accept_buffer: Vec<u8>,
    accept_bytes_received: u32,
    pending: Option<PendingIo>,
}

// Present from a successful asynchronous submission until its completion is
// consumed. AcceptEx also borrows the pre-created accepted socket.
struct PendingIo {
    resource: ResourceRef,
    close_guard: Option<ResourceRef>,
}

impl HostIoResult {
    fn zeroed_overlapped() -> windows_sys::Win32::System::IO::OVERLAPPED {
        let overlapped =
            std::mem::MaybeUninit::<windows_sys::Win32::System::IO::OVERLAPPED>::zeroed();
        unsafe { overlapped.assume_init() }
    }

    fn for_file_read(len: u32, position: i64) -> AsyncHostResult<Self> {
        let buffer = vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?];
        Ok(Self::for_file(IO_RESULT_READ_EVENT, buffer, position))
    }

    fn for_file_write(buffer: Vec<u8>, position: i64) -> Self {
        Self::for_file(IO_RESULT_WRITE_EVENT, buffer, position)
    }

    fn for_file(event: i32, buffer: Vec<u8>, position: i64) -> Self {
        let mut overlapped = Self::zeroed_overlapped();
        overlapped.Anonymous.Anonymous.Offset = position as u32;
        overlapped.Anonymous.Anonymous.OffsetHigh = (position >> 32) as u32;
        Self {
            overlapped,
            completion_event: None,
            completion_port: None,
            kind: HostIoKind::File,
            event,
            buffer,
            read_dir_changes_buffer: None,
            socket_flags: 0,
            addr_buffer: Vec::new(),
            addr_len: 0,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            pending: None,
        }
    }

    fn for_socket_read(len: u32, flags: i32) -> AsyncHostResult<Self> {
        let buffer = vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?];
        Ok(Self::for_socket(IO_RESULT_READ_EVENT, buffer, flags))
    }

    fn for_socket_write(buffer: Vec<u8>, flags: i32) -> Self {
        Self::for_socket(IO_RESULT_WRITE_EVENT, buffer, flags)
    }

    fn for_socket(event: i32, buffer: Vec<u8>, flags: i32) -> Self {
        Self {
            overlapped: Self::zeroed_overlapped(),
            completion_event: None,
            completion_port: None,
            kind: HostIoKind::Socket,
            event,
            buffer,
            read_dir_changes_buffer: None,
            socket_flags: flags as u32,
            addr_buffer: Vec::new(),
            addr_len: 0,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            pending: None,
        }
    }

    fn for_socket_with_addr_read(
        len: u32,
        flags: i32,
        addr_buffer: Vec<u8>,
    ) -> AsyncHostResult<Self> {
        let buffer = vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?];
        Self::for_socket_with_addr(IO_RESULT_READ_EVENT, buffer, flags, addr_buffer)
    }

    fn for_socket_with_addr_write(
        buffer: Vec<u8>,
        flags: i32,
        addr_buffer: Vec<u8>,
    ) -> AsyncHostResult<Self> {
        Self::for_socket_with_addr(IO_RESULT_WRITE_EVENT, buffer, flags, addr_buffer)
    }

    fn for_socket_with_addr(
        event: i32,
        buffer: Vec<u8>,
        flags: i32,
        addr_buffer: Vec<u8>,
    ) -> AsyncHostResult<Self> {
        let addr_len = i32::try_from(addr_buffer.len()).map_err(|_| AsyncHostError::Fault)?;
        Ok(Self {
            overlapped: Self::zeroed_overlapped(),
            completion_event: None,
            completion_port: None,
            kind: HostIoKind::SocketWithAddr,
            event,
            buffer,
            read_dir_changes_buffer: None,
            socket_flags: flags as u32,
            addr_buffer,
            addr_len,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            pending: None,
        })
    }

    fn for_connect(addr_buffer: Vec<u8>) -> Self {
        Self {
            overlapped: Self::zeroed_overlapped(),
            completion_event: None,
            completion_port: None,
            kind: HostIoKind::Connect,
            event: IO_RESULT_WRITE_EVENT,
            buffer: Vec::new(),
            read_dir_changes_buffer: None,
            socket_flags: 0,
            addr_buffer,
            addr_len: 0,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            pending: None,
        }
    }

    fn for_accept(addr_len: u32) -> AsyncHostResult<Self> {
        let addr_len_usize = usize::try_from(addr_len).map_err(|_| AsyncHostError::Fault)?;
        let accept_addr_len = addr_len_usize
            .checked_add(16)
            .ok_or(AsyncHostError::Fault)?;
        let accept_buffer_len = accept_addr_len
            .checked_mul(2)
            .ok_or(AsyncHostError::Fault)?;
        Ok(Self {
            overlapped: Self::zeroed_overlapped(),
            completion_event: None,
            completion_port: None,
            kind: HostIoKind::Accept,
            event: IO_RESULT_READ_EVENT,
            buffer: Vec::new(),
            read_dir_changes_buffer: None,
            socket_flags: 0,
            addr_buffer: Vec::new(),
            addr_len: i32::try_from(addr_len).map_err(|_| AsyncHostError::Fault)?,
            accept_buffer: vec![0; accept_buffer_len],
            accept_bytes_received: 0,
            pending: None,
        })
    }

    fn for_read_dir_changes(buffer: WindowsWatcherBufferLease) -> Self {
        Self {
            overlapped: Self::zeroed_overlapped(),
            completion_event: None,
            completion_port: None,
            kind: HostIoKind::ReadDirChanges,
            event: IO_RESULT_READ_EVENT,
            buffer: Vec::new(),
            read_dir_changes_buffer: Some(buffer),
            socket_flags: 0,
            addr_buffer: Vec::new(),
            addr_len: 0,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            pending: None,
        }
    }

    fn overlapped_ptr(&mut self) -> *mut windows_sys::Win32::System::IO::OVERLAPPED {
        &mut self.overlapped
    }

    fn overlapped_addr(&mut self) -> OverlappedAddr {
        OverlappedAddr::from_ptr(self.overlapped_ptr())
    }

    fn validate_completed_read(&self) -> AsyncHostResult<()> {
        if self.event != IO_RESULT_READ_EVENT {
            return Err(AsyncHostError::Inval);
        }
        if self.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        Ok(())
    }

    fn copy_read_payload(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        dst: u32,
        offset: u32,
        len: u32,
    ) -> AsyncHostResult<()> {
        let bytes_transferred = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let data = self
            .buffer
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
        if matches!(
            self.kind,
            HostIoKind::SocketWithAddr | HostIoKind::ReadDirChanges
        ) {
            return Err(AsyncHostError::Inval);
        }
        self.copy_read_payload(memory, dst, offset, len)
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
        if self.kind != HostIoKind::SocketWithAddr {
            return Err(AsyncHostError::Inval);
        }
        self.copy_read_payload(memory, dst, offset, len)?;
        let actual_addr_len = usize::try_from(self.addr_len).map_err(|_| AsyncHostError::Fault)?;
        let addr_data = self
            .addr_buffer
            .get(..actual_addr_len)
            .ok_or(AsyncHostError::Fault)?;
        memory.write_with_capacity(addr, addr_len, addr_data)?;
        Ok(())
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

    fn cancel_pending(&mut self) -> AsyncHostResult<i32> {
        self.request_cancel()?;
        let Some(pending) = &self.pending else {
            return Ok(0);
        };
        if self.completion_port.is_some() {
            // Cancellation does not retire the packet's pointer identity.
            // The guest must still poll before reusing/freeing this result.
            return Ok(1);
        }
        let raw = raw_overlapped_handle(&pending.resource)?;
        let mut bytes = 0;
        unsafe {
            windows_sys::Win32::System::IO::GetOverlappedResult(
                raw,
                self.overlapped_ptr(),
                &mut bytes,
                0,
            );
        }
        if self.native_completed() {
            self.clear_pending();
            Ok(0)
        } else {
            Ok(1)
        }
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
    ) -> AsyncHostResult<Vec<u8>> {
        let ptr = ptr.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        Ok(memory.read_exact(ptr, len)?.to_vec())
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
        let addr_buffer = memory.read_exact(addr, addr_len)?.to_vec();
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
        let addr_buffer = memory.read_exact(addr, addr_len)?.to_vec();
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
        let addr_buffer = memory.read_exact(addr, addr_len)?.to_vec();
        self.insert_io_result(HostIoResult::for_connect(addr_buffer))
    }

    pub(crate) fn make_accept_io_result(&self, addr_len: u32) -> AsyncHostResult<u64> {
        self.insert_io_result(HostIoResult::for_accept(addr_len)?)
    }

    pub(crate) fn make_read_dir_changes_io_result(
        &self,
        buffer_handle: u64,
    ) -> AsyncHostResult<u64> {
        let buffer = self.lease_windows_watcher_buffer(buffer_handle)?;
        self.insert_io_result(HostIoResult::for_read_dir_changes(buffer))
    }

    fn insert_io_result(&self, result: HostIoResult) -> AsyncHostResult<u64> {
        let mut result = Box::new(result);
        let overlapped = result.overlapped_addr();
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
        if result.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        let mut result = io_results
            .io_results
            .remove(key)
            .ok_or(AsyncHostError::Badf)?;
        let overlapped = result.overlapped_addr();
        io_results.io_results_by_overlapped.remove(&overlapped);
        let buffer = result.read_dir_changes_buffer.take();
        drop(io_results);
        if let Some(buffer) = buffer {
            self.restore_windows_watcher_buffer(buffer);
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
        result.validate_pending_resource(file)?;
        let status = result.cancel_pending();
        let buffer = (!result.is_pending())
            .then(|| result.read_dir_changes_buffer.take())
            .flatten();
        drop(io_results);
        drop(handles);
        if let Some(buffer) = buffer {
            self.restore_windows_watcher_buffer(buffer);
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
        result.validate_pending_resource(file)?;
        if result.is_pending() && result.completion_port.is_some() {
            return Err(AsyncHostError::Native(
                windows_sys::Win32::Foundation::ERROR_IO_INCOMPLETE as i32,
            ));
        }
        let mut bytes_transferred = 0;
        let status = if unsafe {
            GetOverlappedResult(
                raw_handle,
                result.overlapped_ptr(),
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
            if result.native_completed() {
                result.clear_pending();
            } else {
                return Err(error);
            }
            Err(error)
        } else {
            result.clear_pending();
            i32::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)
        };
        if result.kind == HostIoKind::ReadDirChanges {
            let completed_len = status
                .as_ref()
                .ok()
                .and_then(|len| usize::try_from(*len).ok())
                .unwrap_or(0);
            result
                .read_dir_changes_buffer
                .as_mut()
                .ok_or(AsyncHostError::Inval)?
                .buffer_mut()
                .complete_read(completed_len)?;
        }
        let buffer = result.read_dir_changes_buffer.take();
        drop(io_results);
        drop(handles);
        if let Some(buffer) = buffer {
            self.restore_windows_watcher_buffer(buffer);
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
        use windows_sys::Win32::Networking::WinSock as ws;

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
        if result.is_pending() || result.event != IO_RESULT_READ_EVENT {
            return Err(AsyncHostError::Inval);
        }
        result.kind.validate_resource(file)?;
        result.prepare_submission(port)?;
        let mut bytes_transferred = 0;
        let outcome = match result.kind {
            HostIoKind::File => {
                let overlapped = result.overlapped_ptr();
                // The result owns the buffer/OVERLAPPED and retains the resource
                // until completion before exposing the result to guest code.
                unsafe {
                    self.filesystem
                        .read_overlapped(file, &mut result.buffer, overlapped)
                }
            }
            HostIoKind::Socket => {
                let buffer = socket_buffer(&mut result.buffer)?;
                let success = unsafe {
                    ws::WSARecv(
                        file.as_socket()?.as_raw_socket() as usize,
                        &buffer,
                        1,
                        &mut bytes_transferred,
                        &mut result.socket_flags,
                        result.overlapped_ptr(),
                        None,
                    ) == 0
                };
                if success {
                    Ok(bytes_transferred as usize)
                } else {
                    Err(AsyncHostError::Native(last_wsa_errno()))
                }
            }
            HostIoKind::SocketWithAddr => {
                let buffer = socket_buffer(&mut result.buffer)?;
                result.addr_len =
                    i32::try_from(result.addr_buffer.len()).map_err(|_| AsyncHostError::Fault)?;
                let addr_len = std::ptr::addr_of_mut!(result.addr_len);
                let flags = std::ptr::addr_of_mut!(result.socket_flags);
                let addr = result.addr_buffer.as_mut_ptr().cast::<ws::SOCKADDR>();
                let overlapped = result.overlapped_ptr();
                let success = unsafe {
                    ws::WSARecvFrom(
                        file.as_socket()?.as_raw_socket() as usize,
                        &buffer,
                        1,
                        &mut bytes_transferred,
                        flags,
                        addr,
                        addr_len,
                        overlapped,
                        None,
                    ) == 0
                };
                if success {
                    Ok(bytes_transferred as usize)
                } else {
                    Err(AsyncHostError::Native(last_wsa_errno()))
                }
            }
            HostIoKind::Connect | HostIoKind::Accept | HostIoKind::ReadDirChanges => {
                return Err(AsyncHostError::Inval);
            }
        };
        match outcome {
            Ok(bytes) => i32::try_from(bytes).map_err(|_| AsyncHostError::Fault),
            Err(AsyncHostError::Native(errno)) if errno == ERROR_HANDLE_EOF as i32 => Ok(0),
            Err(error) => {
                if matches!(error, AsyncHostError::Native(errno) if errno == ERROR_IO_PENDING as i32)
                {
                    result.mark_pending(Arc::clone(file))?;
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
        if result.kind != HostIoKind::ReadDirChanges || result.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        result.kind.validate_resource(file)?;
        result.prepare_submission(port)?;
        self.filesystem
            .authorize(FsOperation::Watch(FsTarget::File(file.file_grant())))?;
        let overlapped = result.overlapped_ptr();
        let buffer = result
            .read_dir_changes_buffer
            .as_mut()
            .ok_or(AsyncHostError::Inval)?
            .buffer_mut();
        // The result keeps the leased buffer, OVERLAPPED, and Resource alive
        // until completion; submission only selects the native event layout.
        unsafe { buffer.submit(file, overlapped)? };

        // Directory change notification reports successful submission, not
        // synchronous completion. Match native async by
        // exposing the operation as pending even though the call returned TRUE.
        result.mark_pending(Arc::clone(file))?;
        Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
    }

    pub(crate) fn write_io_result(
        &self,
        fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::ERROR_IO_PENDING;
        use windows_sys::Win32::Networking::WinSock as ws;

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
        if result.is_pending() || result.event != IO_RESULT_WRITE_EVENT {
            return Err(AsyncHostError::Inval);
        }
        result.kind.validate_resource(file)?;
        result.prepare_submission(port)?;
        if result.kind == HostIoKind::SocketWithAddr {
            self.network.check_connect(&result.addr_buffer)?;
        }
        let mut bytes_transferred = 0;
        let outcome = match result.kind {
            HostIoKind::File => {
                let overlapped = result.overlapped_ptr();
                // The result owns the buffer/OVERLAPPED and retains the resource
                // until completion before exposing the result to guest code.
                unsafe {
                    self.filesystem
                        .write_overlapped(file, &result.buffer, overlapped)
                }
            }
            HostIoKind::Socket => {
                let buffer = socket_buffer(&mut result.buffer)?;
                let success = unsafe {
                    ws::WSASend(
                        file.as_socket()?.as_raw_socket() as usize,
                        &buffer,
                        1,
                        &mut bytes_transferred,
                        result.socket_flags,
                        result.overlapped_ptr(),
                        None,
                    ) == 0
                };
                if success {
                    Ok(bytes_transferred as usize)
                } else {
                    Err(AsyncHostError::Native(last_wsa_errno()))
                }
            }
            HostIoKind::SocketWithAddr => {
                let buffer = socket_buffer(&mut result.buffer)?;
                let success = unsafe {
                    ws::WSASendTo(
                        file.as_socket()?.as_raw_socket() as usize,
                        &buffer,
                        1,
                        &mut bytes_transferred,
                        result.socket_flags,
                        result.addr_buffer.as_ptr().cast::<ws::SOCKADDR>(),
                        result.addr_len,
                        result.overlapped_ptr(),
                        None,
                    ) == 0
                };
                if success {
                    Ok(bytes_transferred as usize)
                } else {
                    Err(AsyncHostError::Native(last_wsa_errno()))
                }
            }
            HostIoKind::Connect | HostIoKind::Accept | HostIoKind::ReadDirChanges => {
                return Err(AsyncHostError::Inval);
            }
        };
        match outcome {
            Ok(bytes) => i32::try_from(bytes).map_err(|_| AsyncHostError::Fault),
            Err(error) => {
                if matches!(error, AsyncHostError::Native(errno) if errno == ERROR_IO_PENDING as i32)
                {
                    result.mark_pending(Arc::clone(file))?;
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
        use windows_sys::Win32::Networking::WinSock as ws;

        let handles = self.resources.borrow();
        let file = handles.acquire_resource_of_class(fd_handle, ResourceClass::TcpSocket)?;
        let raw_socket = file.as_socket()?.as_raw_socket();
        let result_key = key_from_handle(result_handle);
        let mut io_results = self.io_results.borrow_mut();
        let port = io_results.completion_port(&file);
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.kind != HostIoKind::Connect || result.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        result.prepare_submission(port)?;
        self.network.check_connect(&result.addr_buffer)?;

        bind_any_for_connect(raw_socket, &result.addr_buffer)?;
        let connect_ex = get_wsa_extension::<ws::LPFN_CONNECTEX>(raw_socket, &ws::WSAID_CONNECTEX)?
            .ok_or(AsyncHostError::Inval)?;
        let addr_len = socket_addr_len(&result.addr_buffer)?;
        let success = unsafe {
            connect_ex(
                raw_socket as usize,
                result.addr_buffer.as_ptr().cast::<ws::SOCKADDR>(),
                addr_len,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                result.overlapped_ptr(),
            )
        };
        if success != 0 {
            Ok(1)
        } else {
            let errno = last_wsa_errno();
            if errno == windows_sys::Win32::Foundation::ERROR_IO_PENDING as i32 {
                result.mark_pending(file)?;
            }
            Err(AsyncHostError::Native(errno))
        }
    }

    pub(crate) fn setup_connected_socket(&self, fd_handle: HostHandle) -> AsyncHostResult<()> {
        use windows_sys::Win32::Networking::WinSock as ws;

        self.with_resource_of_class(fd_handle, ResourceClass::TcpSocket, |file| {
            if unsafe {
                ws::setsockopt(
                    file.as_socket()?.as_raw_socket() as usize,
                    ws::SOL_SOCKET,
                    ws::SO_UPDATE_CONNECT_CONTEXT,
                    std::ptr::null(),
                    0,
                )
            } == ws::SOCKET_ERROR
            {
                Err(AsyncHostError::Native(last_wsa_errno()))
            } else {
                Ok(())
            }
        })
    }

    pub(crate) fn accept_io_result(
        &self,
        server_fd_handle: HostHandle,
        conn_fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Networking::WinSock as ws;

        let handles = self.resources.borrow();
        let server_file =
            handles.acquire_resource_of_class(server_fd_handle, ResourceClass::TcpSocket)?;
        let conn_file =
            handles.acquire_resource_of_class(conn_fd_handle, ResourceClass::TcpSocket)?;
        let server_socket = server_file.as_socket()?.as_raw_socket();
        let conn_socket = conn_file.as_socket()?.as_raw_socket();
        let result_key = key_from_handle(result_handle);
        let mut io_results = self.io_results.borrow_mut();
        let port = io_results.completion_port(&server_file);
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.kind != HostIoKind::Accept || result.is_pending() {
            return Err(AsyncHostError::Inval);
        }

        result.prepare_submission(port)?;
        let accept_ex = get_wsa_extension::<ws::LPFN_ACCEPTEX>(server_socket, &ws::WSAID_ACCEPTEX)?
            .ok_or(AsyncHostError::Inval)?;
        let addr_len = u32::try_from(result.addr_len).map_err(|_| AsyncHostError::Fault)?;
        let accept_addr_len = addr_len.checked_add(16).ok_or(AsyncHostError::Fault)?;
        let success = unsafe {
            accept_ex(
                server_socket as usize,
                conn_socket as usize,
                result.accept_buffer.as_mut_ptr().cast(),
                0,
                accept_addr_len,
                accept_addr_len,
                &mut result.accept_bytes_received,
                result.overlapped_ptr(),
            )
        };
        if success != 0 {
            Ok(1)
        } else {
            let errno = last_wsa_errno();
            if errno == windows_sys::Win32::Foundation::ERROR_IO_PENDING as i32 {
                result.mark_pending_with_close_guard(server_file, conn_file)?;
            }
            Err(AsyncHostError::Native(errno))
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
        if result.kind != HostIoKind::Accept || result.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        let addr_len = usize::try_from(result.addr_len).map_err(|_| AsyncHostError::Fault)?;
        let offset = addr_len.checked_add(16).ok_or(AsyncHostError::Fault)?;
        let end = offset.checked_add(addr_len).ok_or(AsyncHostError::Fault)?;
        let addr = result
            .accept_buffer
            .get(offset..end)
            .ok_or(AsyncHostError::Fault)?;
        memory.write_with_capacity(dst, dst_len, addr)?;
        Ok(())
    }

    pub(crate) fn setup_accepted_socket(
        &self,
        listen_fd_handle: HostHandle,
        accept_fd_handle: HostHandle,
    ) -> AsyncHostResult<()> {
        use windows_sys::Win32::Networking::WinSock as ws;

        let handles = self.resources.borrow();
        let listen_file = handles.resource_of_class(listen_fd_handle, ResourceClass::TcpSocket)?;
        let accept_file = handles.resource_of_class(accept_fd_handle, ResourceClass::TcpSocket)?;
        let listen_socket = listen_file.as_socket()?.as_raw_socket() as usize;
        if unsafe {
            ws::setsockopt(
                accept_file.as_socket()?.as_raw_socket() as usize,
                ws::SOL_SOCKET,
                ws::SO_UPDATE_ACCEPT_CONTEXT,
                (&listen_socket as *const usize).cast(),
                std::mem::size_of_val(&listen_socket) as i32,
            )
        } == ws::SOCKET_ERROR
        {
            Err(AsyncHostError::Native(last_wsa_errno()))
        } else {
            Ok(())
        }
    }
}

fn last_errno() -> i32 {
    unsafe { windows_sys::Win32::Foundation::GetLastError() as i32 }
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(last_errno())
}

fn last_wsa_errno() -> i32 {
    use windows_sys::Win32::Foundation::{SetLastError, WIN32_ERROR};
    use windows_sys::Win32::Networking::WinSock;

    let errno = unsafe { WinSock::WSAGetLastError() };
    unsafe {
        SetLastError(errno as WIN32_ERROR);
    }
    errno
}

fn socket_buffer(
    buffer: &mut [u8],
) -> AsyncHostResult<windows_sys::Win32::Networking::WinSock::WSABUF> {
    Ok(windows_sys::Win32::Networking::WinSock::WSABUF {
        len: u32::try_from(buffer.len()).map_err(|_| AsyncHostError::Fault)?,
        buf: buffer.as_mut_ptr().cast(),
    })
}

fn socket_addr_family(addr: &[u8]) -> AsyncHostResult<u16> {
    use windows_sys::Win32::Networking::WinSock;

    if addr.len() < std::mem::size_of::<WinSock::SOCKADDR>() {
        return Err(AsyncHostError::Fault);
    }
    Ok(unsafe { addr.as_ptr().cast::<WinSock::SOCKADDR>().read_unaligned() }.sa_family)
}

fn socket_addr_len(addr: &[u8]) -> AsyncHostResult<i32> {
    use windows_sys::Win32::Networking::WinSock;

    let len = match socket_addr_family(addr)? {
        WinSock::AF_INET => std::mem::size_of::<WinSock::SOCKADDR_IN>(),
        WinSock::AF_INET6 => std::mem::size_of::<WinSock::SOCKADDR_IN6>(),
        _ => return Err(AsyncHostError::Inval),
    };
    if addr.len() < len {
        return Err(AsyncHostError::Fault);
    }
    i32::try_from(len).map_err(|_| AsyncHostError::Fault)
}

fn bind_any_for_connect(raw_socket: RawSocket, remote_addr: &[u8]) -> AsyncHostResult<()> {
    use windows_sys::Win32::Networking::WinSock;

    let result = match socket_addr_family(remote_addr)? {
        WinSock::AF_INET => {
            let mut addr = unsafe { std::mem::zeroed::<WinSock::SOCKADDR_IN>() };
            addr.sin_family = WinSock::AF_INET;
            unsafe {
                WinSock::bind(
                    raw_socket as usize,
                    (&addr as *const WinSock::SOCKADDR_IN).cast::<WinSock::SOCKADDR>(),
                    std::mem::size_of_val(&addr) as i32,
                )
            }
        }
        WinSock::AF_INET6 => {
            let mut addr = unsafe { std::mem::zeroed::<WinSock::SOCKADDR_IN6>() };
            addr.sin6_family = WinSock::AF_INET6;
            unsafe {
                WinSock::bind(
                    raw_socket as usize,
                    (&addr as *const WinSock::SOCKADDR_IN6).cast::<WinSock::SOCKADDR>(),
                    std::mem::size_of_val(&addr) as i32,
                )
            }
        }
        _ => return Err(AsyncHostError::Inval),
    };
    if result == WinSock::SOCKET_ERROR {
        let errno = last_wsa_errno();
        if errno == WinSock::WSAEINVAL {
            unreachable!(
                "moonbitlang/async Tcp::connect creates a fresh unbound socket before ConnectEx"
            );
        }
        Err(AsyncHostError::Native(errno))
    } else {
        Ok(())
    }
}

fn get_wsa_extension<T: Copy>(
    raw_socket: RawSocket,
    guid: &windows_sys::core::GUID,
) -> AsyncHostResult<T> {
    use windows_sys::Win32::Networking::WinSock;

    debug_assert_eq!(
        std::mem::size_of::<T>(),
        std::mem::size_of::<*mut std::ffi::c_void>()
    );
    let mut extension = std::ptr::null_mut::<std::ffi::c_void>();
    let mut bytes_returned = 0;
    let ret = unsafe {
        WinSock::WSAIoctl(
            raw_socket as usize,
            WinSock::SIO_GET_EXTENSION_FUNCTION_POINTER,
            (guid as *const windows_sys::core::GUID).cast(),
            std::mem::size_of_val(guid) as u32,
            (&mut extension as *mut *mut std::ffi::c_void).cast(),
            std::mem::size_of_val(&extension) as u32,
            &mut bytes_returned,
            std::ptr::null_mut(),
            None,
        )
    };
    if ret == WinSock::SOCKET_ERROR {
        Err(AsyncHostError::Native(last_wsa_errno()))
    } else {
        Ok(unsafe { std::mem::transmute_copy(&extension) })
    }
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
