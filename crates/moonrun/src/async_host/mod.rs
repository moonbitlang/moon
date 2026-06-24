// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Moonrun-owned async wasm host state.
//!
//! This module owns one V8 host instance's runtime state: handle tables, host
//! workers, guest memory helpers, and host poll instances.
//!
//! Native async multiplexes pollable IO through epoll, kqueue, or IOCP, with
//! thread-pool completions as one registered event
//! source. The wasm ABI exposes that same shape: MoonBit owns event-loop
//! scheduling and Rust owns the OS poller behind opaque poll handles.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use slotmap::{Key, KeyData, SlotMap, new_key_type};

#[cfg(unix)]
use crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier;
use crate::async_sys::internal::event_loop::{
    poll::{self, PollInstance},
    thread_pool::{
        self, HostFile, HostFileTable, HostHandle, HostWorkerHandle, HostWorkerJob, Job,
    },
};
use crate::async_sys::internal::fd_util::stub::RawFd;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
compile_error!("moonrun async wasm host currently supports only Linux, macOS, and Windows hosts");

#[cfg(not(target_endian = "little"))]
compile_error!("moonrun async wasm host requires little-endian host memory");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncHostError {
    Fault,
    Inval,
    Badf,
    Native(i32),
}

pub(crate) type AsyncHostResult<T> = Result<T, AsyncHostError>;
pub(crate) const INVALID_HOST_HANDLE: u64 = 0;
pub(crate) type HostCBuffer = Arc<Mutex<Box<[u8]>>>;

#[cfg(unix)]
mod native_errno {
    pub(crate) const BADF: i32 = libc::EBADF;
    pub(crate) const FAULT: i32 = libc::EFAULT;
    pub(crate) const INVAL: i32 = libc::EINVAL;
}

#[cfg(windows)]
mod native_errno {
    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_ADDRESS, ERROR_INVALID_HANDLE, ERROR_INVALID_PARAMETER,
    };

    pub(crate) const BADF: i32 = ERROR_INVALID_HANDLE as i32;
    pub(crate) const FAULT: i32 = ERROR_INVALID_ADDRESS as i32;
    pub(crate) const INVAL: i32 = ERROR_INVALID_PARAMETER as i32;
}

impl AsyncHostError {
    pub(crate) fn errno(self) -> i32 {
        match self {
            Self::Fault => native_errno::FAULT,
            Self::Inval => native_errno::INVAL,
            Self::Badf => native_errno::BADF,
            Self::Native(errno) => errno,
        }
    }
}

pub(crate) trait GuestMemory {
    fn bytes(&self) -> &[u8];

    fn bytes_mut(&mut self) -> &mut [u8];

    fn read_exact(&self, offset: i32, len: i32) -> AsyncHostResult<&[u8]> {
        let (offset, end) = guest_bounds(offset, len)?;
        self.bytes().get(offset..end).ok_or(AsyncHostError::Fault)
    }

    fn read_exact_mut(&mut self, offset: i32, len: i32) -> AsyncHostResult<&mut [u8]> {
        let (offset, end) = guest_bounds(offset, len)?;
        self.bytes_mut()
            .get_mut(offset..end)
            .ok_or(AsyncHostError::Fault)
    }

    fn write_exact(&mut self, offset: i32, data: &[u8]) -> AsyncHostResult<()> {
        let len = i32::try_from(data.len()).map_err(|_| AsyncHostError::Fault)?;
        let dst = self.read_exact_mut(offset, len)?;
        dst.copy_from_slice(data);
        Ok(())
    }

    fn write_with_capacity(
        &mut self,
        offset: i32,
        capacity: i32,
        data: &[u8],
    ) -> AsyncHostResult<()> {
        let data_len = i32::try_from(data.len()).map_err(|_| AsyncHostError::Fault)?;
        if data_len > capacity {
            return Err(AsyncHostError::Fault);
        }
        let dst = self.read_exact_mut(offset, capacity)?;
        dst[..data.len()].copy_from_slice(data);
        Ok(())
    }

    fn write_u64_le(&mut self, offset: i32, value: u64) -> AsyncHostResult<()> {
        self.write_exact(offset, &value.to_le_bytes())
    }
}

fn guest_bounds(offset: i32, len: i32) -> AsyncHostResult<(usize, usize)> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let end = offset.checked_add(len).ok_or(AsyncHostError::Fault)?;
    Ok((offset, end))
}

pub(crate) fn read_u16(memory: &[u8], offset: i32, len: i32) -> AsyncHostResult<&[u16]> {
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let (offset, _) = u16_bounds(memory.len(), offset, len)?;
    if len == 0 {
        return Ok(&[]);
    }
    let ptr = unsafe { memory.as_ptr().add(offset).cast::<u16>() };
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
}

pub(crate) fn write_u16(memory: &mut [u8], offset: i32, data: &[u16]) -> AsyncHostResult<()> {
    let (offset, _) = u16_bounds(memory.len(), offset, data.len())?;
    if data.is_empty() {
        return Ok(());
    }
    let ptr = unsafe { memory.as_mut_ptr().add(offset).cast::<u16>() };
    let dst = unsafe { std::slice::from_raw_parts_mut(ptr, data.len()) };
    dst.copy_from_slice(data);
    Ok(())
}

fn u16_bounds(memory_len: usize, offset: i32, len: usize) -> AsyncHostResult<(usize, usize)> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    if len != 0 && offset % std::mem::align_of::<u16>() != 0 {
        return Err(AsyncHostError::Fault);
    }
    let byte_len = len
        .checked_mul(std::mem::size_of::<u16>())
        .ok_or(AsyncHostError::Fault)?;
    let end = offset.checked_add(byte_len).ok_or(AsyncHostError::Fault)?;
    if end > memory_len {
        return Err(AsyncHostError::Fault);
    }
    Ok((offset, end))
}

impl GuestMemory for [u8] {
    fn bytes(&self) -> &[u8] {
        self
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        self
    }
}

