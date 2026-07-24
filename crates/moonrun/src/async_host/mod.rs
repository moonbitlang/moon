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
//! This module owns one V8 host instance's runtime state: the Handle table, host
//! workers, guest memory helpers, and host poll instances.
//!
//! Native async multiplexes pollable IO through epoll, kqueue, or IOCP, with
//! thread-pool completions as one registered event
//! source. The wasm ABI exposes that same shape: MoonBit owns event-loop
//! scheduling and Rust owns the OS poller behind opaque poll handles.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, AsRawSocket, RawHandle};
use std::sync::{Arc, Mutex};

use slotmap::{Key, KeyData, SecondaryMap, SlotMap, new_key_type};

use crate::async_policy::{AsyncPolicy, RuntimePathBase};
#[cfg(unix)]
use crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier;
use crate::async_sys::internal::event_loop::{
    poll::{self, PollInstance},
    thread_pool::{
        self, HostHandle, HostWorkerHandle, HostWorkerJob, Job, JobPayload, OpenJobResource,
        Resource, ResourceClass, ResourceRef, ResourceTable, WorkerCompletionId,
    },
};
use crate::async_sys::internal::fd_util::stub::RawFd;
use crate::async_sys::socket::RawSocket;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
compile_error!("moonrun async wasm host currently supports only Linux, macOS, and Windows hosts");

#[cfg(not(target_endian = "little"))]
compile_error!("moonrun async wasm host requires little-endian host memory");

pub(crate) mod tls;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncHostError {
    Fault,
    Inval,
    Io,
    Badf,
    PermissionDenied,
    Native(i32),
}

pub(crate) type AsyncHostResult<T> = Result<T, AsyncHostError>;
pub(crate) const INVALID_HOST_HANDLE: u64 = 0;
pub(crate) const CHECK_FD_LEAK_ENV: &str = "MOONBIT_ASYNC_CHECK_FD_LEAK";
pub(crate) type HostCBuffer = Arc<Mutex<Box<[u8]>>>;
#[cfg(unix)]
pub(crate) type HostProcessArgv = Arc<Mutex<Vec<Option<OsString>>>>;
#[cfg(unix)]
pub(crate) type HostProcessEnv = Arc<Mutex<Vec<Option<OsString>>>>;
#[cfg(windows)]
pub(crate) type HostProcessEnv = Arc<Mutex<Vec<u16>>>;

#[derive(Default)]
struct ProcessPolicyState {
    // PID authority and stable-handle provenance must change atomically.
    inner: Mutex<ProcessPolicyStateInner>,
}

#[derive(Default)]
struct ProcessPolicyStateInner {
    owned_child_pids: HashSet<i32>,
    process_handle_pids: HashMap<HostHandle, i32>,
}

#[cfg(unix)]
mod native_errno {
    pub(crate) const BADF: i32 = libc::EBADF;
    pub(crate) const ACCESS: i32 = libc::EACCES;
    pub(crate) const FAULT: i32 = libc::EFAULT;
    pub(crate) const INVAL: i32 = libc::EINVAL;
    pub(crate) const IO: i32 = libc::EIO;
}

#[cfg(windows)]
mod native_errno {
    use windows_sys::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_GEN_FAILURE, ERROR_INVALID_ADDRESS, ERROR_INVALID_HANDLE,
        ERROR_INVALID_PARAMETER,
    };

    pub(crate) const ACCESS: i32 = ERROR_ACCESS_DENIED as i32;
    pub(crate) const BADF: i32 = ERROR_INVALID_HANDLE as i32;
    pub(crate) const FAULT: i32 = ERROR_INVALID_ADDRESS as i32;
    pub(crate) const INVAL: i32 = ERROR_INVALID_PARAMETER as i32;
    pub(crate) const IO: i32 = ERROR_GEN_FAILURE as i32;
}

impl AsyncHostError {
    pub(crate) fn errno(self) -> i32 {
        match self {
            Self::Fault => native_errno::FAULT,
            Self::Inval => native_errno::INVAL,
            Self::Io => native_errno::IO,
            Self::Badf => native_errno::BADF,
            Self::PermissionDenied => native_errno::ACCESS,
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

new_key_type! {
    pub(crate) struct HandleKey;
}

#[derive(Debug)]
struct HostAddrInfo {
    addr: Box<[u8]>,
    next: Option<HostHandle>,
}

const STDIN_ID: i32 = 0;
const STDOUT_ID: i32 = 1;
const STDERR_ID: i32 = 2;

#[cfg(windows)]
const WINDOWS_STDIO_IDS: [u32; 3] = [
    windows_sys::Win32::System::Console::STD_INPUT_HANDLE,
    windows_sys::Win32::System::Console::STD_OUTPUT_HANDLE,
    windows_sys::Win32::System::Console::STD_ERROR_HANDLE,
];

#[cfg(unix)]
const STDIO_IDS: [i32; 3] = [STDIN_ID, STDOUT_ID, STDERR_ID];

#[repr(usize)]
#[derive(Clone, Copy)]
enum Stdio {
    Stdin,
    Stdout,
    Stderr,
}

fn handle_from_key(key: HandleKey) -> HostHandle {
    key.data().as_ffi()
}

fn key_from_handle(handle: u64) -> HandleKey {
    KeyData::from_ffi(handle).into()
}

#[cfg(unix)]
fn error_message_buffer(message: String) -> Box<[u8]> {
    let mut bytes = message.into_bytes();
    bytes.push(0);
    bytes.into_boxed_slice()
}

#[cfg(windows)]
fn error_message_buffer(message: String) -> Box<[u8]> {
    let mut bytes = Vec::with_capacity((message.len() + 1) * std::mem::size_of::<u16>());
    for unit in message.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.into_boxed_slice()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HandleKind {
    Resource,
    Job,
    Poll,
    Worker,
    CBuffer,
    #[cfg(unix)]
    ProcessArgv,
    ProcessEnv,
    AddrInfo,
    TlsConnection,
    #[cfg(windows)]
    IoResult,
}

struct HandleTable {
    handles: SlotMap<HandleKey, HandleKind>,
    resources: SecondaryMap<HandleKey, ResourceRef>,
    invalid_resource: HandleKey,
    stdio_resources: [HandleKey; 3],
}

impl Default for HandleTable {
    fn default() -> Self {
        let mut handles = SlotMap::with_key();
        let mut resources = SecondaryMap::new();

        let invalid_resource = handles.insert(HandleKind::Resource);
        resources.insert(invalid_resource, Arc::new(Resource::invalid()));

        #[cfg(unix)]
        let stdio_resources = STDIO_IDS.map(|id| {
            let key = handles.insert(HandleKind::Resource);
            resources.insert(key, Arc::new(Resource::stdio_file(id)));
            key
        });

        #[cfg(windows)]
        let stdio_resources = {
            use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
            use windows_sys::Win32::System::Console::GetStdHandle;

            let mut keys = [invalid_resource; 3];
            let mut raws = [0isize; 3];
            for (index, id) in WINDOWS_STDIO_IDS.iter().enumerate() {
                let handle = unsafe { GetStdHandle(*id) };
                if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                    continue;
                }
                let raw = handle as isize;
                if let Some(prev) = (0..index).find(|prev| raws[*prev] == raw) {
                    keys[index] = keys[prev];
                    continue;
                }
                let key = handles.insert(HandleKind::Resource);
                resources.insert(key, Arc::new(Resource::stdio_file(handle)));
                keys[index] = key;
                raws[index] = raw;
            }
            keys
        };

        Self {
            handles,
            resources,
            invalid_resource,
            stdio_resources,
        }
    }
}

impl HandleTable {
    fn invalid_fd(&self) -> HostHandle {
        handle_from_key(self.invalid_resource)
    }

    fn resource(&self, handle: HostHandle) -> AsyncHostResult<ResourceRef> {
        let key = self.key(handle, HandleKind::Resource)?;
        let resource = self.resources.get(key).ok_or(AsyncHostError::Badf)?;
        if resource.is_invalid() {
            return Err(AsyncHostError::Badf);
        }
        Ok(Arc::clone(resource))
    }

    fn resource_of_class(
        &self,
        handle: HostHandle,
        class: ResourceClass,
    ) -> AsyncHostResult<ResourceRef> {
        let resource = self.resource(handle)?;
        if resource.resource_class() != class {
            return Err(AsyncHostError::Inval);
        }
        Ok(resource)
    }

    fn socket(&self, handle: HostHandle) -> AsyncHostResult<ResourceRef> {
        let resource = self.resource(handle)?;
        if !resource.resource_class().is_socket() {
            return Err(AsyncHostError::Inval);
        }
        Ok(resource)
    }

    fn resource_is(&self, handle: HostHandle, resource: &ResourceRef) -> bool {
        self.key(handle, HandleKind::Resource)
            .ok()
            .and_then(|key| self.resources.get(key))
            .is_some_and(|current| Arc::ptr_eq(current, resource))
    }

    fn contains_resource(&self, handle: HostHandle) -> bool {
        self.key(handle, HandleKind::Resource)
            .ok()
            .and_then(|key| self.resources.get(key))
            .is_some_and(|resource| !resource.is_invalid())
    }

    fn remove_resource(&mut self, handle: HostHandle) -> AsyncHostResult<ResourceRef> {
        let key = self.key(handle, HandleKind::Resource)?;
        if key == self.invalid_resource || self.stdio_resources.contains(&key) {
            return Err(AsyncHostError::Badf);
        }
        self.handles.remove(key).ok_or(AsyncHostError::Badf)?;
        self.resources.remove(key).ok_or(AsyncHostError::Badf)
    }

    fn insert_resource(&mut self, resource: Resource) -> HostHandle {
        let key = self.handles.insert(HandleKind::Resource);
        self.resources.insert(key, Arc::new(resource));
        handle_from_key(key)
    }

    fn insert(&mut self, kind: HandleKind) -> HandleKey {
        self.handles.insert(kind)
    }

    fn job(&self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.key(handle, HandleKind::Job)
    }

    fn remove_job(&mut self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.remove(handle, HandleKind::Job)
    }

    fn poll(&self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.key(handle, HandleKind::Poll)
    }

    fn remove_poll(&mut self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.remove(handle, HandleKind::Poll)
    }

    fn worker(&self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.key(handle, HandleKind::Worker)
    }

    fn remove_worker(&mut self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.remove(handle, HandleKind::Worker)
    }

    fn remove_worker_key(&mut self, worker_key: HandleKey) {
        if self
            .handles
            .get(worker_key)
            .is_some_and(|kind| *kind == HandleKind::Worker)
        {
            self.handles.remove(worker_key);
        }
    }

    fn c_buffer(&self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.key(handle, HandleKind::CBuffer)
    }

    fn remove_c_buffer(&mut self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.remove(handle, HandleKind::CBuffer)
    }

    #[cfg(unix)]
    fn process_argv(&self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.key(handle, HandleKind::ProcessArgv)
    }

    #[cfg(unix)]
    fn remove_process_argv(&mut self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.remove(handle, HandleKind::ProcessArgv)
    }

    fn process_env(&self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.key(handle, HandleKind::ProcessEnv)
    }

    fn remove_process_env(&mut self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.remove(handle, HandleKind::ProcessEnv)
    }

    fn addrinfo(&self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.key(handle, HandleKind::AddrInfo)
    }

    fn remove_addrinfo(&mut self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.remove(handle, HandleKind::AddrInfo)
    }

    fn tls_connection(&self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.key(handle, HandleKind::TlsConnection)
    }

    fn remove_tls_connection(&mut self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.remove(handle, HandleKind::TlsConnection)
    }

    #[cfg(windows)]
    fn io_result(&self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.key(handle, HandleKind::IoResult)
    }

    #[cfg(windows)]
    fn remove_io_result(&mut self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.remove(handle, HandleKind::IoResult)
    }

    fn resource_count_excluding_reserved(&self) -> usize {
        self.resources
            .iter()
            .filter(|(key, resource)| {
                *key != self.invalid_resource
                    && !self.stdio_resources.contains(key)
                    && !resource.is_invalid()
            })
            .count()
    }

    fn handle_count_excluding_reserved(&self) -> usize {
        self.handles
            .iter()
            .filter(|(key, kind)| {
                if *key == self.invalid_resource || self.stdio_resources.contains(key) {
                    return false;
                }
                match kind {
                    HandleKind::Resource => self
                        .resources
                        .get(*key)
                        .is_some_and(|resource| !resource.is_invalid()),
                    _ => true,
                }
            })
            .count()
    }

    fn key(&self, handle: HostHandle, expected: HandleKind) -> AsyncHostResult<HandleKey> {
        let key = key_from_handle(handle);
        match self.handles.get(key) {
            Some(kind) if *kind == expected => Ok(key),
            _ => Err(AsyncHostError::Badf),
        }
    }

    fn remove(&mut self, handle: HostHandle, expected: HandleKind) -> AsyncHostResult<HandleKey> {
        let key = self.key(handle, expected)?;
        self.handles.remove(key).ok_or(AsyncHostError::Badf)?;
        Ok(key)
    }
}

impl ResourceTable for HandleTable {
    fn insert_file(&mut self, file: RawFd) -> AsyncHostResult<u64> {
        Ok(self.insert_resource(Resource::new(file)))
    }
}

// Jobs are one-shot work items with result handles. Queued jobs stay cancellable
// by handle, running jobs keep only a reservation slot, and finished jobs become
// result-readable but cannot be submitted again.
enum HostJobState {
    Ready(Job),
    Queued(Job),
    Running,
    ResultReady(Job),
}

struct JobTable {
    jobs: SecondaryMap<HandleKey, HostJobState>,
}

impl Default for JobTable {
    fn default() -> Self {
        Self {
            jobs: SecondaryMap::new(),
        }
    }
}

impl JobTable {
    fn insert_job(&mut self, key: HandleKey, job: Job) {
        self.jobs.insert(key, HostJobState::Ready(job));
    }

    fn visible_job(&self, key: HandleKey) -> AsyncHostResult<&Job> {
        match self.jobs.get(key) {
            Some(HostJobState::Ready(job) | HostJobState::ResultReady(job)) => Ok(job),
            _ => Err(AsyncHostError::Badf),
        }
    }

    fn visible_job_mut(&mut self, key: HandleKey) -> AsyncHostResult<&mut Job> {
        match self.jobs.get_mut(key) {
            Some(HostJobState::Ready(job) | HostJobState::ResultReady(job)) => Ok(job),
            _ => Err(AsyncHostError::Badf),
        }
    }

    fn take_ready_job(&mut self, key: HandleKey) -> AsyncHostResult<Job> {
        let slot = self.jobs.get_mut(key).ok_or(AsyncHostError::Badf)?;
        match std::mem::replace(slot, HostJobState::Running) {
            HostJobState::Ready(job) => Ok(job),
            other => {
                *slot = other;
                Err(AsyncHostError::Badf)
            }
        }
    }

    fn queue_job(&mut self, key: HandleKey) -> AsyncHostResult<()> {
        let slot = self.jobs.get_mut(key).ok_or(AsyncHostError::Badf)?;
        match std::mem::replace(slot, HostJobState::Running) {
            HostJobState::Ready(job) => {
                *slot = HostJobState::Queued(job);
                Ok(())
            }
            other => {
                *slot = other;
                Err(AsyncHostError::Badf)
            }
        }
    }

    #[cfg(windows)]
    fn queued_job_cancel_resource(&self, key: HandleKey) -> Option<ResourceRef> {
        match self.jobs.get(key) {
            Some(HostJobState::Queued(job)) => thread_pool::job_cancel_resource(job),
            _ => None,
        }
    }

    fn take_queued_job(&mut self, key: HandleKey) -> AsyncHostResult<Job> {
        let slot = self.jobs.get_mut(key).ok_or(AsyncHostError::Badf)?;
        match std::mem::replace(slot, HostJobState::Running) {
            HostJobState::Queued(job) => Ok(job),
            other => {
                *slot = other;
                Err(AsyncHostError::Badf)
            }
        }
    }

    fn unqueue_job(&mut self, key: HandleKey) -> AsyncHostResult<()> {
        let slot = self.jobs.get_mut(key).ok_or(AsyncHostError::Badf)?;
        match std::mem::replace(slot, HostJobState::Running) {
            HostJobState::Queued(job) => {
                *slot = HostJobState::Ready(job);
                Ok(())
            }
            other => {
                *slot = other;
                Err(AsyncHostError::Badf)
            }
        }
    }

    fn restore_job(&mut self, key: HandleKey, job: Job) -> Option<Job> {
        match self.jobs.get_mut(key) {
            Some(slot @ HostJobState::Running) => {
                *slot = HostJobState::ResultReady(job);
                None
            }
            _ => Some(job),
        }
    }
}

struct PollTable {
    polls: SecondaryMap<HandleKey, HostPoll>,
    current_event_poll: Option<HandleKey>,
}

impl Default for PollTable {
    fn default() -> Self {
        Self {
            polls: SecondaryMap::new(),
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
    #[cfg(unix)]
    old_signal_mask: Option<libc::sigset_t>,
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

#[cfg(unix)]
struct ThreadPoolSignalMaskGuard {
    old: Option<libc::sigset_t>,
}

#[cfg(unix)]
impl ThreadPoolSignalMaskGuard {
    fn new(old: libc::sigset_t) -> Self {
        Self { old: Some(old) }
    }

    fn commit(mut self) -> libc::sigset_t {
        self.old.take().unwrap()
    }
}

#[cfg(unix)]
impl Drop for ThreadPoolSignalMaskGuard {
    fn drop(&mut self) {
        if let Some(old) = self.old.take() {
            let _ = crate::async_sys::signal::restore_thread_pool_signal_mask(&old);
        }
    }
}

#[cfg(windows)]
struct IoResultTable {
    io_results: SecondaryMap<HandleKey, Box<HostIoResult>>,
    io_results_by_overlapped: HashMap<OverlappedAddr, HostHandle>,
}

#[cfg(windows)]
impl Default for IoResultTable {
    fn default() -> Self {
        Self {
            io_results: SecondaryMap::new(),
            io_results_by_overlapped: HashMap::new(),
        }
    }
}

#[cfg(windows)]
impl IoResultTable {
    fn has_pending_io_for_resource(&self, file: &ResourceRef) -> bool {
        self.io_results
            .values()
            .any(|result| result.protects_pending_resource(file))
    }
}

#[cfg(windows)]
#[derive(Debug, Clone)]
struct ThreadPoolCompletionTarget {
    poll: HandleKey,
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
struct RegisteredFd {
    handle: HostHandle,
    resource: ResourceRef,
}

#[derive(Debug)]
struct HostPoll {
    instance: PollInstance,
    registered_fds: HashMap<isize, RegisteredFd>,
    #[cfg(unix)]
    completion_notifier: Option<Arc<ThreadPoolCompletionNotifier>>,
    event_fd_handles: Vec<Option<HostHandle>>,
}

#[cfg(windows)]
const IO_RESULT_READ_EVENT: i32 = 1;

#[cfg(windows)]
const IO_RESULT_WRITE_EVENT: i32 = 2;

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
impl HostIoKind {
    fn resource(self, handles: &HandleTable, handle: HostHandle) -> AsyncHostResult<ResourceRef> {
        match self {
            Self::File => handles.resource_of_class(handle, ResourceClass::File),
            Self::Socket => handles.socket(handle),
            Self::SocketWithAddr => handles.resource_of_class(handle, ResourceClass::UdpSocket),
            Self::Connect | Self::Accept => Err(AsyncHostError::Inval),
        }
    }
}

#[cfg(windows)]
struct HostIoResult {
    overlapped: windows_sys::Win32::System::IO::OVERLAPPED,
    kind: HostIoKind,
    event: i32,
    // Native async retains MoonBit objects until free_io_result. The wasm host
    // cannot retain guest pointers, so IOResults keep only host-owned buffers:
    // read constructors allocate output capacity, and write constructors copy
    // the input payload before any overlapped operation can outlive the import.
    buffer: Vec<u8>,
    socket_flags: u32,
    addr_buffer: Vec<u8>,
    // WSARecvFrom may complete asynchronously and write through lpFromlen later.
    // Keep that storage with the overlapped result, not on the submitter stack.
    addr_len: i32,
    accept_buffer: Vec<u8>,
    accept_bytes_received: u32,
    pending_resource: Option<ResourceRef>,
    // AcceptEx submits one overlapped operation with both the listening socket
    // and a pre-created accepted socket. Cancel/status use pending_resource, but
    // close protection must cover the accepted socket as well until completion.
    extra_pending_close_resource: Option<ResourceRef>,
}

#[cfg(windows)]
// SAFETY: IoResultTable stores each value in a Box before its OVERLAPPED
// address can be submitted to Windows, never removes a pending result, and
// serializes all access through its mutex. Moving the Box between threads does
// not move the HostIoResult allocation; its buffers and ResourceRefs are owned
// and remain alive until the operation reaches a terminal state.
unsafe impl Send for HostIoResult {}

#[cfg(windows)]
impl HostIoResult {
    fn zeroed_overlapped() -> windows_sys::Win32::System::IO::OVERLAPPED {
        let overlapped =
            std::mem::MaybeUninit::<windows_sys::Win32::System::IO::OVERLAPPED>::zeroed();
        unsafe { overlapped.assume_init() }
    }

    fn for_file_read(len: i32, position: i64) -> AsyncHostResult<Self> {
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
            kind: HostIoKind::File,
            event,
            buffer,
            socket_flags: 0,
            addr_buffer: Vec::new(),
            addr_len: 0,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            pending_resource: None,
            extra_pending_close_resource: None,
        }
    }

    fn for_socket_read(len: i32, flags: i32) -> AsyncHostResult<Self> {
        let buffer = vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?];
        Ok(Self::for_socket(IO_RESULT_READ_EVENT, buffer, flags))
    }

    fn for_socket_write(buffer: Vec<u8>, flags: i32) -> Self {
        Self::for_socket(IO_RESULT_WRITE_EVENT, buffer, flags)
    }

    fn for_socket(event: i32, buffer: Vec<u8>, flags: i32) -> Self {
        Self {
            overlapped: Self::zeroed_overlapped(),
            kind: HostIoKind::Socket,
            event,
            buffer,
            socket_flags: flags as u32,
            addr_buffer: Vec::new(),
            addr_len: 0,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            pending_resource: None,
            extra_pending_close_resource: None,
        }
    }

    fn for_socket_with_addr_read(
        len: i32,
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
            kind: HostIoKind::SocketWithAddr,
            event,
            buffer,
            socket_flags: flags as u32,
            addr_buffer,
            addr_len,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            pending_resource: None,
            extra_pending_close_resource: None,
        })
    }

    fn for_connect(addr_buffer: Vec<u8>) -> Self {
        Self {
            overlapped: Self::zeroed_overlapped(),
            kind: HostIoKind::Connect,
            event: IO_RESULT_WRITE_EVENT,
            buffer: Vec::new(),
            socket_flags: 0,
            addr_buffer,
            addr_len: 0,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            pending_resource: None,
            extra_pending_close_resource: None,
        }
    }

    fn for_accept(addr_len: i32) -> AsyncHostResult<Self> {
        let addr_len_usize = usize::try_from(addr_len).map_err(|_| AsyncHostError::Fault)?;
        let accept_addr_len = addr_len_usize
            .checked_add(16)
            .ok_or(AsyncHostError::Fault)?;
        let accept_buffer_len = accept_addr_len
            .checked_mul(2)
            .ok_or(AsyncHostError::Fault)?;
        Ok(Self {
            overlapped: Self::zeroed_overlapped(),
            kind: HostIoKind::Accept,
            event: IO_RESULT_READ_EVENT,
            buffer: Vec::new(),
            socket_flags: 0,
            addr_buffer: Vec::new(),
            addr_len,
            accept_buffer: vec![0; accept_buffer_len],
            accept_bytes_received: 0,
            pending_resource: None,
            extra_pending_close_resource: None,
        })
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
        dst: i32,
        offset: i32,
        len: i32,
    ) -> AsyncHostResult<()> {
        let dst_offset = dst.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        let bytes_transferred = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let data = self
            .buffer
            .get(..bytes_transferred)
            .ok_or(AsyncHostError::Fault)?;
        memory.write_exact(dst_offset, data)?;
        Ok(())
    }

    fn copy_read_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        dst: i32,
        offset: i32,
        len: i32,
    ) -> AsyncHostResult<()> {
        self.validate_completed_read()?;
        if self.kind == HostIoKind::SocketWithAddr {
            return Err(AsyncHostError::Inval);
        }
        self.copy_read_payload(memory, dst, offset, len)
    }

    fn copy_read_result_with_addr(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        dst: i32,
        offset: i32,
        len: i32,
        addr: i32,
        addr_len: i32,
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
        memory.write_with_capacity(addr, addr_len, addr_data)
    }

    #[cfg(test)]
    fn pending_resource_identity(&self) -> Option<isize> {
        self.pending_resource
            .as_ref()
            .map(|file| file.raw_identity())
    }

    fn is_pending(&self) -> bool {
        self.pending_resource.is_some() || self.extra_pending_close_resource.is_some()
    }

    fn protects_pending_resource(&self, file: &ResourceRef) -> bool {
        self.pending_resource
            .as_ref()
            .is_some_and(|pending| Arc::ptr_eq(pending, file))
            || self
                .extra_pending_close_resource
                .as_ref()
                .is_some_and(|pending| Arc::ptr_eq(pending, file))
    }

    fn mark_pending(&mut self, file: ResourceRef) -> AsyncHostResult<()> {
        if self.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        self.pending_resource = Some(file);
        Ok(())
    }

    fn mark_pending_with_close_guard(
        &mut self,
        file: ResourceRef,
        close_guard: ResourceRef,
    ) -> AsyncHostResult<()> {
        self.mark_pending(file)?;
        self.extra_pending_close_resource = Some(close_guard);
        Ok(())
    }

    fn clear_pending(&mut self) {
        self.pending_resource = None;
        self.extra_pending_close_resource = None;
    }

    fn validate_pending_resource(&self, file: &ResourceRef) -> AsyncHostResult<()> {
        // The import boundary may receive malformed/stale fd handles. Validate
        // before asserting the internal "pending operation uses submitter fd"
        // invariant so debug builds do not panic on bad guest input.
        if let Some(pending_resource) = &self.pending_resource
            && !Arc::ptr_eq(pending_resource, file)
        {
            return Err(AsyncHostError::Badf);
        }
        debug_assert!(
            match &self.pending_resource {
                Some(pending_resource) => Arc::ptr_eq(pending_resource, file),
                None => true,
            },
            "pending IO operation must use the submitting handle"
        );
        Ok(())
    }

    fn cancel_pending(&mut self) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::{ERROR_IO_INCOMPLETE, ERROR_NOT_FOUND};
        use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult};

        let Some(file) = &self.pending_resource else {
            return Ok(0);
        };
        let raw_handle = raw_overlapped_handle(file)?;
        let overlapped = self.overlapped_ptr();
        if unsafe { CancelIoEx(raw_handle, overlapped) } == 0 {
            let errno = last_errno();
            if errno != ERROR_NOT_FOUND as i32 {
                return Err(AsyncHostError::Native(errno));
            }
        }

        let mut bytes_transferred = 0;
        if unsafe { GetOverlappedResult(raw_handle, overlapped, &mut bytes_transferred, 0) } != 0 {
            self.clear_pending();
            return Ok(0);
        }
        let errno = last_errno();
        if errno == ERROR_IO_INCOMPLETE as i32 {
            // Native leaves the result pending here so MoonBit waits for the
            // completion packet before freeing the IO result.
            Ok(1)
        } else {
            self.clear_pending();
            Ok(0)
        }
    }

