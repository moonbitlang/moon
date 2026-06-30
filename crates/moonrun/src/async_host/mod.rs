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
        self, FileResource, FileResourceRef, FileResourceTable, HostHandle, HostWorkerHandle,
        HostWorkerJob, Job, OpenJobResource, WorkerCompletionId,
    },
};
use crate::async_sys::internal::fd_util::stub::RawFd;
use crate::async_sys::socket::RawSocket;

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
pub(crate) const CHECK_FD_LEAK_ENV: &str = "MOONBIT_ASYNC_CHECK_FD_LEAK";
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

pub(crate) fn read_u16(memory: &[u8], offset: i32, len: i32) -> AsyncHostResult<Vec<u16>> {
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let (offset, end) = u16_bounds(memory.len(), offset, len)?;
    Ok(memory[offset..end]
        .chunks_exact(std::mem::size_of::<u16>())
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect())
}

pub(crate) fn write_u16(memory: &mut [u8], offset: i32, data: &[u16]) -> AsyncHostResult<()> {
    let (offset, end) = u16_bounds(memory.len(), offset, data.len())?;
    for (dst, value) in memory[offset..end]
        .chunks_exact_mut(std::mem::size_of::<u16>())
        .zip(data.iter().copied())
    {
        dst.copy_from_slice(&value.to_le_bytes());
    }
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

impl FileResourceTable for SlotMap<FileResourceKey, FileResourceRef> {
    fn insert_file(&mut self, file: RawFd) -> AsyncHostResult<u64> {
        Ok(handle_from_key(
            self.insert(Arc::new(FileResource::new(file))),
        ))
    }
}

new_key_type! {
    pub(crate) struct HostAddrInfoKey;
    pub(crate) struct HostCBufferKey;
    pub(crate) struct FileResourceKey;
    pub(crate) struct HostIoResultKey;
    pub(crate) struct HostJobKey;
    pub(crate) struct HostPollKey;
    pub(crate) struct HostWorkerKey;
}

#[derive(Debug)]
struct HostAddrInfo {
    addr: Box<[u8]>,
    next: Option<HostAddrInfoKey>,
}

#[derive(Debug)]
struct CompletedJob {
    key: HostJobKey,
    job: Job,
}

fn handle_from_key(key: impl Key) -> u64 {
    key.data().as_ffi()
}

fn key_from_handle<K: Key>(handle: u64) -> K {
    KeyData::from_ffi(handle).into()
}

struct FileTable {
    files: SlotMap<FileResourceKey, FileResourceRef>,
    invalid_file: FileResourceKey,
}

impl Default for FileTable {
    fn default() -> Self {
        let mut files = SlotMap::with_key();
        let invalid_file = files.insert(Arc::new(FileResource::invalid()));
        Self {
            files,
            invalid_file,
        }
    }
}

impl FileTable {
    fn invalid_fd(&self) -> HostHandle {
        handle_from_key(self.invalid_file)
    }

    fn file(&self, handle: HostHandle) -> AsyncHostResult<FileResourceRef> {
        let file = self
            .files
            .get(key_from_handle::<FileResourceKey>(handle))
            .ok_or(AsyncHostError::Badf)?;
        if file.is_invalid() {
            return Err(AsyncHostError::Badf);
        }
        Ok(Arc::clone(file))
    }

    fn remove_file(&mut self, handle: HostHandle) -> AsyncHostResult<FileResourceRef> {
        let key = key_from_handle::<FileResourceKey>(handle);
        if key == self.invalid_file {
            return Err(AsyncHostError::Badf);
        }
        self.files.remove(key).ok_or(AsyncHostError::Badf)
    }

    fn insert_file_resource(&mut self, file: FileResource) -> HostHandle {
        handle_from_key(self.files.insert(Arc::new(file)))
    }
}

struct JobTable {
    jobs: SlotMap<HostJobKey, Option<Job>>,
}

impl Default for JobTable {
    fn default() -> Self {
        Self {
            jobs: SlotMap::with_key(),
        }
    }
}

impl JobTable {
    fn take_job(&mut self, key: HostJobKey) -> AsyncHostResult<Job> {
        self.jobs
            .get_mut(key)
            .and_then(Option::take)
            .ok_or(AsyncHostError::Badf)
    }
}

struct PollTable {
    polls: SlotMap<HostPollKey, Arc<Mutex<HostPoll>>>,
    current_event_poll: Option<HostPollKey>,
}

impl Default for PollTable {
    fn default() -> Self {
        Self {
            polls: SlotMap::with_key(),
            current_event_poll: None,
        }
    }
}

#[derive(Default)]
struct ThreadPoolCompletions {
    #[cfg(unix)]
    notifier: Option<Arc<ThreadPoolCompletionNotifier>>,
    #[cfg(unix)]
    source: Option<HostHandle>,
    #[cfg(windows)]
    target: Option<ThreadPoolCompletionTarget>,
    #[cfg(windows)]
    generation_counter: usize,
}

impl ThreadPoolCompletions {
    #[cfg(windows)]
    fn advance_generation(&mut self) -> usize {
        self.generation_counter = self.generation_counter.wrapping_add(1);
        if self.generation_counter == 0 {
            self.generation_counter = 1;
        }
        self.generation_counter
    }
}

struct WorkerTable {
    workers: SlotMap<HostWorkerKey, HostWorkerHandle>,
}

impl Default for WorkerTable {
    fn default() -> Self {
        Self {
            workers: SlotMap::with_key(),
        }
    }
}

#[cfg(windows)]
struct IoResultTable {
    io_results: SlotMap<HostIoResultKey, Box<HostIoResult>>,
    io_results_by_overlapped: HashMap<OverlappedAddr, HostIoResultKey>,
}

#[cfg(windows)]
impl Default for IoResultTable {
    fn default() -> Self {
        Self {
            io_results: SlotMap::with_key(),
            io_results_by_overlapped: HashMap::new(),
        }
    }
}

#[cfg(windows)]
impl IoResultTable {
    fn has_pending_io_for_raw_fd(&self, raw_fd: RawFd) -> bool {
        self.io_results
            .values()
            .any(|result| result.protects_pending_raw_fd(raw_fd))
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ThreadPoolCompletionTarget {
    poll: HostPollKey,
    port: poll::CompletionPort,
    generation: usize,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct OverlappedAddr(usize);

#[cfg(windows)]
impl OverlappedAddr {
    fn from_ptr(ptr: *mut windows_sys::Win32::System::IO::OVERLAPPED) -> Self {
        Self(ptr as usize)
    }
}

#[derive(Debug)]
struct HostPoll {
    instance: PollInstance,
    registered_fds: HashMap<isize, HostHandle>,
    #[cfg(unix)]
    completion_notifier: Option<Arc<ThreadPoolCompletionNotifier>>,
    event_fd_handles: Vec<Option<HostHandle>>,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostIoDirection {
    Read,
    Write,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostIoKind {
    File,
    Socket,
    SocketWithAddr,
    Connect,
    Accept,
}

#[cfg(windows)]
struct HostIoResult {
    overlapped: windows_sys::Win32::System::IO::OVERLAPPED,
    kind: HostIoKind,
    event: i32,
    // Native async retains the MoonBit buffer object. The V8 host instead keeps
    // a stable host buffer and copies at explicit FFI submit/complete points.
    buffer: Vec<u8>,
    guest_offset: i32,
    socket_flags: u32,
    addr_buffer: Vec<u8>,
    // WSARecvFrom may complete asynchronously and write through lpFromlen later.
    // Keep that storage with the overlapped result, not on the submitter stack.
    addr_len: i32,
    guest_addr_offset: Option<i32>,
    accept_buffer: Vec<u8>,
    accept_bytes_received: u32,
    direction: Option<HostIoDirection>,
    pending_raw_fd: Option<RawFd>,
    // AcceptEx submits one overlapped operation with both the listening socket
    // and a pre-created accepted socket. Cancel/status use pending_raw_fd, but
    // close protection must cover the accepted socket as well until completion.
    extra_pending_close_raw_fd: Option<RawFd>,
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
            kind: HostIoKind::File,
            event,
            buffer,
            guest_offset,
            socket_flags: 0,
            addr_buffer: Vec::new(),
            addr_len: 0,
            guest_addr_offset: None,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            direction: None,
            pending_raw_fd: None,
            extra_pending_close_raw_fd: None,
        }
    }

    fn for_socket(event: i32, buffer: Vec<u8>, guest_offset: i32, flags: i32) -> Self {
        let overlapped =
            std::mem::MaybeUninit::<windows_sys::Win32::System::IO::OVERLAPPED>::zeroed();
        Self {
            overlapped: unsafe { overlapped.assume_init() },
            kind: HostIoKind::Socket,
            event,
            buffer,
            guest_offset,
            socket_flags: flags as u32,
            addr_buffer: Vec::new(),
            addr_len: 0,
            guest_addr_offset: None,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            direction: None,
            pending_raw_fd: None,
            extra_pending_close_raw_fd: None,
        }
    }

    fn for_socket_with_addr(
        event: i32,
        buffer: Vec<u8>,
        guest_offset: i32,
        flags: i32,
        addr_buffer: Vec<u8>,
        addr_len: i32,
        guest_addr_offset: i32,
    ) -> Self {
        let overlapped =
            std::mem::MaybeUninit::<windows_sys::Win32::System::IO::OVERLAPPED>::zeroed();
        Self {
            overlapped: unsafe { overlapped.assume_init() },
            kind: HostIoKind::SocketWithAddr,
            event,
            buffer,
            guest_offset,
            socket_flags: flags as u32,
            addr_len,
            addr_buffer,
            guest_addr_offset: Some(guest_addr_offset),
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            direction: None,
            pending_raw_fd: None,
            extra_pending_close_raw_fd: None,
        }
    }

    fn for_connect(addr_buffer: Vec<u8>) -> Self {
        let overlapped =
            std::mem::MaybeUninit::<windows_sys::Win32::System::IO::OVERLAPPED>::zeroed();
        Self {
            overlapped: unsafe { overlapped.assume_init() },
            kind: HostIoKind::Connect,
            event: 2,
            buffer: Vec::new(),
            guest_offset: 0,
            socket_flags: 0,
            addr_buffer,
            addr_len: 0,
            guest_addr_offset: None,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            direction: None,
            pending_raw_fd: None,
            extra_pending_close_raw_fd: None,
        }
    }

    fn for_accept(addr_len: i32) -> AsyncHostResult<Self> {
        let overlapped =
            std::mem::MaybeUninit::<windows_sys::Win32::System::IO::OVERLAPPED>::zeroed();
        let addr_len_usize = usize::try_from(addr_len).map_err(|_| AsyncHostError::Fault)?;
        let accept_addr_len = addr_len_usize
            .checked_add(16)
            .ok_or(AsyncHostError::Fault)?;
        let accept_buffer_len = accept_addr_len
            .checked_mul(2)
            .ok_or(AsyncHostError::Fault)?;
        Ok(Self {
            overlapped: unsafe { overlapped.assume_init() },
            kind: HostIoKind::Accept,
            event: 1,
            buffer: Vec::new(),
            guest_offset: 0,
            socket_flags: 0,
            addr_buffer: Vec::new(),
            addr_len,
            guest_addr_offset: None,
            accept_buffer: vec![0; accept_buffer_len],
            accept_bytes_received: 0,
            direction: None,
            pending_raw_fd: None,
            extra_pending_close_raw_fd: None,
        })
    }

    fn overlapped_ptr(&mut self) -> *mut windows_sys::Win32::System::IO::OVERLAPPED {
        &mut self.overlapped
    }

    fn overlapped_addr(&mut self) -> OverlappedAddr {
        OverlappedAddr::from_ptr(self.overlapped_ptr())
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
        memory.write_exact(self.guest_offset, data)?;
        if self.kind == HostIoKind::SocketWithAddr
            && let Some(guest_addr_offset) = self.guest_addr_offset
        {
            memory.write_exact(guest_addr_offset, &self.addr_buffer)?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn pending_raw_fd(&self) -> Option<RawFd> {
        self.pending_raw_fd
    }

    fn is_pending(&self) -> bool {
        self.pending_raw_fd.is_some() || self.extra_pending_close_raw_fd.is_some()
    }

    fn protects_pending_raw_fd(&self, raw_fd: RawFd) -> bool {
        self.pending_raw_fd == Some(raw_fd) || self.extra_pending_close_raw_fd == Some(raw_fd)
    }

    fn mark_pending(&mut self, raw_fd: RawFd) -> AsyncHostResult<()> {
        if self.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        self.pending_raw_fd = Some(raw_fd);
        Ok(())
    }

    fn mark_pending_with_close_guard(
        &mut self,
        raw_fd: RawFd,
        close_guard_raw_fd: RawFd,
    ) -> AsyncHostResult<()> {
        self.mark_pending(raw_fd)?;
        self.extra_pending_close_raw_fd = Some(close_guard_raw_fd);
        Ok(())
    }

    fn clear_pending(&mut self) {
        self.pending_raw_fd = None;
        self.extra_pending_close_raw_fd = None;
    }

    fn validate_pending_handle(&self, raw_fd: RawFd) -> AsyncHostResult<()> {
        // The import boundary may receive malformed/stale fd handles. Validate
        // before asserting the internal "pending operation uses submitter fd"
        // invariant so debug builds do not panic on bad guest input.
        if let Some(pending_raw_fd) = self.pending_raw_fd
            && pending_raw_fd != raw_fd
        {
            return Err(AsyncHostError::Badf);
        }
        debug_assert!(
            match self.pending_raw_fd {
                Some(pending_raw_fd) => pending_raw_fd == raw_fd,
                None => true,
            },
            "pending IO operation must use the submitting handle"
        );
        Ok(())
    }

    fn cancel_pending(&mut self) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
        use windows_sys::Win32::System::IO::CancelIoEx;

        let Some(raw_fd) = self.pending_raw_fd else {
            return Ok(0);
        };
        let overlapped = self.overlapped_ptr();
        if unsafe { CancelIoEx(raw_fd, overlapped) } == 0 {
            let errno = last_errno();
            if errno != ERROR_NOT_FOUND as i32 {
                return Err(AsyncHostError::Native(errno));
            }
            // The operation may have completed after the cancellation request
            // raced with IOCP delivery. Keep the result pending until the
            // completion packet is consumed through poll_event_io_result.
        }
        Ok(1)
    }

    fn cancel_and_drain_pending(&mut self) -> AsyncHostResult<()> {
        use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
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
        // With bWait=TRUE the operation has reached a final status when this
        // returns, even if the final status is an error such as EOF or broken
        // pipe. At that point the host no longer treats the result as pending.
        let _ = unsafe { GetOverlappedResult(raw_fd, overlapped, &mut bytes_transferred, 1) };
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
    // These cells split synchronization by native-shaped owner. They are not
    // separate guest concepts: resource handles and ABI values stay unchanged.
    errno: Mutex<i32>,
    addr_infos: Mutex<SlotMap<HostAddrInfoKey, HostAddrInfo>>,
    c_buffers: Mutex<SlotMap<HostCBufferKey, HostCBuffer>>,
    #[cfg(windows)]
    io_results: Mutex<IoResultTable>,
    jobs: Arc<Mutex<JobTable>>,
    polls: Mutex<PollTable>,
    thread_pool_completions: Mutex<ThreadPoolCompletions>,
    files: Mutex<FileTable>,
    workers: Mutex<WorkerTable>,
    completed_jobs: Arc<Mutex<Vec<CompletedJob>>>,
}

impl Default for AsyncHost {
    fn default() -> Self {
        Self {
            errno: Mutex::new(0),
            addr_infos: Mutex::new(SlotMap::with_key()),
            c_buffers: Mutex::new(SlotMap::with_key()),
            #[cfg(windows)]
            io_results: Mutex::new(IoResultTable::default()),
            jobs: Arc::new(Mutex::new(JobTable::default())),
            polls: Mutex::new(PollTable::default()),
            thread_pool_completions: Mutex::new(ThreadPoolCompletions::default()),
            files: Mutex::new(FileTable::default()),
            workers: Mutex::new(WorkerTable::default()),
            completed_jobs: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl AsyncHost {
    pub(crate) fn invalid_fd(&self) -> HostHandle {
        self.files.lock().unwrap().invalid_fd()
    }

    fn restore_completed_jobs(&self) {
        let completed_jobs = {
            let mut completed_jobs = self.completed_jobs.lock().unwrap();
            if completed_jobs.is_empty() {
                return;
            }
            std::mem::take(&mut *completed_jobs)
        };

        for CompletedJob { key, job } in completed_jobs {
            let _ = self.restore_job(key, job);
        }
    }

    pub(crate) fn get_errno(&self) -> i32 {
        *self.errno.lock().unwrap()
    }

    pub(crate) fn set_errno(&self, errno: i32) {
        *self.errno.lock().unwrap() = errno;
    }

    pub(crate) fn record_error(&self, error: AsyncHostError) -> i32 {
        let errno = error.errno();
        self.set_errno(errno);
        errno
    }

    fn restore_job(&self, key: HostJobKey, job: Job) -> AsyncHostResult<()> {
        let mut job = Some(job);
        let restored = {
            let mut jobs = self.jobs.lock().unwrap();
            match jobs.jobs.get_mut(key) {
                Some(slot) if slot.is_none() => {
                    *slot = job.take();
                    true
                }
                _ => false,
            }
        };
        if restored {
            return Ok(());
        }

        let mut job = job.expect("job is only taken after being restored");
        self.discard_job_results(&mut job);
        Err(AsyncHostError::Badf)
    }

    fn discard_job_results(&self, job: &mut Job) {
        if let Some(result) = thread_pool::take_open_job_result(job)
            && let OpenJobResource::Published(fd) = result.resource
        {
            let _ = self.files.lock().unwrap().remove_file(fd);
        }
    }

    fn publish_open_job_result(&self, key: HostJobKey) -> AsyncHostResult<HostHandle> {
        let mut jobs = self.jobs.lock().unwrap();
        let placeholder = OpenJobResource::Published(self.invalid_fd());
        let file = {
            let job = jobs
                .jobs
                .get_mut(key)
                .and_then(Option::as_mut)
                .ok_or(AsyncHostError::Badf)?;
            let result = thread_pool::open_job_result_mut(job)?;
            match std::mem::replace(&mut result.resource, placeholder) {
                OpenJobResource::Published(fd) => {
                    result.resource = OpenJobResource::Published(fd);
                    return Ok(fd);
                }
                OpenJobResource::Unpublished(file) => file,
            }
        };

        let fd = self.files.lock().unwrap().insert_file_resource(file);
        let job = jobs
            .jobs
            .get_mut(key)
            .and_then(Option::as_mut)
            .ok_or(AsyncHostError::Badf)?;
        let result = thread_pool::open_job_result_mut(job)?;
        result.resource = OpenJobResource::Published(fd);
        thread_pool::open_job_get_fd(result)
    }

    pub(crate) fn assert_no_leaked_handles_if_enabled(&self) {
        if std::thread::panicking() || std::env::var_os(CHECK_FD_LEAK_ENV).is_none() {
            return;
        }
        self.restore_completed_jobs();

        let summary = {
            let mut leaks = Vec::new();

            {
                let c_buffers = self.c_buffers.lock().unwrap();
                if !c_buffers.is_empty() {
                    leaks.push(format!("c_buffers={}", c_buffers.len()));
                }
            }
            {
                let addr_infos = self.addr_infos.lock().unwrap();
                if !addr_infos.is_empty() {
                    leaks.push(format!("addr_infos={}", addr_infos.len()));
                }
            }
            #[cfg(windows)]
            {
                let io_results = self.io_results.lock().unwrap();
                if !io_results.io_results.is_empty() {
                    leaks.push(format!("io_results={}", io_results.io_results.len()));
                }
                if !io_results.io_results_by_overlapped.is_empty() {
                    leaks.push(format!(
                        "io_results_by_overlapped={}",
                        io_results.io_results_by_overlapped.len()
                    ));
                }
            }
            {
                let jobs = self.jobs.lock().unwrap();
                if !jobs.jobs.is_empty() {
                    leaks.push(format!("jobs={}", jobs.jobs.len()));
                }
            }
            {
                let polls = self.polls.lock().unwrap();
                if !polls.polls.is_empty() {
                    leaks.push(format!("polls={}", polls.polls.len()));
                }
            }
            {
                let completions = self.thread_pool_completions.lock().unwrap();
                #[cfg(unix)]
                {
                    if completions.notifier.is_some() {
                        leaks.push("completion_notifier=1".to_string());
                    }
                    if completions.source.is_some() {
                        leaks.push("completion_source=1".to_string());
                    }
                }
                #[cfg(windows)]
                {
                    if completions.target.is_some() {
                        leaks.push("completion_port=1".to_string());
                    }
                }
            }
            {
                let files = self.files.lock().unwrap();
                let leaked_files = match files.files.get(files.invalid_file) {
                    Some(file) if file.is_invalid() => files.files.len().saturating_sub(1),
                    Some(_) => {
                        leaks.push("invalid_file=valid".to_string());
                        files.files.len()
                    }
                    None => {
                        leaks.push("invalid_file=missing".to_string());
                        files.files.len()
                    }
                };
                if leaked_files != 0 {
                    leaks.push(format!("files={leaked_files}"));
                }
            }
            {
                let workers = self.workers.lock().unwrap();
                if !workers.workers.is_empty() {
                    leaks.push(format!("workers={}", workers.workers.len()));
                }
            }

            (!leaks.is_empty()).then(|| leaks.join(", "))
        };

        if let Some(summary) = summary {
            panic!("moonrun async host leaked handles: {summary}");
        }
    }

    pub(crate) fn poll_create(&self) -> AsyncHostResult<u64> {
        let instance = poll::poll_create()?;
        let key = self
            .polls
            .lock()
            .unwrap()
            .polls
            .insert(Arc::new(Mutex::new(HostPoll {
                instance,
                registered_fds: HashMap::new(),
                #[cfg(unix)]
                completion_notifier: None,
                event_fd_handles: Vec::new(),
            })));
        Ok(handle_from_key(key))
    }

    pub(crate) fn poll_destroy(&self, handle: u64) -> AsyncHostResult<()> {
        let poll_key = key_from_handle::<HostPollKey>(handle);
        let poll = {
            let mut polls = self.polls.lock().unwrap();
            polls.polls.remove(poll_key).ok_or(AsyncHostError::Badf)?
        };
        let poll = Arc::try_unwrap(poll).map_err(|_| AsyncHostError::Inval)?;
        let poll = poll.into_inner().unwrap();

        {
            let mut polls = self.polls.lock().unwrap();
            if polls.current_event_poll == Some(poll_key) {
                polls.current_event_poll = None;
            }
        }

        #[cfg(unix)]
        let completion_source = {
            let mut completions = self.thread_pool_completions.lock().unwrap();
            if let Some(notifier) = &poll.completion_notifier
                && completions
                    .notifier
                    .as_ref()
                    .is_some_and(|active| Arc::ptr_eq(active, notifier))
            {
                completions.notifier = None;
                completions.source.take()
            } else {
                None
            }
        };
        #[cfg(unix)]
        {
            if let Some(source) = completion_source {
                let _ = self.files.lock().unwrap().remove_file(source);
            }
        }
        #[cfg(windows)]
        {
            let mut completions = self.thread_pool_completions.lock().unwrap();
            if completions
                .target
                .is_some_and(|target| target.poll == poll_key)
            {
                completions.target = None;
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
        let raw_fd = self.files.lock().unwrap().file(fd_handle)?.raw_fd();
        let poll = Arc::clone(
            self.polls
                .lock()
                .unwrap()
                .polls
                .get(key_from_handle::<HostPollKey>(poll_handle))
                .ok_or(AsyncHostError::Badf)?,
        );
        let mut poll = poll.lock().unwrap();
        poll::poll_register(&poll.instance, raw_fd, read_only)?;
        poll.registered_fds.insert(raw_fd_key(raw_fd), fd_handle);
        Ok(())
    }

    pub(crate) fn poll_wait(&self, poll_handle: u64, timeout_ms: i32) -> AsyncHostResult<i32> {
        let poll_key = key_from_handle::<HostPollKey>(poll_handle);
        #[cfg(windows)]
        let (poll, thread_pool_generation, invalid_fd) = {
            let poll = Arc::clone(
                self.polls
                    .lock()
                    .unwrap()
                    .polls
                    .get(poll_key)
                    .ok_or(AsyncHostError::Badf)?,
            );
            let thread_pool_generation = self
                .thread_pool_completions
                .lock()
                .unwrap()
                .target
                .filter(|target| target.poll == poll_key)
                .map(|target| target.generation);
            (poll, thread_pool_generation, self.invalid_fd())
        };
        #[cfg(not(windows))]
        let poll = {
            Arc::clone(
                self.polls
                    .lock()
                    .unwrap()
                    .polls
                    .get(poll_key)
                    .ok_or(AsyncHostError::Badf)?,
            )
        };
        let result = {
            let mut poll_guard = poll.lock().unwrap();
            #[cfg(not(windows))]
            let result = poll::poll_wait(&mut poll_guard.instance, timeout_ms)?;
            #[cfg(windows)]
            let result = {
                let deadline = (timeout_ms >= 0).then(|| {
                    std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms as u64)
                });
                let mut next_timeout = timeout_ms;
                loop {
                    poll::poll_wait(&mut poll_guard.instance, next_timeout)?;
                    let result = poll::retain_current_thread_pool_completions(
                        &mut poll_guard.instance,
                        thread_pool_generation,
                    )?;
                    if result != 0 || timeout_ms == 0 {
                        break result;
                    }
                    let Some(deadline) = deadline else {
                        continue;
                    };
                    let now = std::time::Instant::now();
                    if now >= deadline {
                        break 0;
                    }
                    next_timeout = i32::try_from(deadline.duration_since(now).as_millis())
                        .unwrap_or(i32::MAX)
                        .max(1);
                }
            };
            poll_guard.event_fd_handles.clear();
            for index in 0..result {
                let event = poll::event_list_get(&poll_guard.instance, index)?;
                let raw_fd = poll::event_get_fd(event);
                let fd_handle = poll_guard
                    .registered_fds
                    .get(&raw_fd_key(raw_fd))
                    .copied()
                    .or_else(|| completion_event_fd(raw_fd));
                #[cfg(windows)]
                let fd_handle = fd_handle.or(Some(invalid_fd));
                poll_guard.event_fd_handles.push(fd_handle);
            }
            result
        };
        {
            self.polls.lock().unwrap().current_event_poll = Some(poll_key);
        }
        self.restore_completed_jobs();
        Ok(result)
    }

    pub(crate) fn poll_get_event(&self, poll_handle: u64, index: i32) -> AsyncHostResult<u64> {
        let poll_key = key_from_handle::<HostPollKey>(poll_handle);
        let poll = {
            let polls = self.polls.lock().unwrap();
            if polls.current_event_poll != Some(poll_key) {
                return Err(AsyncHostError::Badf);
            }
            Arc::clone(polls.polls.get(poll_key).ok_or(AsyncHostError::Badf)?)
        };
        let poll = poll.lock().unwrap();
        poll::event_list_get(&poll.instance, index)?;
        u64::try_from(index).map_err(|_| AsyncHostError::Fault)
    }

    fn with_event<T>(
        &self,
        event_handle: u64,
        f: impl FnOnce(&HostPoll, &poll::PollEvent) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let index = event_index(event_handle)?;
        let poll = {
            let polls = self.polls.lock().unwrap();
            let poll_key = polls.current_event_poll.ok_or(AsyncHostError::Badf)?;
            Arc::clone(polls.polls.get(poll_key).ok_or(AsyncHostError::Badf)?)
        };
        let poll = poll.lock().unwrap();
        let poll_event = poll::event_list_get(&poll.instance, index)?;
        f(&poll, poll_event)
    }

    pub(crate) fn poll_event_fd(&self, event_handle: u64) -> AsyncHostResult<HostHandle> {
        let index = event_index(event_handle)?;
        let poll = {
            let polls = self.polls.lock().unwrap();
            let poll_key = polls.current_event_poll.ok_or(AsyncHostError::Badf)?;
            Arc::clone(polls.polls.get(poll_key).ok_or(AsyncHostError::Badf)?)
        };
        let fd = {
            let poll = poll.lock().unwrap();
            poll::event_list_get(&poll.instance, index)?;
            let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
            poll.event_fd_handles
                .get(index)
                .copied()
                .flatten()
                .ok_or(AsyncHostError::Badf)?
        };
        #[cfg(windows)]
        let is_thread_pool_completion =
            fd == raw_fd_to_guest(windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE)?;
        #[cfg(not(windows))]
        let is_thread_pool_completion = false;
        let files = self.files.lock().unwrap();
        if fd == files.invalid_fd()
            || is_thread_pool_completion
            || files
                .files
                .contains_key(key_from_handle::<FileResourceKey>(fd))
        {
            Ok(fd)
        } else {
            Err(AsyncHostError::Badf)
        }
    }

    #[cfg(unix)]
    pub(crate) fn poll_event_events(&self, event_handle: u64) -> AsyncHostResult<i32> {
        self.with_event(event_handle, |_, event| Ok(poll::event_get_events(event)))
    }

    #[cfg(windows)]
    pub(crate) fn poll_event_io_result(&self, event_handle: u64) -> AsyncHostResult<u64> {
        let overlapped = self.with_event(event_handle, |_, event| {
            Ok(OverlappedAddr::from_ptr(poll::event_get_io_result(event)))
        })?;
        let mut io_results = self.io_results.lock().unwrap();
        let key = io_results
            .io_results_by_overlapped
            .get(&overlapped)
            .copied()
            .ok_or(AsyncHostError::Badf)?;
        let result = io_results
            .io_results
            .get_mut(key)
            .ok_or(AsyncHostError::Badf)?;
        result.clear_pending();
        Ok(handle_from_key(key))
    }

    #[cfg(windows)]
    pub(crate) fn poll_event_bytes_transferred(&self, event_handle: u64) -> AsyncHostResult<i32> {
        self.with_event(event_handle, |_, event| {
            Ok(poll::event_get_bytes_transferred(event))
        })
    }

    pub(crate) fn init_thread_pool(&self, poll_handle: u64) -> AsyncHostResult<HostHandle> {
        let poll_key = key_from_handle::<HostPollKey>(poll_handle);
        let poll = {
            let polls = self.polls.lock().unwrap();
            Arc::clone(polls.polls.get(poll_key).ok_or(AsyncHostError::Badf)?)
        };
        #[cfg(unix)]
        if self
            .thread_pool_completions
            .lock()
            .unwrap()
            .source
            .is_some()
        {
            return Err(AsyncHostError::Inval);
        }
        #[cfg(windows)]
        if self
            .thread_pool_completions
            .lock()
            .unwrap()
            .target
            .is_some()
        {
            return Err(AsyncHostError::Inval);
        }
        #[cfg(unix)]
        {
            let (completion_notifier, event_fd) = {
                let poll = poll.lock().unwrap();
                ThreadPoolCompletionNotifier::new(&poll.instance)?
            };
            let completion_notifier = Arc::new(completion_notifier);
            let source = {
                let mut completions = self.thread_pool_completions.lock().unwrap();
                if completions.source.is_some() {
                    drop(completions);
                    let poll = poll.lock().unwrap();
                    let _ = poll::poll_unregister(&poll.instance, event_fd);
                    drop(FileResource::new(event_fd));
                    return Err(AsyncHostError::Inval);
                }
                let source = self
                    .files
                    .lock()
                    .unwrap()
                    .insert_file_resource(FileResource::new(event_fd));
                completions.notifier = Some(Arc::clone(&completion_notifier));
                completions.source = Some(source);
                drop(completions);
                let mut poll = poll.lock().unwrap();
                poll.registered_fds.insert(raw_fd_key(event_fd), source);
                poll.completion_notifier = Some(completion_notifier);
                source
            };
            Ok(source)
        }
        #[cfg(windows)]
        {
            let completion_port = poll::CompletionPort::from_poll(&poll.lock().unwrap().instance);
            let mut completions = self.thread_pool_completions.lock().unwrap();
            if completions.target.is_some() {
                return Err(AsyncHostError::Inval);
            }
            let generation = completions.advance_generation();
            completions.target = Some(ThreadPoolCompletionTarget {
                poll: poll_key,
                port: completion_port,
                generation,
            });
            raw_fd_to_guest(windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE)
        }
    }

    pub(crate) fn destroy_thread_pool(&self) {
        let workers = {
            let mut workers = self.workers.lock().unwrap();
            let keys: Vec<_> = workers.workers.keys().collect();
            keys.into_iter()
                .filter_map(|key| workers.workers.remove(key))
                .collect::<Vec<_>>()
        };
        for worker in workers {
            let _ = thread_pool::free_worker(worker);
        }
        self.restore_completed_jobs();

        #[cfg(unix)]
        {
            let completion_source = {
                let mut completions = self.thread_pool_completions.lock().unwrap();
                let completion_source = completions.source.take();
                completions.notifier = None;
                completion_source
            };
            let polls = self
                .polls
                .lock()
                .unwrap()
                .polls
                .values()
                .cloned()
                .collect::<Vec<_>>();
            if let Some(source) = completion_source
                && let Ok(file) = self.files.lock().unwrap().remove_file(source)
            {
                let raw_fd = file.raw_fd();
                for poll in &polls {
                    let mut poll = poll.lock().unwrap();
                    if poll.registered_fds.contains_key(&raw_fd_key(raw_fd)) {
                        let _ = poll::poll_unregister(&poll.instance, raw_fd);
                    }
                    poll.registered_fds.remove(&raw_fd_key(raw_fd));
                }
            }
            for poll in polls {
                poll.lock().unwrap().completion_notifier = None;
            }
        }
        #[cfg(windows)]
        {
            self.thread_pool_completions.lock().unwrap().target = None;
        }
    }

    pub(crate) fn insert_c_buffer(&self, buffer: Box<[u8]>) -> u64 {
        let key = self
            .c_buffers
            .lock()
            .unwrap()
            .insert(Arc::new(Mutex::new(buffer)));
        handle_from_key(key)
    }

    pub(crate) fn free_c_buffer(&self, handle: u64) -> AsyncHostResult<()> {
        if handle == INVALID_HOST_HANDLE {
            return Ok(());
        }
        self.c_buffers
            .lock()
            .unwrap()
            .remove(key_from_handle::<HostCBufferKey>(handle))
            .map(|_| ())
            .ok_or(AsyncHostError::Badf)
    }

    pub(crate) fn with_c_buffer<T>(
        &self,
        handle: u64,
        f: impl FnOnce(&[u8]) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        // A c_buffer handle always names a whole host-owned buffer entry.
        // Callers that need a subrange must pass explicit offset/length
        // arguments; never reinterpret raw or interior pointers as handles.
        let buffer = self.c_buffer(handle)?;
        let buffer = buffer.lock().unwrap();
        f(buffer.as_ref())
    }

    pub(crate) fn with_c_buffer_mut<T>(
        &self,
        handle: u64,
        f: impl FnOnce(&mut [u8]) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let buffer = self.c_buffer(handle)?;
        let mut buffer = buffer.lock().unwrap();
        f(buffer.as_mut())
    }

    pub(crate) fn c_buffer(&self, handle: u64) -> AsyncHostResult<HostCBuffer> {
        if handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }
        self.c_buffers
            .lock()
            .unwrap()
            .get(key_from_handle::<HostCBufferKey>(handle))
            .cloned()
            .ok_or(AsyncHostError::Badf)
    }

    pub(crate) fn insert_job(&self, job: Job) -> AsyncHostResult<u64> {
        let key = self.jobs.lock().unwrap().jobs.insert(Some(job));
        Ok(handle_from_key(key))
    }

    pub(crate) fn free_job(&self, handle: u64) -> AsyncHostResult<()> {
        self.restore_completed_jobs();
        self.jobs
            .lock()
            .unwrap()
            .jobs
            .remove(key_from_handle::<HostJobKey>(handle))
            .map(|_| ())
            .ok_or(AsyncHostError::Badf)
    }

    pub(crate) fn job_get_ret(&self, handle: u64) -> AsyncHostResult<i64> {
        self.restore_completed_jobs();
        let jobs = self.jobs.lock().unwrap();
        let job = jobs
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        Ok(crate::async_sys::internal::event_loop::thread_pool::job_get_ret(job))
    }

    pub(crate) fn job_get_err(&self, handle: u64) -> AsyncHostResult<i32> {
        self.restore_completed_jobs();
        let jobs = self.jobs.lock().unwrap();
        let job = jobs
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        Ok(crate::async_sys::internal::event_loop::thread_pool::job_get_err(job))
    }

    pub(crate) fn open_job_get_fd(&self, handle: u64) -> AsyncHostResult<HostHandle> {
        self.restore_completed_jobs();
        self.publish_open_job_result(key_from_handle::<HostJobKey>(handle))
    }

    pub(crate) fn open_job_get_kind(&self, handle: u64) -> AsyncHostResult<i32> {
        self.restore_completed_jobs();
        let jobs = self.jobs.lock().unwrap();
        let job = jobs
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        let result = thread_pool::open_job_result(job)?;
        Ok(thread_pool::open_job_get_kind(result))
    }

    pub(crate) fn open_job_get_dev_id(&self, handle: u64) -> AsyncHostResult<u64> {
        self.restore_completed_jobs();
        let jobs = self.jobs.lock().unwrap();
        let job = jobs
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        let result = thread_pool::open_job_result(job)?;
        Ok(thread_pool::open_job_get_dev_id(result))
    }

    pub(crate) fn open_job_get_file_id(&self, handle: u64) -> AsyncHostResult<u64> {
        self.restore_completed_jobs();
        let jobs = self.jobs.lock().unwrap();
        let job = jobs
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        let result = thread_pool::open_job_result(job)?;
        Ok(thread_pool::open_job_get_file_id(result))
    }

    pub(crate) fn get_file_size_result(&self, handle: u64) -> AsyncHostResult<i64> {
        self.restore_completed_jobs();
        let jobs = self.jobs.lock().unwrap();
        let job = jobs
            .jobs
            .get(key_from_handle::<HostJobKey>(handle))
            .and_then(Option::as_ref)
            .ok_or(AsyncHostError::Badf)?;
        crate::async_sys::internal::event_loop::thread_pool::get_file_size_result(job)
    }

    pub(crate) fn get_getaddrinfo_result(&self, handle: u64) -> AsyncHostResult<u64> {
        self.restore_completed_jobs();
        let addrs: Vec<Box<[u8]>> = {
            let jobs = self.jobs.lock().unwrap();
            let job = jobs
                .jobs
                .get(key_from_handle::<HostJobKey>(handle))
                .and_then(Option::as_ref)
                .ok_or(AsyncHostError::Badf)?;
            thread_pool::getaddrinfo_job_result(job)?.to_vec()
        };
        let mut addr_infos = self.addr_infos.lock().unwrap();
        let mut next = None;
        for addr in addrs.into_iter().rev() {
            let key = addr_infos.insert(HostAddrInfo { addr, next });
            next = Some(key);
        }
        Ok(next.map(handle_from_key).unwrap_or(INVALID_HOST_HANDLE))
    }

    pub(crate) fn addrinfo_next(&self, handle: u64) -> AsyncHostResult<u64> {
        if handle == INVALID_HOST_HANDLE {
            return Ok(INVALID_HOST_HANDLE);
        }
        let addr_infos = self.addr_infos.lock().unwrap();
        let addrinfo = addr_infos
            .get(key_from_handle::<HostAddrInfoKey>(handle))
            .ok_or(AsyncHostError::Badf)?;
        Ok(addrinfo
            .next
            .map(handle_from_key)
            .unwrap_or(INVALID_HOST_HANDLE))
    }

    pub(crate) fn addrinfo_addr(&self, handle: u64) -> AsyncHostResult<Box<[u8]>> {
        let addr_infos = self.addr_infos.lock().unwrap();
        let addrinfo = addr_infos
            .get(key_from_handle::<HostAddrInfoKey>(handle))
            .ok_or(AsyncHostError::Badf)?;
        Ok(addrinfo.addr.clone())
    }

    pub(crate) fn free_addrinfo(&self, handle: u64) -> AsyncHostResult<()> {
        if handle == INVALID_HOST_HANDLE {
            return Ok(());
        }
        let mut addr_infos = self.addr_infos.lock().unwrap();
        let mut current = Some(key_from_handle::<HostAddrInfoKey>(handle));
        while let Some(key) = current {
            let addrinfo = addr_infos.remove(key).ok_or(AsyncHostError::Badf)?;
            current = addrinfo.next;
        }
        Ok(())
    }

    pub(crate) fn close_fd(&self, handle: HostHandle) -> AsyncHostResult<()> {
        let raw_fd = self.files.lock().unwrap().file(handle)?.raw_fd();
        #[cfg(windows)]
        if self
            .io_results
            .lock()
            .unwrap()
            .has_pending_io_for_raw_fd(raw_fd)
        {
            return Err(AsyncHostError::Inval);
        }
        #[cfg(unix)]
        {
            let completion_source_closed = {
                let mut completions = self.thread_pool_completions.lock().unwrap();
                if completions.source == Some(handle) {
                    completions.source = None;
                    completions.notifier = None;
                    true
                } else {
                    false
                }
            };
            if completion_source_closed {
                let polls = self
                    .polls
                    .lock()
                    .unwrap()
                    .polls
                    .values()
                    .cloned()
                    .collect::<Vec<_>>();
                for poll in polls {
                    poll.lock().unwrap().completion_notifier = None;
                }
            }
        }
        let polls = self
            .polls
            .lock()
            .unwrap()
            .polls
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for poll in polls {
            let mut poll = poll.lock().unwrap();
            if poll.registered_fds.contains_key(&raw_fd_key(raw_fd)) {
                #[cfg(unix)]
                poll::poll_unregister(&poll.instance, raw_fd)?;
            }
            poll.registered_fds.remove(&raw_fd_key(raw_fd));
        }
        self.files.lock().unwrap().remove_file(handle)?;
        Ok(())
    }

    pub(crate) fn insert_socket_resource(&self, raw_socket: RawSocket) -> HostHandle {
        #[cfg(unix)]
        let file = FileResource::new(raw_socket);
        #[cfg(windows)]
        let file = FileResource::new_socket(raw_socket);
        self.files.lock().unwrap().insert_file_resource(file)
    }

    pub(crate) fn with_raw_file<T>(
        &self,
        handle: HostHandle,
        f: impl FnOnce(RawFd) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let raw_fd = self.files.lock().unwrap().file(handle)?.raw_fd();
        f(raw_fd)
    }

    pub(crate) fn file_resource(&self, handle: HostHandle) -> AsyncHostResult<FileResourceRef> {
        self.files.lock().unwrap().file(handle)
    }

    pub(crate) fn pipe(
        &self,
        read_end_is_async: bool,
        write_end_is_async: bool,
    ) -> AsyncHostResult<[HostHandle; 2]> {
        let mut files = self.files.lock().unwrap();
        crate::async_sys::internal::fd_util::stub::pipe_file_resources(
            &mut files.files,
            read_end_is_async,
            write_end_is_async,
        )
    }

    #[cfg(unix)]
    pub(crate) fn set_nonblocking(&self, handle: HostHandle) -> AsyncHostResult<()> {
        let raw_fd = self.files.lock().unwrap().file(handle)?.raw_fd();
        crate::async_sys::internal::fd_util::stub::set_nonblocking(raw_fd)
    }

    pub(crate) fn set_cloexec(&self, handle: HostHandle) -> AsyncHostResult<()> {
        let raw_fd = self.files.lock().unwrap().file(handle)?.raw_fd();
        #[cfg(unix)]
        {
            crate::async_sys::internal::fd_util::stub::set_cloexec(raw_fd)
        }
        #[cfg(windows)]
        {
            let _ = raw_fd;
            Ok(())
        }
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
        let raw_fd = self.files.lock().unwrap().file(handle)?.raw_fd();
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
        let raw_fd = self.files.lock().unwrap().file(handle)?.raw_fd();
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

        let mut io_results = self.io_results.lock().unwrap();
        let key = io_results.io_results.insert(result);
        let overlapped = io_results
            .io_results
            .get_mut(key)
            .ok_or(AsyncHostError::Badf)?
            .overlapped_addr();
        io_results.io_results_by_overlapped.insert(overlapped, key);
        Ok(handle_from_key(key))
    }

    #[cfg(windows)]
    pub(crate) fn make_socket_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        event: i32,
        buf: i32,
        offset: i32,
        len: i32,
        flags: i32,
    ) -> AsyncHostResult<u64> {
        let guest_offset = buf.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        memory.read_exact(guest_offset, len)?;
        let buffer = vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?];
        self.insert_io_result(HostIoResult::for_socket(event, buffer, guest_offset, flags))
    }

    #[cfg(windows)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn make_socket_with_addr_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        event: i32,
        buf: i32,
        offset: i32,
        len: i32,
        flags: i32,
        addr: i32,
        addr_len: i32,
    ) -> AsyncHostResult<u64> {
        let guest_offset = buf.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        memory.read_exact(guest_offset, len)?;
        let buffer = vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?];
        let addr_buffer = memory.read_exact(addr, addr_len)?.to_vec();
        self.insert_io_result(HostIoResult::for_socket_with_addr(
            event,
            buffer,
            guest_offset,
            flags,
            addr_buffer,
            addr_len,
            addr,
        ))
    }

    #[cfg(windows)]
    pub(crate) fn make_connect_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        addr: i32,
        addr_len: i32,
    ) -> AsyncHostResult<u64> {
        let addr_buffer = memory.read_exact(addr, addr_len)?.to_vec();
        self.insert_io_result(HostIoResult::for_connect(addr_buffer))
    }

    #[cfg(windows)]
    pub(crate) fn make_accept_io_result(&self, addr_len: i32) -> AsyncHostResult<u64> {
        self.insert_io_result(HostIoResult::for_accept(addr_len)?)
    }

    #[cfg(windows)]
    fn insert_io_result(&self, result: HostIoResult) -> AsyncHostResult<u64> {
        let mut io_results = self.io_results.lock().unwrap();
        let key = io_results.io_results.insert(Box::new(result));
        let overlapped = io_results
            .io_results
            .get_mut(key)
            .ok_or(AsyncHostError::Badf)?
            .overlapped_addr();
        io_results.io_results_by_overlapped.insert(overlapped, key);
        Ok(handle_from_key(key))
    }

    #[cfg(windows)]
    pub(crate) fn free_io_result(&self, handle: u64) -> AsyncHostResult<()> {
        let key = key_from_handle::<HostIoResultKey>(handle);
        let mut io_results = self.io_results.lock().unwrap();
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
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn io_result_get_event(&self, handle: u64) -> AsyncHostResult<i32> {
        let io_results = self.io_results.lock().unwrap();
        let result = io_results
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
        let raw_fd = self.files.lock().unwrap().file(fd_handle)?.raw_fd();
        let mut io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get_mut(key_from_handle::<HostIoResultKey>(result_handle))
            .ok_or(AsyncHostError::Badf)?;
        result.validate_pending_handle(raw_fd)?;
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

        let raw_fd = self.files.lock().unwrap().file(fd_handle)?.raw_fd();
        let mut io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get_mut(key_from_handle::<HostIoResultKey>(result_handle))
            .ok_or(AsyncHostError::Badf)?;
        result.validate_pending_handle(raw_fd)?;
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
        use windows_sys::Win32::Networking::WinSock as ws;
        use windows_sys::Win32::Storage::FileSystem::ReadFile;

        let raw_fd = self.files.lock().unwrap().file(fd_handle)?.raw_fd();
        let mut io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get_mut(key_from_handle::<HostIoResultKey>(result_handle))
            .ok_or(AsyncHostError::Badf)?;
        if result.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        result.direction = Some(HostIoDirection::Read);
        let mut bytes_transferred = 0;
        let success = match result.kind {
            HostIoKind::File => {
                let len = u32::try_from(result.buffer.len()).map_err(|_| AsyncHostError::Fault)?;
                unsafe {
                    ReadFile(
                        raw_fd,
                        result.buffer.as_mut_ptr().cast(),
                        len,
                        &mut bytes_transferred,
                        result.overlapped_ptr(),
                    )
                }
            }
            HostIoKind::Socket => {
                let buffer = socket_buffer(&mut result.buffer)?;
                unsafe {
                    i32::from(
                        ws::WSARecv(
                            raw_fd as usize,
                            &buffer,
                            1,
                            &mut bytes_transferred,
                            &mut result.socket_flags,
                            result.overlapped_ptr(),
                            None,
                        ) == 0,
                    )
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
                unsafe {
                    i32::from(
                        ws::WSARecvFrom(
                            raw_fd as usize,
                            &buffer,
                            1,
                            &mut bytes_transferred,
                            flags,
                            addr,
                            addr_len,
                            overlapped,
                            None,
                        ) == 0,
                    )
                }
            }
            HostIoKind::Connect | HostIoKind::Accept => return Err(AsyncHostError::Inval),
        };
        if success != 0 {
            let bytes_transferred =
                i32::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)?;
            result.copy_read_result(memory, bytes_transferred)?;
            return Ok(bytes_transferred);
        }
        let errno = match result.kind {
            HostIoKind::Socket | HostIoKind::SocketWithAddr => last_wsa_errno(),
            _ => last_errno(),
        };
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
        use windows_sys::Win32::Networking::WinSock as ws;
        use windows_sys::Win32::Storage::FileSystem::WriteFile;

        let raw_fd = self.files.lock().unwrap().file(fd_handle)?.raw_fd();
        let mut io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get_mut(key_from_handle::<HostIoResultKey>(result_handle))
            .ok_or(AsyncHostError::Badf)?;
        if result.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        result.direction = Some(HostIoDirection::Write);
        let len_i32 = i32::try_from(result.buffer.len()).map_err(|_| AsyncHostError::Fault)?;
        if matches!(
            result.kind,
            HostIoKind::File | HostIoKind::Socket | HostIoKind::SocketWithAddr
        ) {
            let data = memory.read_exact(result.guest_offset, len_i32)?;
            result.buffer.copy_from_slice(data);
        }
        let mut bytes_transferred = 0;
        let success = match result.kind {
            HostIoKind::File => {
                let len = u32::try_from(result.buffer.len()).map_err(|_| AsyncHostError::Fault)?;
                unsafe {
                    WriteFile(
                        raw_fd,
                        result.buffer.as_ptr().cast(),
                        len,
                        &mut bytes_transferred,
                        result.overlapped_ptr(),
                    )
                }
            }
            HostIoKind::Socket => {
                let buffer = socket_buffer(&mut result.buffer)?;
                unsafe {
                    i32::from(
                        ws::WSASend(
                            raw_fd as usize,
                            &buffer,
                            1,
                            &mut bytes_transferred,
                            result.socket_flags,
                            result.overlapped_ptr(),
                            None,
                        ) == 0,
                    )
                }
            }
            HostIoKind::SocketWithAddr => {
                let buffer = socket_buffer(&mut result.buffer)?;
                let addr_len =
                    i32::try_from(result.addr_buffer.len()).map_err(|_| AsyncHostError::Fault)?;
                unsafe {
                    i32::from(
                        ws::WSASendTo(
                            raw_fd as usize,
                            &buffer,
                            1,
                            &mut bytes_transferred,
                            result.socket_flags,
                            result.addr_buffer.as_ptr().cast::<ws::SOCKADDR>(),
                            addr_len,
                            result.overlapped_ptr(),
                            None,
                        ) == 0,
                    )
                }
            }
            HostIoKind::Connect | HostIoKind::Accept => return Err(AsyncHostError::Inval),
        };
        if success != 0 {
            i32::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)
        } else {
            let errno = match result.kind {
                HostIoKind::Socket | HostIoKind::SocketWithAddr => last_wsa_errno(),
                _ => last_errno(),
            };
            let error = AsyncHostError::Native(errno);
            if matches!(error, AsyncHostError::Native(errno) if errno == ERROR_IO_PENDING as i32) {
                result.mark_pending(raw_fd)?;
            }
            Err(error)
        }
    }

    #[cfg(windows)]
    pub(crate) fn connect_io_result(
        &self,
        fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Networking::WinSock as ws;

        let raw_fd = self.files.lock().unwrap().file(fd_handle)?.raw_fd();
        let mut io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get_mut(key_from_handle::<HostIoResultKey>(result_handle))
            .ok_or(AsyncHostError::Badf)?;
        if result.kind != HostIoKind::Connect || result.is_pending() {
            return Err(AsyncHostError::Inval);
        }

        bind_any_for_connect(raw_fd, &result.addr_buffer)?;
        let connect_ex = get_wsa_extension::<ws::LPFN_CONNECTEX>(raw_fd, &ws::WSAID_CONNECTEX)?
            .ok_or(AsyncHostError::Inval)?;
        let addr_len = socket_addr_len(&result.addr_buffer)?;
        let success = unsafe {
            connect_ex(
                raw_fd as usize,
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
                result.mark_pending(raw_fd)?;
            }
            Err(AsyncHostError::Native(errno))
        }
    }

    #[cfg(windows)]
    pub(crate) fn setup_connected_socket(&self, fd_handle: HostHandle) -> AsyncHostResult<()> {
        use windows_sys::Win32::Networking::WinSock as ws;

        let raw_fd = self.files.lock().unwrap().file(fd_handle)?.raw_fd();
        let yes: u32 = 1;
        if unsafe {
            ws::setsockopt(
                raw_fd as usize,
                ws::SOL_SOCKET,
                ws::SO_UPDATE_CONNECT_CONTEXT,
                (&yes as *const u32).cast(),
                std::mem::size_of_val(&yes) as i32,
            )
        } == ws::SOCKET_ERROR
        {
            Err(AsyncHostError::Native(last_wsa_errno()))
        } else {
            Ok(())
        }
    }

    #[cfg(windows)]
    pub(crate) fn accept_io_result(
        &self,
        server_fd_handle: HostHandle,
        conn_fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Networking::WinSock as ws;

        let files = self.files.lock().unwrap();
        let server_fd = files.file(server_fd_handle)?.raw_fd();
        let conn_fd = files.file(conn_fd_handle)?.raw_fd();
        drop(files);
        let mut io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get_mut(key_from_handle::<HostIoResultKey>(result_handle))
            .ok_or(AsyncHostError::Badf)?;
        if result.kind != HostIoKind::Accept || result.is_pending() {
            return Err(AsyncHostError::Inval);
        }

        let accept_ex = get_wsa_extension::<ws::LPFN_ACCEPTEX>(server_fd, &ws::WSAID_ACCEPTEX)?
            .ok_or(AsyncHostError::Inval)?;
        let addr_len = u32::try_from(result.addr_len).map_err(|_| AsyncHostError::Fault)?;
        let accept_addr_len = addr_len.checked_add(16).ok_or(AsyncHostError::Fault)?;
        let success = unsafe {
            accept_ex(
                server_fd as usize,
                conn_fd as usize,
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
                result.mark_pending_with_close_guard(server_fd, conn_fd)?;
            }
            Err(AsyncHostError::Native(errno))
        }
    }

    #[cfg(windows)]
    pub(crate) fn get_accept_peer_addr(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        result_handle: u64,
        dst: i32,
        dst_len: i32,
    ) -> AsyncHostResult<()> {
        let io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get(key_from_handle::<HostIoResultKey>(result_handle))
            .ok_or(AsyncHostError::Badf)?;
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
        memory.write_with_capacity(dst, dst_len, addr)
    }

    #[cfg(windows)]
    pub(crate) fn setup_accepted_socket(
        &self,
        listen_fd_handle: HostHandle,
        accept_fd_handle: HostHandle,
    ) -> AsyncHostResult<()> {
        use windows_sys::Win32::Networking::WinSock as ws;

        let files = self.files.lock().unwrap();
        let listen_fd = files.file(listen_fd_handle)?.raw_fd();
        let accept_fd = files.file(accept_fd_handle)?.raw_fd();
        let listen_socket = listen_fd as usize;
        if unsafe {
            ws::setsockopt(
                accept_fd as usize,
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

    pub(crate) fn try_lock_file(&self, handle: HostHandle, exclusive: bool) -> AsyncHostResult<()> {
        let file = self.files.lock().unwrap().file(handle)?;
        crate::async_sys::fs::stub::try_lock_file_resource(&file, exclusive)
    }

    pub(crate) fn unlock_file(&self, handle: HostHandle) -> AsyncHostResult<()> {
        let file = self.files.lock().unwrap().file(handle)?;
        crate::async_sys::fs::stub::unlock_file_resource(&file)
    }

    pub(crate) fn run_job(&self, handle: u64) -> AsyncHostResult<()> {
        let key = key_from_handle::<HostJobKey>(handle);
        let mut job = self.jobs.lock().unwrap().take_job(key)?;
        thread_pool::run_host_job(&mut job);
        self.restore_job(key, job)
    }

    pub(crate) fn spawn_worker(&self, completion_id: i32, job_handle: u64) -> AsyncHostResult<u64> {
        self.restore_completed_jobs();
        let completion_id = WorkerCompletionId::from_abi(completion_id);
        let job_key = key_from_handle::<HostJobKey>(job_handle);
        {
            let jobs = self.jobs.lock().unwrap();
            jobs.jobs
                .get(job_key)
                .and_then(Option::as_ref)
                .ok_or(AsyncHostError::Badf)?;
        }
        #[cfg(unix)]
        let worker = {
            let completion_notifier = self
                .thread_pool_completions
                .lock()
                .unwrap()
                .notifier
                .clone()
                .ok_or(AsyncHostError::Badf)?;
            self.spawn_worker_thread(
                HostWorkerJob {
                    completion_id,
                    job_key,
                },
                move |completion_id| {
                    let _ = completion_notifier.notify(completion_id.as_i32());
                },
            )
        };
        #[cfg(windows)]
        let worker = {
            let completion_target = self
                .thread_pool_completions
                .lock()
                .unwrap()
                .target
                .ok_or(AsyncHostError::Badf)?;
            self.spawn_worker_thread(
                HostWorkerJob {
                    completion_id,
                    job_key,
                },
                move |completion_id| {
                    let _ = poll::post_thread_pool_completion(
                        completion_target.port,
                        completion_id.as_i32(),
                        completion_target.generation,
                    );
                },
            )
        };
        let key = self.workers.lock().unwrap().workers.insert(worker);
        Ok(handle_from_key(key))
    }

    pub(crate) fn wake_worker(
        &self,
        worker_handle: u64,
        completion_id: i32,
        job_handle: u64,
    ) -> AsyncHostResult<()> {
        self.restore_completed_jobs();
        let completion_id = WorkerCompletionId::from_abi(completion_id);
        let worker_key = key_from_handle::<HostWorkerKey>(worker_handle);
        let job_key = key_from_handle::<HostJobKey>(job_handle);
        if self
            .workers
            .lock()
            .unwrap()
            .workers
            .get(worker_key)
            .is_none()
        {
            return Err(AsyncHostError::Badf);
        }
        {
            let jobs = self.jobs.lock().unwrap();
            jobs.jobs
                .get(job_key)
                .and_then(Option::as_ref)
                .ok_or(AsyncHostError::Badf)?;
        }
        let replaced_job = {
            let workers = self.workers.lock().unwrap();
            let Some(worker) = workers.workers.get(worker_key) else {
                return Err(AsyncHostError::Badf);
            };
            thread_pool::wake_worker(
                worker,
                HostWorkerJob {
                    completion_id,
                    job_key,
                },
            )
        };
        let _ = replaced_job;
        Ok(())
    }

    pub(crate) fn worker_enter_idle(&self, worker_handle: u64) -> AsyncHostResult<()> {
        self.restore_completed_jobs();
        let _ = {
            let workers = self.workers.lock().unwrap();
            let worker = workers
                .workers
                .get(key_from_handle::<HostWorkerKey>(worker_handle))
                .ok_or(AsyncHostError::Badf)?;
            thread_pool::worker_enter_idle(worker)
        };
        Ok(())
    }

    pub(crate) fn free_worker(&self, worker_handle: u64) -> AsyncHostResult<()> {
        self.restore_completed_jobs();
        let worker = self
            .workers
            .lock()
            .unwrap()
            .workers
            .remove(key_from_handle::<HostWorkerKey>(worker_handle))
            .ok_or(AsyncHostError::Badf)?;
        let _ = thread_pool::free_worker(worker);
        self.restore_completed_jobs();
        Ok(())
    }

    pub(crate) fn cancel_worker(&self, worker_handle: u64) -> AsyncHostResult<i32> {
        let workers = self.workers.lock().unwrap();
        let worker = workers
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
        self.restore_completed_jobs();
        let jobs = self.jobs.lock().unwrap();
        let job = jobs
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
        self.restore_completed_jobs();
        let jobs = self.jobs.lock().unwrap();
        let job = jobs
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
        self.restore_completed_jobs();
        let (completion_notifier, completion_source) = {
            let completions = self.thread_pool_completions.lock().unwrap();
            (
                completions.notifier.clone().ok_or(AsyncHostError::Badf)?,
                completions.source.ok_or(AsyncHostError::Badf)?,
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
        mut complete_job: impl FnMut(WorkerCompletionId) + Send + 'static,
    ) -> HostWorkerHandle {
        let jobs = Arc::clone(&self.jobs);
        let completed_jobs = Arc::clone(&self.completed_jobs);
        thread_pool::spawn_worker(
            init_job,
            move |worker_job| {
                let Ok(mut job) = jobs.lock().unwrap().take_job(worker_job.job_key) else {
                    return None;
                };
                thread_pool::run_host_job(&mut job);
                Some(job)
            },
            move |worker_job| {
                // Even if cancellation discarded the job handle, the event loop
                // still needs the completion to move the worker out of running.
                let completion_id = worker_job.completion_id;
                if let Some(job) = worker_job.job {
                    completed_jobs.lock().unwrap().push(CompletedJob {
                        key: worker_job.job_key,
                        job,
                    });
                }
                complete_job(completion_id);
            },
        )
    }
}

impl Drop for AsyncHost {
    fn drop(&mut self) {
        self.destroy_thread_pool();
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
fn last_wsa_errno() -> i32 {
    use windows_sys::Win32::Foundation::{SetLastError, WIN32_ERROR};
    use windows_sys::Win32::Networking::WinSock;

    let errno = unsafe { WinSock::WSAGetLastError() };
    unsafe {
        SetLastError(errno as WIN32_ERROR);
    }
    errno
}

#[cfg(windows)]
fn socket_buffer(
    buffer: &mut [u8],
) -> AsyncHostResult<windows_sys::Win32::Networking::WinSock::WSABUF> {
    Ok(windows_sys::Win32::Networking::WinSock::WSABUF {
        len: u32::try_from(buffer.len()).map_err(|_| AsyncHostError::Fault)?,
        buf: buffer.as_mut_ptr().cast(),
    })
}

#[cfg(windows)]
fn socket_addr_family(addr: &[u8]) -> AsyncHostResult<u16> {
    use windows_sys::Win32::Networking::WinSock;

    if addr.len() < std::mem::size_of::<WinSock::SOCKADDR>() {
        return Err(AsyncHostError::Fault);
    }
    Ok(unsafe { addr.as_ptr().cast::<WinSock::SOCKADDR>().read_unaligned() }.sa_family)
}

#[cfg(windows)]
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

#[cfg(windows)]
fn bind_any_for_connect(raw_fd: RawFd, remote_addr: &[u8]) -> AsyncHostResult<()> {
    use windows_sys::Win32::Networking::WinSock;

    let result = match socket_addr_family(remote_addr)? {
        WinSock::AF_INET => {
            let mut addr = unsafe { std::mem::zeroed::<WinSock::SOCKADDR_IN>() };
            addr.sin_family = WinSock::AF_INET;
            unsafe {
                WinSock::bind(
                    raw_fd as usize,
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
                    raw_fd as usize,
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

#[cfg(windows)]
fn get_wsa_extension<T: Copy>(raw_fd: RawFd, guid: &windows_sys::core::GUID) -> AsyncHostResult<T> {
    use windows_sys::Win32::Networking::WinSock;

    debug_assert_eq!(
        std::mem::size_of::<T>(),
        std::mem::size_of::<*mut std::ffi::c_void>()
    );
    let mut extension = std::ptr::null_mut::<std::ffi::c_void>();
    let mut bytes_returned = 0;
    let ret = unsafe {
        WinSock::WSAIoctl(
            raw_fd as usize,
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

fn event_index(event: u64) -> AsyncHostResult<i32> {
    i32::try_from(event).map_err(|_| AsyncHostError::Fault)
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

    #[repr(align(2))]
    struct AlignedBytes<const N: usize>([u8; N]);

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
        let mut memory = AlignedBytes([0; 8]);

        write_u16(&mut memory.0, 2, &[0x1234, 0x5678]).unwrap();

        assert_eq!(read_u16(&memory.0, 2, 2).unwrap(), &[0x1234, 0x5678]);
        assert_eq!(read_u16(&memory.0, 1, 1), Err(AsyncHostError::Fault));
        assert_eq!(
            write_u16(&mut memory.0, 6, &[1, 2]),
            Err(AsyncHostError::Fault)
        );
        assert_eq!(&memory.0[2..6], &[0x34, 0x12, 0x78, 0x56]);
    }

    #[test]
    fn guest_memory_helpers_reject_odd_u16_offsets() {
        let mut memory = [0; 8];

        assert_eq!(read_u16(&memory, 1, 1), Err(AsyncHostError::Fault));
        assert_eq!(write_u16(&mut memory, 1, &[1]), Err(AsyncHostError::Fault));
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
    fn completion_source_is_file_resource_handle() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_source = host.init_thread_pool(poll).unwrap();
        let raw_completion_fd = {
            let files = host.files.lock().unwrap();
            files.file(completion_source).unwrap().raw_fd()
        };
        {
            let polls = host.polls.lock().unwrap();
            let poll = polls
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
            let completions = host.thread_pool_completions.lock().unwrap();
            completions.notifier.as_ref().unwrap().notify(17).unwrap();
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
    fn fetch_completion_publishes_completion_id_without_copying_payload() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let job = thread_pool::make_read_job(Arc::new(FileResource::invalid()), 3, -1);
        let job_handle = host.insert_job(job).unwrap();
        {
            let mut jobs = host.jobs.lock().unwrap();
            let job = jobs
                .jobs
                .get_mut(key_from_handle::<HostJobKey>(job_handle))
                .and_then(Option::as_mut)
                .unwrap();
            let thread_pool::JobPayload::Read { result, .. } = job.payload_mut() else {
                panic!("expected read job");
            };
            *result = Some(b"abc".to_vec());
            host.thread_pool_completions
                .lock()
                .unwrap()
                .notifier
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

    #[test]
    fn c_buffer_access_rejects_interior_raw_pointer() {
        let host = AsyncHost::default();
        let handle = host.insert_c_buffer(b"abcd".to_vec().into_boxed_slice());
        let interior_ptr = host
            .with_c_buffer(handle, |buffer| {
                // `c_buffer` values are slot-map handles, not addresses into
                // host-owned buffers.
                Ok((buffer.as_ptr() as u64) + 1)
            })
            .unwrap();

        assert_eq!(
            host.with_c_buffer(interior_ptr, |_| Ok(())).unwrap_err(),
            AsyncHostError::Badf
        );
        assert_eq!(
            host.with_c_buffer_mut(interior_ptr, |_| Ok(()))
                .unwrap_err(),
            AsyncHostError::Badf
        );
    }

    #[cfg(unix)]
    #[test]
    fn fetch_completion_leaves_unfetched_completion_ids_in_os_source() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        {
            let completions = host.thread_pool_completions.lock().unwrap();
            let notifier = completions.notifier.as_ref().unwrap();
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
    fn open_job_get_fd_publishes_opened_resource_once() {
        let host = AsyncHost::default();
        let path =
            std::env::temp_dir().join(format!("moonrun-published-open-job-{}", std::process::id()));
        let job = host
            .insert_job(thread_pool::make_open_job(
                path.as_os_str().to_os_string(),
                2,
                3,
                false,
                0,
                0o600,
            ))
            .unwrap();

        host.run_job(job).unwrap();
        {
            let files = host.files.lock().unwrap();
            assert_eq!(files.files.len(), 1);
        }

        let opened = host.open_job_get_fd(job).unwrap();
        assert_eq!(host.open_job_get_fd(job).unwrap(), opened);
        {
            let files = host.files.lock().unwrap();
            assert_eq!(files.files.len(), 2);
            assert!(files.file(opened).is_ok());
        }

        host.close_fd(opened).unwrap();
        host.free_job(job).unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn discarded_completed_open_job_drops_unpublished_resource() {
        let host = AsyncHost::default();
        let path =
            std::env::temp_dir().join(format!("moonrun-discarded-open-job-{}", std::process::id()));
        let job_handle = host
            .insert_job(thread_pool::make_open_job(
                path.as_os_str().to_os_string(),
                2,
                3,
                false,
                0,
                0o600,
            ))
            .unwrap();
        let key = key_from_handle::<HostJobKey>(job_handle);
        let mut job = host
            .jobs
            .lock()
            .unwrap()
            .jobs
            .get_mut(key)
            .and_then(Option::take)
            .unwrap();

        thread_pool::run_host_job(&mut job);

        assert_eq!(job.err(), 0);
        assert!(matches!(
            thread_pool::open_job_result(&job).unwrap().resource,
            OpenJobResource::Unpublished(_)
        ));
        assert_eq!(host.files.lock().unwrap().files.len(), 1);
        host.jobs.lock().unwrap().jobs.remove(key);

        {
            assert_eq!(host.restore_job(key, job), Err(AsyncHostError::Badf));
            assert_eq!(host.files.lock().unwrap().files.len(), 1);
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn drop_destroys_pool_even_when_worker_holds_state() {
        let host = AsyncHost::default();
        let completed_jobs = Arc::downgrade(&host.completed_jobs);
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        host.spawn_worker(42, job).unwrap();

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

        drop(host);

        assert!(completed_jobs.upgrade().is_none());
    }

    #[test]
    fn worker_result_is_available_after_completion_event() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_source = host.init_thread_pool(poll).unwrap();
        let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let worker = host.spawn_worker(42, job).unwrap();

        assert_eq!(host.poll_wait(poll, 1000).unwrap(), 1);
        assert_eq!(host.job_get_ret(job).unwrap(), 0);

        #[cfg(unix)]
        {
            let mut memory = [0; 4];
            host.fetch_completion(memory.as_mut_slice(), completion_source, 0, 1)
                .unwrap();
            assert_eq!(i32::from_le_bytes(memory), 42);
        }
        #[cfg(windows)]
        {
            let event = host.poll_get_event(poll, 0).unwrap();
            assert_eq!(host.poll_event_fd(event).unwrap(), completion_source);
            assert_eq!(host.poll_event_bytes_transferred(event).unwrap(), 42);
        }

        host.free_worker(worker).unwrap();
        host.free_job(job).unwrap();
        host.destroy_thread_pool();
    }

    #[test]
    fn queued_worker_job_can_be_freed_before_worker_runs_it() {
        let host = AsyncHost::default();
        let first_job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let first_key = key_from_handle::<HostJobKey>(first_job);
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let (completion_sender, completion_receiver) = std::sync::mpsc::channel();
        let worker = {
            let jobs = Arc::clone(&host.jobs);
            let completed_jobs = Arc::clone(&host.completed_jobs);
            thread_pool::spawn_worker(
                HostWorkerJob {
                    completion_id: WorkerCompletionId::from_abi(1),
                    job_key: first_key,
                },
                move |worker_job| {
                    let Ok(mut job) = jobs.lock().unwrap().take_job(worker_job.job_key) else {
                        return None;
                    };
                    started_sender.send(worker_job.completion_id).unwrap();
                    if worker_job.job_key == first_key {
                        release_receiver.recv().unwrap();
                    }
                    thread_pool::run_host_job(&mut job);
                    Some(job)
                },
                move |worker_job| {
                    let completed = worker_job.job.is_some();
                    if let Some(job) = worker_job.job {
                        completed_jobs.lock().unwrap().push(CompletedJob {
                            key: worker_job.job_key,
                            job,
                        });
                    }
                    completion_sender
                        .send((worker_job.completion_id, completed))
                        .unwrap();
                },
            )
        };
        let worker = handle_from_key(host.workers.lock().unwrap().workers.insert(worker));

        assert_eq!(
            started_receiver.recv().unwrap(),
            WorkerCompletionId::from_abi(1)
        );

        let path = std::env::temp_dir().join(format!(
            "moonrun-cancelled-queued-worker-job-{}",
            std::process::id()
        ));
        std::fs::write(&path, b"queued").unwrap();
        let queued_job = host
            .insert_job(thread_pool::make_remove_job(
                path.as_os_str().to_os_string(),
            ))
            .unwrap();

        host.wake_worker(worker, 2, queued_job).unwrap();
        assert_eq!(host.job_get_ret(queued_job).unwrap(), 0);
        host.free_job(queued_job).unwrap();

        release_sender.send(()).unwrap();
        assert_eq!(
            completion_receiver.recv().unwrap(),
            (WorkerCompletionId::from_abi(1), true)
        );
        host.restore_completed_jobs();
        host.free_job(first_job).unwrap();
        assert_eq!(
            completion_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            (WorkerCompletionId::from_abi(2), false)
        );
        assert!(path.exists());

        host.free_worker(worker).unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn worker_handles_stay_stale_after_thread_pool_reinit() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let first_job = host
            .insert_job(thread_pool::make_read_job(
                Arc::new(FileResource::invalid()),
                1,
                -1,
            ))
            .unwrap();
        let old_worker = host.spawn_worker(42, first_job).unwrap();
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

        host.destroy_thread_pool();

        host.init_thread_pool(poll).unwrap();
        let second_job = host
            .insert_job(thread_pool::make_read_job(
                Arc::new(FileResource::invalid()),
                1,
                -1,
            ))
            .unwrap();
        let new_worker = host.spawn_worker(43, second_job).unwrap();
        let wake_job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();

        assert_ne!(old_worker, new_worker);
        assert_eq!(host.cancel_worker(old_worker), Err(AsyncHostError::Badf));
        assert_eq!(
            host.wake_worker(old_worker, 44, wake_job),
            Err(AsyncHostError::Badf)
        );
        host.free_job(wake_job).unwrap();
        assert_eq!(host.free_worker(old_worker), Err(AsyncHostError::Badf));

        host.destroy_thread_pool();
    }

    #[cfg(windows)]
    #[test]
    fn stale_thread_pool_completions_are_ignored_after_reinit() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();

        host.init_thread_pool(poll).unwrap();
        let stale_completion = host.thread_pool_completions.lock().unwrap().target.unwrap();
        // Fill the current IOCP batch so a valid completion can sit behind
        // stale completions from the destroyed pool generation.
        for completion_id in 0..1024 {
            poll::post_thread_pool_completion(
                stale_completion.port,
                completion_id,
                stale_completion.generation,
            )
            .unwrap();
        }
        host.destroy_thread_pool();

        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let current_completion = host.thread_pool_completions.lock().unwrap().target.unwrap();
        assert_ne!(stale_completion.generation, current_completion.generation);

        poll::post_thread_pool_completion(
            current_completion.port,
            43,
            current_completion.generation,
        )
        .unwrap();
        assert_eq!(host.poll_wait(poll, 1000).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();
        assert_eq!(host.poll_event_fd(event).unwrap(), completion_notifier);
        assert_eq!(host.poll_event_bytes_transferred(event).unwrap(), 43);
    }

    #[cfg(windows)]
    #[test]
    fn thread_pool_completion_reports_native_sentinel() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let completion = host.thread_pool_completions.lock().unwrap().target.unwrap();

        poll::post_thread_pool_completion(completion.port, 42, completion.generation).unwrap();

        assert_eq!(host.poll_wait(poll, 1000).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();
        assert_eq!(host.poll_event_fd(event).unwrap(), completion_notifier);
        assert_eq!(host.poll_event_bytes_transferred(event).unwrap(), 42);
    }

    #[test]
    fn stale_worker_handle_is_rejected_after_free() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let job = host
            .insert_job(thread_pool::make_read_job(
                Arc::new(FileResource::invalid()),
                1,
                -1,
            ))
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
    fn acquired_file_resource_survives_guest_close() {
        let host = AsyncHost::default();
        let [read, write] = host.pipe(false, false).unwrap();
        let file = host.file_resource(read).unwrap();

        host.close_fd(read).unwrap();
        let mut input = *b"x";
        host.write_fd(&mut input, write, 0, 0, 1).unwrap();

        let mut output = [0];
        let ret = unsafe { libc::read(file.raw_fd(), output.as_mut_ptr().cast(), output.len()) };
        assert_eq!(ret, 1);

        assert_eq!(output[0], b'x');
        assert_eq!(host.close_fd(read), Err(AsyncHostError::Badf));
    }

    #[cfg(unix)]
    #[test]
    fn close_fd_unregisters_poll_when_job_still_holds_resource() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let [read, write] = host.pipe(true, true).unwrap();
        host.poll_register(poll, read, true).unwrap();
        let job = host
            .insert_job(thread_pool::make_read_job(
                host.file_resource(read).unwrap(),
                1,
                -1,
            ))
            .unwrap();

        host.close_fd(read).unwrap();
        let fd = host.file_resource(write).unwrap().raw_fd();
        let byte = b"x";
        let ret = unsafe { libc::write(fd, byte.as_ptr().cast(), byte.len()) };
        assert_eq!(ret, 1);

        assert_eq!(host.poll_wait(poll, 0).unwrap(), 0);

        host.free_job(job).unwrap();
        host.close_fd(write).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn io_result_status_rejects_wrong_fd_without_clearing_pending() {
        let mut result = HostIoResult::for_file(0, Vec::new(), 0, 0);
        let pending_fd = 1usize as RawFd;
        let other_fd = 2usize as RawFd;

        result.mark_pending(pending_fd).unwrap();

        assert_eq!(
            result.validate_pending_handle(other_fd),
            Err(AsyncHostError::Badf)
        );
        assert_eq!(result.pending_raw_fd(), Some(pending_fd));
    }

    #[cfg(windows)]
    #[test]
    fn cancel_io_result_rejects_wrong_fd_without_clearing_pending() {
        let host = AsyncHost::default();
        let [read, write] = host.pipe(true, true).unwrap();
        let result = host.make_file_io_result(&mut [], 0, 0, 0, 0, 0).unwrap();
        let raw_read = {
            let raw_read = host.files.lock().unwrap().file(read).unwrap().raw_fd();
            host.io_results
                .lock()
                .unwrap()
                .io_results
                .get_mut(key_from_handle::<HostIoResultKey>(result))
                .unwrap()
                .mark_pending(raw_read)
                .unwrap();
            raw_read
        };

        assert_eq!(
            host.cancel_io_result(result, write),
            Err(AsyncHostError::Badf)
        );
        {
            let mut io_results = host.io_results.lock().unwrap();
            let result = io_results
                .io_results
                .get_mut(key_from_handle::<HostIoResultKey>(result))
                .unwrap();
            assert_eq!(result.pending_raw_fd(), Some(raw_read));
            result.clear_pending();
        }

        host.free_io_result(result).unwrap();
        host.close_fd(read).unwrap();
        host.close_fd(write).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn cancel_io_result_keeps_pending_until_completion_is_delivered() {
        let host = AsyncHost::default();
        let [read, write] = host.pipe(true, true).unwrap();
        let result = host.make_file_io_result(&mut [], 0, 0, 0, 0, 0).unwrap();
        let raw_read = {
            let raw_read = host.files.lock().unwrap().file(read).unwrap().raw_fd();
            host.io_results
                .lock()
                .unwrap()
                .io_results
                .get_mut(key_from_handle::<HostIoResultKey>(result))
                .unwrap()
                .mark_pending(raw_read)
                .unwrap();
            raw_read
        };

        assert_eq!(host.cancel_io_result(result, read), Ok(1));
        assert_eq!(host.free_io_result(result), Err(AsyncHostError::Inval));
        assert_eq!(host.close_fd(read), Err(AsyncHostError::Inval));
        {
            let mut io_results = host.io_results.lock().unwrap();
            let result = io_results
                .io_results
                .get_mut(key_from_handle::<HostIoResultKey>(result))
                .unwrap();
            assert_eq!(result.pending_raw_fd(), Some(raw_read));
            result.clear_pending();
        }

        host.free_io_result(result).unwrap();
        host.close_fd(read).unwrap();
        host.close_fd(write).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn close_fd_rejects_pending_io_result() {
        let host = AsyncHost::default();
        let [read, write] = host.pipe(true, true).unwrap();
        let result = host.make_file_io_result(&mut [], 0, 0, 0, 0, 0).unwrap();
        let raw_read = {
            let raw_read = host.files.lock().unwrap().file(read).unwrap().raw_fd();
            host.io_results
                .lock()
                .unwrap()
                .io_results
                .get_mut(key_from_handle::<HostIoResultKey>(result))
                .unwrap()
                .mark_pending(raw_read)
                .unwrap();
            raw_read
        };

        assert_eq!(host.close_fd(read), Err(AsyncHostError::Inval));
        {
            assert!(host.files.lock().unwrap().file(read).is_ok());
            let mut io_results = host.io_results.lock().unwrap();
            let result = io_results
                .io_results
                .get_mut(key_from_handle::<HostIoResultKey>(result))
                .unwrap();
            assert_eq!(result.pending_raw_fd(), Some(raw_read));
            result.clear_pending();
        }

        host.free_io_result(result).unwrap();
        host.close_fd(read).unwrap();
        host.close_fd(write).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn close_fd_rejects_extra_pending_close_guard() {
        let host = AsyncHost::default();
        let [read, write] = host.pipe(true, true).unwrap();
        let result = host.make_file_io_result(&mut [], 0, 0, 0, 0, 0).unwrap();
        let (raw_read, raw_write) = {
            let files = host.files.lock().unwrap();
            let raw_read = files.file(read).unwrap().raw_fd();
            let raw_write = files.file(write).unwrap().raw_fd();
            drop(files);
            host.io_results
                .lock()
                .unwrap()
                .io_results
                .get_mut(key_from_handle::<HostIoResultKey>(result))
                .unwrap()
                .mark_pending_with_close_guard(raw_read, raw_write)
                .unwrap();
            (raw_read, raw_write)
        };

        assert_eq!(host.close_fd(write), Err(AsyncHostError::Inval));
        assert_eq!(
            host.cancel_io_result(result, write),
            Err(AsyncHostError::Badf)
        );
        assert_eq!(host.close_fd(read), Err(AsyncHostError::Inval));
        {
            let files = host.files.lock().unwrap();
            assert!(files.file(read).is_ok());
            assert!(files.file(write).is_ok());
            drop(files);
            let mut io_results = host.io_results.lock().unwrap();
            let result = io_results
                .io_results
                .get_mut(key_from_handle::<HostIoResultKey>(result))
                .unwrap();
            assert_eq!(result.pending_raw_fd(), Some(raw_read));
            assert!(result.protects_pending_raw_fd(raw_write));
            result.clear_pending();
        }

        host.free_io_result(result).unwrap();
        host.close_fd(read).unwrap();
        host.close_fd(write).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn free_io_result_rejects_pending_result() {
        let host = AsyncHost::default();
        let result = host.make_file_io_result(&mut [], 0, 0, 0, 0, 0).unwrap();
        {
            host.io_results
                .lock()
                .unwrap()
                .io_results
                .get_mut(key_from_handle::<HostIoResultKey>(result))
                .unwrap()
                .mark_pending(1usize as RawFd)
                .unwrap();
        }

        assert_eq!(host.free_io_result(result), Err(AsyncHostError::Inval));
        assert!(
            host.io_results
                .lock()
                .unwrap()
                .io_results
                .contains_key(key_from_handle::<HostIoResultKey>(result))
        );
    }

    #[cfg(windows)]
    #[test]
    fn poll_event_io_result_marks_pending_result_delivered() {
        use windows_sys::Win32::System::IO::PostQueuedCompletionStatus;

        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_port = {
            let polls = host.polls.lock().unwrap();
            let poll = polls
                .polls
                .get(key_from_handle::<HostPollKey>(poll))
                .unwrap()
                .lock()
                .unwrap();
            poll.instance.raw_fd()
        };
        let result = host.make_file_io_result(&mut [], 0, 0, 0, 0, 0).unwrap();
        let raw_fd = 0x1234usize as RawFd;
        let overlapped = {
            let mut io_results = host.io_results.lock().unwrap();
            let result = io_results
                .io_results
                .get_mut(key_from_handle::<HostIoResultKey>(result))
                .unwrap();
            result.mark_pending(raw_fd).unwrap();
            result.overlapped_ptr()
        };
        let posted =
            unsafe { PostQueuedCompletionStatus(completion_port, 0, raw_fd as usize, overlapped) };
        assert_ne!(posted, 0);

        assert_eq!(host.poll_wait(poll, 1000).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();

        assert_eq!(host.poll_event_io_result(event).unwrap(), result);
        assert_eq!(
            host.io_results
                .lock()
                .unwrap()
                .io_results
                .get(key_from_handle::<HostIoResultKey>(result))
                .unwrap()
                .pending_raw_fd(),
            None
        );
        host.free_io_result(result).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn unregistered_iocp_completion_reports_invalid_fd() {
        use windows_sys::Win32::System::IO::PostQueuedCompletionStatus;

        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_port = {
            let polls = host.polls.lock().unwrap();
            let poll = polls
                .polls
                .get(key_from_handle::<HostPollKey>(poll))
                .unwrap()
                .lock()
                .unwrap();
            poll.instance.raw_fd()
        };
        let raw_fd = 0x1234usize as RawFd;
        let posted = unsafe {
            PostQueuedCompletionStatus(completion_port, 0, raw_fd as usize, std::ptr::null_mut())
        };
        assert_ne!(posted, 0);

        assert_eq!(host.poll_wait(poll, 1000).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();

        assert_eq!(host.poll_event_fd(event).unwrap(), host.invalid_fd());
    }

    #[cfg(unix)]
    #[test]
    fn poll_reports_registered_pipe_readiness_as_guest_fd() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let [read, write] = host.pipe(true, true).unwrap();
        host.poll_register(poll, read, true).unwrap();

        let fd = host.file_resource(write).unwrap().raw_fd();
        let byte = b"x";
        let ret = unsafe { libc::write(fd, byte.as_ptr().cast(), byte.len()) };
        assert_eq!(ret, 1);

        assert_eq!(host.poll_wait(poll, 100).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();
        assert_eq!(event, 0);
        assert_eq!(host.poll_get_event(poll, 0).unwrap(), event);
        assert_eq!(host.poll_event_fd(event).unwrap(), read);
        assert_eq!(
            host.poll_event_events(event).unwrap() & poll::READ_EVENT,
            poll::READ_EVENT
        );
        host.close_fd(read).unwrap();
        host.close_fd(write).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn close_fd_invalidates_cached_poll_event_fd() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let [read, write] = host.pipe(true, true).unwrap();
        host.poll_register(poll, read, true).unwrap();

        let fd = host.file_resource(write).unwrap().raw_fd();
        let byte = b"x";
        let ret = unsafe { libc::write(fd, byte.as_ptr().cast(), byte.len()) };
        assert_eq!(ret, 1);
        assert_eq!(host.poll_wait(poll, 100).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();

        host.close_fd(read).unwrap();

        assert_eq!(host.poll_event_fd(event), Err(AsyncHostError::Badf));
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