impl<const N: usize> GuestMemory for [u8; N] {
    fn bytes(&self) -> &[u8] {
        self.as_slice()
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

impl HostFileTable for SlotMap<HostFileKey, HostFile> {
    fn insert_file(&mut self, file: RawFd) -> AsyncHostResult<u64> {
        Ok(handle_from_key(self.insert(HostFile::new(file))))
    }

    fn is_invalid_file_handle(&self, handle: HostHandle) -> bool {
        self.get(key_from_handle::<HostFileKey>(handle))
            .is_some_and(HostFile::is_invalid)
    }

    #[cfg(windows)]
    fn borrowed_raw_file(&mut self, handle: HostHandle) -> AsyncHostResult<RawFd> {
        let file = self
            .get(key_from_handle::<HostFileKey>(handle))
            .ok_or(AsyncHostError::Badf)?;
        if file.is_invalid() {
            return Err(AsyncHostError::Badf);
        }
        Ok(file.raw_fd())
    }

    fn with_raw_file<U>(
        &mut self,
        handle: u64,
        f: impl FnOnce(RawFd) -> AsyncHostResult<U>,
    ) -> AsyncHostResult<U> {
        let file = self
            .get_mut(key_from_handle::<HostFileKey>(handle))
            .ok_or(AsyncHostError::Badf)?;
        if file.is_invalid() {
            return Err(AsyncHostError::Badf);
        }
        f(file.raw_fd())
    }

    fn with_host_file_mut<U>(
        &mut self,
        handle: u64,
        f: impl FnOnce(&mut HostFile) -> AsyncHostResult<U>,
    ) -> AsyncHostResult<U> {
        let file = self
            .get_mut(key_from_handle::<HostFileKey>(handle))
            .ok_or(AsyncHostError::Badf)?;
        if file.is_invalid() {
            return Err(AsyncHostError::Badf);
        }
        f(file)
    }
}

new_key_type! {
    pub(crate) struct HostCBufferKey;
    pub(crate) struct HostEventKey;
    pub(crate) struct HostFileKey;
    pub(crate) struct HostIoResultKey;
    pub(crate) struct HostJobKey;
    pub(crate) struct HostPollKey;
    pub(crate) struct HostWorkerKey;
}

fn handle_from_key(key: impl Key) -> u64 {
    key.data().as_ffi()
}

fn key_from_handle<K: Key>(handle: u64) -> K {
    KeyData::from_ffi(handle).into()
}

struct AsyncHostState {
    errno: i32,
    c_buffers: SlotMap<HostCBufferKey, HostCBuffer>,
    #[cfg(windows)]
    io_results: SlotMap<HostIoResultKey, Box<HostIoResult>>,
    #[cfg(windows)]
    io_results_by_overlapped: HashMap<usize, HostIoResultKey>,
    events: SlotMap<HostEventKey, HostEvent>,
    jobs: SlotMap<HostJobKey, Option<Job>>,
    polls: SlotMap<HostPollKey, Arc<Mutex<HostPoll>>>,
    files: SlotMap<HostFileKey, HostFile>,
    invalid_file: HostFileKey,
    workers: SlotMap<HostWorkerKey, HostWorkerHandle>,
    #[cfg(unix)]
    completion_notifier: Option<Arc<ThreadPoolCompletionNotifier>>,
    #[cfg(unix)]
    completion_source: Option<HostHandle>,
    #[cfg(windows)]
    completion_port: Option<poll::CompletionPort>,
}

impl Default for AsyncHostState {
    fn default() -> Self {
        let mut files = SlotMap::with_key();
        let invalid_file = files.insert(HostFile::invalid());
        Self {
            errno: 0,
            c_buffers: SlotMap::with_key(),
            #[cfg(windows)]
            io_results: SlotMap::with_key(),
            #[cfg(windows)]
            io_results_by_overlapped: HashMap::new(),
            events: SlotMap::with_key(),
            jobs: SlotMap::with_key(),
            polls: SlotMap::with_key(),
            files,
            invalid_file,
            workers: SlotMap::with_key(),
            #[cfg(unix)]
            completion_notifier: None,
            #[cfg(unix)]
            completion_source: None,
            #[cfg(windows)]
            completion_port: None,
        }
    }
}

impl AsyncHostState {
    fn invalid_fd(&self) -> HostHandle {
        handle_from_key(self.invalid_file)
    }

    fn file(&self, handle: HostHandle) -> AsyncHostResult<&HostFile> {
        let file = self
            .files
            .get(key_from_handle::<HostFileKey>(handle))
            .ok_or(AsyncHostError::Badf)?;
        if file.is_invalid() {
            return Err(AsyncHostError::Badf);
        }
        Ok(file)
    }

    fn file_mut(&mut self, handle: HostHandle) -> AsyncHostResult<&mut HostFile> {
        let file = self
            .files
            .get_mut(key_from_handle::<HostFileKey>(handle))
            .ok_or(AsyncHostError::Badf)?;
        if file.is_invalid() {
            return Err(AsyncHostError::Badf);
        }
        Ok(file)
    }

    fn remove_file(&mut self, handle: HostHandle) -> AsyncHostResult<HostFile> {
        let key = key_from_handle::<HostFileKey>(handle);
        if key == self.invalid_file {
            return Err(AsyncHostError::Badf);
        }
        self.files.remove(key).ok_or(AsyncHostError::Badf)
    }

    fn take_workers(&mut self) -> Vec<HostWorkerHandle> {
        std::mem::take(&mut self.workers)
            .into_iter()
            .map(|(_, worker)| worker)
            .collect()
    }

    #[cfg(windows)]
    fn drain_pending_io_for_raw_fd(&mut self, raw_fd: RawFd) -> AsyncHostResult<()> {
        let pending: Vec<_> = self
            .io_results
            .iter()
            .filter_map(|(key, result)| (result.pending_raw_fd() == Some(raw_fd)).then_some(key))
            .collect();
        for key in pending {
            if let Some(result) = self.io_results.get_mut(key) {
                result.cancel_and_drain_pending()?;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
struct HostPoll {
    instance: PollInstance,
    registered_fds: HashMap<isize, HostHandle>,
    #[cfg(unix)]
    completion_notifier: Option<Arc<ThreadPoolCompletionNotifier>>,
    event_generation: u64,
    event_handles: Vec<HostEventKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HostEvent {
    poll: HostPollKey,
    generation: u64,
    index: i32,
    fd_handle: Option<HostHandle>,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostIoDirection {
    Read,
    Write,
}

#[cfg(windows)]
struct HostIoResult {
    overlapped: windows_sys::Win32::System::IO::OVERLAPPED,
    event: i32,
    // Native async retains the MoonBit buffer object. The V8 host instead keeps
    // a stable host buffer and copies at explicit FFI submit/complete points.
    buffer: Vec<u8>,
    guest_offset: i32,
    direction: Option<HostIoDirection>,
    pending_raw_fd: Option<RawFd>,
}

#[cfg(windows)]
unsafe impl Send for HostIoResult {}

#[cfg(windows)]
impl HostIoResult {
    fn for_file(event: i32, buffer: Vec<u8>, guest_offset: i32, position: i64) -> Self {
        let overlapped =
            std::mem::MaybeUninit::<windows_sys::Win32::System::IO::OVERLAPPED>::zeroed();
        let mut overlapped = unsafe { overlapped.assume_init() };
        overlapped.Anonymous.Anonymous.Offset = position as u32;
        overlapped.Anonymous.Anonymous.OffsetHigh = (position >> 32) as u32;
        Self {
            overlapped,
            event,
            buffer,
            guest_offset,
            direction: None,
            pending_raw_fd: None,
        }
    }

    fn overlapped_ptr(&mut self) -> *mut windows_sys::Win32::System::IO::OVERLAPPED {
        &mut self.overlapped
    }

    fn copy_read_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        bytes_transferred: i32,
    ) -> AsyncHostResult<()> {
        if self.direction != Some(HostIoDirection::Read) {
            return Ok(());
        }
        let len = usize::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)?;
        let data = self.buffer.get(..len).ok_or(AsyncHostError::Fault)?;
        memory.write_exact(self.guest_offset, data)
    }

    fn pending_raw_fd(&self) -> Option<RawFd> {
        self.pending_raw_fd
    }

    fn mark_pending(&mut self, raw_fd: RawFd) -> AsyncHostResult<()> {
        if self.pending_raw_fd.is_some() {
            return Err(AsyncHostError::Inval);
        }
        self.pending_raw_fd = Some(raw_fd);
        Ok(())
    }

    fn clear_pending(&mut self) {
        self.pending_raw_fd = None;
    }

    fn cancel_pending(&mut self) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::{ERROR_IO_INCOMPLETE, ERROR_NOT_FOUND};
        use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult};

        let Some(raw_fd) = self.pending_raw_fd else {
            return Ok(0);
        };
        let overlapped = self.overlapped_ptr();
        if unsafe { CancelIoEx(raw_fd, overlapped) } == 0 {
            let errno = last_errno();
            if errno != ERROR_NOT_FOUND as i32 {
                return Err(AsyncHostError::Native(errno));
            }
            self.clear_pending();
            return Ok(0);
        }

        let mut bytes_transferred = 0;
        if unsafe { GetOverlappedResult(raw_fd, overlapped, &mut bytes_transferred, 0) } != 0 {
            self.clear_pending();
            return Ok(0);
        }
        if last_errno() == ERROR_IO_INCOMPLETE as i32 {
            Ok(1)
        } else {
            self.clear_pending();
            Ok(0)
        }
    }

    fn cancel_and_drain_pending(&mut self) -> AsyncHostResult<()> {
        use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, ERROR_OPERATION_ABORTED};
        use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult};

        let Some(raw_fd) = self.pending_raw_fd else {
            return Ok(());
        };
        let overlapped = self.overlapped_ptr();
        if unsafe { CancelIoEx(raw_fd, overlapped) } == 0 {
            let errno = last_errno();
            if errno != ERROR_NOT_FOUND as i32 {
                return Err(AsyncHostError::Native(errno));
            }
        }

        let mut bytes_transferred = 0;
        if unsafe { GetOverlappedResult(raw_fd, overlapped, &mut bytes_transferred, 1) } == 0 {
            let errno = last_errno();
            if errno != ERROR_OPERATION_ABORTED as i32 && errno != ERROR_NOT_FOUND as i32 {
                return Err(AsyncHostError::Native(errno));
            }
        }
        self.clear_pending();
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for HostIoResult {
    fn drop(&mut self) {
        let _ = self.cancel_and_drain_pending();
    }
}

pub(crate) struct AsyncHost {
    state: Arc<Mutex<AsyncHostState>>,
}

impl Default for AsyncHost {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(AsyncHostState::default())),
        }
    }
}

impl AsyncHost {
    pub(crate) fn invalid_fd(&self) -> HostHandle {
        self.state.lock().unwrap().invalid_fd()
    }