    fn cancel_and_drain_pending(&mut self) -> AsyncHostResult<()> {
        use windows_sys::Win32::Foundation::ERROR_NOT_FOUND;
        use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult};

        let Some(file) = &self.pending_resource else {
            return Ok(());
        };
        let raw_handle = raw_overlapped_handle(file)?;
        let overlapped = self.overlapped_ptr();
        if unsafe { CancelIoEx(raw_handle, overlapped) } == 0 {
            let errno = last_errno();
            if errno != ERROR_NOT_FOUND as i32 {
                return Err(AsyncHostError::Native(errno));
            }
        }

        let mut bytes_transferred = 0;
        // With bWait=TRUE the operation has reached a final status when this
        // returns, even if the final status is an error such as EOF or broken
        // pipe. At that point the host no longer treats the result as pending.
        let _ = unsafe { GetOverlappedResult(raw_handle, overlapped, &mut bytes_transferred, 1) };
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
    // V8 enters this host synchronously on one thread. Handles and pollers stay
    // thread-local; worker threads receive only the explicitly shared state
    // below (jobs, cancellation state, policy state, and individual payloads).
    // RefCell encodes that ownership and avoids synchronization in the poll hot
    // path.
    //
    // State captured by worker threads remains behind Arc<Mutex<_>>. The other
    // payload tables keep their existing representation in this refactor.
    policy: Arc<AsyncPolicy>,
    errno: Mutex<i32>,
    addr_infos: Mutex<SecondaryMap<HandleKey, HostAddrInfo>>,
    c_buffers: Mutex<SecondaryMap<HandleKey, HostCBuffer>>,
    #[cfg(unix)]
    process_argvs: Mutex<SecondaryMap<HandleKey, HostProcessArgv>>,
    process_envs: Mutex<SecondaryMap<HandleKey, HostProcessEnv>>,
    // The unrestricted path leaves this absent, avoiding registry allocation and locking.
    process_policy_state: Option<Arc<ProcessPolicyState>>,
    #[cfg(windows)]
    io_results: Mutex<IoResultTable>,
    jobs: Arc<Mutex<JobTable>>,
    #[cfg(windows)]
    running_job_cancellations: Arc<Mutex<HashMap<HandleKey, ResourceRef>>>,
    polls: RefCell<PollTable>,
    thread_pool_completions: Mutex<ThreadPoolCompletions>,
    handles: RefCell<HandleTable>,
    tls_connections: Mutex<SecondaryMap<HandleKey, tls::TlsHandleRef>>,
    tls_error: Mutex<Option<String>>,
    workers: Mutex<SecondaryMap<HandleKey, HostWorkerHandle>>,
}

impl Default for AsyncHost {
    fn default() -> Self {
        Self::new(Arc::new(AsyncPolicy::allow_all()))
    }
}

impl AsyncHost {
    pub(crate) fn new(policy: Arc<AsyncPolicy>) -> Self {
        let process_policy_state = policy
            .has_process_policy()
            .then(|| Arc::new(ProcessPolicyState::default()));
        Self {
            policy,
            errno: Mutex::new(0),
            addr_infos: Mutex::new(SecondaryMap::new()),
            c_buffers: Mutex::new(SecondaryMap::new()),
            #[cfg(unix)]
            process_argvs: Mutex::new(SecondaryMap::new()),
            process_envs: Mutex::new(SecondaryMap::new()),
            process_policy_state,
            #[cfg(windows)]
            io_results: Mutex::new(IoResultTable::default()),
            jobs: Arc::new(Mutex::new(JobTable::default())),
            #[cfg(windows)]
            running_job_cancellations: Arc::new(Mutex::new(HashMap::new())),
            polls: RefCell::new(PollTable::default()),
            thread_pool_completions: Mutex::new(ThreadPoolCompletions::default()),
            handles: RefCell::new(HandleTable::default()),
            tls_connections: Mutex::new(SecondaryMap::new()),
            tls_error: Mutex::new(None),
            workers: Mutex::new(SecondaryMap::new()),
        }
    }

    pub(crate) fn invalid_fd(&self) -> HostHandle {
        self.handles.borrow().invalid_fd()
    }