    pub(crate) fn get_errno(&self) -> i32 {
        self.state.lock().unwrap().errno
    }

    pub(crate) fn set_errno(&self, errno: i32) {
        self.state.lock().unwrap().errno = errno;
    }

    pub(crate) fn record_error(&self, error: AsyncHostError) -> i32 {
        let errno = error.errno();
        self.set_errno(errno);
        errno
    }

    pub(crate) fn poll_create(&self) -> AsyncHostResult<u64> {
        let instance = poll::poll_create()?;
        let mut state = self.state.lock().unwrap();
        let key = state.polls.insert(Arc::new(Mutex::new(HostPoll {
            instance,
            registered_fds: HashMap::new(),
            #[cfg(unix)]
            completion_notifier: None,
            event_generation: 0,
            event_handles: Vec::new(),
        })));
        Ok(handle_from_key(key))
    }

    pub(crate) fn poll_destroy(&self, handle: u64) -> AsyncHostResult<()> {
        let mut state = self.state.lock().unwrap();
        let poll = state
            .polls
            .remove(key_from_handle::<HostPollKey>(handle))
            .ok_or(AsyncHostError::Badf)?;
        let poll = Arc::try_unwrap(poll).map_err(|_| AsyncHostError::Inval)?;
        let mut poll = poll.into_inner().unwrap();
        for event in poll.event_handles.drain(..) {
            state.events.remove(event);
        }
        #[cfg(unix)]
        {
            if let Some(notifier) = &poll.completion_notifier
                && state
                    .completion_notifier
                    .as_ref()
                    .is_some_and(|active| Arc::ptr_eq(active, notifier))
            {
                state.completion_notifier = None;
                if let Some(source) = state.completion_source.take() {
                    let _ = state.remove_file(source);
                }
            }
        }
        #[cfg(windows)]
        {
            if state.completion_port == Some(poll::CompletionPort::from_poll(&poll.instance)) {
                state.completion_port = None;
            }
        }
        poll::poll_destroy(poll.instance);
        Ok(())
    }

    pub(crate) fn poll_register(
        &self,
        poll_handle: u64,
        fd_handle: HostHandle,
        read_only: bool,
    ) -> AsyncHostResult<()> {
        let (raw_fd, poll) = {
            let state = self.state.lock().unwrap();
            let raw_fd = state.file(fd_handle)?.raw_fd();
            let poll = Arc::clone(
                state
                    .polls
                    .get(key_from_handle::<HostPollKey>(poll_handle))
                    .ok_or(AsyncHostError::Badf)?,
            );
            (raw_fd, poll)
        };
        let mut poll = poll.lock().unwrap();
        poll::poll_register(&poll.instance, raw_fd, read_only)?;
        poll.registered_fds.insert(raw_fd_key(raw_fd), fd_handle);
        Ok(())
    }

    pub(crate) fn poll_wait(&self, poll_handle: u64, timeout_ms: i32) -> AsyncHostResult<i32> {
        let poll = {
            let state = self.state.lock().unwrap();
            Arc::clone(
                state
                    .polls
                    .get(key_from_handle::<HostPollKey>(poll_handle))
                    .ok_or(AsyncHostError::Badf)?,
            )
        };
        let mut poll = poll.lock().unwrap();
        let result = poll::poll_wait(&mut poll.instance, timeout_ms);
        if result.is_ok() {
            poll.event_generation = poll.event_generation.wrapping_add(1);
            let old_events = std::mem::take(&mut poll.event_handles);
            drop(poll);
            if !old_events.is_empty() {
                let mut state = self.state.lock().unwrap();
                for event in old_events {
                    state.events.remove(event);
                }
            }
        }
        result
    }

    pub(crate) fn poll_get_event(&self, poll_handle: u64, index: i32) -> AsyncHostResult<u64> {
        let poll_key = key_from_handle::<HostPollKey>(poll_handle);
        let mut state = self.state.lock().unwrap();
        let poll = Arc::clone(state.polls.get(poll_key).ok_or(AsyncHostError::Badf)?);
        let mut poll = poll.lock().unwrap();
        let event = poll::event_list_get(&poll.instance, index)?;
        let raw_fd = poll::event_get_fd(event);
        let fd_handle = poll
            .registered_fds
            .get(&raw_fd_key(raw_fd))
            .copied()
            .or_else(|| completion_event_fd(raw_fd));
        let key = state.events.insert(HostEvent {
            poll: poll_key,
            generation: poll.event_generation,
            index,
            fd_handle,
        });
        poll.event_handles.push(key);
        Ok(handle_from_key(key))
    }

    fn with_event<T>(
        &self,
        event_handle: u64,
        f: impl FnOnce(&HostPoll, &poll::PollEvent) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let event = key_from_handle::<HostEventKey>(event_handle);
        let (event, poll) = {
            let state = self.state.lock().unwrap();
            let event = *state.events.get(event).ok_or(AsyncHostError::Badf)?;
            let poll = Arc::clone(state.polls.get(event.poll).ok_or(AsyncHostError::Badf)?);
            (event, poll)
        };
        let poll = poll.lock().unwrap();
        if poll.event_generation != event.generation {
            return Err(AsyncHostError::Badf);
        }
        let poll_event = poll::event_list_get(&poll.instance, event.index)?;
        f(&poll, poll_event)
    }

    pub(crate) fn poll_event_fd(&self, event_handle: u64) -> AsyncHostResult<HostHandle> {
        let event = key_from_handle::<HostEventKey>(event_handle);
        let (event, poll) = {
            let state = self.state.lock().unwrap();
            let event = *state.events.get(event).ok_or(AsyncHostError::Badf)?;
            let poll = Arc::clone(state.polls.get(event.poll).ok_or(AsyncHostError::Badf)?);
            (event, poll)
        };
        let poll = poll.lock().unwrap();
        if poll.event_generation != event.generation {
            return Err(AsyncHostError::Badf);
        }
        event.fd_handle.ok_or(AsyncHostError::Badf)
    }

    #[cfg(unix)]
    pub(crate) fn poll_event_events(&self, event_handle: u64) -> AsyncHostResult<i32> {
        self.with_event(event_handle, |_, event| Ok(poll::event_get_events(event)))
    }

    #[cfg(windows)]
    pub(crate) fn poll_event_io_result(&self, event_handle: u64) -> AsyncHostResult<u64> {
        let overlapped = self.with_event(event_handle, |_, event| {
            Ok(poll::event_get_io_result(event) as usize)
        })?;
        let state = self.state.lock().unwrap();
        state
            .io_results_by_overlapped
            .get(&overlapped)
            .copied()
            .map(handle_from_key)
            .ok_or(AsyncHostError::Badf)
    }

    #[cfg(windows)]
    pub(crate) fn poll_event_bytes_transferred(&self, event_handle: u64) -> AsyncHostResult<i32> {
        self.with_event(event_handle, |_, event| {
            Ok(poll::event_get_bytes_transferred(event))
        })
    }

    pub(crate) fn init_thread_pool(&self, poll_handle: u64) -> AsyncHostResult<HostHandle> {
        let poll = {
            let state = self.state.lock().unwrap();
            #[cfg(unix)]
            if state.completion_source.is_some() {
                return Err(AsyncHostError::Inval);
            }
            #[cfg(windows)]
            if state.completion_port.is_some() {
                return Err(AsyncHostError::Inval);
            }
            Arc::clone(
                state
                    .polls
                    .get(key_from_handle::<HostPollKey>(poll_handle))
                    .ok_or(AsyncHostError::Badf)?,
            )
        };
        #[cfg(unix)]
        {
            let (completion_notifier, event_fd) = {
                let poll = poll.lock().unwrap();
                ThreadPoolCompletionNotifier::new(&poll.instance)?
            };
            let completion_notifier = Arc::new(completion_notifier);
            let source = {
                let mut state = self.state.lock().unwrap();
                let source = handle_from_key(state.files.insert(HostFile::new(event_fd)));
                state.completion_notifier = Some(Arc::clone(&completion_notifier));
                state.completion_source = Some(source);
                source
            };
            let mut poll = poll.lock().unwrap();
            poll.registered_fds.insert(raw_fd_key(event_fd), source);
            poll.completion_notifier = Some(completion_notifier);
            Ok(source)
        }
        #[cfg(windows)]
        {
            let completion_port = poll::CompletionPort::from_poll(&poll.lock().unwrap().instance);
            self.state.lock().unwrap().completion_port = Some(completion_port);
            raw_fd_to_guest(windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE)
        }
    }

    pub(crate) fn destroy_thread_pool(&self) {
        let workers = {
            let mut state = self.state.lock().unwrap();
            state.take_workers()
        };
        for worker in workers {
            thread_pool::free_worker(worker);
        }

        let mut state = self.state.lock().unwrap();
        #[cfg(unix)]
        {
            if let Some(source) = state.completion_source.take()
                && let Ok(file) = state.remove_file(source)
            {
                let raw_fd = file.raw_fd();
                for poll in state.polls.values_mut() {
                    poll.lock()
                        .unwrap()
                        .registered_fds
                        .remove(&raw_fd_key(raw_fd));
                }
            }
            state.completion_notifier = None;
            for poll in state.polls.values_mut() {
                poll.lock().unwrap().completion_notifier = None;
            }
        }
        #[cfg(windows)]
        {
            state.completion_port = None;
        }
    }

    pub(crate) fn insert_c_buffer(&self, buffer: Box<[u8]>) -> u64 {
        let key = self
            .state
            .lock()
            .unwrap()
            .c_buffers
            .insert(Arc::new(Mutex::new(buffer)));
        handle_from_key(key)
    }

    pub(crate) fn free_c_buffer(&self, handle: u64) -> AsyncHostResult<()> {
        if handle == INVALID_HOST_HANDLE {
            return Ok(());
        }
        self.state
            .lock()
            .unwrap()
            .c_buffers
            .remove(key_from_handle::<HostCBufferKey>(handle))
            .map(|_| ())
            .ok_or(AsyncHostError::Badf)
    }

    pub(crate) fn with_c_buffer<T>(
        &self,
        handle: u64,
        f: impl FnOnce(&[u8]) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let (buffer, offset) = self.c_buffer_slice(handle)?;
        let buffer = buffer.lock().unwrap();
        let buffer = buffer.get(offset..).ok_or(AsyncHostError::Badf)?;
        f(buffer)
    }

    pub(crate) fn with_c_buffer_mut<T>(
        &self,
        handle: u64,
        f: impl FnOnce(&mut [u8]) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let (buffer, offset) = self.c_buffer_slice(handle)?;
        let mut buffer = buffer.lock().unwrap();
        let buffer = buffer.get_mut(offset..).ok_or(AsyncHostError::Badf)?;
        f(buffer)
    }

    pub(crate) fn c_buffer(&self, handle: u64) -> AsyncHostResult<HostCBuffer> {
        if handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }
        self.state
            .lock()
            .unwrap()
            .c_buffers
            .get(key_from_handle::<HostCBufferKey>(handle))
            .cloned()
            .ok_or(AsyncHostError::Badf)
    }

    fn c_buffer_slice(&self, handle: u64) -> AsyncHostResult<(HostCBuffer, usize)> {
        if let Ok(buffer) = self.c_buffer(handle) {
            return Ok((buffer, 0));
        }

        let ptr = usize::try_from(handle).map_err(|_| AsyncHostError::Badf)?;
        let state = self.state.lock().unwrap();
        for buffer in state.c_buffers.values() {
            let guard = buffer.lock().unwrap();
            let start = guard.as_ptr() as usize;
            let Some(end) = start.checked_add(guard.len()) else {
                continue;
            };
            if (start..end).contains(&ptr) {
                return Ok((Arc::clone(buffer), ptr - start));
            }
        }
        Err(AsyncHostError::Badf)
    }

    pub(crate) fn insert_job(&self, job: Job) -> AsyncHostResult<u64> {
        let key = self.state.lock().unwrap().jobs.insert(Some(job));
        Ok(handle_from_key(key))
    }

    pub(crate) fn free_job(&self, handle: u64) -> AsyncHostResult<()> {
        self.state
            .lock()
            .unwrap()
            .jobs
            .remove(key_from_handle::<HostJobKey>(handle))
            .map(|_| ())
            .ok_or(AsyncHostError::Badf)
    }