    pub(crate) fn std_handle(&self, id: i32) -> AsyncHostResult<HostHandle> {
        let stdio = match id {
            STDIN_ID => Stdio::Stdin,
            STDOUT_ID => Stdio::Stdout,
            STDERR_ID => Stdio::Stderr,
            _ => return Err(AsyncHostError::Inval),
        };
        let handles = self.handles.borrow();
        let handle = handle_from_key(handles.stdio_resources[stdio as usize]);
        handles.resource(handle)?;
        Ok(handle)
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

    fn restore_job(&self, key: HandleKey, job: Job) -> AsyncHostResult<()> {
        let result = { self.jobs.lock().unwrap().restore_job(key, job) };
        if let Some(job) = result {
            Self::revoke_unclaimed_spawn(self.process_policy_state.as_deref(), &job);
            Err(AsyncHostError::Badf)
        } else {
            Ok(())
        }
    }

    fn queue_worker_job(&self, key: HandleKey) -> AsyncHostResult<()> {
        let mut jobs = self.jobs.lock().unwrap();
        jobs.queue_job(key)?;
        #[cfg(windows)]
        let cancel = jobs.queued_job_cancel_resource(key);
        drop(jobs);
        #[cfg(windows)]
        if let Some(cancel) = cancel {
            self.running_job_cancellations
                .lock()
                .unwrap()
                .insert(key, cancel);
        }
        Ok(())
    }

    #[cfg(windows)]
    fn unregister_worker_job_cancel(&self, key: HandleKey) {
        self.running_job_cancellations.lock().unwrap().remove(&key);
    }

    fn revoke_unclaimed_spawn(process_policy_state: Option<&ProcessPolicyState>, job: &Job) {
        let Some(state) = process_policy_state else {
            return;
        };
        if job.err() != 0 || job.ret() < 0 {
            return;
        }
        let unclaimed = match job.payload() {
            #[cfg(unix)]
            JobPayload::SpawnUnix { result, .. } => {
                !matches!(result, Some(OpenJobResource::Published(_)))
            }
            #[cfg(windows)]
            JobPayload::SpawnWindows { result, .. } => {
                !matches!(result, Some(OpenJobResource::Published(_)))
            }
            _ => false,
        };
        if unclaimed {
            let pid = job.ret() as i32;
            let mut state = state.inner.lock().unwrap();
            if !state
                .process_handle_pids
                .values()
                .any(|tracked_pid| *tracked_pid == pid)
            {
                state.owned_child_pids.remove(&pid);
            }
        }
    }

    fn publish_open_job_result(&self, key: HandleKey) -> AsyncHostResult<HostHandle> {
        let mut jobs = self.jobs.lock().unwrap();
        let placeholder = OpenJobResource::Published(self.invalid_fd());
        let file = {
            let job = jobs.visible_job_mut(key)?;
            let result = thread_pool::open_job_result_mut(job)?;
            match std::mem::replace(&mut result.resource, placeholder) {
                OpenJobResource::Published(fd) => {
                    result.resource = OpenJobResource::Published(fd);
                    return Ok(fd);
                }
                OpenJobResource::Unpublished(file) => file,
            }
        };

        let fd = self.handles.borrow_mut().insert_resource(file);
        let job = jobs.visible_job_mut(key)?;
        let result = thread_pool::open_job_result_mut(job)?;
        result.resource = OpenJobResource::Published(fd);
        thread_pool::open_job_get_fd(result)
    }

    pub(crate) fn assert_no_leaked_handles_if_enabled(&self) {
        if std::thread::panicking() || std::env::var_os(CHECK_FD_LEAK_ENV).is_none() {
            return;
        }
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
                let polls = self.polls.borrow();
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
                let tls_connections = self.tls_connections.lock().unwrap();
                if !tls_connections.is_empty() {
                    leaks.push(format!("tls_connections={}", tls_connections.len()));
                }
            }
            {
                let handles = self.handles.borrow();
                let leaked_resources = handles.resource_count_excluding_reserved();
                let leaked_handles = handles.handle_count_excluding_reserved();
                let invalid_resource_is_valid = handles
                    .handles
                    .get(handles.invalid_resource)
                    .is_some_and(|kind| *kind == HandleKind::Resource)
                    && handles
                        .resources
                        .get(handles.invalid_resource)
                        .is_some_and(|resource| resource.is_invalid());
                if !invalid_resource_is_valid {
                    leaks.push("invalid_resource=invalid".to_string());
                }
                if leaked_handles != 0 {
                    leaks.push(format!("handles={leaked_handles}"));
                }
                if leaked_resources != 0 {
                    leaks.push(format!("resources={leaked_resources}"));
                }
            }
            {
                let workers = self.workers.lock().unwrap();
                if !workers.is_empty() {
                    leaks.push(format!("workers={}", workers.len()));
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
        let key = self.handles.borrow_mut().insert(HandleKind::Poll);
        self.polls.borrow_mut().polls.insert(
            key,
            HostPoll {
                instance,
                registered_fds: HashMap::new(),
                #[cfg(unix)]
                completion_notifier: None,
                event_fd_handles: Vec::new(),
            },
        );
        Ok(handle_from_key(key))
    }

    pub(crate) fn poll_destroy(&self, handle: u64) -> AsyncHostResult<()> {
        let poll_key = self.handles.borrow_mut().remove_poll(handle)?;
        let poll = {
            let mut polls = self.polls.borrow_mut();
            polls.polls.remove(poll_key).ok_or(AsyncHostError::Badf)?
        };

        {
            let mut polls = self.polls.borrow_mut();
            if polls.current_event_poll == Some(poll_key) {
                polls.current_event_poll = None;
            }
        }

        #[cfg(unix)]
        let (completion_source, old_signal_mask) = {
            let mut completions = self.thread_pool_completions.lock().unwrap();
            if let Some(notifier) = &poll.completion_notifier
                && completions
                    .notifier
                    .as_ref()
                    .is_some_and(|active| Arc::ptr_eq(active, notifier))
            {
                completions.notifier = None;
                (
                    completions.source.take(),
                    completions.old_signal_mask.take(),
                )
            } else {
                (None, None)
            }
        };
        #[cfg(unix)]
        {
            if let Some(source) = completion_source {
                let _ = self.handles.borrow_mut().remove_resource(source);
            }
            if let Some(old_signal_mask) = old_signal_mask {
                let _ = crate::async_sys::signal::restore_thread_pool_signal_mask(&old_signal_mask);
            }
        }
        #[cfg(windows)]
        {
            let mut completions = self.thread_pool_completions.lock().unwrap();
            if completions
                .target
                .as_ref()
                .is_some_and(|target| target.poll == poll_key)
            {
                let _ = crate::async_sys::signal::set_console_control_handler(false, None);
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
        let resource = self.resource(fd_handle)?;
        let resource_identity = resource.raw_identity();
        #[cfg(unix)]
        let raw_fd = resource.as_file()?.as_raw_fd();
        let poll_key = self.handles.borrow().poll(poll_handle)?;
        let mut polls = self.polls.borrow_mut();
        let poll = polls.polls.get_mut(poll_key).ok_or(AsyncHostError::Badf)?;
        #[cfg(unix)]
        poll::poll_register(&poll.instance, raw_fd, read_only)?;
        #[cfg(windows)]
        if resource.resource_class().is_socket() {
            poll::poll_register_socket(&poll.instance, resource.as_socket()?, read_only)?;
        } else {
            poll::poll_register_file(
                &poll.instance,
                resource.as_file()?.as_raw_handle(),
                read_only,
            )?;
        }
        let handles = self.handles.borrow();
        if !handles.resource_is(fd_handle, &resource) {
            drop(handles);
            #[cfg(unix)]
            poll::poll_unregister(&poll.instance, raw_fd)?;
            return Err(AsyncHostError::Badf);
        }
        poll.registered_fds.insert(
            resource_identity,
            RegisteredFd {
                handle: fd_handle,
                resource,
            },
        );
        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn poll_register_pid(&self, poll_handle: u64, pid: i32) -> AsyncHostResult<i32> {
        self.with_owned_child_pid(pid, || {
            let poll_key = self.handles.borrow().poll(poll_handle)?;
            let polls = self.polls.borrow();
            let poll = polls.polls.get(poll_key).ok_or(AsyncHostError::Badf)?;
            poll::poll_register_pid(&poll.instance, pid)
        })
    }

    pub(crate) fn poll_wait(&self, poll_handle: u64, timeout_ms: i32) -> AsyncHostResult<i32> {
        let poll_key = self.handles.borrow().poll(poll_handle)?;
        #[cfg(windows)]
        let (thread_pool_generation, invalid_fd) = {
            let thread_pool_generation = self
                .thread_pool_completions
                .lock()
                .unwrap()
                .target
                .as_ref()
                .filter(|target| target.poll == poll_key)
                .map(|target| target.generation);
            (thread_pool_generation, self.invalid_fd())
        };
        let mut polls = self.polls.borrow_mut();
        let result = {
            let poll = polls.polls.get_mut(poll_key).ok_or(AsyncHostError::Badf)?;
            #[cfg(not(windows))]
            let result = poll::poll_wait(&mut poll.instance, timeout_ms)?;
            #[cfg(windows)]
            let result = {
                let deadline = (timeout_ms >= 0).then(|| {
                    std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms as u64)
                });
                let mut next_timeout = timeout_ms;
                loop {
                    poll::poll_wait(&mut poll.instance, next_timeout)?;
                    let result = poll::retain_current_thread_pool_completions(
                        &mut poll.instance,
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
            poll.event_fd_handles.clear();
            for index in 0..result {
                let event = poll::event_list_get(&poll.instance, index)?;
                let raw_fd = poll::event_get_fd(event);
                let fd_handle = poll
                    .registered_fds
                    .get(&raw_fd_key(raw_fd))
                    .map(|registered| registered.handle)
                    .or_else(|| completion_event_fd(raw_fd));
                #[cfg(windows)]
                let fd_handle = fd_handle.or(Some(invalid_fd));
                poll.event_fd_handles.push(fd_handle);
            }
            result
        };
        polls.current_event_poll = Some(poll_key);
        Ok(result)
    }

    pub(crate) fn poll_get_event(&self, poll_handle: u64, index: i32) -> AsyncHostResult<u64> {
        let poll_key = self.handles.borrow().poll(poll_handle)?;
        let polls = self.polls.borrow();
        if polls.current_event_poll != Some(poll_key) {
            return Err(AsyncHostError::Badf);
        }
        let poll = polls.polls.get(poll_key).ok_or(AsyncHostError::Badf)?;
        poll::event_list_get(&poll.instance, index)?;
        u64::try_from(index).map_err(|_| AsyncHostError::Fault)
    }

    fn with_event<T>(
        &self,
        event_handle: u64,
        f: impl FnOnce(&HostPoll, &poll::PollEvent) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let index = event_index(event_handle)?;
        let polls = self.polls.borrow();
        let poll_key = polls.current_event_poll.ok_or(AsyncHostError::Badf)?;
        let poll = polls.polls.get(poll_key).ok_or(AsyncHostError::Badf)?;
        let poll_event = poll::event_list_get(&poll.instance, index)?;
        f(poll, poll_event)
    }

    pub(crate) fn poll_event_fd(&self, event_handle: u64) -> AsyncHostResult<HostHandle> {
        let index = event_index(event_handle)?;
        let (fd, registered_resource) = {
            let polls = self.polls.borrow();
            let poll_key = polls.current_event_poll.ok_or(AsyncHostError::Badf)?;
            let poll = polls.polls.get(poll_key).ok_or(AsyncHostError::Badf)?;
            let event = poll::event_list_get(&poll.instance, index)?;
            let raw_fd = poll::event_get_fd(event);
            let registered_resource = poll
                .registered_fds
                .get(&raw_fd_key(raw_fd))
                .map(|registered| Arc::clone(&registered.resource));
            let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
            let fd = poll
                .event_fd_handles
                .get(index)
                .copied()
                .flatten()
                .ok_or(AsyncHostError::Badf)?;
            (fd, registered_resource)
        };
        #[cfg(windows)]
        let is_thread_pool_completion =
            fd == raw_fd_to_guest(windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE)?;
        #[cfg(not(windows))]
        let is_thread_pool_completion = false;
        let handles = self.handles.borrow();
        if fd == handles.invalid_fd() || is_thread_pool_completion {
            Ok(fd)
        } else if let Some(registered_resource) = registered_resource {
            if handles.resource_is(fd, &registered_resource) {
                Ok(fd)
            } else {
                Err(AsyncHostError::Badf)
            }
        } else {
            handles
                .contains_resource(fd)
                .then_some(fd)
                .ok_or(AsyncHostError::Badf)
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn poll_event_pid(&self, event_handle: u64) -> AsyncHostResult<i32> {
        self.with_event(event_handle, |_, event| Ok(poll::event_get_fd(event)))
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
        let handle = {
            let io_results = self.io_results.lock().unwrap();
            io_results
                .io_results_by_overlapped
                .get(&overlapped)
                .copied()
                .ok_or(AsyncHostError::Badf)?
        };
        let key = self.handles.borrow().io_result(handle)?;
        let mut io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get_mut(key)
            .ok_or(AsyncHostError::Badf)?;
        result.clear_pending();
        Ok(handle)
    }

    #[cfg(windows)]
    pub(crate) fn poll_event_bytes_transferred(&self, event_handle: u64) -> AsyncHostResult<i32> {
        self.with_event(event_handle, |_, event| {
            Ok(poll::event_get_bytes_transferred(event))
        })
    }

    pub(crate) fn init_thread_pool(&self, poll_handle: u64) -> AsyncHostResult<HostHandle> {
        let poll_key = self.handles.borrow().poll(poll_handle)?;
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
            let old_signal_mask = crate::async_sys::signal::init_thread_pool_signal_mask()?;
            let signal_mask_guard = ThreadPoolSignalMaskGuard::new(old_signal_mask);
            let mut polls = self.polls.borrow_mut();
            let poll = polls.polls.get_mut(poll_key).ok_or(AsyncHostError::Badf)?;
            let (completion_notifier, event_fd) =
                ThreadPoolCompletionNotifier::new(&poll.instance)?;
            let completion_notifier = Arc::new(completion_notifier);
            let source = {
                let mut completions = self.thread_pool_completions.lock().unwrap();
                if completions.source.is_some() {
                    drop(completions);
                    let _ = poll::poll_unregister(&poll.instance, event_fd);
                    drop(Resource::new(event_fd));
                    return Err(AsyncHostError::Inval);
                }
                let mut handles = self.handles.borrow_mut();
                let source = handles.insert_resource(Resource::new(event_fd));
                let source_resource = handles.resource(source)?;
                drop(handles);
                // Publish the poll-side mapping before exposing the notifier:
                // workers can notify as soon as completions.notifier is visible.
                poll.registered_fds.insert(
                    raw_fd_key(event_fd),
                    RegisteredFd {
                        handle: source,
                        resource: source_resource,
                    },
                );
                poll.completion_notifier = Some(Arc::clone(&completion_notifier));
                completions.notifier = Some(completion_notifier);
                completions.source = Some(source);
                completions.old_signal_mask = Some(signal_mask_guard.commit());
                source
            };
            Ok(source)
        }
        #[cfg(windows)]
        {
            let polls = self.polls.borrow();
            let poll = polls.polls.get(poll_key).ok_or(AsyncHostError::Badf)?;
            let completion_port = poll::CompletionPort::from_poll(&poll.instance);
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
        let worker_keys = self
            .workers
            .lock()
            .unwrap()
            .iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>();
        let workers = worker_keys
            .into_iter()
            .filter_map(|key| {
                self.handles.borrow_mut().remove_worker_key(key);
                self.workers.lock().unwrap().remove(key)
            })
            .collect::<Vec<_>>();
        for worker in &workers {
            let _ = self.cancel_host_worker(worker);
        }
        for worker in workers {
            if let Some(replaced_job) = thread_pool::free_worker(worker) {
                #[cfg(windows)]
                self.unregister_worker_job_cancel(replaced_job.job_key);
                let _ = self.jobs.lock().unwrap().unqueue_job(replaced_job.job_key);
            }
        }
        #[cfg(unix)]
        {
            let (completion_source, old_signal_mask) = {
                let mut completions = self.thread_pool_completions.lock().unwrap();
                let completion_source = completions.source.take();
                completions.notifier = None;
                (completion_source, completions.old_signal_mask.take())
            };
            let mut polls = self.polls.borrow_mut();
            if let Some(source) = completion_source
                && let Ok(file) = self.handles.borrow_mut().remove_resource(source)
                && let Ok(file) = file.as_fd()
            {
                let raw_fd = file.as_raw_fd();
                for poll in polls.polls.values_mut() {
                    if poll.registered_fds.contains_key(&raw_fd_key(raw_fd)) {
                        let _ = poll::poll_unregister(&poll.instance, raw_fd);
                    }
                    poll.registered_fds.remove(&raw_fd_key(raw_fd));
                }
            }
            for poll in polls.polls.values_mut() {
                poll.completion_notifier = None;
            }
            if let Some(old_signal_mask) = old_signal_mask {
                let _ = crate::async_sys::signal::restore_thread_pool_signal_mask(&old_signal_mask);
            }
        }
        #[cfg(windows)]
        {
            let _ = crate::async_sys::signal::set_console_control_handler(false, None);
            self.thread_pool_completions.lock().unwrap().target = None;
        }
    }

    pub(crate) fn insert_c_buffer(&self, buffer: Box<[u8]>) -> u64 {
        let key = self.handles.borrow_mut().insert(HandleKind::CBuffer);
        self.c_buffers
            .lock()
            .unwrap()
            .insert(key, Arc::new(Mutex::new(buffer)));
        handle_from_key(key)
    }

    pub(crate) fn free_c_buffer(&self, handle: u64) -> AsyncHostResult<()> {
        if handle == INVALID_HOST_HANDLE {
            return Ok(());
        }
        let key = self.handles.borrow_mut().remove_c_buffer(handle)?;
        self.c_buffers
            .lock()
            .unwrap()
            .remove(key)
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
        let key = self.handles.borrow().c_buffer(handle)?;
        self.c_buffers
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or(AsyncHostError::Badf)
    }

    #[cfg(unix)]
    pub(crate) fn insert_process_argv(&self, len: i32) -> AsyncHostResult<u64> {
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let key = self.handles.borrow_mut().insert(HandleKind::ProcessArgv);
        self.process_argvs
            .lock()
            .unwrap()
            .insert(key, Arc::new(Mutex::new(vec![None; len])));
        Ok(handle_from_key(key))
    }

    #[cfg(unix)]
    pub(crate) fn process_argv_add_entry(
        &self,
        handle: u64,
        index: i32,
        value: OsString,
    ) -> AsyncHostResult<()> {
        let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
        let argv = self.process_argv(handle)?;
        let mut argv = argv.lock().unwrap();
        let slot = argv.get_mut(index).ok_or(AsyncHostError::Fault)?;
        *slot = Some(value);
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn take_process_spawn_buffers(
        &self,
        argv_handle: u64,
        env_handle: u64,
    ) -> AsyncHostResult<(Vec<OsString>, Vec<OsString>)> {
        if argv_handle == INVALID_HOST_HANDLE || env_handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }

        // Validate both buffers before consuming either handle. A malformed
        // spawn request must not partially transfer ownership.
        let mut handles = self.handles.borrow_mut();
        let argv_key = handles.process_argv(argv_handle)?;
        let env_key = handles.process_env(env_handle)?;
        let mut process_argvs = self.process_argvs.lock().unwrap();
        let argv = process_argvs
            .get(argv_key)
            .cloned()
            .ok_or(AsyncHostError::Badf)?;
        let mut process_envs = self.process_envs.lock().unwrap();
        let env = process_envs
            .get(env_key)
            .cloned()
            .ok_or(AsyncHostError::Badf)?;
        let mut argv = argv.lock().unwrap();
        let mut env = env.lock().unwrap();
        if argv.iter().any(Option::is_none) || env.iter().any(Option::is_none) {
            return Err(AsyncHostError::Inval);
        }

        handles.remove_process_argv(argv_handle)?;
        handles.remove_process_env(env_handle)?;
        let _ = process_argvs.remove(argv_key);
        let _ = process_envs.remove(env_key);
        let argv = std::mem::take(&mut *argv)
            .into_iter()
            .map(Option::unwrap)
            .collect();
        let env = std::mem::take(&mut *env)
            .into_iter()
            .map(Option::unwrap)
            .collect();
        Ok((argv, env))
    }

    #[cfg(unix)]
    pub(crate) fn insert_process_env(&self, entries: Vec<Option<OsString>>) -> u64 {
        let key = self.handles.borrow_mut().insert(HandleKind::ProcessEnv);
        self.process_envs
            .lock()
            .unwrap()
            .insert(key, Arc::new(Mutex::new(entries)));
        handle_from_key(key)
    }

    #[cfg(windows)]
    pub(crate) fn insert_process_env(&self, env: Vec<u16>) -> u64 {
        let key = self.handles.borrow_mut().insert(HandleKind::ProcessEnv);
        self.process_envs
            .lock()
            .unwrap()
            .insert(key, Arc::new(Mutex::new(env)));
        handle_from_key(key)
    }

    #[cfg(unix)]
    pub(crate) fn process_env_length(&self, handle: u64) -> AsyncHostResult<i32> {
        let env = self.process_env(handle)?;
        let env = env.lock().unwrap();
        i32::try_from(env.len()).map_err(|_| AsyncHostError::Fault)
    }

    #[cfg(windows)]
    pub(crate) fn process_env_length(&self, handle: u64) -> AsyncHostResult<i32> {
        let env = self.process_env(handle)?;
        let env = env.lock().unwrap();
        let len = env.len().checked_sub(1).ok_or(AsyncHostError::Fault)?;
        i32::try_from(len).map_err(|_| AsyncHostError::Fault)
    }

    #[cfg(unix)]
    pub(crate) fn transfer_process_env_block(
        &self,
        dst_handle: u64,
        src_handle: u64,
    ) -> AsyncHostResult<()> {
        if dst_handle == src_handle {
            return Err(AsyncHostError::Inval);
        }
        let dst = self.process_env(dst_handle)?;
        let src = self.take_process_env_handle(src_handle)?;
        // The source is the temporary snapshot returned by get_curr_env.
        // Consume it here so its lifetime does not depend on deprecated free_env.
        let src = std::mem::take(&mut *src.lock().unwrap());
        let mut dst = dst.lock().unwrap();
        if dst.len() < src.len() {
            return Err(AsyncHostError::Fault);
        }
        for (index, entry) in src.into_iter().enumerate() {
            dst[index] = entry;
        }
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn transfer_process_env_block(
        &self,
        dst_handle: u64,
        src_handle: u64,
    ) -> AsyncHostResult<()> {
        if dst_handle == src_handle {
            return Err(AsyncHostError::Inval);
        }
        let dst = self.process_env(dst_handle)?;
        let src = self.take_process_env_handle(src_handle)?;
        // The source is the temporary snapshot returned by get_curr_env.
        // Consume it here so its lifetime does not depend on deprecated free_env.
        let src = std::mem::take(&mut *src.lock().unwrap());
        let mut dst = dst.lock().unwrap();
        let src_len = src.len().checked_sub(1).ok_or(AsyncHostError::Fault)?;
        if dst.len() <= src_len {
            return Err(AsyncHostError::Fault);
        }
        dst[..src_len].copy_from_slice(&src[..src_len]);
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn process_env_add_entry(
        &self,
        handle: u64,
        index: i32,
        entry: OsString,
    ) -> AsyncHostResult<()> {
        let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
        let env = self.process_env(handle)?;
        let mut env = env.lock().unwrap();
        let slot = env.get_mut(index).ok_or(AsyncHostError::Fault)?;
        *slot = Some(entry);
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn process_env_add_entry(
        &self,
        handle: u64,
        offset: i32,
        key: &[u16],
        value: &[u16],
    ) -> AsyncHostResult<()> {
        let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
        let env = self.process_env(handle)?;
        let mut env = env.lock().unwrap();
        let value_start = offset
            .checked_add(key.len())
            .and_then(|index| index.checked_add(1))
            .ok_or(AsyncHostError::Fault)?;
        let end = value_start
            .checked_add(value.len())
            .and_then(|index| index.checked_add(1))
            .ok_or(AsyncHostError::Fault)?;
        if end > env.len() {
            return Err(AsyncHostError::Fault);
        }
        env[offset..offset + key.len()].copy_from_slice(key);
        env[offset + key.len()] = b'=' as u16;
        env[value_start..value_start + value.len()].copy_from_slice(value);
        env[value_start + value.len()] = 0;
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn take_process_env(&self, handle: u64) -> AsyncHostResult<Vec<u16>> {
        let env = self.take_process_env_handle(handle)?;
        let result = std::mem::take(&mut *env.lock().unwrap());
        Ok(result)
    }

    fn take_process_env_handle(&self, handle: u64) -> AsyncHostResult<HostProcessEnv> {
        if handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }
        let key = self.handles.borrow_mut().remove_process_env(handle)?;
        self.process_envs
            .lock()
            .unwrap()
            .remove(key)
            .ok_or(AsyncHostError::Badf)
    }

    #[cfg(unix)]
    fn process_argv(&self, handle: u64) -> AsyncHostResult<HostProcessArgv> {
        if handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }
        let key = self.handles.borrow().process_argv(handle)?;
        self.process_argvs
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or(AsyncHostError::Badf)
    }

    fn process_env(&self, handle: u64) -> AsyncHostResult<HostProcessEnv> {
        if handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }
        let key = self.handles.borrow().process_env(handle)?;
        self.process_envs
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or(AsyncHostError::Badf)
    }

    pub(crate) fn insert_job(&self, job: Job) -> AsyncHostResult<u64> {
        let key = self.handles.borrow_mut().insert(HandleKind::Job);
        self.jobs.lock().unwrap().insert_job(key, job);
        Ok(handle_from_key(key))
    }

    pub(crate) fn free_job(&self, handle: u64) -> AsyncHostResult<()> {
        let key = self.handles.borrow_mut().remove_job(handle)?;
        let state = self
            .jobs
            .lock()
            .unwrap()
            .jobs
            .remove(key)
            .ok_or(AsyncHostError::Badf)?;
        match state {
            HostJobState::Ready(job)
            | HostJobState::Queued(job)
            | HostJobState::ResultReady(job) => {
                Self::revoke_unclaimed_spawn(self.process_policy_state.as_deref(), &job);

                // Native realpath frees its resolved path from the job finalizer.
                // After get_realpath_result exposes that path as a host c_buffer,
                // freeing the job must also release the c_buffer slot.
                if let thread_pool::JobPayload::Realpath {
                    result: Some(thread_pool::RealpathJobResult::Published(buffer_handle)),
                    ..
                } = job.payload()
                {
                    let _ = self.free_c_buffer(*buffer_handle);
                }
            }
            HostJobState::Running => {}
        }
        Ok(())
    }

    pub(crate) fn job_get_ret(&self, handle: u64) -> AsyncHostResult<i64> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.lock().unwrap();
        let job = jobs.visible_job(key)?;
        Ok(crate::async_sys::internal::event_loop::thread_pool::job_get_ret(job))
    }

    pub(crate) fn job_get_err(&self, handle: u64) -> AsyncHostResult<i32> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.lock().unwrap();
        let job = jobs.visible_job(key)?;
        Ok(crate::async_sys::internal::event_loop::thread_pool::job_get_err(job))
    }

    pub(crate) fn open_job_get_fd(&self, handle: u64) -> AsyncHostResult<HostHandle> {
        let key = self.handles.borrow().job(handle)?;
        self.publish_open_job_result(key)
    }

    pub(crate) fn open_job_get_kind(&self, handle: u64) -> AsyncHostResult<i32> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.lock().unwrap();
        let job = jobs.visible_job(key)?;
        let result = thread_pool::open_job_result(job)?;
        Ok(thread_pool::open_job_get_kind(result))
    }

    pub(crate) fn open_job_get_dev_id(&self, handle: u64) -> AsyncHostResult<u64> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.lock().unwrap();
        let job = jobs.visible_job(key)?;
        let result = thread_pool::open_job_result(job)?;
        Ok(thread_pool::open_job_get_dev_id(result))
    }

    pub(crate) fn open_job_get_file_id(&self, handle: u64) -> AsyncHostResult<u64> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.lock().unwrap();
        let job = jobs.visible_job(key)?;
        let result = thread_pool::open_job_result(job)?;
        Ok(thread_pool::open_job_get_file_id(result))
    }

    pub(crate) fn get_file_size_result(&self, handle: u64) -> AsyncHostResult<i64> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.lock().unwrap();
        let job = jobs.visible_job(key)?;
        crate::async_sys::internal::event_loop::thread_pool::get_file_size_result(job)
    }

    pub(crate) fn get_getaddrinfo_result(&self, handle: u64) -> AsyncHostResult<u64> {
        let (host, addrs) = {
            let key = self.handles.borrow().job(handle)?;
            let jobs = self.jobs.lock().unwrap();
            let job = jobs.visible_job(key)?;
            let (host, addrs) = thread_pool::getaddrinfo_job_result(job)?;
            (host.to_os_string(), addrs.to_vec())
        };
        self.policy.register_dns_result(&host, &addrs)?;
        let (entries, next) = {
            let mut handles = self.handles.borrow_mut();
            let mut entries = Vec::new();
            let mut next = None;
            for addr in addrs.into_iter().rev() {
                let key = handles.insert(HandleKind::AddrInfo);
                let handle = handle_from_key(key);
                entries.push((key, HostAddrInfo { addr, next }));
                next = Some(handle);
            }
            (entries, next)
        };
        let mut addr_infos = self.addr_infos.lock().unwrap();
        for (key, addrinfo) in entries {
            addr_infos.insert(key, addrinfo);
        }
        Ok(next.unwrap_or(INVALID_HOST_HANDLE))
    }

    pub(crate) fn get_spawn_job_result_handle(&self, handle: u64) -> AsyncHostResult<HostHandle> {
        let key = self.handles.borrow().job(handle)?;
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs.visible_job_mut(key)?;
        let Some(result) = thread_pool::take_spawn_job_result(job)? else {
            let fd = self.invalid_fd();
            thread_pool::set_spawn_job_result(job, OpenJobResource::Published(fd))?;
            return Ok(fd);
        };
        let resource = match result {
            OpenJobResource::Published(fd) => {
                thread_pool::set_spawn_job_result(job, OpenJobResource::Published(fd))?;
                return Ok(fd);
            }
            OpenJobResource::Unpublished(resource) => resource,
        };
        let process_pid = self.process_policy_state.as_ref().map(|_| job.ret() as i32);
        let fd = self.handles.borrow_mut().insert_resource(resource);
        if let Some(pid) = process_pid {
            self.track_process_handle(fd, pid);
        }
        thread_pool::set_spawn_job_result(job, OpenJobResource::Published(fd))?;
        thread_pool::get_spawn_job_result_handle(job)
    }

    pub(crate) fn addrinfo_next(&self, handle: u64) -> AsyncHostResult<u64> {
        if handle == INVALID_HOST_HANDLE {
            return Ok(INVALID_HOST_HANDLE);
        }
        let key = self.handles.borrow().addrinfo(handle)?;
        let addr_infos = self.addr_infos.lock().unwrap();
        let addrinfo = addr_infos.get(key).ok_or(AsyncHostError::Badf)?;
        Ok(addrinfo.next.unwrap_or(INVALID_HOST_HANDLE))
    }

    pub(crate) fn addrinfo_addr(&self, handle: u64) -> AsyncHostResult<Box<[u8]>> {
        let key = self.handles.borrow().addrinfo(handle)?;
        let addr_infos = self.addr_infos.lock().unwrap();
        let addrinfo = addr_infos.get(key).ok_or(AsyncHostError::Badf)?;
        Ok(addrinfo.addr.clone())
    }

    pub(crate) fn free_addrinfo(&self, handle: u64) -> AsyncHostResult<()> {
        if handle == INVALID_HOST_HANDLE {
            return Ok(());
        }
        let mut current = Some(handle);
        while let Some(handle) = current {
            let key = self.handles.borrow_mut().remove_addrinfo(handle)?;
            let mut addr_infos = self.addr_infos.lock().unwrap();
            let addrinfo = addr_infos.remove(key).ok_or(AsyncHostError::Badf)?;
            current = addrinfo.next;
        }
        Ok(())
    }

    pub(crate) fn close_fd(&self, handle: HostHandle) -> AsyncHostResult<()> {
        let file = {
            let mut handles = self.handles.borrow_mut();
            #[cfg(windows)]
            {
                let file = handles.resource(handle)?;
                if self
                    .io_results
                    .lock()
                    .unwrap()
                    .has_pending_io_for_resource(&file)
                {
                    return Err(AsyncHostError::Inval);
                }
            }
            handles.remove_resource(handle)?
        };
        self.untrack_process_handle(handle);
        let resource_identity = file.raw_identity();
        #[cfg(unix)]
        let raw_fd = file.as_fd()?.as_raw_fd();
        let mut polls = self.polls.borrow_mut();
        for poll in polls.polls.values_mut() {
            if poll.registered_fds.contains_key(&resource_identity) {
                #[cfg(unix)]
                let _ = poll::poll_unregister(&poll.instance, raw_fd);
            }
            poll.registered_fds.remove(&resource_identity);
        }
        #[cfg(unix)]
        {
            let (completion_source_closed, old_signal_mask) = {
                let mut completions = self.thread_pool_completions.lock().unwrap();
                if completions.source == Some(handle) {
                    completions.source = None;
                    completions.notifier = None;
                    (true, completions.old_signal_mask.take())
                } else {
                    (false, None)
                }
            };
            if completion_source_closed {
                for poll in polls.polls.values_mut() {
                    poll.completion_notifier = None;
                }
                if let Some(old_signal_mask) = old_signal_mask {
                    let _ =
                        crate::async_sys::signal::restore_thread_pool_signal_mask(&old_signal_mask);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn insert_socket_resource(
        &self,
        raw_socket: RawSocket,
        class: ResourceClass,
        family: i32,
    ) -> HostHandle {
        let file = Resource::new_socket(raw_socket, class, family);
        self.handles.borrow_mut().insert_resource(file)
    }

    pub(crate) fn insert_failed_job(&self, error: AsyncHostError) -> AsyncHostResult<u64> {
        self.insert_job(thread_pool::make_failed_job(error.errno()))
    }

    pub(crate) fn policy(&self) -> &AsyncPolicy {
        &self.policy
    }

    #[cfg(test)]
    pub(crate) fn check_owned_child_pid(&self, pid: i32) -> AsyncHostResult<()> {
        Self::check_owned_child_pid_in(self.process_policy_state.as_deref(), pid)
    }

    pub(crate) fn with_owned_child_pid<T>(
        &self,
        pid: i32,
        f: impl FnOnce() -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let Some(state) = self.process_policy_state.as_deref() else {
            return f();
        };
        let state = state.inner.lock().unwrap();
        if !state.owned_child_pids.contains(&pid) {
            return Err(AsyncHostError::PermissionDenied);
        }
        f()
    }

    #[cfg(unix)]
    pub(crate) fn finish_owned_child<T>(
        &self,
        pid: i32,
        handle: Option<HostHandle>,
        f: impl FnOnce() -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let Some(state) = self.process_policy_state.as_deref() else {
            return f();
        };
        let mut state = state.inner.lock().unwrap();
        if !state.owned_child_pids.contains(&pid)
            || handle.is_some_and(|handle| state.process_handle_pids.get(&handle) != Some(&pid))
        {
            return Err(AsyncHostError::PermissionDenied);
        }
        let result = f()?;
        state.owned_child_pids.remove(&pid);
        Ok(result)
    }

    #[cfg(windows)]
    pub(crate) fn finish_process_handle<T>(
        &self,
        pid: i32,
        handle: HostHandle,
        f: impl FnOnce() -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let Some(state) = self.process_policy_state.as_deref() else {
            return f();
        };
        let mut state = state.inner.lock().unwrap();
        if state.process_handle_pids.get(&handle) != Some(&pid) {
            return Err(AsyncHostError::PermissionDenied);
        }
        let result = f()?;
        state.owned_child_pids.remove(&pid);
        Ok(result)
    }

    pub(crate) fn process_handle_pid(&self, handle: HostHandle) -> AsyncHostResult<Option<i32>> {
        let Some(state) = self.process_policy_state.as_deref() else {
            return Ok(None);
        };
        if handle == INVALID_HOST_HANDLE || handle == self.invalid_fd() {
            return Ok(None);
        }
        state
            .inner
            .lock()
            .unwrap()
            .process_handle_pids
            .get(&handle)
            .copied()
            .map(Some)
            .ok_or(AsyncHostError::PermissionDenied)
    }

    #[cfg(test)]
    pub(crate) fn check_process_handle_pid(
        &self,
        handle: HostHandle,
        pid: i32,
    ) -> AsyncHostResult<()> {
        let Some(state) = self.process_policy_state.as_deref() else {
            return Ok(());
        };
        if state.inner.lock().unwrap().process_handle_pids.get(&handle) == Some(&pid) {
            Ok(())
        } else {
            Err(AsyncHostError::PermissionDenied)
        }
    }

    fn track_process_handle(&self, handle: HostHandle, pid: i32) {
        if let Some(state) = self.process_policy_state.as_deref() {
            state
                .inner
                .lock()
                .unwrap()
                .process_handle_pids
                .insert(handle, pid);
        }
    }

    fn untrack_process_handle(&self, handle: HostHandle) {
        if let Some(state) = self.process_policy_state.as_deref() {
            let mut state = state.inner.lock().unwrap();
            if let Some(pid) = state.process_handle_pids.remove(&handle)
                && !state
                    .process_handle_pids
                    .values()
                    .any(|tracked_pid| *tracked_pid == pid)
            {
                state.owned_child_pids.remove(&pid);
            }
        }
    }

    pub(crate) fn with_raw_resource_class<T>(
        &self,
        handle: HostHandle,
        class: ResourceClass,
        f: impl FnOnce(RawSocket) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        debug_assert!(class.is_socket());
        let file = self.resource_of_class(handle, class)?;
        #[cfg(unix)]
        let socket = file.as_fd()?.as_raw_fd();
        #[cfg(windows)]
        let socket = file.as_socket()?.as_raw_socket();
        f(socket)
    }

    pub(crate) fn with_raw_socket<T>(
        &self,
        handle: HostHandle,
        f: impl FnOnce(RawSocket) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let file = self.socket_resource(handle)?;
        #[cfg(unix)]
        let socket = file.as_fd()?.as_raw_fd();
        #[cfg(windows)]
        let socket = file.as_socket()?.as_raw_socket();
        f(socket)
    }

    #[cfg(any(windows, target_os = "linux"))]
    pub(crate) fn insert_host_process_handle(&self, raw_fd: RawFd, pid: i32) -> HostHandle {
        let handle = self
            .handles
            .borrow_mut()
            .insert_resource(Resource::new(raw_fd));
        self.track_process_handle(handle, pid);
        handle
    }

    pub(crate) fn resource(&self, handle: HostHandle) -> AsyncHostResult<ResourceRef> {
        self.handles.borrow().resource(handle)
    }

    pub(crate) fn resource_of_class(
        &self,
        handle: HostHandle,
        class: ResourceClass,
    ) -> AsyncHostResult<ResourceRef> {
        self.handles.borrow().resource_of_class(handle, class)
    }

    pub(crate) fn socket_resource(&self, handle: HostHandle) -> AsyncHostResult<ResourceRef> {
        self.handles.borrow().socket(handle)
    }

    pub(crate) fn pipe(
        &self,
        read_end_is_async: bool,
        write_end_is_async: bool,
    ) -> AsyncHostResult<[HostHandle; 2]> {
        let mut handles = self.handles.borrow_mut();
        crate::async_sys::internal::fd_util::stub::pipe_resources(
            &mut *handles,
            read_end_is_async,
            write_end_is_async,
        )
    }

    pub(crate) fn kind_of_fd(&self, handle: HostHandle) -> AsyncHostResult<i32> {
        let file = self.resource(handle)?;
        #[cfg(unix)]
        {
            crate::async_sys::internal::fd_util::stub::kind_of_file(file.as_file()?)
        }
        #[cfg(windows)]
        {
            if file.resource_class().is_socket() {
                crate::async_sys::internal::fd_util::stub::kind_of_socket(file.as_socket()?)
            } else {
                crate::async_sys::internal::fd_util::stub::kind_of_file(file.as_file()?)
            }
        }
    }

    pub(crate) fn set_cloexec(&self, handle: HostHandle) -> AsyncHostResult<()> {
        let file = self.resource(handle)?;
        #[cfg(unix)]
        {
            crate::async_sys::internal::fd_util::stub::set_cloexec(file.as_file()?.as_raw_fd())
        }
        #[cfg(windows)]
        {
            let _ = file;
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
        let file = self.resource(handle)?;
        crate::async_sys::internal::event_loop::io::read(file.as_file()?.as_raw_fd(), dst)
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
        let file = self.resource(handle)?;
        crate::async_sys::internal::event_loop::io::write(file.as_file()?.as_raw_fd(), src)
            .and_then(|ret| i32::try_from(ret).map_err(|_| AsyncHostError::Fault))
    }

    #[cfg(windows)]
    fn read_guest_slice(
        memory: &mut (impl GuestMemory + ?Sized),
        ptr: i32,
        offset: i32,
        len: i32,
    ) -> AsyncHostResult<Vec<u8>> {
        let start = ptr.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        Ok(memory.read_exact(start, len)?.to_vec())
    }

    #[cfg(windows)]
    pub(crate) fn make_file_read_io_result(&self, len: i32, position: i64) -> AsyncHostResult<u64> {
        self.insert_io_result(HostIoResult::for_file_read(len, position)?)
    }

    #[cfg(windows)]
    pub(crate) fn make_file_write_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        src: i32,
        offset: i32,
        len: i32,
        position: i64,
    ) -> AsyncHostResult<u64> {
        let buffer = Self::read_guest_slice(memory, src, offset, len)?;
        self.insert_io_result(HostIoResult::for_file_write(buffer, position))
    }

    #[cfg(windows)]
    pub(crate) fn make_socket_read_io_result(&self, len: i32, flags: i32) -> AsyncHostResult<u64> {
        self.insert_io_result(HostIoResult::for_socket_read(len, flags)?)
    }

    #[cfg(windows)]
    pub(crate) fn make_socket_write_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        src: i32,
        offset: i32,
        len: i32,
        flags: i32,
    ) -> AsyncHostResult<u64> {
        let buffer = Self::read_guest_slice(memory, src, offset, len)?;
        self.insert_io_result(HostIoResult::for_socket_write(buffer, flags))
    }

    #[cfg(windows)]
    pub(crate) fn make_socket_with_addr_read_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        len: i32,
        flags: i32,
        addr: i32,
        addr_len: i32,
    ) -> AsyncHostResult<u64> {
        let addr_buffer = memory.read_exact(addr, addr_len)?.to_vec();
        self.insert_io_result(HostIoResult::for_socket_with_addr_read(
            len,
            flags,
            addr_buffer,
        )?)
    }

    #[cfg(windows)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn make_socket_with_addr_write_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        src: i32,
        offset: i32,
        len: i32,
        flags: i32,
        addr: i32,
        addr_len: i32,
    ) -> AsyncHostResult<u64> {
        let buffer = Self::read_guest_slice(memory, src, offset, len)?;
        let addr_buffer = memory.read_exact(addr, addr_len)?.to_vec();
        self.insert_io_result(HostIoResult::for_socket_with_addr_write(
            buffer,
            flags,
            addr_buffer,
        )?)
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
        let key = self.handles.borrow_mut().insert(HandleKind::IoResult);
        let handle = handle_from_key(key);
        let overlapped = {
            let mut io_results = self.io_results.lock().unwrap();
            io_results.io_results.insert(key, Box::new(result));
            io_results
                .io_results
                .get_mut(key)
                .ok_or(AsyncHostError::Badf)?
                .overlapped_addr()
        };
        self.io_results
            .lock()
            .unwrap()
            .io_results_by_overlapped
            .insert(overlapped, handle);
        Ok(handle)
    }

    #[cfg(windows)]
    pub(crate) fn free_io_result(&self, handle: u64) -> AsyncHostResult<()> {
        let mut handles = self.handles.borrow_mut();
        let key = handles.io_result(handle)?;
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
        handles.remove_io_result(handle)?;
        let overlapped = result.overlapped_addr();
        io_results.io_results_by_overlapped.remove(&overlapped);
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn io_result_get_event(&self, handle: u64) -> AsyncHostResult<i32> {
        let key = self.handles.borrow().io_result(handle)?;
        let io_results = self.io_results.lock().unwrap();
        let result = io_results.io_results.get(key).ok_or(AsyncHostError::Badf)?;
        Ok(result.event)
    }

    #[cfg(windows)]
    pub(crate) fn cancel_io_result(
        &self,
        result_handle: u64,
        fd_handle: HostHandle,
    ) -> AsyncHostResult<i32> {
        let file = self.resource(fd_handle)?;
        let result_key = self.handles.borrow().io_result(result_handle)?;
        let mut io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        result.validate_pending_resource(&file)?;
        result.cancel_pending()
    }

    #[cfg(windows)]
    pub(crate) fn io_result_get_status(
        &self,
        result_handle: u64,
        fd_handle: HostHandle,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::System::IO::GetOverlappedResult;

        let file = self.resource(fd_handle)?;
        let raw_handle = raw_overlapped_handle(&file)?;
        let result_key = self.handles.borrow().io_result(result_handle)?;
        let mut io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        result.validate_pending_resource(&file)?;
        let mut bytes_transferred = 0;
        if unsafe {
            GetOverlappedResult(
                raw_handle,
                result.overlapped_ptr(),
                &mut bytes_transferred,
                0,
            )
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
        i32::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)
    }

    #[cfg(windows)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn io_result_copy_read(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        result_handle: u64,
        dst: i32,
        offset: i32,
        len: i32,
    ) -> AsyncHostResult<()> {
        let key = self.handles.borrow().io_result(result_handle)?;
        let io_results = self.io_results.lock().unwrap();
        let result = io_results.io_results.get(key).ok_or(AsyncHostError::Badf)?;
        result.copy_read_result(memory, dst, offset, len)
    }

    #[cfg(windows)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn io_result_copy_read_with_addr(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        result_handle: u64,
        dst: i32,
        offset: i32,
        len: i32,
        addr: i32,
        addr_len: i32,
    ) -> AsyncHostResult<()> {
        let key = self.handles.borrow().io_result(result_handle)?;
        let io_results = self.io_results.lock().unwrap();
        let result = io_results.io_results.get(key).ok_or(AsyncHostError::Badf)?;
        result.copy_read_result_with_addr(memory, dst, offset, len, addr, addr_len)
    }

    #[cfg(windows)]
    pub(crate) fn read_io_result(
        &self,
        fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::{ERROR_HANDLE_EOF, ERROR_IO_PENDING};
        use windows_sys::Win32::Networking::WinSock as ws;
        use windows_sys::Win32::Storage::FileSystem::ReadFile;

        // The accepted resource class depends on the IO result kind, so resolve
        // both while their tables are borrowed.
        let handles = self.handles.borrow();
        let result_key = handles.io_result(result_handle)?;
        let mut io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.is_pending() || result.event != IO_RESULT_READ_EVENT {
            return Err(AsyncHostError::Inval);
        }
        let file = result.kind.resource(&handles, fd_handle)?;
        drop(handles);
        let mut bytes_transferred = 0;
        let success = match result.kind {
            HostIoKind::File => {
                let len = u32::try_from(result.buffer.len()).map_err(|_| AsyncHostError::Fault)?;
                unsafe {
                    ReadFile(
                        file.as_file()?.as_raw_handle(),
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
                            file.as_socket()?.as_raw_socket() as usize,
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
                            file.as_socket()?.as_raw_socket() as usize,
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
            return i32::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault);
        }
        let errno = match result.kind {
            HostIoKind::Socket | HostIoKind::SocketWithAddr => last_wsa_errno(),
            _ => last_errno(),
        };
        if errno == ERROR_HANDLE_EOF as i32 {
            Ok(0)
        } else if errno == ERROR_IO_PENDING as i32 {
            result.mark_pending(file)?;
            Err(AsyncHostError::Native(errno))
        } else {
            Err(AsyncHostError::Native(errno))
        }
    }

    #[cfg(windows)]
    pub(crate) fn write_io_result(
        &self,
        fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::ERROR_IO_PENDING;
        use windows_sys::Win32::Networking::WinSock as ws;
        use windows_sys::Win32::Storage::FileSystem::WriteFile;

        // The accepted resource class depends on the IO result kind, so resolve
        // both while their tables are borrowed.
        let handles = self.handles.borrow();
        let result_key = handles.io_result(result_handle)?;
        let mut io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.is_pending() || result.event != IO_RESULT_WRITE_EVENT {
            return Err(AsyncHostError::Inval);
        }
        let file = result.kind.resource(&handles, fd_handle)?;
        drop(handles);
        if result.kind == HostIoKind::SocketWithAddr {
            self.policy.connect_socket(&result.addr_buffer)?;
        }
        let mut bytes_transferred = 0;
        let success = match result.kind {
            HostIoKind::File => {
                let len = u32::try_from(result.buffer.len()).map_err(|_| AsyncHostError::Fault)?;
                unsafe {
                    WriteFile(
                        file.as_file()?.as_raw_handle(),
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
                            file.as_socket()?.as_raw_socket() as usize,
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
                unsafe {
                    i32::from(
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
                result.mark_pending(file)?;
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

        let (file, raw_socket, result_key) = {
            let handles = self.handles.borrow();
            let file = handles.resource_of_class(fd_handle, ResourceClass::TcpSocket)?;
            let raw_socket = file.as_socket()?.as_raw_socket();
            let result_key = handles.io_result(result_handle)?;
            (file, raw_socket, result_key)
        };
        let mut io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.kind != HostIoKind::Connect || result.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        self.policy.connect_socket(&result.addr_buffer)?;

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

    #[cfg(windows)]
    pub(crate) fn setup_connected_socket(&self, fd_handle: HostHandle) -> AsyncHostResult<()> {
        use windows_sys::Win32::Networking::WinSock as ws;

        let file = self.resource_of_class(fd_handle, ResourceClass::TcpSocket)?;
        let yes: u32 = 1;
        if unsafe {
            ws::setsockopt(
                file.as_socket()?.as_raw_socket() as usize,
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

        let (server_file, conn_file, server_socket, conn_socket, result_key) = {
            let handles = self.handles.borrow();
            let server_file =
                handles.resource_of_class(server_fd_handle, ResourceClass::TcpSocket)?;
            let conn_file = handles.resource_of_class(conn_fd_handle, ResourceClass::TcpSocket)?;
            let server_socket = server_file.as_socket()?.as_raw_socket();
            let conn_socket = conn_file.as_socket()?.as_raw_socket();
            let result_key = handles.io_result(result_handle)?;
            (
                server_file,
                conn_file,
                server_socket,
                conn_socket,
                result_key,
            )
        };
        let mut io_results = self.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.kind != HostIoKind::Accept || result.is_pending() {
            return Err(AsyncHostError::Inval);
        }

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

    #[cfg(windows)]
    pub(crate) fn get_accept_peer_addr(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        result_handle: u64,
        dst: i32,
        dst_len: i32,
    ) -> AsyncHostResult<()> {
        let key = self.handles.borrow().io_result(result_handle)?;
        let io_results = self.io_results.lock().unwrap();
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
        memory.write_with_capacity(dst, dst_len, addr)
    }

    #[cfg(windows)]
    pub(crate) fn setup_accepted_socket(
        &self,
        listen_fd_handle: HostHandle,
        accept_fd_handle: HostHandle,
    ) -> AsyncHostResult<()> {
        use windows_sys::Win32::Networking::WinSock as ws;

        let listen_file = self.resource_of_class(listen_fd_handle, ResourceClass::TcpSocket)?;
        let accept_file = self.resource_of_class(accept_fd_handle, ResourceClass::TcpSocket)?;
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

    pub(crate) fn try_lock_file(&self, handle: HostHandle, exclusive: bool) -> AsyncHostResult<()> {
        let file = self
            .handles
            .borrow()
            .resource_of_class(handle, ResourceClass::File)?;
        Self::check_file_lock_policy(&self.policy, Some(&file), exclusive)?;
        crate::async_sys::fs::stub::try_lock_acquired_file(&file, exclusive)
    }

    pub(crate) fn unlock_file(&self, handle: HostHandle) -> AsyncHostResult<()> {
        let file = self
            .handles
            .borrow()
            .resource_of_class(handle, ResourceClass::File)?;
        crate::async_sys::fs::stub::unlock_acquired_file(&file)
    }

    pub(crate) fn run_job(&self, handle: u64) -> AsyncHostResult<()> {
        let key = self.handles.borrow().job(handle)?;
        let mut job = self.jobs.lock().unwrap().take_ready_job(key)?;
        Self::run_policy_checked_job(&self.policy, self.process_policy_state.as_deref(), &mut job);
        self.restore_job(key, job)
    }

    fn run_policy_checked_job(
        policy: &AsyncPolicy,
        process_policy_state: Option<&ProcessPolicyState>,
        job: &mut Job,
    ) {
        if let Err(error) = Self::check_job_policy(policy, process_policy_state, job) {
            job.set_err(error.errno());
            return;
        }
        thread_pool::run_host_job(job);
        if let Err(error) = Self::update_owned_child_pids(process_policy_state, job) {
            job.set_err(error.errno());
        }
    }

    fn check_job_policy(
        policy: &AsyncPolicy,
        process_policy_state: Option<&ProcessPolicyState>,
        job: &Job,
    ) -> AsyncHostResult<()> {
        match job.payload() {
            JobPayload::Open {
                filename,
                access,
                create_mode,
                append,
                ..
            } => policy.open_path(
                RuntimePathBase::CurrentDirectory,
                filename,
                *access,
                *create_mode,
                *append,
            ),
            JobPayload::FileKindByPath {
                parent,
                path,
                follow_symlink,
            } => Self::check_path_metadata_policy(
                policy,
                Self::resource_path_base(parent.as_ref()),
                path,
                *follow_symlink,
            ),
            JobPayload::FileSize { file, .. } | JobPayload::FileTime { file, .. } => {
                Self::check_file_metadata_policy(policy, file.as_ref())
            }
            JobPayload::FileTimeByPath {
                path,
                follow_symlink,
                ..
            } => Self::check_path_metadata_policy(
                policy,
                RuntimePathBase::CurrentDirectory,
                path,
                *follow_symlink,
            ),
            JobPayload::Realpath { path, .. } => {
                policy.stat_path(RuntimePathBase::CurrentDirectory, path)
            }
            JobPayload::Access { path, access } => policy.access_path(path, *access),
            JobPayload::Chmod { path, .. } => policy.chmod_path(path),
            JobPayload::Flock { file, exclusive } => {
                Self::check_file_lock_policy(policy, file.as_ref(), *exclusive)
            }
            JobPayload::Remove { path } => policy.remove_path(path),
            JobPayload::Rename {
                old_path, new_path, ..
            } => policy.rename_path(old_path, new_path),
            JobPayload::Symlink { path, .. } => policy.symlink_path(path),
            JobPayload::Mkdir { path, .. } => policy.mkdir_path(path),
            JobPayload::Rmdir { path } => policy.rmdir_path(path),
            #[cfg(unix)]
            JobPayload::SpawnUnix { .. } => policy.spawn_process(),
            #[cfg(windows)]
            JobPayload::SpawnWindows { .. } => policy.spawn_process(),
            JobPayload::WaitForProcess {
                handle,
                tracked_pid,
                pid,
                ..
            } => {
                let Some(state) = process_policy_state else {
                    return Ok(());
                };
                Self::check_owned_child_pid_in(Some(state), *pid)?;
                match (handle.is_some(), *tracked_pid) {
                    (false, None) => Ok(()),
                    (true, Some(tracked_pid)) if tracked_pid == *pid => Ok(()),
                    _ => Err(AsyncHostError::PermissionDenied),
                }
            }
            _ => Ok(()),
        }
    }

    fn check_owned_child_pid_in(
        process_policy_state: Option<&ProcessPolicyState>,
        pid: i32,
    ) -> AsyncHostResult<()> {
        let Some(state) = process_policy_state else {
            return Ok(());
        };
        if state.inner.lock().unwrap().owned_child_pids.contains(&pid) {
            Ok(())
        } else {
            Err(AsyncHostError::PermissionDenied)
        }
    }

    fn update_owned_child_pids(
        process_policy_state: Option<&ProcessPolicyState>,
        job: &Job,
    ) -> AsyncHostResult<()> {
        if job.err() != 0 {
            return Ok(());
        }
        match job.payload() {
            #[cfg(unix)]
            JobPayload::SpawnUnix { .. } => {
                if job.ret() >= 0
                    && let Some(state) = process_policy_state
                {
                    state
                        .inner
                        .lock()
                        .unwrap()
                        .owned_child_pids
                        .insert(job.ret() as i32);
                }
            }
            #[cfg(windows)]
            JobPayload::SpawnWindows { .. } => {
                if job.ret() >= 0
                    && let Some(state) = process_policy_state
                {
                    state
                        .inner
                        .lock()
                        .unwrap()
                        .owned_child_pids
                        .insert(job.ret() as i32);
                }
            }
            JobPayload::WaitForProcess {
                pid,
                #[cfg(unix)]
                defer_reap,
                ..
            } => {
                if let Some(state) = process_policy_state {
                    let mut state = state.inner.lock().unwrap();
                    #[cfg(unix)]
                    if *defer_reap {
                        crate::async_sys::process::reap_process(*pid)?;
                    }
                    state.owned_child_pids.remove(pid);
                } else {
                    #[cfg(unix)]
                    if *defer_reap {
                        crate::async_sys::process::reap_process(*pid)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn check_file_metadata_policy(
        policy: &AsyncPolicy,
        file: Option<&ResourceRef>,
    ) -> AsyncHostResult<()> {
        let file = file.ok_or(AsyncHostError::Badf)?;
        match file.policy_path() {
            Some(path) => policy.stat_path(RuntimePathBase::CurrentDirectory, path.as_os_str()),
            None => policy.stat_path(RuntimePathBase::Untracked, std::ffi::OsStr::new("")),
        }
    }

    fn check_path_metadata_policy(
        policy: &AsyncPolicy,
        base: RuntimePathBase<'_>,
        path: &std::ffi::OsStr,
        follow_symlink: bool,
    ) -> AsyncHostResult<()> {
        if follow_symlink {
            policy.stat_path(base, path)
        } else {
            policy.stat_entry_path(base, path)
        }
    }

    fn check_file_lock_policy(
        policy: &AsyncPolicy,
        file: Option<&ResourceRef>,
        exclusive: bool,
    ) -> AsyncHostResult<()> {
        let file = file.ok_or(AsyncHostError::Badf)?;
        match file.policy_path() {
            Some(path) => policy.lock_path(
                RuntimePathBase::CurrentDirectory,
                path.as_os_str(),
                exclusive,
            ),
            None => policy.lock_path(
                RuntimePathBase::Untracked,
                std::ffi::OsStr::new(""),
                exclusive,
            ),
        }
    }

    fn resource_path_base(parent: Option<&ResourceRef>) -> RuntimePathBase<'_> {
        match parent {
            None => RuntimePathBase::CurrentDirectory,
            Some(parent) => parent
                .policy_path()
                .map(RuntimePathBase::PolicyPath)
                .unwrap_or(RuntimePathBase::Untracked),
        }
    }

    pub(crate) fn spawn_worker(&self, completion_id: i32, job_handle: u64) -> AsyncHostResult<u64> {
        let completion_id = WorkerCompletionId::from_abi(completion_id);
        let job_key = self.handles.borrow().job(job_handle)?;
        #[cfg(unix)]
        let worker = {
            let completion_notifier = self
                .thread_pool_completions
                .lock()
                .unwrap()
                .notifier
                .clone()
                .ok_or(AsyncHostError::Badf)?;
            self.queue_worker_job(job_key)?;
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
                .clone()
                .ok_or(AsyncHostError::Badf)?;
            self.queue_worker_job(job_key)?;
            self.spawn_worker_thread(
                HostWorkerJob {
                    completion_id,
                    job_key,
                },
                move |completion_id| {
                    let _ = poll::post_thread_pool_completion(
                        &completion_target.port,
                        completion_id.as_i32(),
                        completion_target.generation,
                    );
                },
            )
        };
        let key = self.handles.borrow_mut().insert(HandleKind::Worker);
        self.workers.lock().unwrap().insert(key, worker);
        Ok(handle_from_key(key))
    }

    pub(crate) fn wake_worker(
        &self,
        worker_handle: u64,
        completion_id: i32,
        job_handle: u64,
    ) -> AsyncHostResult<()> {
        let completion_id = WorkerCompletionId::from_abi(completion_id);
        let (worker_key, job_key) = {
            let handles = self.handles.borrow();
            (handles.worker(worker_handle)?, handles.job(job_handle)?)
        };
        let replaced_job = {
            let workers = self.workers.lock().unwrap();
            let Some(worker) = workers.get(worker_key) else {
                return Err(AsyncHostError::Badf);
            };
            self.queue_worker_job(job_key)?;
            thread_pool::wake_worker(
                worker,
                HostWorkerJob {
                    completion_id,
                    job_key,
                },
            )
        };
        if let Some(replaced_job) = replaced_job {
            #[cfg(windows)]
            self.unregister_worker_job_cancel(replaced_job.job_key);
            let _ = self.jobs.lock().unwrap().unqueue_job(replaced_job.job_key);
        }
        Ok(())
    }

    pub(crate) fn worker_enter_idle(&self, worker_handle: u64) -> AsyncHostResult<()> {
        let worker_key = self.handles.borrow().worker(worker_handle)?;
        let replaced_job = {
            let workers = self.workers.lock().unwrap();
            let worker = workers.get(worker_key).ok_or(AsyncHostError::Badf)?;
            thread_pool::worker_enter_idle(worker)
        };
        if let Some(replaced_job) = replaced_job {
            #[cfg(windows)]
            self.unregister_worker_job_cancel(replaced_job.job_key);
            let _ = self.jobs.lock().unwrap().unqueue_job(replaced_job.job_key);
        }
        Ok(())
    }

    pub(crate) fn free_worker(&self, worker_handle: u64) -> AsyncHostResult<()> {
        let worker_key = self.handles.borrow().worker(worker_handle)?;
        let worker = self
            .workers
            .lock()
            .unwrap()
            .remove(worker_key)
            .ok_or(AsyncHostError::Badf)?;
        self.handles.borrow_mut().remove_worker(worker_handle)?;
        let _ = self.cancel_host_worker(&worker);
        if let Some(replaced_job) = thread_pool::free_worker(worker) {
            #[cfg(windows)]
            self.unregister_worker_job_cancel(replaced_job.job_key);
            let _ = self.jobs.lock().unwrap().unqueue_job(replaced_job.job_key);
        }
        Ok(())
    }

    pub(crate) fn cancel_worker(&self, worker_handle: u64) -> AsyncHostResult<i32> {
        let worker_key = self.handles.borrow().worker(worker_handle)?;
        let workers = self.workers.lock().unwrap();
        let worker = workers.get(worker_key).ok_or(AsyncHostError::Badf)?;
        self.cancel_host_worker(worker)
    }

    fn cancel_host_worker(&self, worker: &HostWorkerHandle) -> AsyncHostResult<i32> {
        #[cfg(windows)]
        {
            if let Some(running_job) = thread_pool::worker_cancellable_job(worker) {
                let cancel = self
                    .running_job_cancellations
                    .lock()
                    .unwrap()
                    .get(&running_job.job_key)
                    .cloned();
                if let Some(cancel) = cancel {
                    thread_pool::cancel_job_resource(&cancel)?;
                    return Ok(1);
                }
            }
        }
        thread_pool::cancel_worker(worker)
    }

    #[cfg(unix)]
    pub(crate) fn thread_pool_child_signal_mask(&self) -> AsyncHostResult<libc::sigset_t> {
        self.thread_pool_completions
            .lock()
            .unwrap()
            .old_signal_mask
            .ok_or(AsyncHostError::Badf)
    }

    #[cfg(windows)]
    pub(crate) fn thread_pool_completion_target(
        &self,
    ) -> AsyncHostResult<(poll::CompletionPort, usize)> {
        self.thread_pool_completions
            .lock()
            .unwrap()
            .target
            .as_ref()
            .map(|target| (target.port.clone(), target.generation))
            .ok_or(AsyncHostError::Badf)
    }

    pub(crate) fn get_read_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: u64,
        dst: i32,
        offset: i32,
        len: i32,
    ) -> AsyncHostResult<()> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.lock().unwrap();
        let job = jobs.visible_job(key)?;
        thread_pool::get_read_result(job, memory, dst, offset, len)
    }

    pub(crate) fn get_file_time_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: u64,
        dst: i32,
    ) -> AsyncHostResult<()> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.lock().unwrap();
        let job = jobs.visible_job(key)?;
        thread_pool::get_file_time_result(job, memory, dst)
    }

    pub(crate) fn get_realpath_result(&self, handle: u64) -> AsyncHostResult<u64> {
        let key = self.handles.borrow().job(handle)?;
        let mut jobs = self.jobs.lock().unwrap();
        let job = jobs.visible_job_mut(key)?;
        // Keep the job locked through publication so free_job either observes
        // and cleans up the c_buffer or removes the job before publication.
        thread_pool::publish_realpath_result(job, |buffer| {
            let buffer_key = self.handles.borrow_mut().insert(HandleKind::CBuffer);
            self.c_buffers
                .lock()
                .unwrap()
                .insert(buffer_key, Arc::new(Mutex::new(buffer)));
            handle_from_key(buffer_key)
        })
    }

    #[cfg(unix)]
    pub(crate) fn thread_pool_notifier(
        &self,
    ) -> AsyncHostResult<Arc<ThreadPoolCompletionNotifier>> {
        self.thread_pool_completions
            .lock()
            .unwrap()
            .notifier
            .clone()
            .ok_or(AsyncHostError::Badf)
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

    pub(crate) fn tls_take_error(&self, handle: HostHandle) -> AsyncHostResult<HostHandle> {
        let tls = self.tls_connection(handle)?;
        let message = match &mut *tls.lock().unwrap() {
            tls::TlsHandle::Connection(tls) => tls
                .take_error()
                .unwrap_or_else(|| "unknown TLS error".to_string()),
            tls::TlsHandle::Empty(pending) => pending
                .take_error()
                .unwrap_or_else(|| "unknown TLS error".to_string()),
        };
        Ok(self.insert_c_buffer(error_message_buffer(message)))
    }

    pub(crate) fn tls_take_global_error(&self) -> HostHandle {
        let message = self
            .tls_error
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| "unknown TLS error".to_string());
        self.insert_c_buffer(error_message_buffer(message))
    }

    pub(crate) fn tls_new(&self) -> HostHandle {
        self.insert_tls_handle(tls::TlsHandle::Empty(tls::TlsPending::new()))
    }

    pub(crate) fn tls_set_client(
        &self,
        handle: HostHandle,
        host: String,
        sni: bool,
        trust: tls::TlsTrust,
    ) -> AsyncHostResult<i32> {
        let tls = self.tls_connection(handle)?;
        let mut handle = tls.lock().unwrap();
        match &mut *handle {
            tls::TlsHandle::Empty(pending) => {
                match tls::TlsConnection::client(&host, sni, pending.client_config(trust)) {
                    Ok(connection) => {
                        *handle = tls::TlsHandle::Connection(Box::new(connection));
                        Ok(0)
                    }
                    Err(message) => Ok(pending.set_error(message)),
                }
            }
            tls::TlsHandle::Connection(_) => Err(AsyncHostError::Inval),
        }
    }

    pub(crate) fn tls_add_root_certificate(
        &self,
        handle: HostHandle,
        root: &[u8],
    ) -> AsyncHostResult<i32> {
        self.with_tls_pending_mut(handle, |pending| pending.add_root_certificate(root))
    }

    pub(crate) fn tls_set_server_files(
        &self,
        handle: HostHandle,
        private_key_file: std::path::PathBuf,
        private_key_type: tls::TlsFileType,
        certificate_file: std::path::PathBuf,
        certificate_type: tls::TlsFileType,
    ) -> AsyncHostResult<i32> {
        for (label, path) in [
            ("TLS private key", private_key_file.as_path()),
            ("TLS certificate", certificate_file.as_path()),
        ] {
            if let Err(error) = self.policy.open_path(
                RuntimePathBase::CurrentDirectory,
                path.as_os_str(),
                0,
                0,
                false,
            ) {
                return self.with_tls_pending_mut(handle, |pending| {
                    pending.set_error(format!("failed to access {label} file: {error:?}"))
                });
            }
        }
        let tls = self.tls_connection(handle)?;
        let mut handle = tls.lock().unwrap();
        match &mut *handle {
            tls::TlsHandle::Empty(pending) => {
                if pending.has_root_certificates() {
                    return Ok(pending.set_error(
                        "TLS root certificates require client custom root trust".to_string(),
                    ));
                }
                match tls::TlsConnection::server(tls::TlsConfig::ServerFiles {
                    private_key_file,
                    private_key_type,
                    certificate_file,
                    certificate_type,
                }) {
                    Ok(connection) => {
                        *handle = tls::TlsHandle::Connection(Box::new(connection));
                        Ok(0)
                    }
                    Err(message) => Ok(pending.set_error(message)),
                }
            }
            tls::TlsHandle::Connection(_) => Err(AsyncHostError::Inval),
        }
    }

    pub(crate) fn tls_set_server_pfx(
        &self,
        handle: HostHandle,
        pfx_content: Vec<u8>,
    ) -> AsyncHostResult<i32> {
        let tls = self.tls_connection(handle)?;
        let mut handle = tls.lock().unwrap();
        match &mut *handle {
            tls::TlsHandle::Empty(pending) => {
                if pending.has_root_certificates() {
                    return Ok(pending.set_error(
                        "TLS root certificates require client custom root trust".to_string(),
                    ));
                }
                match tls::TlsConnection::server(tls::TlsConfig::ServerPfx { pfx_content }) {
                    Ok(connection) => {
                        *handle = tls::TlsHandle::Connection(Box::new(connection));
                        Ok(0)
                    }
                    Err(message) => Ok(pending.set_error(message)),
                }
            }
            tls::TlsHandle::Connection(_) => Err(AsyncHostError::Inval),
        }
    }

    fn insert_tls_handle(&self, handle: tls::TlsHandle) -> HostHandle {
        let handle = Arc::new(Mutex::new(handle));
        let key = self.handles.borrow_mut().insert(HandleKind::TlsConnection);
        self.tls_connections.lock().unwrap().insert(key, handle);
        handle_from_key(key)
    }

    pub(crate) fn tls_free(&self, handle: HostHandle) -> AsyncHostResult<()> {
        if handle == INVALID_HOST_HANDLE {
            return Ok(());
        }
        let key = self.handles.borrow_mut().remove_tls_connection(handle)?;
        self.tls_connections
            .lock()
            .unwrap()
            .remove(key)
            .map(|_| ())
            .ok_or(AsyncHostError::Badf)
    }

    pub(crate) fn tls_read_plain(
        &self,
        handle: HostHandle,
        input: &mut [u8],
        plain: &mut [u8],
        output: &mut [u8],
    ) -> AsyncHostResult<i32> {
        self.with_tls_connection_mut(handle, tls::TLS_ERROR_STATUS, |tls| {
            tls.read_plain(input, plain, output)
        })
    }

    pub(crate) fn tls_write_plain(
        &self,
        handle: HostHandle,
        input: &mut [u8],
        plain: &[u8],
        output: &mut [u8],
    ) -> AsyncHostResult<i32> {
        self.with_tls_connection_mut(handle, tls::TLS_ERROR_STATUS, |tls| {
            tls.write_plain(input, plain, output)
        })
    }

    pub(crate) fn tls_connect(
        &self,
        handle: HostHandle,
        input: &mut [u8],
        output: &mut [u8],
    ) -> AsyncHostResult<i32> {
        self.with_tls_connection_mut(handle, tls::TlsState::Error.code(), |tls| {
            let status = tls.connect(input, output);
            tls::TlsState::from_status(status, tls.wants_read(), tls.wants_write()).code()
        })
    }

    pub(crate) fn tls_accept(
        &self,
        handle: HostHandle,
        input: &mut [u8],
        output: &mut [u8],
    ) -> AsyncHostResult<i32> {
        self.with_tls_connection_mut(handle, tls::TlsState::Error.code(), |tls| {
            let status = tls.accept(input, output);
            tls::TlsState::from_status(status, tls.wants_read(), tls.wants_write()).code()
        })
    }

    pub(crate) fn tls_bytes_read(&self, handle: HostHandle) -> AsyncHostResult<i32> {
        self.with_tls_connection_mut(handle, 0, |tls| tls.bytes_read())
    }

    pub(crate) fn tls_bytes_to_write(&self, handle: HostHandle) -> AsyncHostResult<i32> {
        self.with_tls_connection_mut(handle, 0, |tls| tls.bytes_to_write())
    }

    pub(crate) fn tls_wants_read(&self, handle: HostHandle) -> AsyncHostResult<i32> {
        self.with_tls_connection_mut(handle, 0, |tls| i32::from(tls.wants_read()))
    }

    pub(crate) fn tls_wants_write(&self, handle: HostHandle) -> AsyncHostResult<i32> {
        self.with_tls_connection_mut(handle, 0, |tls| i32::from(tls.wants_write()))
    }

    pub(crate) fn tls_shutdown(&self, handle: HostHandle) -> AsyncHostResult<i32> {
        self.with_tls_connection_mut(handle, tls::TLS_ERROR_STATUS, |tls| tls.shutdown())
    }

    pub(crate) fn tls_peer_certificate(&self, handle: HostHandle) -> AsyncHostResult<HostHandle> {
        self.tls_c_buffer(handle, |tls| tls.peer_certificate())
    }

    pub(crate) fn tls_unique_channel_binding(
        &self,
        handle: HostHandle,
    ) -> AsyncHostResult<HostHandle> {
        self.tls_c_buffer(handle, |tls| tls.unique_channel_binding())
    }

    pub(crate) fn tls_server_endpoint_channel_binding(
        &self,
        handle: HostHandle,
    ) -> AsyncHostResult<HostHandle> {
        self.tls_c_buffer(handle, |tls| tls.server_endpoint_channel_binding())
    }

    fn tls_c_buffer(
        &self,
        handle: HostHandle,
        f: impl FnOnce(&mut tls::TlsConnection) -> Result<Option<Vec<u8>>, ()>,
    ) -> AsyncHostResult<HostHandle> {
        match self.with_tls_connection_mut(handle, Err(()), f)? {
            Ok(Some(buffer)) => Ok(self.insert_c_buffer(buffer.into_boxed_slice())),
            Ok(None) => Ok(INVALID_HOST_HANDLE),
            Err(()) => Ok(INVALID_HOST_HANDLE),
        }
    }

    fn with_tls_connection_mut<T>(
        &self,
        handle: HostHandle,
        unconfigured_value: T,
        f: impl FnOnce(&mut tls::TlsConnection) -> T,
    ) -> AsyncHostResult<T> {
        let connection = self.tls_connection(handle)?;
        let mut handle = connection.lock().unwrap();
        match &mut *handle {
            tls::TlsHandle::Connection(connection) => Ok(f(connection)),
            tls::TlsHandle::Empty(pending) => {
                pending.set_error("TLS handle is not configured".to_string());
                Ok(unconfigured_value)
            }
        }
    }

    fn with_tls_pending_mut<T>(
        &self,
        handle: HostHandle,
        f: impl FnOnce(&mut tls::TlsPending) -> T,
    ) -> AsyncHostResult<T> {
        let tls = self.tls_connection(handle)?;
        let mut handle = tls.lock().unwrap();
        match &mut *handle {
            tls::TlsHandle::Empty(pending) => Ok(f(pending)),
            tls::TlsHandle::Connection(_) => Err(AsyncHostError::Inval),
        }
    }

    fn tls_connection(&self, handle: HostHandle) -> AsyncHostResult<tls::TlsHandleRef> {
        let key = self.handles.borrow().tls_connection(handle)?;
        self.tls_connections
            .lock()
            .unwrap()
            .get(key)
            .map(Arc::clone)
            .ok_or(AsyncHostError::Badf)
    }

    fn spawn_worker_thread(
        &self,
        init_job: HostWorkerJob,
        mut complete_job: impl FnMut(WorkerCompletionId) + Send + 'static,
    ) -> HostWorkerHandle {
        let jobs_for_runner = Arc::clone(&self.jobs);
        let jobs_for_completion = Arc::clone(&self.jobs);
        #[cfg(windows)]
        let running_job_cancellations = Arc::clone(&self.running_job_cancellations);
        let policy = Arc::clone(&self.policy);
        let process_policy_state_for_runner = self.process_policy_state.clone();
        let process_policy_state_for_completion = self.process_policy_state.clone();
        thread_pool::spawn_worker(
            init_job,
            move |worker_job| {
                let Ok(mut job) = jobs_for_runner
                    .lock()
                    .unwrap()
                    .take_queued_job(worker_job.job_key)
                else {
                    #[cfg(windows)]
                    running_job_cancellations
                        .lock()
                        .unwrap()
                        .remove(&worker_job.job_key);
                    return None;
                };
                Self::run_policy_checked_job(
                    &policy,
                    process_policy_state_for_runner.as_deref(),
                    &mut job,
                );
                #[cfg(windows)]
                {
                    running_job_cancellations
                        .lock()
                        .unwrap()
                        .remove(&worker_job.job_key);
                }
                Some(job)
            },
            move |worker_job| {
                let completion_id = worker_job.completion_id;
                if let Some(job) = worker_job.job {
                    let discarded = jobs_for_completion
                        .lock()
                        .unwrap()
                        .restore_job(worker_job.job_key, job);
                    if let Some(job) = discarded {
                        Self::revoke_unclaimed_spawn(
                            process_policy_state_for_completion.as_deref(),
                            &job,
                        );
                    }
                }
                // Even if cancellation discarded the job handle, the event loop
                // still needs the completion to move the worker out of running.
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
    unsafe { windows_sys::Win32::Foundation::GetLastError() as i32 }
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

#[cfg(windows)]
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

#[cfg(windows)]
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
    use std::ffi::OsString;

    use super::*;

    #[repr(align(2))]
    struct AlignedBytes<const N: usize>([u8; N]);

    fn poll_key(host: &AsyncHost, handle: HostHandle) -> HandleKey {
        host.handles.borrow().poll(handle).unwrap()
    }

    fn job_key(host: &AsyncHost, handle: HostHandle) -> HandleKey {
        host.handles.borrow().job(handle).unwrap()
    }

    fn resource_count(host: &AsyncHost) -> usize {
        host.handles.borrow().resource_count_excluding_reserved()
    }

    fn host_with_policy(path: &std::path::Path) -> AsyncHost {
        AsyncHost::new(Arc::new(AsyncPolicy::from_file(path).unwrap()))
    }

    #[cfg(unix)]
    fn successful_process_job() -> Job {
        let mut child_signal_mask = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::sigemptyset(&mut child_signal_mask) }, 0);
        thread_pool::make_spawn_job_unix(
            OsString::from("/usr/bin/true"),
            vec![OsString::from("/usr/bin/true")],
            Vec::new(),
            None,
            None,
            None,
            None,
            thread_pool::SpawnOptions { child_signal_mask },
        )
    }

    #[cfg(windows)]
    fn successful_process_job() -> Job {
        thread_pool::make_spawn_job_windows(
            OsString::from("cmd.exe /D /C exit 0"),
            vec![0, 0],
            None,
            None,
            None,
            None,
            thread_pool::SpawnOptions {
                no_console_window: true,
                is_orphan: false,
            },
        )
    }

    #[test]
    fn no_policy_does_not_allocate_child_ownership_tracking() {
        let host = AsyncHost::default();

        assert!(host.process_policy_state.is_none());
        host.check_owned_child_pid(i32::MAX).unwrap();
    }

    #[test]
    fn process_policy_denies_spawn_before_running_the_job() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "").unwrap();
        let host = host_with_policy(&policy_file);
        let job = host.insert_job(successful_process_job()).unwrap();

        host.run_job(job).unwrap();

        assert_eq!(host.job_get_ret(job).unwrap(), -1);
        assert_eq!(
            host.job_get_err(job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        assert_eq!(
            host.check_owned_child_pid(i32::MAX),
            Err(AsyncHostError::PermissionDenied)
        );
        host.free_job(job).unwrap();
    }

    #[test]
    fn process_policy_tracks_spawned_children_until_wait_completes() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[process]\nspawn = true\n").unwrap();
        let host = host_with_policy(&policy_file);
        let spawn_job = host.insert_job(successful_process_job()).unwrap();

        host.run_job(spawn_job).unwrap();

        assert_eq!(host.job_get_err(spawn_job).unwrap(), 0);
        let pid = host.job_get_ret(spawn_job).unwrap() as i32;
        host.check_owned_child_pid(pid).unwrap();
        let process_handle = host.get_spawn_job_result_handle(spawn_job).unwrap();
        let process_resource = if process_handle == host.invalid_fd() {
            None
        } else {
            Some(host.resource(process_handle).unwrap())
        };
        let process_handle_pid = host.process_handle_pid(process_handle).unwrap();
        if process_resource.is_some() {
            assert_eq!(process_handle_pid, Some(pid));
        }
        #[cfg(windows)]
        assert_eq!(
            crate::async_sys::process::process_id_from_handle(
                process_resource
                    .as_ref()
                    .unwrap()
                    .as_handle()
                    .unwrap()
                    .as_raw_handle()
            )
            .unwrap(),
            pid
        );
        let wait_job = host
            .insert_job(
                thread_pool::make_wait_for_process_job(
                    process_resource,
                    process_handle_pid,
                    pid,
                    #[cfg(unix)]
                    true,
                )
                .unwrap(),
            )
            .unwrap();

        host.run_job(wait_job).unwrap();

        assert_eq!(host.job_get_err(wait_job).unwrap(), 0);
        assert_eq!(
            host.check_owned_child_pid(pid),
            Err(AsyncHostError::PermissionDenied)
        );
        host.free_job(wait_job).unwrap();
        host.free_job(spawn_job).unwrap();
    }

    #[test]
    fn process_policy_rejects_wait_handle_for_another_pid() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[process]\nspawn = true\n").unwrap();
        let host = host_with_policy(&policy_file);
        let checked_pid = 1001;
        let tracked_pid = 1002;
        let state = host.process_policy_state.as_deref().unwrap();
        state
            .inner
            .lock()
            .unwrap()
            .owned_child_pids
            .extend([checked_pid, tracked_pid]);
        let [process_handle, other] = host.pipe(false, false).unwrap();
        host.track_process_handle(process_handle, tracked_pid);
        let process_resource = host.resource(process_handle).unwrap();

        assert_eq!(
            host.check_process_handle_pid(process_handle, checked_pid),
            Err(AsyncHostError::PermissionDenied)
        );
        let wait_job = host
            .insert_job(
                thread_pool::make_wait_for_process_job(
                    Some(process_resource),
                    host.process_handle_pid(process_handle).unwrap(),
                    checked_pid,
                    #[cfg(unix)]
                    true,
                )
                .unwrap(),
            )
            .unwrap();

        host.run_job(wait_job).unwrap();

        assert_eq!(
            host.job_get_err(wait_job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        host.check_owned_child_pid(checked_pid).unwrap();
        host.check_owned_child_pid(tracked_pid).unwrap();
        host.free_job(wait_job).unwrap();
        host.close_fd(process_handle).unwrap();
        host.close_fd(other).unwrap();
    }

    #[test]
    fn process_policy_revokes_pid_after_last_process_handle_closes() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[process]\nspawn = true\n").unwrap();
        let host = host_with_policy(&policy_file);
        let pid = 1001;
        host.process_policy_state
            .as_deref()
            .unwrap()
            .inner
            .lock()
            .unwrap()
            .owned_child_pids
            .insert(pid);
        let [first, second] = host.pipe(false, false).unwrap();
        host.track_process_handle(first, pid);
        host.track_process_handle(second, pid);

        host.close_fd(first).unwrap();
        host.check_owned_child_pid(pid).unwrap();
        host.close_fd(second).unwrap();

        assert_eq!(
            host.check_owned_child_pid(pid),
            Err(AsyncHostError::PermissionDenied)
        );
    }

    #[test]
    fn process_policy_revokes_unclaimed_spawn_result() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[process]\nspawn = true\n").unwrap();
        let host = host_with_policy(&policy_file);
        let pid = 1001;
        let mut job = successful_process_job();
        job.set_ret(i64::from(pid));
        host.process_policy_state
            .as_deref()
            .unwrap()
            .inner
            .lock()
            .unwrap()
            .owned_child_pids
            .insert(pid);
        let job = host.insert_job(job).unwrap();

        host.free_job(job).unwrap();

        assert_eq!(
            host.check_owned_child_pid(pid),
            Err(AsyncHostError::PermissionDenied)
        );
    }

    #[test]
    fn get_spawn_result_rejects_non_spawn_job() {
        let host = AsyncHost::default();
        let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();

        assert_eq!(
            host.get_spawn_job_result_handle(job),
            Err(AsyncHostError::Badf)
        );
        host.free_job(job).unwrap();
    }

    #[test]
    fn process_env_block_transfer_consumes_source() {
        let host = AsyncHost::default();
        #[cfg(unix)]
        let src = host.insert_process_env(vec![Some(OsString::from("A=B"))]);
        #[cfg(windows)]
        let src = host.insert_process_env(vec![b'A' as u16, b'=' as u16, b'B' as u16, 0, 0]);
        #[cfg(unix)]
        let dst = host.insert_process_env(vec![None, None]);
        #[cfg(windows)]
        let dst = host.insert_process_env(vec![0; 7]);

        host.transfer_process_env_block(dst, src).unwrap();

        assert!(matches!(host.process_env(src), Err(AsyncHostError::Badf)));
        #[cfg(unix)]
        assert_eq!(
            &*host.process_env(dst).unwrap().lock().unwrap(),
            &[Some(OsString::from("A=B")), None]
        );
        #[cfg(windows)]
        assert_eq!(
            &*host.process_env(dst).unwrap().lock().unwrap(),
            &[b'A' as u16, b'=' as u16, b'B' as u16, 0, 0, 0, 0]
        );
    }

    #[test]
    fn process_env_block_transfer_consumes_source_on_failure() {
        let host = AsyncHost::default();
        #[cfg(unix)]
        let src = host.insert_process_env(vec![Some(OsString::from("A=B"))]);
        #[cfg(windows)]
        let src = host.insert_process_env(vec![b'A' as u16, b'=' as u16, b'B' as u16, 0, 0]);
        #[cfg(unix)]
        let dst = host.insert_process_env(vec![]);
        #[cfg(windows)]
        let dst = host.insert_process_env(vec![0]);

        assert_eq!(
            host.transfer_process_env_block(dst, src),
            Err(AsyncHostError::Fault)
        );
        assert!(matches!(host.process_env(src), Err(AsyncHostError::Badf)));
    }

    #[cfg(unix)]
    #[test]
    fn process_spawn_buffers_transfer_ownership_together() {
        let host = AsyncHost::default();
        let argv = host.insert_process_argv(2).unwrap();
        host.process_argv_add_entry(argv, 0, OsString::from("command"))
            .unwrap();
        host.process_argv_add_entry(argv, 1, OsString::from("argument"))
            .unwrap();
        let env = host.insert_process_env(vec![Some(OsString::from("A=B"))]);

        let (args, entries) = host.take_process_spawn_buffers(argv, env).unwrap();

        assert_eq!(
            args,
            vec![OsString::from("command"), OsString::from("argument")]
        );
        assert_eq!(entries, vec![OsString::from("A=B")]);
        assert!(matches!(host.process_argv(argv), Err(AsyncHostError::Badf)));
        assert!(matches!(host.process_env(env), Err(AsyncHostError::Badf)));
    }

    #[cfg(unix)]
    #[test]
    fn invalid_process_spawn_buffers_are_not_partially_consumed() {
        let host = AsyncHost::default();
        let argv = host.insert_process_argv(1).unwrap();
        host.process_argv_add_entry(argv, 0, OsString::from("command"))
            .unwrap();
        let env = host.insert_process_env(vec![None]);

        assert_eq!(
            host.take_process_spawn_buffers(argv, env),
            Err(AsyncHostError::Inval)
        );
        assert!(host.process_argv(argv).is_ok());
        assert!(host.process_env(env).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn process_spawn_environment_transfers_ownership() {
        let host = AsyncHost::default();
        let block = vec![b'A' as u16, b'=' as u16, b'B' as u16, 0, 0];
        let env = host.insert_process_env(block.clone());

        assert_eq!(host.take_process_env(env).unwrap(), block);
        assert!(matches!(host.process_env(env), Err(AsyncHostError::Badf)));
    }

    #[cfg(windows)]
    #[test]
    fn process_policy_preserves_unsigned_windows_pid_bits() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[process]\nspawn = true\n").unwrap();
        let host = host_with_policy(&policy_file);
        let mut job = successful_process_job();
        job.set_ret(i64::from(0x8000_0000u32));

        AsyncHost::update_owned_child_pids(host.process_policy_state.as_deref(), &job).unwrap();

        host.check_owned_child_pid(i32::MIN).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn error_message_buffer_uses_native_string_encoding_on_unix() {
        assert_eq!(error_message_buffer("ab".to_string()).as_ref(), b"ab\0");
    }

    #[cfg(windows)]
    #[test]
    fn error_message_buffer_uses_native_string_encoding_on_windows() {
        assert_eq!(
            error_message_buffer("ab".to_string()).as_ref(),
            &[b'a', 0, b'b', 0, 0, 0]
        );
    }

    #[cfg(windows)]
    fn io_result_key(host: &AsyncHost, handle: HostHandle) -> HandleKey {
        host.handles.borrow().io_result(handle).unwrap()
    }

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

    #[test]
    fn resource_class_rejects_file_as_socket() {
        let host = AsyncHost::default();
        let [read, write] = host.pipe(true, true).unwrap();

        assert_eq!(
            host.socket_resource(read).unwrap_err(),
            AsyncHostError::Inval
        );
        assert!(host.resource_of_class(read, ResourceClass::File).is_ok());

        host.close_fd(read).unwrap();
        host.close_fd(write).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn pipe_applies_async_flags_to_nonblocking_state() {
        let host = AsyncHost::default();

        for (read_is_async, write_is_async) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let [read, write] = host.pipe(read_is_async, write_is_async).unwrap();
            let read_fd = host.resource(read).unwrap().as_fd().unwrap().as_raw_fd();
            let write_fd = host.resource(write).unwrap().as_fd().unwrap().as_raw_fd();
            let read_flags = unsafe { libc::fcntl(read_fd, libc::F_GETFL) };
            let write_flags = unsafe { libc::fcntl(write_fd, libc::F_GETFL) };

            assert!(read_flags >= 0);
            assert!(write_flags >= 0);
            assert_eq!((read_flags & libc::O_NONBLOCK) != 0, read_is_async);
            assert_eq!((write_flags & libc::O_NONBLOCK) != 0, write_is_async);

            host.close_fd(read).unwrap();
            host.close_fd(write).unwrap();
        }
    }

    #[test]
    fn resource_class_rejects_tcp_and_udp_mixups() {
        #[cfg(windows)]
        assert_eq!(crate::async_sys::internal::event_loop::io::init_wsa(), 0);

        let host = AsyncHost::default();
        let tcp = host.insert_socket_resource(
            crate::async_sys::socket::make_tcp_socket(4).unwrap(),
            ResourceClass::TcpSocket,
            4,
        );
        let udp = host.insert_socket_resource(
            crate::async_sys::socket::make_udp_socket(4, false).unwrap(),
            ResourceClass::UdpSocket,
            4,
        );

        assert!(
            host.with_raw_resource_class(tcp, ResourceClass::TcpSocket, |_| Ok(()))
                .is_ok()
        );
        assert_eq!(host.resource(tcp).unwrap().socket_family(), Some(4));
        assert_eq!(
            host.with_raw_resource_class(tcp, ResourceClass::UdpSocket, |_| Ok(())),
            Err(AsyncHostError::Inval)
        );
        assert!(
            host.with_raw_resource_class(udp, ResourceClass::UdpSocket, |_| Ok(()))
                .is_ok()
        );
        assert_eq!(
            host.with_raw_resource_class(udp, ResourceClass::TcpSocket, |_| Ok(())),
            Err(AsyncHostError::Inval)
        );

        host.close_fd(tcp).unwrap();
        host.close_fd(udp).unwrap();

        #[cfg(windows)]
        assert_eq!(crate::async_sys::internal::event_loop::io::cleanup_wsa(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn completion_source_is_resource_handle() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_source = host.init_thread_pool(poll).unwrap();
        let raw_completion_fd = host
            .resource(completion_source)
            .unwrap()
            .as_fd()
            .unwrap()
            .as_raw_fd();
        {
            let polls = host.polls.borrow();
            let poll = polls.polls.get(poll_key(&host, poll)).unwrap();
            assert_eq!(
                poll.registered_fds
                    .get(&raw_fd_key(raw_completion_fd))
                    .map(|registered| registered.handle),
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
        let job = thread_pool::make_read_job(Arc::new(Resource::invalid()), 3, -1);
        let job_handle = host.insert_job(job).unwrap();
        {
            let mut jobs = host.jobs.lock().unwrap();
            let job = jobs.visible_job_mut(job_key(&host, job_handle)).unwrap();
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
    fn realpath_result_is_registered_c_buffer_cleaned_up_with_job() {
        let host = AsyncHost::default();
        let job_handle = host
            .insert_job(thread_pool::make_realpath_job(std::ffi::OsString::from(
                "/tmp/example",
            )))
            .unwrap();
        {
            let mut jobs = host.jobs.lock().unwrap();
            let job = jobs.visible_job_mut(job_key(&host, job_handle)).unwrap();
            let thread_pool::JobPayload::Realpath { result, .. } = job.payload_mut() else {
                panic!("expected realpath job");
            };
            *result = Some(thread_pool::RealpathJobResult::Unpublished(
                b"/tmp/example\0".to_vec().into_boxed_slice(),
            ));
        }

        let buffer_handle = host.get_realpath_result(job_handle).unwrap();
        assert_eq!(host.get_realpath_result(job_handle).unwrap(), buffer_handle);
        host.with_c_buffer(buffer_handle, |buffer| {
            assert_eq!(buffer, b"/tmp/example\0");
            Ok(())
        })
        .unwrap();

        host.free_job(job_handle).unwrap();

        assert_eq!(
            host.with_c_buffer(buffer_handle, |_| Ok(())).unwrap_err(),
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
        assert_eq!(resource_count(&host), 0);

        let opened = host.open_job_get_fd(job).unwrap();
        assert_eq!(host.open_job_get_fd(job).unwrap(), opened);
        assert_eq!(host.run_job(job), Err(AsyncHostError::Badf));
        assert_eq!(resource_count(&host), 1);
        assert!(host.resource(opened).is_ok());

        host.close_fd(opened).unwrap();
        host.free_job(job).unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn run_job_checks_open_policy_at_execution() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let denied_file = denied.join("secret.txt");
        std::fs::write(&denied_file, "secret").unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nread = [\"allowed\"]\n").unwrap();
        let host = host_with_policy(&policy_file);
        let job = host
            .insert_job(thread_pool::make_open_job(
                denied_file.as_os_str().to_os_string(),
                0,
                0,
                false,
                0,
                0,
            ))
            .unwrap();

        host.run_job(job).unwrap();

        assert_eq!(host.job_get_ret(job).unwrap(), -1);
        assert_eq!(
            host.job_get_err(job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        host.free_job(job).unwrap();
    }

    #[test]
    fn run_job_checks_realpath_policy_at_execution() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let denied_file = denied.join("secret.txt");
        std::fs::write(&denied_file, "secret").unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nread = [\"allowed\"]\n").unwrap();
        let host = host_with_policy(&policy_file);
        let job = host
            .insert_job(thread_pool::make_realpath_job(
                denied_file.as_os_str().to_os_string(),
            ))
            .unwrap();

        host.run_job(job).unwrap();

        assert_eq!(host.job_get_ret(job).unwrap(), -1);
        assert_eq!(
            host.job_get_err(job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        host.free_job(job).unwrap();
    }

    #[test]
    fn worker_checks_open_policy_at_execution() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let denied_file = denied.join("secret.txt");
        std::fs::write(&denied_file, "secret").unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nread = [\"allowed\"]\n").unwrap();
        let host = host_with_policy(&policy_file);
        let poll = host.poll_create().unwrap();
        let completion_source = host.init_thread_pool(poll).unwrap();
        let job = host
            .insert_job(thread_pool::make_open_job(
                denied_file.as_os_str().to_os_string(),
                0,
                0,
                false,
                0,
                0,
            ))
            .unwrap();
        let worker = host.spawn_worker(42, job).unwrap();

        assert_eq!(host.poll_wait(poll, 1000).unwrap(), 1);
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

        assert_eq!(host.job_get_ret(job).unwrap(), -1);
        assert_eq!(
            host.job_get_err(job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        host.free_worker(worker).unwrap();
        host.free_job(job).unwrap();
        host.destroy_thread_pool();
    }

    #[cfg(unix)]
    #[test]
    fn run_job_rechecks_swapped_open_symlink_at_execution() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let allowed_file = allowed.join("input.txt");
        let denied_file = denied.join("secret.txt");
        let link = allowed.join("link.txt");
        std::fs::write(&allowed_file, "allowed").unwrap();
        std::fs::write(&denied_file, "secret").unwrap();
        std::os::unix::fs::symlink(&allowed_file, &link).unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nread = [\"allowed\"]\n").unwrap();
        let host = host_with_policy(&policy_file);

        host.policy()
            .open_path(
                RuntimePathBase::CurrentDirectory,
                link.as_os_str(),
                0,
                0,
                false,
            )
            .unwrap();
        let job = host
            .insert_job(thread_pool::make_open_job(
                link.as_os_str().to_os_string(),
                0,
                0,
                false,
                0,
                0,
            ))
            .unwrap();
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&denied_file, &link).unwrap();

        host.run_job(job).unwrap();

        assert_eq!(host.job_get_ret(job).unwrap(), -1);
        assert_eq!(
            host.job_get_err(job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        host.free_job(job).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn entry_mutation_jobs_check_link_path_not_target() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let allowed_target = allowed.join("target.txt");
        let allowed_source = allowed.join("source.txt");
        let denied_link = denied.join("link.txt");
        std::fs::write(&allowed_target, "target").unwrap();
        std::fs::write(&allowed_source, "source").unwrap();
        std::os::unix::fs::symlink(&allowed_target, &denied_link).unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nwrite = [\"allowed\"]\n").unwrap();
        let host = host_with_policy(&policy_file);
        let remove_job = host
            .insert_job(thread_pool::make_remove_job(
                denied_link.as_os_str().to_os_string(),
            ))
            .unwrap();
        let rename_job = host
            .insert_job(thread_pool::make_rename_job(
                allowed_source.as_os_str().to_os_string(),
                denied_link.as_os_str().to_os_string(),
                true,
            ))
            .unwrap();

        host.run_job(remove_job).unwrap();
        host.run_job(rename_job).unwrap();

        assert_eq!(host.job_get_ret(remove_job).unwrap(), -1);
        assert_eq!(
            host.job_get_err(remove_job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        assert_eq!(host.job_get_ret(rename_job).unwrap(), -1);
        assert_eq!(
            host.job_get_err(rename_job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        assert!(
            std::fs::symlink_metadata(&denied_link)
                .unwrap()
                .is_symlink()
        );
        assert!(allowed_source.exists());
        host.free_job(rename_job).unwrap();
        host.free_job(remove_job).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn no_follow_metadata_jobs_check_link_path_not_target() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let allowed_file = allowed.join("target.txt");
        let denied_link = denied.join("link.txt");
        std::fs::write(&allowed_file, "target").unwrap();
        std::os::unix::fs::symlink(&allowed_file, &denied_link).unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nread = [\"allowed\"]\n").unwrap();
        let host = host_with_policy(&policy_file);
        let kind_job = host
            .insert_job(thread_pool::make_file_kind_by_path_job(
                None,
                denied_link.as_os_str().to_os_string(),
                false,
            ))
            .unwrap();
        let time_job = host
            .insert_job(thread_pool::make_file_time_by_path_job(
                denied_link.as_os_str().to_os_string(),
                false,
            ))
            .unwrap();

        host.run_job(kind_job).unwrap();
        host.run_job(time_job).unwrap();

        assert_eq!(host.job_get_ret(kind_job).unwrap(), -1);
        assert_eq!(
            host.job_get_err(kind_job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        assert_eq!(host.job_get_ret(time_job).unwrap(), -1);
        assert_eq!(
            host.job_get_err(time_job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        host.free_job(time_job).unwrap();
        host.free_job(kind_job).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn no_follow_metadata_jobs_honor_parent_resource_base() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let allowed_file = allowed.join("target.txt");
        let denied_file = denied.join("target.txt");
        let allowed_link = allowed.join("link.txt");
        let denied_link = denied.join("link.txt");
        std::fs::write(&allowed_file, "allowed").unwrap();
        std::fs::write(&denied_file, "denied").unwrap();
        std::os::unix::fs::symlink(&denied_file, &allowed_link).unwrap();
        std::os::unix::fs::symlink(&allowed_file, &denied_link).unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nread = [\"allowed\"]\n").unwrap();
        let host = host_with_policy(&policy_file);
        let parent_open_job = host
            .insert_job(thread_pool::make_open_job(
                allowed.as_os_str().to_os_string(),
                0,
                0,
                false,
                0,
                0,
            ))
            .unwrap();

        host.run_job(parent_open_job).unwrap();
        let parent_fd = host.open_job_get_fd(parent_open_job).unwrap();
        let parent = host.resource(parent_fd).unwrap();
        let allowed_link_job = host
            .insert_job(thread_pool::make_file_kind_by_path_job(
                Some(Arc::clone(&parent)),
                std::ffi::OsString::from("link.txt"),
                false,
            ))
            .unwrap();
        let denied_link_job = host
            .insert_job(thread_pool::make_file_kind_by_path_job(
                None,
                denied_link.as_os_str().to_os_string(),
                false,
            ))
            .unwrap();

        host.run_job(allowed_link_job).unwrap();
        host.run_job(denied_link_job).unwrap();

        assert_eq!(host.job_get_ret(allowed_link_job).unwrap(), 3);
        assert_eq!(host.job_get_ret(denied_link_job).unwrap(), -1);
        assert_eq!(
            host.job_get_err(denied_link_job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        host.free_job(denied_link_job).unwrap();
        host.free_job(allowed_link_job).unwrap();
        host.close_fd(parent_fd).unwrap();
        host.free_job(parent_open_job).unwrap();
    }

    #[test]
    fn fd_metadata_jobs_require_read_policy() {
        let tmp = tempfile::tempdir().unwrap();
        let writable = tmp.path().join("writable");
        std::fs::create_dir(&writable).unwrap();
        let file = writable.join("data.txt");
        std::fs::write(&file, "secret").unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nwrite = [\"writable\"]\n").unwrap();
        let host = host_with_policy(&policy_file);
        let open_job = host
            .insert_job(thread_pool::make_open_job(
                file.as_os_str().to_os_string(),
                1,
                0,
                false,
                0,
                0,
            ))
            .unwrap();

        host.run_job(open_job).unwrap();
        let fd = host.open_job_get_fd(open_job).unwrap();
        let resource = host.resource(fd).unwrap();
        let size_job = host
            .insert_job(thread_pool::make_file_size_job(Arc::clone(&resource)))
            .unwrap();
        let time_job = host
            .insert_job(thread_pool::make_file_time_job(Arc::clone(&resource)))
            .unwrap();

        host.run_job(size_job).unwrap();
        host.run_job(time_job).unwrap();

        assert_eq!(host.job_get_ret(size_job).unwrap(), -1);
        assert_eq!(
            host.job_get_err(size_job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        assert_eq!(host.job_get_ret(time_job).unwrap(), -1);
        assert_eq!(
            host.job_get_err(time_job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        host.free_job(time_job).unwrap();
        host.free_job(size_job).unwrap();
        host.close_fd(fd).unwrap();
        host.free_job(open_job).unwrap();
    }

    #[test]
    fn direct_exclusive_lock_requires_write_policy() {
        let tmp = tempfile::tempdir().unwrap();
        let readable = tmp.path().join("readable");
        std::fs::create_dir(&readable).unwrap();
        let file = readable.join("data.txt");
        std::fs::write(&file, "secret").unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nread = [\"readable\"]\n").unwrap();
        let host = host_with_policy(&policy_file);
        let open_job = host
            .insert_job(thread_pool::make_open_job(
                file.as_os_str().to_os_string(),
                0,
                0,
                false,
                0,
                0,
            ))
            .unwrap();

        host.run_job(open_job).unwrap();
        let fd = host.open_job_get_fd(open_job).unwrap();

        assert_eq!(
            host.try_lock_file(fd, true),
            Err(AsyncHostError::PermissionDenied)
        );

        host.close_fd(fd).unwrap();
        host.free_job(open_job).unwrap();
    }

    #[test]
    fn flock_job_exclusive_lock_requires_write_policy() {
        let tmp = tempfile::tempdir().unwrap();
        let readable = tmp.path().join("readable");
        std::fs::create_dir(&readable).unwrap();
        let file = readable.join("data.txt");
        std::fs::write(&file, "secret").unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nread = [\"readable\"]\n").unwrap();
        let host = host_with_policy(&policy_file);
        let open_job = host
            .insert_job(thread_pool::make_open_job(
                file.as_os_str().to_os_string(),
                0,
                0,
                false,
                0,
                0,
            ))
            .unwrap();

        host.run_job(open_job).unwrap();
        let fd = host.open_job_get_fd(open_job).unwrap();
        let resource = host.resource(fd).unwrap();
        let flock_job = host
            .insert_job(thread_pool::make_flock_job(Arc::clone(&resource), true))
            .unwrap();

        host.run_job(flock_job).unwrap();

        assert_eq!(host.job_get_ret(flock_job).unwrap(), -1);
        assert_eq!(
            host.job_get_err(flock_job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        host.free_job(flock_job).unwrap();
        host.close_fd(fd).unwrap();
        host.free_job(open_job).unwrap();
    }

    #[test]
    fn tls_set_server_files_checks_file_policy_before_backend_load() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let key_file = denied.join("key.pem");
        let cert_file = allowed.join("cert.pem");
        std::fs::write(&key_file, "key").unwrap();
        std::fs::write(&cert_file, "cert").unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nread = [\"allowed\"]\n").unwrap();
        let host = host_with_policy(&policy_file);

        let handle = host.tls_new();
        let status = host
            .tls_set_server_files(
                handle,
                key_file,
                tls::TlsFileType::Pem,
                cert_file,
                tls::TlsFileType::Pem,
            )
            .unwrap();

        assert_eq!(status, tls::TLS_ERROR_STATUS);
        let error = host.tls_take_error(handle).unwrap();
        host.with_c_buffer(error, |buffer| {
            let expected = error_message_buffer(
                "failed to access TLS private key file: PermissionDenied".to_string(),
            );
            assert_eq!(buffer, &*expected);
            Ok(())
        })
        .unwrap();
        host.free_c_buffer(error).unwrap();
        host.tls_free(handle).unwrap();
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
        let key = job_key(&host, job_handle);
        let mut job = host.jobs.lock().unwrap().take_ready_job(key).unwrap();

        thread_pool::run_host_job(&mut job);

        assert_eq!(job.err(), 0);
        assert!(matches!(
            thread_pool::open_job_result(&job).unwrap().resource,
            OpenJobResource::Unpublished(_)
        ));
        assert_eq!(resource_count(&host), 0);
        host.jobs.lock().unwrap().jobs.remove(key);

        {
            assert_eq!(host.restore_job(key, job), Err(AsyncHostError::Badf));
            assert_eq!(resource_count(&host), 0);
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn drop_destroys_pool_even_when_worker_holds_state() {
        let host = AsyncHost::default();
        let jobs = Arc::downgrade(&host.jobs);
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

        assert!(jobs.upgrade().is_none());
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
        assert_eq!(host.run_job(job), Err(AsyncHostError::Badf));
        assert_eq!(host.spawn_worker(43, job), Err(AsyncHostError::Badf));
        assert_eq!(host.wake_worker(worker, 44, job), Err(AsyncHostError::Badf));

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
    fn freed_running_worker_job_is_not_restored_after_completion() {
        let host = AsyncHost::default();
        let first_job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let first_key = job_key(&host, first_job);
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let (completion_sender, completion_receiver) = std::sync::mpsc::channel();
        let worker = {
            let jobs_for_runner = Arc::clone(&host.jobs);
            let jobs_for_completion = Arc::clone(&host.jobs);
            host.jobs.lock().unwrap().queue_job(first_key).unwrap();
            thread_pool::spawn_worker(
                HostWorkerJob {
                    completion_id: WorkerCompletionId::from_abi(1),
                    job_key: first_key,
                },
                move |worker_job| {
                    let Ok(mut job) = jobs_for_runner
                        .lock()
                        .unwrap()
                        .take_queued_job(worker_job.job_key)
                    else {
                        return None;
                    };
                    started_sender.send(worker_job.completion_id).unwrap();
                    release_receiver.recv().unwrap();
                    thread_pool::run_host_job(&mut job);
                    Some(job)
                },
                move |worker_job| {
                    if let Some(job) = worker_job.job {
                        let _ = jobs_for_completion
                            .lock()
                            .unwrap()
                            .restore_job(worker_job.job_key, job);
                    }
                    completion_sender.send(worker_job.completion_id).unwrap();
                },
            )
        };
        let worker_key = host.handles.borrow_mut().insert(HandleKind::Worker);
        host.workers.lock().unwrap().insert(worker_key, worker);
        let worker = handle_from_key(worker_key);

        assert_eq!(
            started_receiver.recv().unwrap(),
            WorkerCompletionId::from_abi(1)
        );
        host.free_job(first_job).unwrap();

        release_sender.send(()).unwrap();
        assert_eq!(
            completion_receiver.recv().unwrap(),
            WorkerCompletionId::from_abi(1)
        );
        assert_eq!(host.job_get_ret(first_job), Err(AsyncHostError::Badf));
        assert_eq!(host.run_job(first_job), Err(AsyncHostError::Badf));

        host.free_worker(worker).unwrap();
        host.destroy_thread_pool();
    }

    #[test]
    fn queued_worker_job_can_be_freed_before_worker_runs_it() {
        let host = AsyncHost::default();
        let first_job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let first_key = job_key(&host, first_job);
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let (completion_sender, completion_receiver) = std::sync::mpsc::channel();
        let worker = {
            let jobs_for_runner = Arc::clone(&host.jobs);
            let jobs_for_completion = Arc::clone(&host.jobs);
            host.jobs.lock().unwrap().queue_job(first_key).unwrap();
            thread_pool::spawn_worker(
                HostWorkerJob {
                    completion_id: WorkerCompletionId::from_abi(1),
                    job_key: first_key,
                },
                move |worker_job| {
                    let Ok(mut job) = jobs_for_runner
                        .lock()
                        .unwrap()
                        .take_queued_job(worker_job.job_key)
                    else {
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
                        let _ = jobs_for_completion
                            .lock()
                            .unwrap()
                            .restore_job(worker_job.job_key, job);
                    }
                    completion_sender
                        .send((worker_job.completion_id, completed))
                        .unwrap();
                },
            )
        };
        let worker_key = host.handles.borrow_mut().insert(HandleKind::Worker);
        host.workers.lock().unwrap().insert(worker_key, worker);
        let worker = handle_from_key(worker_key);

        assert_eq!(
            started_receiver.recv().unwrap(),
            WorkerCompletionId::from_abi(1)
        );

        let displaced_path = std::env::temp_dir().join(format!(
            "moonrun-displaced-queued-worker-job-{}",
            std::process::id()
        ));
        std::fs::write(&displaced_path, b"displaced").unwrap();
        let displaced_job = host
            .insert_job(thread_pool::make_remove_job(
                displaced_path.as_os_str().to_os_string(),
            ))
            .unwrap();
        let queued_path = std::env::temp_dir().join(format!(
            "moonrun-cancelled-queued-worker-job-{}",
            std::process::id()
        ));
        std::fs::write(&queued_path, b"queued").unwrap();
        let queued_job = host
            .insert_job(thread_pool::make_remove_job(
                queued_path.as_os_str().to_os_string(),
            ))
            .unwrap();

        host.wake_worker(worker, 2, displaced_job).unwrap();
        host.wake_worker(worker, 3, queued_job).unwrap();
        host.run_job(displaced_job).unwrap();
        assert!(!displaced_path.exists());
        host.free_job(displaced_job).unwrap();
        assert_eq!(host.job_get_ret(queued_job), Err(AsyncHostError::Badf));
        assert_eq!(host.run_job(queued_job), Err(AsyncHostError::Badf));
        assert_eq!(host.spawn_worker(4, queued_job), Err(AsyncHostError::Badf));
        host.free_job(queued_job).unwrap();

        release_sender.send(()).unwrap();
        assert_eq!(
            completion_receiver.recv().unwrap(),
            (WorkerCompletionId::from_abi(1), true)
        );
        host.free_job(first_job).unwrap();
        assert_eq!(
            completion_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            (WorkerCompletionId::from_abi(3), false)
        );
        assert!(queued_path.exists());

        host.free_worker(worker).unwrap();
        let _ = std::fs::remove_file(displaced_path);
        let _ = std::fs::remove_file(queued_path);
    }

    #[test]
    fn worker_handles_stay_stale_after_thread_pool_reinit() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let first_job = host
            .insert_job(thread_pool::make_read_job(
                Arc::new(Resource::invalid()),
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
                Arc::new(Resource::invalid()),
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
    fn native_order_completion_before_poll_destroy_remains_supported() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let completion = host.thread_pool_completion_target().unwrap();

        poll::post_thread_pool_completion(&completion.0, 42, completion.1).unwrap();

        assert_eq!(host.poll_wait(poll, 1000).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();
        assert_eq!(host.poll_event_fd(event).unwrap(), completion_notifier);
        assert_eq!(host.poll_event_bytes_transferred(event).unwrap(), 42);

        drop(completion);
        host.poll_destroy(poll).unwrap();
        host.destroy_thread_pool();
    }

    #[cfg(windows)]
    #[test]
    fn alternate_order_poll_destroy_before_completion_remains_safe() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();

        host.init_thread_pool(poll).unwrap();
        // A worker captures this target when it is spawned. Destroying the
        // guest poll handle must not invalidate the target while that worker
        // can still publish its terminal completion.
        let completion = host.thread_pool_completion_target().unwrap();

        host.poll_destroy(poll).unwrap();

        poll::post_thread_pool_completion(&completion.0, 42, completion.1).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn stale_thread_pool_completions_are_ignored_after_reinit() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();

        host.init_thread_pool(poll).unwrap();
        let stale_completion = host
            .thread_pool_completions
            .lock()
            .unwrap()
            .target
            .clone()
            .unwrap();
        // Fill the current IOCP batch so a valid completion can sit behind
        // stale completions from the destroyed pool generation.
        for completion_id in 0..1024 {
            poll::post_thread_pool_completion(
                &stale_completion.port,
                completion_id,
                stale_completion.generation,
            )
            .unwrap();
        }
        host.destroy_thread_pool();

        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let current_completion = host
            .thread_pool_completions
            .lock()
            .unwrap()
            .target
            .clone()
            .unwrap();
        assert_ne!(stale_completion.generation, current_completion.generation);

        poll::post_thread_pool_completion(
            &current_completion.port,
            43,
            current_completion.generation,
        )
        .unwrap();
        assert_eq!(host.poll_wait(poll, 1000).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();
        assert_eq!(host.poll_event_fd(event).unwrap(), completion_notifier);
        assert_eq!(host.poll_event_bytes_transferred(event).unwrap(), 43);
    }

    #[test]
    fn stale_worker_handle_is_rejected_after_free() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let job = host
            .insert_job(thread_pool::make_read_job(
                Arc::new(Resource::invalid()),
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
    fn acquired_resource_survives_guest_close() {
        let host = AsyncHost::default();
        let [read, write] = host.pipe(false, false).unwrap();
        let file = host.resource(read).unwrap();

        host.close_fd(read).unwrap();
        let mut input = *b"x";
        host.write_fd(&mut input, write, 0, 0, 1).unwrap();

        let mut output = [0];
        let ret = unsafe {
            libc::read(
                file.as_fd().unwrap().as_raw_fd(),
                output.as_mut_ptr().cast(),
                output.len(),
            )
        };
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
                host.resource(read).unwrap(),
                1,
                -1,
            ))
            .unwrap();

        host.close_fd(read).unwrap();
        let fd = host.resource(write).unwrap().as_fd().unwrap().as_raw_fd();
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
        let mut result = HostIoResult::for_file_read(0, 0).unwrap();
        let pending_resource = Arc::new(Resource::invalid());
        let other_file = Arc::new(Resource::invalid());
        let pending_fd = pending_resource.raw_identity();

        result.mark_pending(pending_resource).unwrap();

        assert_eq!(
            result.validate_pending_resource(&other_file),
            Err(AsyncHostError::Badf)
        );
        assert_eq!(result.pending_resource_identity(), Some(pending_fd));
    }

    #[cfg(windows)]
    #[test]
    fn io_result_creation_keeps_only_host_buffer_capacity() {
        let result = HostIoResult::for_file_read(3, 0).unwrap();

        assert_eq!(result.buffer, vec![0; 3]);
        assert_eq!(result.event, IO_RESULT_READ_EVENT);
        assert_eq!(result.pending_resource_identity(), None);
    }

    #[cfg(windows)]
    #[test]
    fn io_result_read_copy_uses_current_guest_destination() {
        let mut result = HostIoResult::for_file_read(3, 0).unwrap();
        result.buffer.copy_from_slice(b"abc");
        let mut memory = vec![0; 16];

        result
            .copy_read_result(memory.as_mut_slice(), 8, 1, 3)
            .unwrap();

        assert_eq!(&memory[9..12], b"abc");
    }

    #[cfg(windows)]
    #[test]
    fn io_result_read_with_addr_copy_uses_current_guest_buffers() {
        let mut result = HostIoResult::for_socket_with_addr_read(3, 0, b"addr".to_vec()).unwrap();
        result.buffer.copy_from_slice(b"abc");
        result.addr_len = 4;
        let mut memory = vec![0; 16];

        result
            .copy_read_result_with_addr(memory.as_mut_slice(), 8, 1, 3, 0, 4)
            .unwrap();

        assert_eq!(&memory[0..4], b"addr");
        assert_eq!(&memory[9..12], b"abc");
        assert_eq!(
            result.copy_read_result(memory.as_mut_slice(), 8, 1, 3),
            Err(AsyncHostError::Inval)
        );
    }

    #[cfg(windows)]
    #[test]
    fn io_result_socket_addr_creation_copies_guest_source() {
        let result = HostIoResult::for_socket_with_addr_read(3, 0, b"addr".to_vec()).unwrap();

        assert_eq!(result.addr_buffer, b"addr");
        assert_eq!(result.addr_len, 4);
        assert_eq!(result.event, IO_RESULT_READ_EVENT);
    }

    #[cfg(windows)]
    #[test]
    fn io_result_write_creation_copies_guest_source() {
        let host = AsyncHost::default();
        let mut memory = b"zzzabc".to_vec();

        let result = host
            .make_file_write_io_result(memory.as_mut_slice(), 3, 0, 3, 0)
            .unwrap();
        memory[3..6].copy_from_slice(b"xxx");

        let io_results = host.io_results.lock().unwrap();
        let result = io_results
            .io_results
            .get(io_result_key(&host, result))
            .unwrap();
        assert_eq!(result.buffer, b"abc");
        assert_eq!(result.event, IO_RESULT_WRITE_EVENT);
    }

    #[cfg(windows)]
    #[test]
    fn cancel_io_result_rejects_wrong_fd_without_clearing_pending() {
        let host = AsyncHost::default();
        let [read, write] = host.pipe(true, true).unwrap();
        let result = host.make_file_read_io_result(0, 0).unwrap();
        let raw_read = {
            let read_file = host.resource(read).unwrap();
            let raw_read = read_file.raw_identity();
            host.io_results
                .lock()
                .unwrap()
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap()
                .mark_pending(read_file)
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
                .get_mut(io_result_key(&host, result))
                .unwrap();
            assert_eq!(result.pending_resource_identity(), Some(raw_read));
            result.clear_pending();
        }

        host.free_io_result(result).unwrap();
        host.close_fd(read).unwrap();
        host.close_fd(write).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn cancel_io_result_clears_pending_result_when_no_wait_is_needed() {
        let host = AsyncHost::default();
        let [read, write] = host.pipe(true, true).unwrap();
        let result = host.make_file_read_io_result(0, 0).unwrap();
        {
            let read_file = host.resource(read).unwrap();
            host.io_results
                .lock()
                .unwrap()
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap()
                .mark_pending(read_file)
                .unwrap();
        }

        assert_eq!(host.cancel_io_result(result, read), Ok(0));
        {
            let io_results = host.io_results.lock().unwrap();
            let result = io_results
                .io_results
                .get(io_result_key(&host, result))
                .unwrap();
            assert_eq!(result.pending_resource_identity(), None);
        }

        host.free_io_result(result).unwrap();
        host.close_fd(read).unwrap();
        host.close_fd(write).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn cancel_io_result_keeps_pending_result_when_wait_is_needed() {
        let host = AsyncHost::default();
        let [read, write] = host.pipe(true, true).unwrap();
        let result = host.make_file_read_io_result(0, 0).unwrap();
        let raw_read = {
            let read_file = host.resource(read).unwrap();
            let raw_read = read_file.raw_identity();
            let mut io_results = host.io_results.lock().unwrap();
            let result = io_results
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap();
            result.overlapped.Internal = windows_sys::Win32::Foundation::STATUS_PENDING as usize;
            result.mark_pending(read_file).unwrap();
            raw_read
        };

        assert_eq!(host.cancel_io_result(result, read), Ok(1));
        assert_eq!(host.free_io_result(result), Err(AsyncHostError::Inval));
        assert_eq!(host.close_fd(read), Err(AsyncHostError::Inval));
        {
            let mut io_results = host.io_results.lock().unwrap();
            let result = io_results
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap();
            assert_eq!(result.pending_resource_identity(), Some(raw_read));
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
        let result = host.make_file_read_io_result(0, 0).unwrap();
        let raw_read = {
            let read_file = host.resource(read).unwrap();
            let raw_read = read_file.raw_identity();
            host.io_results
                .lock()
                .unwrap()
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap()
                .mark_pending(read_file)
                .unwrap();
            raw_read
        };

        assert_eq!(host.close_fd(read), Err(AsyncHostError::Inval));
        {
            assert!(host.resource(read).is_ok());
            let mut io_results = host.io_results.lock().unwrap();
            let result = io_results
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap();
            assert_eq!(result.pending_resource_identity(), Some(raw_read));
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
        let result = host.make_file_read_io_result(0, 0).unwrap();
        let (raw_read, raw_write) = {
            let read_file = host.resource(read).unwrap();
            let write_file = host.resource(write).unwrap();
            let raw_read = read_file.raw_identity();
            let raw_write = write_file.raw_identity();
            host.io_results
                .lock()
                .unwrap()
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap()
                .mark_pending_with_close_guard(read_file, write_file)
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
            assert!(host.resource(read).is_ok());
            assert!(host.resource(write).is_ok());
            let mut io_results = host.io_results.lock().unwrap();
            let result = io_results
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap();
            assert_eq!(result.pending_resource_identity(), Some(raw_read));
            assert!(result.protects_pending_resource(&host.resource(write).unwrap()));
            assert_eq!(
                result
                    .extra_pending_close_resource
                    .as_ref()
                    .map(|file| file.raw_identity()),
                Some(raw_write)
            );
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
        let [read, write] = host.pipe(true, true).unwrap();
        let result = host.make_file_read_io_result(0, 0).unwrap();
        {
            let read_file = host.resource(read).unwrap();
            host.io_results
                .lock()
                .unwrap()
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap()
                .mark_pending(read_file)
                .unwrap();
        }

        assert_eq!(host.free_io_result(result), Err(AsyncHostError::Inval));
        assert!(
            host.io_results
                .lock()
                .unwrap()
                .io_results
                .contains_key(io_result_key(&host, result))
        );
        host.io_results
            .lock()
            .unwrap()
            .io_results
            .get_mut(io_result_key(&host, result))
            .unwrap()
            .clear_pending();
        host.free_io_result(result).unwrap();
        host.close_fd(read).unwrap();
        host.close_fd(write).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn poll_event_io_result_marks_pending_result_delivered() {
        use windows_sys::Win32::System::IO::PostQueuedCompletionStatus;

        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_port = {
            let polls = host.polls.borrow();
            let poll = polls.polls.get(poll_key(&host, poll)).unwrap();
            poll.instance.raw_fd()
        };
        let result = host.make_file_read_io_result(0, 0).unwrap();
        let [read, write] = host.pipe(true, true).unwrap();
        let read_file = host.resource(read).unwrap();
        let raw_fd = read_file.raw_identity();
        let overlapped = {
            let mut io_results = host.io_results.lock().unwrap();
            let result = io_results
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap();
            result.mark_pending(read_file).unwrap();
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
                .get(io_result_key(&host, result))
                .unwrap()
                .pending_resource_identity(),
            None
        );
        host.free_io_result(result).unwrap();
        host.close_fd(read).unwrap();
        host.close_fd(write).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn unregistered_iocp_completion_reports_invalid_fd() {
        use windows_sys::Win32::System::IO::PostQueuedCompletionStatus;

        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_port = {
            let polls = host.polls.borrow();
            let poll = polls.polls.get(poll_key(&host, poll)).unwrap();
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

        let fd = host.resource(write).unwrap().as_fd().unwrap().as_raw_fd();
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

        let fd = host.resource(write).unwrap().as_fd().unwrap().as_raw_fd();
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