    pub(crate) fn job_get_ret(&self, handle: u64) -> AsyncHostResult<i64> {
        let state = self.state.lock().unwrap();
        let job = state
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        Ok(crate::async_sys::internal::event_loop::thread_pool::job_get_ret(job))
    }

    pub(crate) fn job_get_err(&self, handle: u64) -> AsyncHostResult<i32> {
        let state = self.state.lock().unwrap();
        let job = state
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        Ok(crate::async_sys::internal::event_loop::thread_pool::job_get_err(job))
    }

    pub(crate) fn open_job_get_fd(&self, handle: u64) -> AsyncHostResult<HostHandle> {
        let state = self.state.lock().unwrap();
        let job = state
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        let result = thread_pool::open_job_result(job)?;
        Ok(thread_pool::open_job_get_fd(result))
    }

    pub(crate) fn open_job_get_kind(&self, handle: u64) -> AsyncHostResult<i32> {
        let state = self.state.lock().unwrap();
        let job = state
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        let result = thread_pool::open_job_result(job)?;
        Ok(thread_pool::open_job_get_kind(result))
    }

    pub(crate) fn open_job_get_dev_id(&self, handle: u64) -> AsyncHostResult<u64> {
        let state = self.state.lock().unwrap();
        let job = state
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        let result = thread_pool::open_job_result(job)?;
        Ok(thread_pool::open_job_get_dev_id(result))
    }

    pub(crate) fn open_job_get_file_id(&self, handle: u64) -> AsyncHostResult<u64> {
        let state = self.state.lock().unwrap();
        let job = state
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        let result = thread_pool::open_job_result(job)?;
        Ok(thread_pool::open_job_get_file_id(result))
    }

    pub(crate) fn get_file_size_result(&self, handle: u64) -> AsyncHostResult<i64> {
        let state = self.state.lock().unwrap();
        let job = state
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        crate::async_sys::internal::event_loop::thread_pool::get_file_size_result(job)
    }

    pub(crate) fn close_fd(&self, handle: HostHandle) -> AsyncHostResult<()> {
        let mut state = self.state.lock().unwrap();
        let raw_fd = state.file(handle)?.raw_fd();
        #[cfg(windows)]
        state.drain_pending_io_for_raw_fd(raw_fd)?;
        #[cfg(unix)]
        if state.completion_source == Some(handle) {
            state.completion_source = None;
            state.completion_notifier = None;
            for poll in state.polls.values_mut() {
                poll.lock().unwrap().completion_notifier = None;
            }
        }
        for poll in state.polls.values() {
            poll.lock()
                .unwrap()
                .registered_fds
                .remove(&raw_fd_key(raw_fd));
        }
        state.remove_file(handle)?;
        Ok(())
    }

    pub(crate) fn pipe(
        &self,
        read_end_is_async: bool,
        write_end_is_async: bool,
    ) -> AsyncHostResult<[HostHandle; 2]> {
        crate::async_sys::internal::fd_util::stub::pipe_host_files(
            &mut self.state.lock().unwrap().files,
            read_end_is_async,
            write_end_is_async,
        )
    }

    #[cfg(unix)]
    pub(crate) fn set_nonblocking(&self, handle: HostHandle) -> AsyncHostResult<()> {
        let state = self.state.lock().unwrap();
        let raw_fd = state.file(handle)?.raw_fd();
        crate::async_sys::internal::fd_util::stub::set_nonblocking(raw_fd)
    }

    #[cfg(unix)]
    pub(crate) fn read_fd(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: HostHandle,
        dst: i32,
        offset: i32,
        len: i32,
    ) -> AsyncHostResult<i32> {
        let offset_dst = dst.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        let dst = memory.read_exact_mut(offset_dst, len)?;
        let state = self.state.lock().unwrap();
        let raw_fd = state.file(handle)?.raw_fd();
        crate::async_sys::internal::event_loop::io::read(raw_fd, dst)
            .and_then(|ret| i32::try_from(ret).map_err(|_| AsyncHostError::Fault))
    }

    #[cfg(unix)]
    pub(crate) fn write_fd(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: HostHandle,
        src: i32,
        offset: i32,
        len: i32,
    ) -> AsyncHostResult<i32> {
        let offset_src = src.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        let src = memory.read_exact(offset_src, len)?;
        let state = self.state.lock().unwrap();
        let raw_fd = state.file(handle)?.raw_fd();
        crate::async_sys::internal::event_loop::io::write(raw_fd, src)
            .and_then(|ret| i32::try_from(ret).map_err(|_| AsyncHostError::Fault))
    }

    #[cfg(windows)]
    pub(crate) fn make_file_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        event: i32,
        buf: i32,
        offset: i32,
        len: i32,
        position: i64,
    ) -> AsyncHostResult<u64> {
        let guest_offset = buf.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        memory.read_exact(guest_offset, len)?;
        let buffer = vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?];
        let result = Box::new(HostIoResult::for_file(
            event,
            buffer,
            guest_offset,
            position,
        ));

        let mut state = self.state.lock().unwrap();
        let key = state.io_results.insert(result);
        let overlapped = state
            .io_results
            .get_mut(key)
            .ok_or(AsyncHostError::Badf)?
            .overlapped_ptr() as usize;
        state.io_results_by_overlapped.insert(overlapped, key);
        Ok(handle_from_key(key))
    }

    #[cfg(windows)]
    pub(crate) fn free_io_result(&self, handle: u64) -> AsyncHostResult<()> {
        let key = key_from_handle::<HostIoResultKey>(handle);
        let mut state = self.state.lock().unwrap();
        let result = state.io_results.get_mut(key).ok_or(AsyncHostError::Badf)?;
        result.cancel_and_drain_pending()?;
        let mut result = state.io_results.remove(key).ok_or(AsyncHostError::Badf)?;
        state
            .io_results_by_overlapped
            .remove(&(result.overlapped_ptr() as usize));
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn io_result_get_event(&self, handle: u64) -> AsyncHostResult<i32> {
        let state = self.state.lock().unwrap();
        let result = state
            .io_results
            .get(key_from_handle::<HostIoResultKey>(handle))
            .ok_or(AsyncHostError::Badf)?;
        Ok(result.event)
    }

    #[cfg(windows)]
    pub(crate) fn cancel_io_result(
        &self,
        result_handle: u64,
        fd_handle: HostHandle,
    ) -> AsyncHostResult<i32> {
        let mut state = self.state.lock().unwrap();
        state.file(fd_handle)?;
        let result = state
            .io_results
            .get_mut(key_from_handle::<HostIoResultKey>(result_handle))
            .ok_or(AsyncHostError::Badf)?;
        result.cancel_pending()
    }

    #[cfg(windows)]
    pub(crate) fn io_result_get_status(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        result_handle: u64,
        fd_handle: HostHandle,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::System::IO::GetOverlappedResult;

        let mut state = self.state.lock().unwrap();
        let raw_fd = state.file(fd_handle)?.raw_fd();
        let result = state
            .io_results
            .get_mut(key_from_handle::<HostIoResultKey>(result_handle))
            .ok_or(AsyncHostError::Badf)?;
        let mut bytes_transferred = 0;
        if unsafe {
            GetOverlappedResult(raw_fd, result.overlapped_ptr(), &mut bytes_transferred, 0)
        } == 0
        {
            let error = last_native_error();
            if !matches!(
                error,
                AsyncHostError::Native(errno)
                    if errno == windows_sys::Win32::Foundation::ERROR_IO_INCOMPLETE as i32
            ) {
                result.clear_pending();
            }
            return Err(error);
        }
        result.clear_pending();
        let bytes_transferred =
            i32::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)?;
        result.copy_read_result(memory, bytes_transferred)?;
        Ok(bytes_transferred)
    }

    #[cfg(windows)]
    pub(crate) fn read_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::{ERROR_HANDLE_EOF, ERROR_IO_PENDING};
        use windows_sys::Win32::Storage::FileSystem::ReadFile;

        let mut state = self.state.lock().unwrap();
        let raw_fd = state.file(fd_handle)?.raw_fd();
        let result = state
            .io_results
            .get_mut(key_from_handle::<HostIoResultKey>(result_handle))
            .ok_or(AsyncHostError::Badf)?;
        if result.pending_raw_fd().is_some() {
            return Err(AsyncHostError::Inval);
        }
        result.direction = Some(HostIoDirection::Read);
        let len = u32::try_from(result.buffer.len()).map_err(|_| AsyncHostError::Fault)?;
        let mut bytes_transferred = 0;
        let success = unsafe {
            ReadFile(
                raw_fd,
                result.buffer.as_mut_ptr().cast(),
                len,
                &mut bytes_transferred,
                result.overlapped_ptr(),
            )
        };
        if success != 0 {
            let bytes_transferred =
                i32::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)?;
            result.copy_read_result(memory, bytes_transferred)?;
            return Ok(bytes_transferred);
        }
        let errno = last_errno();
        if errno == ERROR_HANDLE_EOF as i32 {
            Ok(0)
        } else if errno == ERROR_IO_PENDING as i32 {
            result.mark_pending(raw_fd)?;
            Err(AsyncHostError::Native(errno))
        } else {
            Err(AsyncHostError::Native(errno))
        }
    }

    #[cfg(windows)]
    pub(crate) fn write_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::ERROR_IO_PENDING;
        use windows_sys::Win32::Storage::FileSystem::WriteFile;

        let mut state = self.state.lock().unwrap();
        let raw_fd = state.file(fd_handle)?.raw_fd();
        let result = state
            .io_results
            .get_mut(key_from_handle::<HostIoResultKey>(result_handle))
            .ok_or(AsyncHostError::Badf)?;
        if result.pending_raw_fd().is_some() {
            return Err(AsyncHostError::Inval);
        }
        result.direction = Some(HostIoDirection::Write);
        let len_i32 = i32::try_from(result.buffer.len()).map_err(|_| AsyncHostError::Fault)?;
        let data = memory.read_exact(result.guest_offset, len_i32)?;
        result.buffer.copy_from_slice(data);
        let len = u32::try_from(result.buffer.len()).map_err(|_| AsyncHostError::Fault)?;
        let mut bytes_transferred = 0;
        let success = unsafe {
            WriteFile(
                raw_fd,
                result.buffer.as_ptr().cast(),
                len,
                &mut bytes_transferred,
                result.overlapped_ptr(),
            )
        };
        if success != 0 {
            i32::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)
        } else {
            let error = last_native_error();
            if matches!(error, AsyncHostError::Native(errno) if errno == ERROR_IO_PENDING as i32) {
                result.mark_pending(raw_fd)?;
            }
            Err(error)
        }
    }

    pub(crate) fn try_lock_file(&self, handle: HostHandle, exclusive: bool) -> AsyncHostResult<()> {
        let mut state = self.state.lock().unwrap();
        let file = state.file_mut(handle)?;
        crate::async_sys::fs::stub::try_lock_host_file(file, exclusive)
    }

    pub(crate) fn unlock_file(&self, handle: HostHandle) -> AsyncHostResult<()> {
        let mut state = self.state.lock().unwrap();
        let file = state.file_mut(handle)?;
        crate::async_sys::fs::stub::unlock_host_file(file)
    }

    pub(crate) fn run_job(&self, handle: u64) -> AsyncHostResult<()> {
        let key = key_from_handle::<HostJobKey>(handle);
        let mut job = self
            .state
            .lock()
            .unwrap()
            .jobs
            .get_mut(key)
            .and_then(Option::take)
            .ok_or(AsyncHostError::Badf)?;
        let mut files = SharedFileTable {
            state: Arc::clone(&self.state),
        };
        thread_pool::run_host_job(&mut job, &mut files);
        let mut state = self.state.lock().unwrap();
        let slot = state.jobs.get_mut(key).ok_or(AsyncHostError::Badf)?;
        if slot.is_some() {
            return Err(AsyncHostError::Badf);
        }
        *slot = Some(job);
        Ok(())
    }

    pub(crate) fn spawn_worker(&self, job_id: i32, job_handle: u64) -> AsyncHostResult<u64> {
        #[cfg(unix)]
        let worker = {
            let completion_notifier = self
                .state
                .lock()
                .unwrap()
                .completion_notifier
                .clone()
                .ok_or(AsyncHostError::Badf)?;
            self.spawn_worker_thread(HostWorkerJob { job_id, job_handle }, move |worker_job| {
                let _ = completion_notifier.notify(worker_job.job_id);
            })
        };
        #[cfg(windows)]
        let worker = {
            let completion_port = self
                .state
                .lock()
                .unwrap()
                .completion_port
                .ok_or(AsyncHostError::Badf)?;
            self.spawn_worker_thread(HostWorkerJob { job_id, job_handle }, move |worker_job| {
                let _ = poll::post_thread_pool_completion(completion_port, worker_job.job_id);
            })
        };
        let key = self.state.lock().unwrap().workers.insert(worker);
        Ok(handle_from_key(key))
    }

    pub(crate) fn wake_worker(
        &self,
        worker_handle: u64,
        job_id: i32,
        job_handle: u64,
    ) -> AsyncHostResult<()> {
        let state = self.state.lock().unwrap();
        let worker = state
            .workers
            .get(key_from_handle::<HostWorkerKey>(worker_handle))
            .ok_or(AsyncHostError::Badf)?;
        thread_pool::wake_worker(worker, HostWorkerJob { job_id, job_handle });
        Ok(())
    }

    pub(crate) fn worker_enter_idle(&self, worker_handle: u64) -> AsyncHostResult<()> {
        let state = self.state.lock().unwrap();
        let worker = state
            .workers
            .get(key_from_handle::<HostWorkerKey>(worker_handle))
            .ok_or(AsyncHostError::Badf)?;
        thread_pool::worker_enter_idle(worker);
        Ok(())
    }

    pub(crate) fn free_worker(&self, worker_handle: u64) -> AsyncHostResult<()> {
        let worker = self
            .state
            .lock()
            .unwrap()
            .workers
            .remove(key_from_handle::<HostWorkerKey>(worker_handle))
            .ok_or(AsyncHostError::Badf)?;
        thread_pool::free_worker(worker);
        Ok(())
    }

    pub(crate) fn cancel_worker(&self, worker_handle: u64) -> AsyncHostResult<i32> {
        let state = self.state.lock().unwrap();
        let worker = state
            .workers
            .get(key_from_handle::<HostWorkerKey>(worker_handle))
            .ok_or(AsyncHostError::Badf)?;
        thread_pool::cancel_worker(worker)
    }

    pub(crate) fn get_read_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: u64,
        dst: i32,
        offset: i32,
        len: i32,
    ) -> AsyncHostResult<()> {
        let state = self.state.lock().unwrap();
        let job = state
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        thread_pool::get_read_result(job, memory, dst, offset, len)
    }

    pub(crate) fn get_file_time_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: u64,
        dst: i32,
    ) -> AsyncHostResult<()> {
        let state = self.state.lock().unwrap();
        let job = state
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        thread_pool::get_file_time_result(job, memory, dst)
    }

    #[cfg(unix)]
    pub(crate) fn fetch_completion(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        source_fd: HostHandle,
        dst: i32,
        max_jobs: i32,
    ) -> AsyncHostResult<i32> {
        let (completion_notifier, completion_source) = {
            let state = self.state.lock().unwrap();
            (
                state
                    .completion_notifier
                    .clone()
                    .ok_or(AsyncHostError::Badf)?,
                state.completion_source.ok_or(AsyncHostError::Badf)?,
            )
        };
        if completion_source != source_fd {
            return Err(AsyncHostError::Badf);
        }

        let max_jobs = usize::try_from(max_jobs).map_err(|_| AsyncHostError::Fault)?;
        if max_jobs == 0 {
            return Ok(0);
        }
        let max_bytes = max_jobs
            .checked_mul(std::mem::size_of::<i32>())
            .ok_or(AsyncHostError::Fault)?;
        let max_bytes_i32 = i32::try_from(max_bytes).map_err(|_| AsyncHostError::Fault)?;
        memory.read_exact(dst, max_bytes_i32)?;

        let mut completions = vec![0; max_bytes];
        let bytes = completion_notifier.fetch(&mut completions)?;
        debug_assert_eq!(bytes % std::mem::size_of::<i32>(), 0);
        let bytes_i32 = i32::try_from(bytes).map_err(|_| AsyncHostError::Fault)?;
        memory.write_exact(dst, &completions[..bytes])?;
        Ok(bytes_i32)
    }

    fn spawn_worker_thread(
        &self,
        init_job: HostWorkerJob,
        mut complete_job: impl FnMut(HostWorkerJob) + Send + 'static,
    ) -> HostWorkerHandle {
        let state = Arc::clone(&self.state);
        let run_state = Arc::clone(&state);
        thread_pool::spawn_worker(
            init_job,
            move |worker_job| {
                let key = key_from_handle::<HostJobKey>(worker_job.job_handle);
                let Some(mut job) = run_state
                    .lock()
                    .unwrap()
                    .jobs
                    .get_mut(key)
                    .and_then(Option::take)
                else {
                    return;
                };

                let mut files = SharedFileTable {
                    state: Arc::clone(&run_state),
                };
                thread_pool::run_host_job(&mut job, &mut files);

                let mut state = run_state.lock().unwrap();
                if let Some(slot) = state.jobs.get_mut(key)
                    && slot.is_none()
                {
                    *slot = Some(job);
                }
            },
            move |worker_job| {
                // Even if cancellation discarded the job handle, the event loop
                // still needs the completion to move the worker out of running.
                complete_job(worker_job);
            },
        )
    }
}

impl Drop for AsyncHost {
    fn drop(&mut self) {
        self.destroy_thread_pool();
    }
}

struct SharedFileTable {
    state: Arc<Mutex<AsyncHostState>>,
}

impl HostFileTable for SharedFileTable {
    fn insert_file(&mut self, file: RawFd) -> AsyncHostResult<HostHandle> {
        Ok(handle_from_key(
            self.state.lock().unwrap().files.insert(HostFile::new(file)),
        ))
    }

    fn is_invalid_file_handle(&self, handle: HostHandle) -> bool {
        self.state
            .lock()
            .unwrap()
            .files
            .get(key_from_handle::<HostFileKey>(handle))
            .is_some_and(HostFile::is_invalid)
    }

    #[cfg(windows)]
    fn borrowed_raw_file(&mut self, handle: HostHandle) -> AsyncHostResult<RawFd> {
        let state = self.state.lock().unwrap();
        Ok(state.file(handle)?.raw_fd())
    }

    fn with_raw_file<U>(
        &mut self,
        handle: HostHandle,
        f: impl FnOnce(RawFd) -> AsyncHostResult<U>,
    ) -> AsyncHostResult<U> {
        let raw_fd = {
            let state = self.state.lock().unwrap();
            state.file(handle)?.raw_fd()
        };
        f(raw_fd)
    }

    fn with_host_file_mut<U>(
        &mut self,
        handle: HostHandle,
        f: impl FnOnce(&mut HostFile) -> AsyncHostResult<U>,
    ) -> AsyncHostResult<U> {
        // Keep this path for host-owned file state operations such as readdir.
        // Ordinary worker jobs should use with_raw_file and mirror native async
        // by operating on the raw fd value without duplicating the OS handle.
        let mut state = self.state.lock().unwrap();
        let file = state.file_mut(handle)?;
        f(file)
    }
}

#[cfg(windows)]
fn last_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_else(|| AsyncHostError::Inval.errno())
}

#[cfg(windows)]
fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(last_errno())
}

#[cfg(windows)]
fn raw_fd_to_guest(fd: RawFd) -> AsyncHostResult<HostHandle> {
    Ok(fd as usize as u64)
}

#[cfg(unix)]
fn completion_event_fd(_fd: RawFd) -> Option<HostHandle> {
    None
}

#[cfg(windows)]
fn completion_event_fd(fd: RawFd) -> Option<HostHandle> {
    if fd == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
        raw_fd_to_guest(fd).ok()
    } else {
        None
    }
}

#[cfg(unix)]
fn raw_fd_key(fd: RawFd) -> isize {
    fd as isize
}

#[cfg(windows)]
fn raw_fd_key(fd: RawFd) -> isize {
    fd as isize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_memory_read_exact_accepts_in_bounds_access() {
        let memory = [1, 2, 3, 4];

        assert_eq!(memory.read_exact(1, 2).unwrap(), &[2, 3]);
        assert!(memory.read_exact(4, 0).unwrap().is_empty());
    }

    #[test]
    fn guest_memory_read_exact_rejects_out_of_bounds_access() {
        let memory = [0; 4];

        for (offset, len) in [(-1, 1), (0, -1), (3, 2), (i32::MAX, 1), (2, i32::MAX)] {
            assert_eq!(memory.read_exact(offset, len), Err(AsyncHostError::Fault));
        }
    }

    #[test]
    fn guest_memory_read_exact_mut_accepts_in_bounds_access() {
        let mut memory = [1, 2, 3, 4];

        memory.read_exact_mut(1, 2).unwrap().fill(9);

        assert_eq!(memory, [1, 9, 9, 4]);
    }

    #[test]
    fn guest_memory_read_exact_mut_rejects_out_of_bounds_access() {
        let mut memory = [0; 4];

        for (offset, len) in [(-1, 1), (0, -1), (3, 2), (i32::MAX, 1), (2, i32::MAX)] {
            assert_eq!(
                memory.read_exact_mut(offset, len),
                Err(AsyncHostError::Fault)
            );
        }
    }

    #[test]
    fn guest_memory_helpers_read_and_write_u16_units() {
        let mut memory = [0; 8];

        write_u16(&mut memory, 2, &[0x1234, 0x5678]).unwrap();

        assert_eq!(read_u16(&memory, 2, 2).unwrap(), &[0x1234, 0x5678]);
        assert_eq!(read_u16(&memory, 1, 1), Err(AsyncHostError::Fault));
        assert_eq!(
            write_u16(&mut memory, 6, &[1, 2]),
            Err(AsyncHostError::Fault)
        );
        assert_eq!(&memory[2..6], &[0x34, 0x12, 0x78, 0x56]);
    }

    #[test]
    fn guest_memory_helpers_allow_empty_u16_access_on_empty_memory() {
        let mut memory = [];

        assert!(read_u16(&memory, 0, 0).unwrap().is_empty());
        write_u16(&mut memory, 0, &[]).unwrap();
    }

    #[test]
    fn guest_memory_writes_fixed_little_endian_words() {
        let mut memory = [0; 16];

        memory.write_u64_le(2, 0x1020_3040_5060_7080).unwrap();

        assert_eq!(
            &memory[2..10],
            &[0x80, 0x70, 0x60, 0x50, 0x40, 0x30, 0x20, 0x10]
        );
        assert_eq!(memory.write_u64_le(10, 1), Err(AsyncHostError::Fault));
    }

    #[cfg(unix)]
    #[test]
    fn completion_source_is_host_file_handle() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_source = host.init_thread_pool(poll).unwrap();
        let raw_completion_fd = {
            let state = host.state.lock().unwrap();
            state.file(completion_source).unwrap().raw_fd()
        };
        {
            let state = host.state.lock().unwrap();
            let poll = state
                .polls
                .get(key_from_handle::<HostPollKey>(poll))
                .unwrap()
                .lock()
                .unwrap();
            assert_eq!(
                poll.registered_fds
                    .get(&raw_fd_key(raw_completion_fd))
                    .copied(),
                Some(completion_source)
            );
        }

        {
            let state = host.state.lock().unwrap();
            state
                .completion_notifier
                .as_ref()
                .unwrap()
                .notify(17)
                .unwrap();
        }
        assert_eq!(host.poll_wait(poll, 1000).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();
        assert_eq!(host.poll_event_fd(event).unwrap(), completion_source);

        let mut memory = [0; 4];
        assert_eq!(
            host.fetch_completion(memory.as_mut_slice(), completion_source, 0, 1)
                .unwrap(),
            4
        );
        assert_eq!(i32::from_le_bytes(memory), 17);
    }

    #[cfg(unix)]
    #[test]
    fn fetch_completion_publishes_job_id_without_copying_payload() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let job = thread_pool::make_read_job(0, 3, -1);
        let job_handle = host.insert_job(job).unwrap();
        {
            let mut state = host.state.lock().unwrap();
            let job = state
                .jobs
                .get_mut(key_from_handle::<HostJobKey>(job_handle))
                .and_then(Option::as_mut)
                .unwrap();
            let thread_pool::JobPayload::Read { result, .. } = job.payload_mut() else {
                panic!("expected read job");
            };
            *result = Some(b"abc".to_vec());
            state
                .completion_notifier
                .as_ref()
                .unwrap()
                .notify(42)
                .unwrap();
        }

        let mut memory = vec![0; 16];
        let bytes = host
            .fetch_completion(memory.as_mut_slice(), completion_notifier, 0, 1)
            .unwrap();

        assert_eq!(bytes, 4);
        assert_eq!(i32::from_le_bytes(memory[0..4].try_into().unwrap()), 42);
        assert_eq!(&memory[8..11], &[0, 0, 0]);

        host.get_read_result(memory.as_mut_slice(), job_handle, 8, 0, 3)
            .unwrap();

        assert_eq!(&memory[8..11], b"abc");
    }

    #[cfg(unix)]
    #[test]
    fn fetch_completion_leaves_unfetched_job_ids_in_os_source() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        {
            let state = host.state.lock().unwrap();
            let notifier = state.completion_notifier.as_ref().unwrap();
            notifier.notify(41).unwrap();
            notifier.notify(42).unwrap();
        }

        let mut memory = vec![0; 8];
        let bytes = host
            .fetch_completion(memory.as_mut_slice(), completion_notifier, 0, 0)
            .unwrap();
        assert_eq!(bytes, 0);

        let bytes = host
            .fetch_completion(memory.as_mut_slice(), completion_notifier, 0, 1)
            .unwrap();
        assert_eq!(bytes, 4);
        assert_eq!(i32::from_le_bytes(memory[0..4].try_into().unwrap()), 41);

        let bytes = host
            .fetch_completion(memory.as_mut_slice(), completion_notifier, 4, 1)
            .unwrap();
        assert_eq!(bytes, 4);
        assert_eq!(i32::from_le_bytes(memory[4..8].try_into().unwrap()), 42);
    }

    #[test]
    fn stale_job_handle_is_rejected_after_free() {
        let host = AsyncHost::default();
        let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();

        host.free_job(job).unwrap();

        assert_eq!(host.job_get_ret(job), Err(AsyncHostError::Badf));
        assert_eq!(host.free_job(job), Err(AsyncHostError::Badf));
    }

    #[test]
    fn stale_worker_handle_is_rejected_after_free() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let job = host
            .insert_job(thread_pool::make_read_job(0, 1, -1))
            .unwrap();
        let worker = host.spawn_worker(42, job).unwrap();
        host.poll_wait(poll, 1000).unwrap();
        #[cfg(unix)]
        {
            let mut memory = [0; 4];
            host.fetch_completion(memory.as_mut_slice(), completion_notifier, 0, 1)
                .unwrap();
        }
        #[cfg(windows)]
        {
            let event = host.poll_get_event(poll, 0).unwrap();
            assert_eq!(host.poll_event_fd(event).unwrap(), completion_notifier);
            assert_eq!(host.poll_event_bytes_transferred(event).unwrap(), 42);
        }

        host.free_worker(worker).unwrap();

        assert_eq!(host.free_worker(worker), Err(AsyncHostError::Badf));
    }

    #[cfg(unix)]
    #[test]
    fn poll_reports_registered_pipe_readiness_as_guest_fd() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let [read, write] = host.pipe(true, true).unwrap();
        host.poll_register(poll, read, true).unwrap();

        {
            let mut state = host.state.lock().unwrap();
            state
                .files
                .with_raw_file(write, |fd| {
                    let byte = b"x";
                    let ret = unsafe { libc::write(fd, byte.as_ptr().cast(), byte.len()) };
                    if ret < 0 {
                        Err(AsyncHostError::Native(
                            std::io::Error::last_os_error()
                                .raw_os_error()
                                .unwrap_or_else(|| AsyncHostError::Inval.errno()),
                        ))
                    } else {
                        Ok(())
                    }
                })
                .unwrap();
        }

        assert_eq!(host.poll_wait(poll, 100).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();
        assert_eq!(host.poll_event_fd(event).unwrap(), read);
        assert_eq!(
            host.poll_event_events(event).unwrap() & poll::READ_EVENT,
            poll::READ_EVENT
        );
        host.close_fd(read).unwrap();
        host.close_fd(write).unwrap();
    }

    #[test]
    fn stale_file_handle_is_rejected_after_close() {
        let host = AsyncHost::default();
        let [read, write] = host.pipe(true, true).unwrap();

        host.close_fd(read).unwrap();

        assert_eq!(host.close_fd(read), Err(AsyncHostError::Badf));
        host.close_fd(write).unwrap();
    }
}
