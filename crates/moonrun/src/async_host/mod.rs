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

//! Moonrun-owned async domain state.
//!
//! This module owns async resources and operations for one run: Handle-indexed
//! resources, workers, jobs, and poll instances. Wasm runtime adapter concerns such
//! as V8 callbacks and guest-memory access live in `async_api` and `v8`.
//!
//! Native async multiplexes pollable IO through epoll, kqueue, or IOCP, with
//! thread-pool completions as one registered event
//! source. The wasm ABI exposes that same shape: MoonBit owns event-loop
//! scheduling and Rust owns the OS poller behind opaque poll handles.

use std::cell::{Cell, RefCell};
#[cfg(windows)]
use std::collections::HashMap;
#[cfg(unix)]
use std::collections::HashSet;
use std::ffi::OsString;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, AsRawSocket, RawHandle};
use std::rc::Rc;
use std::sync::Arc;

use slotmap::{Key, SecondaryMap};

#[cfg(unix)]
use crate::async_sys::internal::event_loop::ThreadPoolCompletionNotifier;
use crate::async_sys::internal::event_loop::{
    poll::{self, PollInstance},
    thread_pool::{
        self, HostHandle, HostWorkerJob, Job, JobPayload, ResourceTable, WorkerCompletionId,
    },
};
use crate::async_sys::internal::fd_util::stub::RawFd;
use crate::async_sys::socket::RawSocket;
use crate::guest_memory::{GuestMemory, GuestMemoryError};
use crate::network::HostNetwork;
use crate::policy::Policy;
use crate::process::HostProcess;
use crate::resource::{Resource, ResourceClass, ResourcePublication, ResourceRef};
pub(crate) use crate::runtime::HostKey as HandleKey;
use crate::runtime::{Env, HostKeys, HostResourceKind as HandleKind, WorkingDirectory};
use crate::temp_dir::TempDir;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
compile_error!("moonrun async wasm host currently supports only Linux, macOS, and Windows hosts");

#[cfg(not(target_endian = "little"))]
compile_error!("moonrun async wasm host requires little-endian host memory");

pub(crate) mod tls;
mod workers;

use workers::InstanceWorkers;

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
    fn new(key: HandleKey, buffer: Box<[u8]>) -> Self {
        Self { key, buffer }
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    fn into_parts(self) -> (HandleKey, Box<[u8]>) {
        (self.key, self.buffer)
    }
}

#[cfg(windows)]
enum HostWindowsWatcherBuffer {
    Available(crate::async_sys::fs::watch_windows::EventBuffer),
    Leased,
}

#[cfg(windows)]
#[derive(Debug)]
pub(crate) struct WindowsWatcherBufferLease {
    key: HandleKey,
    buffer: crate::async_sys::fs::watch_windows::EventBuffer,
}

#[cfg(windows)]
impl WindowsWatcherBufferLease {
    fn new(key: HandleKey, buffer: crate::async_sys::fs::watch_windows::EventBuffer) -> Self {
        Self { key, buffer }
    }

    fn buffer(&self) -> &crate::async_sys::fs::watch_windows::EventBuffer {
        &self.buffer
    }

    fn buffer_mut(&mut self) -> &mut crate::async_sys::fs::watch_windows::EventBuffer {
        &mut self.buffer
    }

    fn into_parts(self) -> (HandleKey, crate::async_sys::fs::watch_windows::EventBuffer) {
        (self.key, self.buffer)
    }
}
#[cfg(unix)]
type HostProcessArgv = Vec<Option<OsString>>;
#[cfg(unix)]
type HostProcessEnv = Vec<Option<crate::async_sys::process::LegacyProcessEnvEntry>>;
#[cfg(windows)]
type HostProcessEnv = Vec<u16>;
type HostProcessEnvBuilder = crate::async_sys::process::ProcessEnvBuilder;

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

impl From<GuestMemoryError> for AsyncHostError {
    fn from(_error: GuestMemoryError) -> Self {
        Self::Fault
    }
}

pub(crate) fn read_u16(memory: &[u8], offset: u32, len: u32) -> AsyncHostResult<Vec<u16>> {
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let (offset, end) = u16_bounds(memory.len(), offset, len)?;
    Ok(memory[offset..end]
        .chunks_exact(std::mem::size_of::<u16>())
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect())
}

pub(crate) fn write_u16(memory: &mut [u8], offset: u32, data: &[u16]) -> AsyncHostResult<()> {
    let (offset, end) = u16_bounds(memory.len(), offset, data.len())?;
    for (dst, value) in memory[offset..end]
        .chunks_exact_mut(std::mem::size_of::<u16>())
        .zip(data.iter().copied())
    {
        dst.copy_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn u16_bounds(memory_len: usize, offset: u32, len: usize) -> AsyncHostResult<(usize, usize)> {
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

struct HandleTable {
    keys: Rc<RefCell<HostKeys>>,
    resources: SecondaryMap<HandleKey, ResourceRef>,
    invalid_resource: HandleKey,
    stdio_resources: [HandleKey; 3],
}

impl Default for HandleTable {
    fn default() -> Self {
        Self::with_keys(Rc::new(RefCell::new(HostKeys::default())))
    }
}

impl HandleTable {
    fn with_keys(keys: Rc<RefCell<HostKeys>>) -> Self {
        let mut handles = keys.borrow_mut();
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

        drop(handles);
        Self {
            keys,
            resources,
            invalid_resource,
            stdio_resources,
        }
    }

    fn invalid_fd(&self) -> HostHandle {
        handle_from_key(self.invalid_resource)
    }

    fn resource_ref(&self, handle: HostHandle) -> AsyncHostResult<&ResourceRef> {
        let key = self.key(handle, HandleKind::Resource)?;
        let resource = self.resources.get(key).ok_or(AsyncHostError::Badf)?;
        if resource.is_invalid() {
            return Err(AsyncHostError::Badf);
        }
        Ok(resource)
    }

    fn resource(&self, handle: HostHandle) -> AsyncHostResult<&Resource> {
        self.resource_ref(handle).map(ResourceRef::as_ref)
    }

    fn resource_of_class(
        &self,
        handle: HostHandle,
        class: ResourceClass,
    ) -> AsyncHostResult<&Resource> {
        let resource = self.resource(handle)?;
        if resource.resource_class() != class {
            return Err(AsyncHostError::Inval);
        }
        Ok(resource)
    }

    fn socket(&self, handle: HostHandle) -> AsyncHostResult<&Resource> {
        let resource = self.resource(handle)?;
        if !resource.resource_class().is_socket() {
            return Err(AsyncHostError::Inval);
        }
        Ok(resource)
    }

    fn acquire_resource(&self, handle: HostHandle) -> AsyncHostResult<ResourceRef> {
        self.resource_ref(handle).map(Arc::clone)
    }

    fn acquire_resource_of_class(
        &self,
        handle: HostHandle,
        class: ResourceClass,
    ) -> AsyncHostResult<ResourceRef> {
        let resource = self.resource_ref(handle)?;
        if resource.resource_class() != class {
            return Err(AsyncHostError::Inval);
        }
        Ok(Arc::clone(resource))
    }

    fn acquire_socket(&self, handle: HostHandle) -> AsyncHostResult<ResourceRef> {
        let resource = self.resource_ref(handle)?;
        if !resource.resource_class().is_socket() {
            return Err(AsyncHostError::Inval);
        }
        Ok(Arc::clone(resource))
    }

    fn remove_resource(&mut self, handle: HostHandle) -> AsyncHostResult<ResourceRef> {
        let key = self.key(handle, HandleKind::Resource)?;
        if key == self.invalid_resource || self.stdio_resources.contains(&key) {
            return Err(AsyncHostError::Badf);
        }
        self.keys
            .borrow_mut()
            .remove(key)
            .ok_or(AsyncHostError::Badf)?;
        self.resources.remove(key).ok_or(AsyncHostError::Badf)
    }

    fn insert_resource(&mut self, resource: Resource) -> HostHandle {
        let key = self.keys.borrow_mut().insert(HandleKind::Resource);
        self.resources.insert(key, Arc::new(resource));
        handle_from_key(key)
    }

    fn insert(&mut self, kind: HandleKind) -> HandleKey {
        self.keys.borrow_mut().insert(kind)
    }

    fn job(&self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.key(handle, HandleKind::Job)
    }

    fn remove_job_key(&mut self, key: HandleKey) {
        let removed = self.keys.borrow_mut().remove(key);
        debug_assert!(
            matches!(removed, Some(HandleKind::Job)),
            "validated Job Handle must remain reserved until removal"
        );
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
        let mut keys = self.keys.borrow_mut();
        if keys.kind(worker_key) == Some(HandleKind::Worker) {
            keys.remove(worker_key);
        }
    }

    fn c_buffer(&self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.key(handle, HandleKind::CBuffer)
    }

    fn remove_c_buffer(&mut self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.remove(handle, HandleKind::CBuffer)
    }

    #[cfg(windows)]
    fn windows_watcher_buffer(&self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.key(handle, HandleKind::WindowsWatcherBuffer)
    }

    #[cfg(windows)]
    fn remove_windows_watcher_buffer(&mut self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.remove(handle, HandleKind::WindowsWatcherBuffer)
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

    fn process_env_builder(&self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.key(handle, HandleKind::ProcessEnvBuilder)
    }

    fn remove_process_env_builder(&mut self, handle: HostHandle) -> AsyncHostResult<HandleKey> {
        self.remove(handle, HandleKind::ProcessEnvBuilder)
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

    fn key(&self, handle: HostHandle, expected: HandleKind) -> AsyncHostResult<HandleKey> {
        self.keys
            .borrow()
            .key(handle, expected)
            .ok_or(AsyncHostError::Badf)
    }

    fn remove(&mut self, handle: HostHandle, expected: HandleKind) -> AsyncHostResult<HandleKey> {
        let key = self.key(handle, expected)?;
        self.keys
            .borrow_mut()
            .remove(key)
            .ok_or(AsyncHostError::Badf)?;
        Ok(key)
    }

    fn kind(&self, key: HandleKey) -> Option<HandleKind> {
        self.keys.borrow().kind(key)
    }
}

impl ResourceTable for HandleTable {
    fn insert_file(&mut self, file: RawFd) -> AsyncHostResult<u64> {
        Ok(self.insert_resource(Resource::new(file)))
    }
}

// Jobs are one-shot work items with result handles. Once a Job leaves this
// table for synchronous execution or worker submission, its slot stays reserved
// until the Job returns or the guest detaches it with free_job.
enum HostJobState {
    Ready(Job),
    Reserved,
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

    fn ready_job_mut(&mut self, key: HandleKey) -> AsyncHostResult<&mut Job> {
        match self.jobs.get_mut(key) {
            Some(HostJobState::Ready(job)) => Ok(job),
            _ => Err(AsyncHostError::Badf),
        }
    }

    fn take_ready_job(&mut self, key: HandleKey) -> AsyncHostResult<Job> {
        let slot = self.jobs.get_mut(key).ok_or(AsyncHostError::Badf)?;
        match std::mem::replace(slot, HostJobState::Reserved) {
            HostJobState::Ready(job) => Ok(job),
            other => {
                *slot = other;
                Err(AsyncHostError::Badf)
            }
        }
    }

    fn restore_job(&mut self, key: HandleKey, job: Job) -> Option<Job> {
        match self.jobs.get_mut(key) {
            Some(slot @ HostJobState::Reserved) => {
                *slot = HostJobState::ResultReady(job);
                None
            }
            _ => Some(job),
        }
    }

    fn restore_unrun_job(&mut self, key: HandleKey, job: Job) -> Option<Job> {
        match self.jobs.get_mut(key) {
            Some(slot @ HostJobState::Reserved) => {
                *slot = HostJobState::Ready(job);
                None
            }
            _ => Some(job),
        }
    }

    fn take_for_free(&mut self, key: HandleKey) -> AsyncHostResult<Option<Job>> {
        match self.jobs.remove(key) {
            Some(HostJobState::Ready(job) | HostJobState::ResultReady(job)) => Ok(Some(job)),
            // The worker owns an in-flight Job. Removing its reservation
            // detaches the guest handle; completion will discard the result.
            Some(HostJobState::Reserved) => Ok(None),
            None => Err(AsyncHostError::Badf),
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
    fn has_pending_io_for_resource(&self, file: &Resource) -> bool {
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
    #[cfg(unix)]
    registered_fds: HashSet<RawFd>,
    #[cfg(unix)]
    completion_notifier: Option<Arc<ThreadPoolCompletionNotifier>>,
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
    ReadDirChanges,
}

#[cfg(windows)]
impl HostIoKind {
    fn resource_ref(
        self,
        handles: &HandleTable,
        handle: HostHandle,
    ) -> AsyncHostResult<&ResourceRef> {
        let resource = handles.resource_ref(handle)?;
        let class = resource.resource_class();
        match self {
            Self::File | Self::ReadDirChanges if class == ResourceClass::File => Ok(resource),
            Self::Socket if class.is_socket() => Ok(resource),
            Self::SocketWithAddr if class == ResourceClass::UdpSocket => Ok(resource),
            _ => Err(AsyncHostError::Inval),
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
    // Directory change notification writes into a stable host watcher buffer.
    // Lease it while the OVERLAPPED operation is pending, then restore the
    // bytes and selected event layout before the guest parses the batch.
    read_dir_changes_buffer: Option<WindowsWatcherBufferLease>,
    read_dir_changes_len: usize,
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
            kind: HostIoKind::File,
            event,
            buffer,
            read_dir_changes_buffer: None,
            read_dir_changes_len: 0,
            socket_flags: 0,
            addr_buffer: Vec::new(),
            addr_len: 0,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            pending_resource: None,
            extra_pending_close_resource: None,
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
            kind: HostIoKind::Socket,
            event,
            buffer,
            read_dir_changes_buffer: None,
            read_dir_changes_len: 0,
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
            kind: HostIoKind::SocketWithAddr,
            event,
            buffer,
            read_dir_changes_buffer: None,
            read_dir_changes_len: 0,
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
            read_dir_changes_buffer: None,
            read_dir_changes_len: 0,
            socket_flags: 0,
            addr_buffer,
            addr_len: 0,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            pending_resource: None,
            extra_pending_close_resource: None,
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
            kind: HostIoKind::Accept,
            event: IO_RESULT_READ_EVENT,
            buffer: Vec::new(),
            read_dir_changes_buffer: None,
            read_dir_changes_len: 0,
            socket_flags: 0,
            addr_buffer: Vec::new(),
            addr_len: i32::try_from(addr_len).map_err(|_| AsyncHostError::Fault)?,
            accept_buffer: vec![0; accept_buffer_len],
            accept_bytes_received: 0,
            pending_resource: None,
            extra_pending_close_resource: None,
        })
    }

    fn for_read_dir_changes(buffer: WindowsWatcherBufferLease) -> Self {
        let len = buffer.buffer().capacity();
        Self {
            overlapped: Self::zeroed_overlapped(),
            kind: HostIoKind::ReadDirChanges,
            event: IO_RESULT_READ_EVENT,
            buffer: Vec::new(),
            read_dir_changes_buffer: Some(buffer),
            read_dir_changes_len: len,
            socket_flags: 0,
            addr_buffer: Vec::new(),
            addr_len: 0,
            accept_buffer: Vec::new(),
            accept_bytes_received: 0,
            pending_resource: None,
            extra_pending_close_resource: None,
        }
    }

    fn overlapped_ptr(&mut self) -> *mut windows_sys::Win32::System::IO::OVERLAPPED {
        &mut self.overlapped
    }

    fn reset_overlapped(&mut self) {
        self.overlapped = Self::zeroed_overlapped();
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
        self.pending_resource
            .as_ref()
            .map(|file| file.raw_identity())
    }

    fn is_pending(&self) -> bool {
        self.pending_resource.is_some() || self.extra_pending_close_resource.is_some()
    }

    fn protects_pending_resource(&self, file: &Resource) -> bool {
        self.pending_resource
            .as_ref()
            .is_some_and(|pending| std::ptr::eq(pending.as_ref(), file))
            || self
                .extra_pending_close_resource
                .as_ref()
                .is_some_and(|pending| std::ptr::eq(pending.as_ref(), file))
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

    fn validate_pending_resource(&self, file: &Resource) -> AsyncHostResult<()> {
        // The import boundary may receive malformed/stale fd handles. Validate
        // before asserting the internal "pending operation uses submitter fd"
        // invariant so debug builds do not panic on bad guest input.
        if let Some(pending_resource) = &self.pending_resource
            && !std::ptr::eq(pending_resource.as_ref(), file)
        {
            return Err(AsyncHostError::Badf);
        }
        debug_assert!(
            match &self.pending_resource {
                Some(pending_resource) => std::ptr::eq(pending_resource.as_ref(), file),
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
    // V8 enters this host synchronously on one thread. Cell and RefCell encode
    // that ownership; worker threads own their Jobs and return them through the
    // completion channel instead of sharing the host's tables.
    policy: Arc<Policy>,
    environment: Arc<Env>,
    temp_dir: TempDir,
    network: HostNetwork,
    errno: Cell<i32>,
    addr_infos: RefCell<SecondaryMap<HandleKey, HostAddrInfo>>,
    c_buffers: RefCell<SecondaryMap<HandleKey, HostCBuffer>>,
    #[cfg(windows)]
    windows_watcher_buffers: RefCell<SecondaryMap<HandleKey, HostWindowsWatcherBuffer>>,
    #[cfg(unix)]
    process_argvs: RefCell<SecondaryMap<HandleKey, HostProcessArgv>>,
    process_envs: RefCell<SecondaryMap<HandleKey, HostProcessEnv>>,
    process_env_builders: RefCell<SecondaryMap<HandleKey, HostProcessEnvBuilder>>,
    process: HostProcess,
    #[cfg(windows)]
    io_results: RefCell<IoResultTable>,
    jobs: RefCell<JobTable>,
    workers: InstanceWorkers,
    polls: RefCell<PollTable>,
    thread_pool_completions: RefCell<ThreadPoolCompletions>,
    handles: RefCell<HandleTable>,
    tls_connections: RefCell<SecondaryMap<HandleKey, tls::TlsHandle>>,
    tls_error: RefCell<Option<String>>,
}

#[cfg(test)]
impl Default for AsyncHost {
    fn default() -> Self {
        Self::new(Arc::new(Policy::allow_all()))
    }
}

impl AsyncHost {
    #[cfg(test)]
    pub(crate) fn new(policy: Arc<Policy>) -> Self {
        let environment = Arc::new(
            policy
                .realize_env()
                .expect("construct test Host environment"),
        );
        Self::with_keys(
            policy,
            environment,
            Rc::new(RefCell::new(HostKeys::default())),
            WorkingDirectory::Ambient,
        )
    }

    pub(crate) fn with_keys(
        policy: Arc<Policy>,
        environment: Arc<Env>,
        keys: Rc<RefCell<HostKeys>>,
        working_directory: WorkingDirectory,
    ) -> Self {
        let process = HostProcess::new(Arc::clone(&policy), working_directory);
        let network = HostNetwork::new(Arc::clone(&policy));
        let temp_dir = TempDir::new(Arc::clone(&environment), &policy);
        Self {
            policy,
            environment,
            temp_dir,
            network,
            errno: Cell::new(0),
            addr_infos: RefCell::new(SecondaryMap::new()),
            c_buffers: RefCell::new(SecondaryMap::new()),
            #[cfg(windows)]
            windows_watcher_buffers: RefCell::new(SecondaryMap::new()),
            #[cfg(unix)]
            process_argvs: RefCell::new(SecondaryMap::new()),
            process_envs: RefCell::new(SecondaryMap::new()),
            process_env_builders: RefCell::new(SecondaryMap::new()),
            process,
            #[cfg(windows)]
            io_results: RefCell::new(IoResultTable::default()),
            jobs: RefCell::new(JobTable::default()),
            workers: InstanceWorkers::new(),
            polls: RefCell::new(PollTable::default()),
            thread_pool_completions: RefCell::new(ThreadPoolCompletions::default()),
            handles: RefCell::new(HandleTable::with_keys(keys)),
            tls_connections: RefCell::new(SecondaryMap::new()),
            tls_error: RefCell::new(None),
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
        self.errno.get()
    }

    pub(crate) fn set_errno(&self, errno: i32) {
        self.errno.set(errno);
    }

    pub(crate) fn record_error(&self, error: AsyncHostError) -> i32 {
        let errno = error.errno();
        self.set_errno(errno);
        errno
    }

    fn restore_job(&self, key: HandleKey, mut job: Job) -> AsyncHostResult<()> {
        self.restore_c_buffer_lease(&mut job);
        let result = self.jobs.borrow_mut().restore_job(key, job);
        if let Some(job) = result {
            Self::revoke_unclaimed_spawn(&self.process, &job);
            Err(AsyncHostError::Badf)
        } else {
            Ok(())
        }
    }

    fn take_worker_job(
        &self,
        completion_id: WorkerCompletionId,
        key: HandleKey,
    ) -> AsyncHostResult<HostWorkerJob> {
        let job = self.jobs.borrow_mut().take_ready_job(key)?;
        Ok(HostWorkerJob::new(completion_id, key, job))
    }

    fn restore_unrun_worker_job(&self, worker_job: HostWorkerJob) {
        let discarded = self
            .jobs
            .borrow_mut()
            .restore_unrun_job(worker_job.job_key, worker_job.job);
        if let Some(mut job) = discarded {
            self.restore_c_buffer_lease(&mut job);
        }
    }

    fn restore_completed_worker_jobs(&self) {
        while let Ok(worker_job) = self.workers.try_recv_completed() {
            let _ = self.restore_job(worker_job.job_key, worker_job.job);
        }
    }

    fn restore_c_buffer_lease(&self, job: &mut Job) {
        let Ok(job) = job.filesystem_mut() else {
            return;
        };
        let Some(buffer) = job.take_c_buffer_lease() else {
            return;
        };
        self.restore_c_buffer(buffer);
    }

    fn restore_c_buffer(&self, buffer: CBufferLease) {
        let (key, buffer) = buffer.into_parts();
        if let Some(entry) = self.c_buffers.borrow_mut().get_mut(key)
            && matches!(entry, HostCBuffer::Leased)
        {
            *entry = HostCBuffer::Available(buffer);
        }
    }

    #[cfg(windows)]
    fn restore_windows_watcher_buffer(&self, buffer: WindowsWatcherBufferLease) {
        let (key, buffer) = buffer.into_parts();
        if let Some(entry) = self.windows_watcher_buffers.borrow_mut().get_mut(key)
            && matches!(entry, HostWindowsWatcherBuffer::Leased)
        {
            *entry = HostWindowsWatcherBuffer::Available(buffer);
        }
    }

    fn revoke_unclaimed_spawn(process: &HostProcess, job: &Job) {
        if let Ok(process_job) = job.process() {
            process.revoke_unclaimed_spawn(process_job, job.ret(), job.err());
        }
    }

    fn publish_open_job_result(&self, key: HandleKey) -> AsyncHostResult<HostHandle> {
        let mut jobs = self.jobs.borrow_mut();
        let placeholder = ResourcePublication::Published(self.invalid_fd());
        let file = {
            let job = jobs.visible_job_mut(key)?;
            let result = job.filesystem_mut()?.open_result_mut()?;
            match std::mem::replace(&mut result.resource, placeholder) {
                ResourcePublication::Published(fd) => {
                    result.resource = ResourcePublication::Published(fd);
                    return Ok(fd);
                }
                ResourcePublication::Unpublished(file) => file,
            }
        };

        let fd = self.handles.borrow_mut().insert_resource(file);
        let job = jobs.visible_job_mut(key)?;
        let result = job.filesystem_mut()?.open_result_mut()?;
        result.resource = ResourcePublication::Published(fd);
        result.published_resource_handle()
    }

    /// Describe live async payloads without inspecting another domain's keys.
    pub(crate) fn leak_summary(&self) -> Option<String> {
        {
            let mut leaks = Vec::new();

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
            {
                let addr_infos = self.addr_infos.borrow();
                if !addr_infos.is_empty() {
                    leaks.push(format!("addr_infos={}", addr_infos.len()));
                }
            }
            #[cfg(windows)]
            {
                let io_results = self.io_results.borrow();
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
                let jobs = self.jobs.borrow();
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
            #[cfg(unix)]
            {
                let process_argvs = self.process_argvs.borrow();
                if !process_argvs.is_empty() {
                    leaks.push(format!("process_argvs={}", process_argvs.len()));
                }
            }
            {
                let process_envs = self.process_envs.borrow();
                if !process_envs.is_empty() {
                    leaks.push(format!("process_envs={}", process_envs.len()));
                }
            }
            {
                let process_env_builders = self.process_env_builders.borrow();
                if !process_env_builders.is_empty() {
                    leaks.push(format!(
                        "process_env_builders={}",
                        process_env_builders.len()
                    ));
                }
            }
            {
                let completions = self.thread_pool_completions.borrow();
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
                let tls_connections = self.tls_connections.borrow();
                if !tls_connections.is_empty() {
                    leaks.push(format!("tls_connections={}", tls_connections.len()));
                }
            }
            {
                let handles = self.handles.borrow();
                let leaked_resources = handles.resource_count_excluding_reserved();
                let invalid_resource_is_valid = handles
                    .kind(handles.invalid_resource)
                    .is_some_and(|kind| kind == HandleKind::Resource)
                    && handles
                        .resources
                        .get(handles.invalid_resource)
                        .is_some_and(|resource| resource.is_invalid());
                if !invalid_resource_is_valid {
                    leaks.push("invalid_resource=invalid".to_string());
                }
                if leaked_resources != 0 {
                    leaks.push(format!("resources={leaked_resources}"));
                }
            }
            if self.workers.len() != 0 {
                leaks.push(format!("workers={}", self.workers.len()));
            }
            (!leaks.is_empty()).then(|| leaks.join(", "))
        }
    }

    pub(crate) fn poll_create(&self) -> AsyncHostResult<u64> {
        let instance = poll::poll_create()?;
        let key = self.handles.borrow_mut().insert(HandleKind::Poll);
        self.polls.borrow_mut().polls.insert(
            key,
            HostPoll {
                instance,
                #[cfg(unix)]
                registered_fds: HashSet::new(),
                #[cfg(unix)]
                completion_notifier: None,
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
            let mut completions = self.thread_pool_completions.borrow_mut();
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
            let mut completions = self.thread_pool_completions.borrow_mut();
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

    // Native's tri-state contract distinguishes a registered resource (1),
    // an unsupported-but-valid resource (0), and an error (-1 at the ABI).
    pub(crate) fn poll_register(
        &self,
        poll_handle: u64,
        fd_handle: HostHandle,
        read_only: bool,
    ) -> AsyncHostResult<i32> {
        let handles = self.handles.borrow();
        let resource = handles.resource(fd_handle)?;
        #[cfg(unix)]
        let raw_fd = resource.as_file()?.as_raw_fd();
        let poll_key = handles.poll(poll_handle)?;
        let mut polls = self.polls.borrow_mut();
        let poll = polls.polls.get_mut(poll_key).ok_or(AsyncHostError::Badf)?;
        #[cfg(unix)]
        let registered = poll::poll_register(&poll.instance, raw_fd, read_only, fd_handle)?;
        #[cfg(windows)]
        let registered = if resource.resource_class().is_socket() {
            poll::poll_register_socket(&poll.instance, resource.as_socket()?, read_only, fd_handle)?
        } else {
            poll::poll_register_file(
                &poll.instance,
                resource.as_file()?.as_raw_handle(),
                read_only,
                fd_handle,
            )?
        };
        #[cfg(unix)]
        if registered > 0 {
            poll.registered_fds.insert(raw_fd);
        }
        Ok(registered)
    }

    pub(crate) fn poll_register_legacy(
        &self,
        poll_handle: u64,
        fd_handle: HostHandle,
        read_only: bool,
    ) -> AsyncHostResult<()> {
        // The old import predates tri-state registration. Preserve its
        // platform syscall behavior as well as its 0/-1 ABI result.
        let handles = self.handles.borrow();
        let resource = handles.resource(fd_handle)?;
        #[cfg(unix)]
        let raw_fd = resource.as_file()?.as_raw_fd();
        let poll_key = handles.poll(poll_handle)?;
        let mut polls = self.polls.borrow_mut();
        let poll = polls.polls.get_mut(poll_key).ok_or(AsyncHostError::Badf)?;
        #[cfg(unix)]
        poll::poll_register_legacy(&poll.instance, raw_fd, read_only, fd_handle)?;
        #[cfg(windows)]
        if resource.resource_class().is_socket() {
            poll::poll_register_socket(
                &poll.instance,
                resource.as_socket()?,
                read_only,
                fd_handle,
            )?;
        } else {
            poll::poll_register_file(
                &poll.instance,
                resource.as_file()?.as_raw_handle(),
                read_only,
                fd_handle,
            )?;
        }
        #[cfg(unix)]
        poll.registered_fds.insert(raw_fd);
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
        let mut polls = self.polls.borrow_mut();
        let result = {
            let poll = polls.polls.get_mut(poll_key).ok_or(AsyncHostError::Badf)?;
            poll::poll_wait(&mut poll.instance, timeout_ms)?
        };
        polls.current_event_poll = Some(poll_key);
        drop(polls);
        self.restore_completed_worker_jobs();
        Ok(result)
    }

    pub(crate) fn poll_get_event(&self, poll_handle: u64, index: u32) -> AsyncHostResult<u64> {
        let poll_key = self.handles.borrow().poll(poll_handle)?;
        let polls = self.polls.borrow();
        if polls.current_event_poll != Some(poll_key) {
            return Err(AsyncHostError::Badf);
        }
        let poll = polls.polls.get(poll_key).ok_or(AsyncHostError::Badf)?;
        poll::event_list_get(&poll.instance, index)?;
        Ok(u64::from(index))
    }

    fn with_event<T>(
        &self,
        event_handle: u64,
        f: impl FnOnce(&poll::PollEvent) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let index = u32::try_from(event_handle).map_err(|_| AsyncHostError::Fault)?;
        let polls = self.polls.borrow();
        let poll_key = polls.current_event_poll.ok_or(AsyncHostError::Badf)?;
        let poll = polls.polls.get(poll_key).ok_or(AsyncHostError::Badf)?;
        let poll_event = poll::event_list_get(&poll.instance, index)?;
        f(poll_event)
    }

    pub(crate) fn poll_event_fd(&self, event_handle: u64) -> AsyncHostResult<HostHandle> {
        self.with_event(event_handle, |event| Ok(poll::event_get_fd(event)))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn poll_event_pid(&self, event_handle: u64) -> AsyncHostResult<i32> {
        self.with_event(event_handle, |event| Ok(poll::event_get_pid(event)))
    }

    #[cfg(unix)]
    pub(crate) fn poll_event_events(&self, event_handle: u64) -> AsyncHostResult<i32> {
        self.with_event(event_handle, |event| Ok(poll::event_get_events(event)))
    }

    #[cfg(windows)]
    pub(crate) fn poll_event_io_result(&self, event_handle: u64) -> AsyncHostResult<u64> {
        let overlapped = self.with_event(event_handle, |event| {
            Ok(OverlappedAddr::from_ptr(poll::event_get_io_result(event)))
        })?;
        let handle = {
            let io_results = self.io_results.borrow();
            io_results
                .io_results_by_overlapped
                .get(&overlapped)
                .copied()
                .ok_or(AsyncHostError::Badf)?
        };
        let key = self.handles.borrow().io_result(handle)?;
        let mut io_results = self.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(key)
            .ok_or(AsyncHostError::Badf)?;
        result.clear_pending();
        Ok(handle)
    }

    #[cfg(windows)]
    pub(crate) fn poll_event_bytes_transferred(&self, event_handle: u64) -> AsyncHostResult<u32> {
        self.with_event(event_handle, |event| {
            Ok(poll::event_get_bytes_transferred(event))
        })
    }

    pub(crate) fn init_thread_pool(&self, poll_handle: u64) -> AsyncHostResult<HostHandle> {
        let poll_key = self.handles.borrow().poll(poll_handle)?;
        #[cfg(unix)]
        if self.thread_pool_completions.borrow().source.is_some() {
            return Err(AsyncHostError::Inval);
        }
        #[cfg(windows)]
        if self.thread_pool_completions.borrow().target.is_some() {
            return Err(AsyncHostError::Inval);
        }
        #[cfg(unix)]
        {
            let old_signal_mask = crate::async_sys::signal::init_thread_pool_signal_mask()?;
            let signal_mask_guard = ThreadPoolSignalMaskGuard::new(old_signal_mask);
            let mut polls = self.polls.borrow_mut();
            let poll = polls.polls.get_mut(poll_key).ok_or(AsyncHostError::Badf)?;
            let (completion_notifier, event_fd) = ThreadPoolCompletionNotifier::new()?;
            let completion_notifier = Arc::new(completion_notifier);
            let source = self
                .handles
                .borrow_mut()
                .insert_resource(Resource::new(event_fd));
            if let Err(error) = poll::poll_register(&poll.instance, event_fd, true, source) {
                let _ = self.handles.borrow_mut().remove_resource(source);
                return Err(error);
            }
            let source = {
                let mut completions = self.thread_pool_completions.borrow_mut();
                if completions.source.is_some() {
                    drop(completions);
                    let _ = poll::poll_unregister(&poll.instance, event_fd);
                    let _ = self.handles.borrow_mut().remove_resource(source);
                    return Err(AsyncHostError::Inval);
                }
                // Publish the poll-side mapping before exposing the notifier:
                // workers can notify as soon as completions.notifier is visible.
                poll.registered_fds.insert(event_fd);
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
            let mut completions = self.thread_pool_completions.borrow_mut();
            if completions.target.is_some() {
                return Err(AsyncHostError::Inval);
            }
            completions.target = Some(ThreadPoolCompletionTarget {
                poll: poll_key,
                port: completion_port,
            });
            raw_fd_to_guest(windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE)
        }
    }

    pub(crate) fn destroy_thread_pool(&self) {
        for worker in self.workers.destroy() {
            self.handles.borrow_mut().remove_worker_key(worker.key);
            if let Some(unrun_job) = worker.unrun_job {
                self.restore_unrun_worker_job(unrun_job);
            }
        }
        self.restore_completed_worker_jobs();
        #[cfg(unix)]
        {
            let (completion_source, old_signal_mask) = {
                let mut completions = self.thread_pool_completions.borrow_mut();
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
                    if poll.registered_fds.contains(&raw_fd) {
                        let _ = poll::poll_unregister(&poll.instance, raw_fd);
                    }
                    poll.registered_fds.remove(&raw_fd);
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
            self.thread_pool_completions.borrow_mut().target = None;
        }
    }

    pub(crate) fn insert_c_buffer(&self, buffer: Box<[u8]>) -> u64 {
        let key = self.handles.borrow_mut().insert(HandleKind::CBuffer);
        self.c_buffers
            .borrow_mut()
            .insert(key, HostCBuffer::Available(buffer));
        handle_from_key(key)
    }

    pub(crate) fn free_c_buffer(&self, handle: u64) -> AsyncHostResult<()> {
        if handle == INVALID_HOST_HANDLE {
            return Ok(());
        }
        let key = self.handles.borrow_mut().remove_c_buffer(handle)?;
        self.c_buffers
            .borrow_mut()
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
        let key = self.handles.borrow().c_buffer(handle)?;
        let buffers = self.c_buffers.borrow();
        match buffers.get(key).ok_or(AsyncHostError::Badf)? {
            HostCBuffer::Available(buffer) => f(buffer),
            HostCBuffer::Leased => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn with_c_buffer_mut<T>(
        &self,
        handle: u64,
        f: impl FnOnce(&mut [u8]) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let key = self.handles.borrow().c_buffer(handle)?;
        let mut buffers = self.c_buffers.borrow_mut();
        match buffers.get_mut(key).ok_or(AsyncHostError::Badf)? {
            HostCBuffer::Available(buffer) => f(buffer),
            HostCBuffer::Leased => Err(AsyncHostError::Badf),
        }
    }

    pub(crate) fn lease_c_buffer(&self, handle: u64) -> AsyncHostResult<CBufferLease> {
        if handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }
        let key = self.handles.borrow().c_buffer(handle)?;
        let mut buffers = self.c_buffers.borrow_mut();
        let entry = buffers.get_mut(key).ok_or(AsyncHostError::Badf)?;
        match std::mem::replace(entry, HostCBuffer::Leased) {
            HostCBuffer::Available(buffer) => Ok(CBufferLease::new(key, buffer)),
            HostCBuffer::Leased => {
                *entry = HostCBuffer::Leased;
                Err(AsyncHostError::Badf)
            }
        }
    }

    #[cfg(windows)]
    pub(crate) fn insert_windows_watcher_buffer(&self) -> u64 {
        let key = self
            .handles
            .borrow_mut()
            .insert(HandleKind::WindowsWatcherBuffer);
        self.windows_watcher_buffers.borrow_mut().insert(
            key,
            HostWindowsWatcherBuffer::Available(
                crate::async_sys::fs::watch_windows::EventBuffer::new(),
            ),
        );
        handle_from_key(key)
    }

    #[cfg(windows)]
    pub(crate) fn free_windows_watcher_buffer(&self, handle: u64) -> AsyncHostResult<()> {
        if handle == INVALID_HOST_HANDLE {
            return Ok(());
        }
        let key = self
            .handles
            .borrow_mut()
            .remove_windows_watcher_buffer(handle)?;
        self.windows_watcher_buffers
            .borrow_mut()
            .remove(key)
            .map(|_| ())
            .ok_or(AsyncHostError::Badf)
    }

    #[cfg(windows)]
    pub(crate) fn with_windows_watcher_buffer<T>(
        &self,
        handle: u64,
        f: impl FnOnce(&crate::async_sys::fs::watch_windows::EventBuffer) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let key = self.handles.borrow().windows_watcher_buffer(handle)?;
        let buffers = self.windows_watcher_buffers.borrow();
        match buffers.get(key).ok_or(AsyncHostError::Badf)? {
            HostWindowsWatcherBuffer::Available(buffer) => f(buffer),
            HostWindowsWatcherBuffer::Leased => Err(AsyncHostError::Badf),
        }
    }

    #[cfg(windows)]
    fn lease_windows_watcher_buffer(
        &self,
        handle: u64,
    ) -> AsyncHostResult<WindowsWatcherBufferLease> {
        if handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }
        let key = self.handles.borrow().windows_watcher_buffer(handle)?;
        let mut buffers = self.windows_watcher_buffers.borrow_mut();
        let entry = buffers.get_mut(key).ok_or(AsyncHostError::Badf)?;
        match std::mem::replace(entry, HostWindowsWatcherBuffer::Leased) {
            HostWindowsWatcherBuffer::Available(buffer) => {
                Ok(WindowsWatcherBufferLease::new(key, buffer))
            }
            HostWindowsWatcherBuffer::Leased => {
                *entry = HostWindowsWatcherBuffer::Leased;
                Err(AsyncHostError::Badf)
            }
        }
    }

    #[cfg(unix)]
    pub(crate) fn insert_process_argv(&self, len: u32) -> AsyncHostResult<u64> {
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        let key = self.handles.borrow_mut().insert(HandleKind::ProcessArgv);
        self.process_argvs.borrow_mut().insert(key, vec![None; len]);
        Ok(handle_from_key(key))
    }

    #[cfg(unix)]
    pub(crate) fn process_argv_add_entry(
        &self,
        handle: u64,
        index: u32,
        value: OsString,
    ) -> AsyncHostResult<()> {
        let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
        let key = self.process_argv(handle)?;
        let mut process_argvs = self.process_argvs.borrow_mut();
        let argv = process_argvs.get_mut(key).ok_or(AsyncHostError::Badf)?;
        let slot = argv.get_mut(index).ok_or(AsyncHostError::Fault)?;
        *slot = Some(value);
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn take_legacy_process_spawn_inputs(
        &self,
        argv_handle: u64,
        env_handle: u64,
        inherited_env_entry_count: u32,
    ) -> AsyncHostResult<(Vec<OsString>, Vec<OsString>)> {
        if argv_handle == INVALID_HOST_HANDLE || env_handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }
        let inherited_env_entry_count =
            usize::try_from(inherited_env_entry_count).map_err(|_| AsyncHostError::Inval)?;

        // Validate both buffers before consuming either handle. A malformed
        // spawn request must not partially transfer ownership.
        let mut handles = self.handles.borrow_mut();
        let argv_key = handles.process_argv(argv_handle)?;
        let env_key = handles.process_env(env_handle)?;
        let mut process_argvs = self.process_argvs.borrow_mut();
        let argv = process_argvs.get(argv_key).ok_or(AsyncHostError::Badf)?;
        let mut process_envs = self.process_envs.borrow_mut();
        let env = process_envs.get(env_key).ok_or(AsyncHostError::Badf)?;
        if inherited_env_entry_count > env.len()
            || argv.iter().any(Option::is_none)
            || env.iter().any(Option::is_none)
        {
            return Err(AsyncHostError::Inval);
        }

        handles.remove_process_argv(argv_handle)?;
        handles.remove_process_env(env_handle)?;
        let argv = process_argvs
            .remove(argv_key)
            .ok_or(AsyncHostError::Badf)?
            .into_iter()
            .map(Option::unwrap)
            .collect();
        let env = process_envs
            .remove(env_key)
            .ok_or(AsyncHostError::Badf)?
            .into_iter()
            .map(Option::unwrap)
            .collect();
        // The legacy ABI materializes inherited entries first and appends
        // extras. Convert that layout into the current builder model so both
        // ABIs use the same override and ordering semantics from here on.
        let env = HostProcessEnvBuilder::from_legacy_env(env, inherited_env_entry_count);
        let env = crate::async_sys::process::finish_process_env_builder(env);
        Ok((argv, env))
    }

    #[cfg(unix)]
    pub(crate) fn take_process_spawn_inputs(
        &self,
        argv_handle: u64,
        env_handle: u64,
    ) -> AsyncHostResult<(Vec<OsString>, Vec<OsString>)> {
        if argv_handle == INVALID_HOST_HANDLE || env_handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }

        let mut handles = self.handles.borrow_mut();
        let argv_key = handles.process_argv(argv_handle)?;
        let env_key = handles.process_env_builder(env_handle)?;
        let mut process_argvs = self.process_argvs.borrow_mut();
        let argv = process_argvs.get(argv_key).ok_or(AsyncHostError::Badf)?;
        let mut process_env_builders = self.process_env_builders.borrow_mut();
        if argv.iter().any(Option::is_none) || process_env_builders.get(env_key).is_none() {
            return Err(AsyncHostError::Inval);
        }

        handles.remove_process_argv(argv_handle)?;
        handles.remove_process_env_builder(env_handle)?;
        let argv = process_argvs
            .remove(argv_key)
            .ok_or(AsyncHostError::Badf)?
            .into_iter()
            .map(Option::unwrap)
            .collect();
        let env = process_env_builders
            .remove(env_key)
            .ok_or(AsyncHostError::Badf)?;
        let env = crate::async_sys::process::finish_process_env_builder(env);
        Ok((argv, env))
    }

    #[cfg(unix)]
    pub(crate) fn insert_process_env(&self, entries: Vec<Option<OsString>>) -> u64 {
        let key = self.handles.borrow_mut().insert(HandleKind::ProcessEnv);
        self.process_envs.borrow_mut().insert(
            key,
            entries
                .into_iter()
                .map(|entry| {
                    entry.map(crate::async_sys::process::LegacyProcessEnvEntry::Materialized)
                })
                .collect(),
        );
        handle_from_key(key)
    }

    #[cfg(windows)]
    pub(crate) fn insert_process_env(&self, env: Vec<u16>) -> u64 {
        let key = self.handles.borrow_mut().insert(HandleKind::ProcessEnv);
        self.process_envs.borrow_mut().insert(key, env);
        handle_from_key(key)
    }

    pub(crate) fn insert_process_env_builder(&self, inherited: Vec<OsString>) -> u64 {
        let key = self
            .handles
            .borrow_mut()
            .insert(HandleKind::ProcessEnvBuilder);
        self.process_env_builders
            .borrow_mut()
            .insert(key, HostProcessEnvBuilder::new(inherited));
        handle_from_key(key)
    }

    pub(crate) fn process_env_builder_add_entry(
        &self,
        handle: u64,
        key: OsString,
        value: OsString,
    ) -> AsyncHostResult<()> {
        let builder_key = self.handles.borrow().process_env_builder(handle)?;
        let mut builders = self.process_env_builders.borrow_mut();
        let builder = builders.get_mut(builder_key).ok_or(AsyncHostError::Badf)?;
        crate::async_sys::process::process_env_builder_add_entry(builder, key, value);
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn process_env_length(&self, handle: u64) -> AsyncHostResult<u32> {
        let key = self.process_env(handle)?;
        let process_envs = self.process_envs.borrow();
        let env = process_envs.get(key).ok_or(AsyncHostError::Badf)?;
        u32::try_from(env.len()).map_err(|_| AsyncHostError::Fault)
    }

    #[cfg(windows)]
    pub(crate) fn process_env_length(&self, handle: u64) -> AsyncHostResult<u32> {
        let key = self.process_env(handle)?;
        let process_envs = self.process_envs.borrow();
        let env = process_envs.get(key).ok_or(AsyncHostError::Badf)?;
        let len = env.len().checked_sub(1).ok_or(AsyncHostError::Fault)?;
        u32::try_from(len).map_err(|_| AsyncHostError::Fault)
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
        let dst_key = self.process_env(dst_handle)?;
        if self.process_envs.borrow().get(dst_key).is_none() {
            return Err(AsyncHostError::Badf);
        }
        let src = self.take_process_env_buffer(src_handle)?;
        // The source is the temporary snapshot returned by get_curr_env.
        // Consume it here so its lifetime does not depend on deprecated free_env.
        let mut process_envs = self.process_envs.borrow_mut();
        let dst = process_envs.get_mut(dst_key).ok_or(AsyncHostError::Badf)?;
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
        let dst_key = self.process_env(dst_handle)?;
        if self.process_envs.borrow().get(dst_key).is_none() {
            return Err(AsyncHostError::Badf);
        }
        let src = self.take_process_env_buffer(src_handle)?;
        // The source is the temporary snapshot returned by get_curr_env.
        // Consume it here so its lifetime does not depend on deprecated free_env.
        let mut process_envs = self.process_envs.borrow_mut();
        let dst = process_envs.get_mut(dst_key).ok_or(AsyncHostError::Badf)?;
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
        index: u32,
        key: OsString,
        value: OsString,
    ) -> AsyncHostResult<()> {
        let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
        let env_key = self.process_env(handle)?;
        let mut process_envs = self.process_envs.borrow_mut();
        let env = process_envs.get_mut(env_key).ok_or(AsyncHostError::Badf)?;
        let slot = env.get_mut(index).ok_or(AsyncHostError::Fault)?;
        *slot = Some(crate::async_sys::process::LegacyProcessEnvEntry::Added { key, value });
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn process_env_add_entry(
        &self,
        handle: u64,
        offset: u32,
        key: &[u16],
        value: &[u16],
    ) -> AsyncHostResult<()> {
        let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
        let handle = self.process_env(handle)?;
        let mut process_envs = self.process_envs.borrow_mut();
        let env = process_envs.get_mut(handle).ok_or(AsyncHostError::Badf)?;
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
        self.take_process_env_buffer(handle)
    }

    #[cfg(windows)]
    pub(crate) fn take_process_env_builder(&self, handle: u64) -> AsyncHostResult<Vec<u16>> {
        if handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }
        let key = self
            .handles
            .borrow_mut()
            .remove_process_env_builder(handle)?;
        self.process_env_builders
            .borrow_mut()
            .remove(key)
            .map(crate::async_sys::process::finish_process_env_builder)
            .ok_or(AsyncHostError::Badf)
    }

    fn take_process_env_buffer(&self, handle: u64) -> AsyncHostResult<HostProcessEnv> {
        if handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }
        let key = self.handles.borrow_mut().remove_process_env(handle)?;
        self.process_envs
            .borrow_mut()
            .remove(key)
            .ok_or(AsyncHostError::Badf)
    }

    #[cfg(unix)]
    fn process_argv(&self, handle: u64) -> AsyncHostResult<HandleKey> {
        if handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }
        self.handles.borrow().process_argv(handle)
    }

    fn process_env(&self, handle: u64) -> AsyncHostResult<HandleKey> {
        if handle == INVALID_HOST_HANDLE {
            return Err(AsyncHostError::Badf);
        }
        self.handles.borrow().process_env(handle)
    }

    pub(crate) fn insert_job(&self, job: impl Into<Job>) -> AsyncHostResult<u64> {
        let key = self.handles.borrow_mut().insert(HandleKind::Job);
        self.jobs.borrow_mut().insert_job(key, job.into());
        Ok(handle_from_key(key))
    }

    pub(crate) fn free_job(&self, handle: u64) -> AsyncHostResult<()> {
        let mut handles = self.handles.borrow_mut();
        let key = handles.job(handle)?;
        let job = self.jobs.borrow_mut().take_for_free(key)?;
        handles.remove_job_key(key);
        drop(handles);
        let Some(mut job) = job else {
            return Ok(());
        };
        self.restore_c_buffer_lease(&mut job);
        Self::revoke_unclaimed_spawn(&self.process, &job);

        // Native realpath frees its resolved path from the job finalizer.
        // After get_realpath_result exposes that path as a host c_buffer,
        // freeing the job must also release the c_buffer slot.
        if let Ok(job) = job.filesystem()
            && let Some(buffer_handle) = job.published_realpath_handle()
        {
            let _ = self.free_c_buffer(buffer_handle);
        }
        Ok(())
    }

    pub(crate) fn job_get_ret(&self, handle: u64) -> AsyncHostResult<i64> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        Ok(crate::async_sys::internal::event_loop::thread_pool::job_get_ret(job))
    }

    pub(crate) fn job_get_err(&self, handle: u64) -> AsyncHostResult<i32> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        Ok(crate::async_sys::internal::event_loop::thread_pool::job_get_err(job))
    }

    pub(crate) fn open_job_get_fd(&self, handle: u64) -> AsyncHostResult<HostHandle> {
        let key = self.handles.borrow().job(handle)?;
        self.publish_open_job_result(key)
    }

    pub(crate) fn open_job_get_kind(&self, handle: u64) -> AsyncHostResult<i32> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        job.filesystem()?.open_result()?.file_kind()
    }

    pub(crate) fn open_job_get_dev_id(&self, handle: u64) -> AsyncHostResult<u64> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        job.filesystem()?.open_result()?.device_id()
    }

    pub(crate) fn open_job_get_file_id(&self, handle: u64) -> AsyncHostResult<u64> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        job.filesystem()?.open_result()?.file_id()
    }

    pub(crate) fn get_file_size_result(&self, handle: u64) -> AsyncHostResult<i64> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        job.filesystem()?.file_size_result()
    }

    pub(crate) fn get_getaddrinfo_result(&self, handle: u64) -> AsyncHostResult<u64> {
        let addrs = {
            let key = self.handles.borrow().job(handle)?;
            let jobs = self.jobs.borrow();
            let job = jobs.visible_job(key)?;
            let JobPayload::Network(job) = job.payload() else {
                return Err(AsyncHostError::Badf);
            };
            self.network.getaddrinfo_result(job)?
        };
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
        let mut addr_infos = self.addr_infos.borrow_mut();
        for (key, addrinfo) in entries {
            addr_infos.insert(key, addrinfo);
        }
        Ok(next.unwrap_or(INVALID_HOST_HANDLE))
    }

    pub(crate) fn get_spawn_job_result_handle(&self, handle: u64) -> AsyncHostResult<HostHandle> {
        let key = self.handles.borrow().job(handle)?;
        let mut jobs = self.jobs.borrow_mut();
        let job = jobs.visible_job_mut(key)?;
        let Some(result) = job.process_mut()?.take_spawn_result()? else {
            let fd = self.invalid_fd();
            job.process_mut()?
                .set_spawn_result(ResourcePublication::Published(fd))?;
            return Ok(fd);
        };
        let resource = match result {
            ResourcePublication::Published(fd) => {
                job.process_mut()?
                    .set_spawn_result(ResourcePublication::Published(fd))?;
                return Ok(fd);
            }
            ResourcePublication::Unpublished(resource) => resource,
        };
        let process_pid = self.process.has_policy().then(|| job.ret() as i32);
        let fd = self.handles.borrow_mut().insert_resource(resource);
        if let Some(pid) = process_pid {
            self.process.track_process_handle(fd, pid);
        }
        job.process_mut()?
            .set_spawn_result(ResourcePublication::Published(fd))?;
        Ok(fd)
    }

    pub(crate) fn spawn_job_set_cwd(&self, handle: u64, cwd: OsString) -> AsyncHostResult<()> {
        let key = self.handles.borrow().job(handle)?;
        let mut jobs = self.jobs.borrow_mut();
        let job = jobs.ready_job_mut(key)?;
        job.process_mut()?.set_cwd(cwd)
    }

    #[cfg(windows)]
    pub(crate) fn spawn_job_set_no_console_window(&self, handle: u64) -> AsyncHostResult<()> {
        let key = self.handles.borrow().job(handle)?;
        let mut jobs = self.jobs.borrow_mut();
        let job = jobs.ready_job_mut(key)?;
        job.process_mut()?.set_no_console_window()
    }

    pub(crate) fn addrinfo_next(&self, handle: u64) -> AsyncHostResult<u64> {
        if handle == INVALID_HOST_HANDLE {
            return Ok(INVALID_HOST_HANDLE);
        }
        let key = self.handles.borrow().addrinfo(handle)?;
        let addr_infos = self.addr_infos.borrow();
        let addrinfo = addr_infos.get(key).ok_or(AsyncHostError::Badf)?;
        Ok(addrinfo.next.unwrap_or(INVALID_HOST_HANDLE))
    }

    pub(crate) fn addrinfo_addr(&self, handle: u64) -> AsyncHostResult<Box<[u8]>> {
        let key = self.handles.borrow().addrinfo(handle)?;
        let addr_infos = self.addr_infos.borrow();
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
            let mut addr_infos = self.addr_infos.borrow_mut();
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
                if self.io_results.borrow().has_pending_io_for_resource(file) {
                    return Err(AsyncHostError::Inval);
                }
            }
            handles.remove_resource(handle)?
        };
        self.untrack_process_handle(handle);
        #[cfg(windows)]
        drop(file);
        #[cfg(unix)]
        {
            let raw_fd = file.as_fd()?.as_raw_fd();
            let mut polls = self.polls.borrow_mut();
            for poll in polls.polls.values_mut() {
                if poll.registered_fds.remove(&raw_fd) {
                    let _ = poll::poll_unregister(&poll.instance, raw_fd);
                }
            }
            let (completion_source_closed, old_signal_mask) = {
                let mut completions = self.thread_pool_completions.borrow_mut();
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

    pub(crate) fn make_tcp_socket(&self, family: i32) -> AsyncHostResult<HostHandle> {
        let socket = self.network.make_tcp_socket(family)?;
        Ok(self.handles.borrow_mut().insert_resource(socket))
    }

    pub(crate) fn make_udp_socket(
        &self,
        family: i32,
        multicast: bool,
    ) -> AsyncHostResult<HostHandle> {
        let socket = self.network.make_udp_socket(family, multicast)?;
        Ok(self.handles.borrow_mut().insert_resource(socket))
    }

    pub(crate) fn bind(&self, handle: HostHandle, addr: &[u8]) -> AsyncHostResult<()> {
        self.with_resource(handle, |socket| self.network.bind(socket, addr))
    }

    pub(crate) fn listen(&self, handle: HostHandle) -> AsyncHostResult<()> {
        self.with_resource(handle, |socket| self.network.listen(socket))
    }

    pub(crate) fn connect_udp(&self, handle: HostHandle, addr: &[u8]) -> AsyncHostResult<()> {
        self.with_resource(handle, |socket| self.network.connect_udp(socket, addr))
    }

    #[cfg(unix)]
    pub(crate) fn connect_tcp(&self, handle: HostHandle, addr: &[u8]) -> AsyncHostResult<()> {
        self.with_resource(handle, |socket| self.network.connect_tcp(socket, addr))
    }

    #[cfg(unix)]
    pub(crate) fn recv_from(
        &self,
        handle: HostHandle,
        data: &mut [u8],
        addr: &mut [u8],
    ) -> AsyncHostResult<usize> {
        self.with_resource(handle, |socket| self.network.recv_from(socket, data, addr))
    }

    #[cfg(unix)]
    pub(crate) fn send_to(
        &self,
        handle: HostHandle,
        data: &[u8],
        addr: &[u8],
    ) -> AsyncHostResult<usize> {
        self.with_resource(handle, |socket| self.network.send_to(socket, data, addr))
    }

    #[cfg(unix)]
    pub(crate) fn accept(
        &self,
        handle: HostHandle,
        addr: &mut [u8],
    ) -> AsyncHostResult<HostHandle> {
        let socket = self.with_resource(handle, |socket| self.network.accept(socket, addr))?;
        Ok(self.handles.borrow_mut().insert_resource(socket))
    }

    pub(crate) fn make_bind_job(&self, socket: ResourceRef, addr: Vec<u8>) -> AsyncHostResult<u64> {
        let job = self
            .network
            .make_bind_job(socket, addr)
            .map(thread_pool::Job::from)
            .unwrap_or_else(|error| thread_pool::make_failed_job(error.errno()));
        self.insert_job(job)
    }

    pub(crate) fn make_getaddrinfo_job(&self, host: OsString) -> AsyncHostResult<u64> {
        let job = self
            .network
            .make_getaddrinfo_job(host)
            .map(thread_pool::Job::from)
            .unwrap_or_else(|error| thread_pool::make_failed_job(error.errno()));
        self.insert_job(job)
    }

    #[cfg(unix)]
    pub(crate) fn insert_file_resource(&self, raw_fd: RawFd) -> HostHandle {
        self.handles
            .borrow_mut()
            .insert_resource(Resource::new(raw_fd))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn kqueue_watcher_add_file(
        &self,
        kqueue_handle: HostHandle,
        file_handle: HostHandle,
        is_dir: bool,
    ) -> AsyncHostResult<()> {
        use std::os::fd::AsRawFd;

        let kqueue = self.acquire_resource(kqueue_handle)?;
        let file = self.acquire_resource(file_handle)?;
        Self::check_file_metadata_policy(&self.policy, Some(file.as_ref()))?;
        crate::async_sys::fs::watch_kqueue::add_file(
            kqueue.as_fd()?.as_raw_fd(),
            file.as_fd()?.as_raw_fd(),
            is_dir,
            file_handle,
        )
    }

    pub(crate) fn policy(&self) -> &Policy {
        &self.policy
    }

    pub(crate) fn environment(&self) -> &Env {
        &self.environment
    }

    pub(crate) fn temp_dir(&self) -> AsyncHostResult<OsString> {
        self.temp_dir.path()
    }

    #[cfg(test)]
    pub(crate) fn check_owned_child_pid(&self, pid: i32) -> AsyncHostResult<()> {
        self.process.check_owned_child_pid(pid)
    }

    pub(crate) fn with_owned_child_pid<T>(
        &self,
        pid: i32,
        f: impl FnOnce() -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        self.process.with_owned_child_pid(pid, f)
    }

    #[cfg(unix)]
    pub(crate) fn finish_owned_child<T>(
        &self,
        pid: i32,
        handle: Option<HostHandle>,
        f: impl FnOnce() -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        self.process.finish_owned_child(pid, handle, f)
    }

    #[cfg(windows)]
    pub(crate) fn finish_process_handle<T>(
        &self,
        pid: i32,
        handle: HostHandle,
        f: impl FnOnce() -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        self.process.finish_process_handle(pid, handle, f)
    }

    pub(crate) fn process_handle_pid(&self, handle: HostHandle) -> AsyncHostResult<Option<i32>> {
        if handle == INVALID_HOST_HANDLE || handle == self.invalid_fd() {
            return Ok(None);
        }
        self.process.process_handle_pid(handle)
    }

    #[cfg(test)]
    pub(crate) fn check_process_handle_pid(
        &self,
        handle: HostHandle,
        pid: i32,
    ) -> AsyncHostResult<()> {
        self.process.check_process_handle_pid(handle, pid)
    }

    fn untrack_process_handle(&self, handle: HostHandle) {
        self.process.untrack_process_handle(handle);
    }

    pub(crate) fn with_raw_resource_class<T>(
        &self,
        handle: HostHandle,
        class: ResourceClass,
        f: impl FnOnce(RawSocket) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        debug_assert!(class.is_socket());
        self.with_resource_of_class(handle, class, |file| {
            #[cfg(unix)]
            let socket = file.as_fd()?.as_raw_fd();
            #[cfg(windows)]
            let socket = file.as_socket()?.as_raw_socket();
            f(socket)
        })
    }

    pub(crate) fn with_raw_socket<T>(
        &self,
        handle: HostHandle,
        f: impl FnOnce(RawSocket) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let handles = self.handles.borrow();
        let file = handles.socket(handle)?;
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
        self.process.track_process_handle(handle, pid);
        handle
    }

    pub(crate) fn with_resource<T>(
        &self,
        handle: HostHandle,
        f: impl FnOnce(&Resource) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let handles = self.handles.borrow();
        f(handles.resource(handle)?)
    }

    pub(crate) fn with_resource_of_class<T>(
        &self,
        handle: HostHandle,
        class: ResourceClass,
        f: impl FnOnce(&Resource) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let handles = self.handles.borrow();
        f(handles.resource_of_class(handle, class)?)
    }

    pub(crate) fn acquire_resource(&self, handle: HostHandle) -> AsyncHostResult<ResourceRef> {
        self.handles.borrow().acquire_resource(handle)
    }

    pub(crate) fn acquire_resource_of_class(
        &self,
        handle: HostHandle,
        class: ResourceClass,
    ) -> AsyncHostResult<ResourceRef> {
        self.handles
            .borrow()
            .acquire_resource_of_class(handle, class)
    }

    pub(crate) fn acquire_socket_resource(
        &self,
        handle: HostHandle,
    ) -> AsyncHostResult<ResourceRef> {
        self.handles.borrow().acquire_socket(handle)
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
        self.with_resource(handle, |file| {
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
        })
    }

    pub(crate) fn set_cloexec(&self, handle: HostHandle) -> AsyncHostResult<()> {
        self.with_resource(handle, |file| {
            #[cfg(unix)]
            {
                crate::async_sys::internal::fd_util::stub::set_cloexec(file.as_file()?.as_raw_fd())
            }
            #[cfg(windows)]
            {
                let _ = file;
                Ok(())
            }
        })
    }

    #[cfg(unix)]
    pub(crate) fn read_fd(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: HostHandle,
        dst: u32,
        offset: u32,
        len: u32,
    ) -> AsyncHostResult<i32> {
        let dst = dst.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        let dst = memory.read_exact_mut(dst, len)?;
        self.with_resource(handle, |file| {
            crate::async_sys::internal::event_loop::io::read(file.as_file()?.as_raw_fd(), dst)
                .and_then(|ret| i32::try_from(ret).map_err(|_| AsyncHostError::Fault))
        })
    }

    #[cfg(unix)]
    pub(crate) fn write_fd(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: HostHandle,
        src: u32,
        offset: u32,
        len: u32,
    ) -> AsyncHostResult<i32> {
        let src = src.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        let src = memory.read_exact(src, len)?;
        self.with_resource(handle, |file| {
            crate::async_sys::internal::event_loop::io::write(file.as_file()?.as_raw_fd(), src)
                .and_then(|ret| i32::try_from(ret).map_err(|_| AsyncHostError::Fault))
        })
    }

    #[cfg(windows)]
    fn read_guest_slice(
        memory: &mut (impl GuestMemory + ?Sized),
        ptr: u32,
        offset: u32,
        len: u32,
    ) -> AsyncHostResult<Vec<u8>> {
        let ptr = ptr.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        Ok(memory.read_exact(ptr, len)?.to_vec())
    }

    #[cfg(windows)]
    pub(crate) fn make_file_read_io_result(&self, len: u32, position: i64) -> AsyncHostResult<u64> {
        self.insert_io_result(HostIoResult::for_file_read(len, position)?)
    }

    #[cfg(windows)]
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

    #[cfg(windows)]
    pub(crate) fn make_socket_read_io_result(&self, len: u32, flags: i32) -> AsyncHostResult<u64> {
        self.insert_io_result(HostIoResult::for_socket_read(len, flags)?)
    }

    #[cfg(windows)]
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

    #[cfg(windows)]
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

    #[cfg(windows)]
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

    #[cfg(windows)]
    pub(crate) fn make_connect_io_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        addr: u32,
        addr_len: u32,
    ) -> AsyncHostResult<u64> {
        let addr_buffer = memory.read_exact(addr, addr_len)?.to_vec();
        self.insert_io_result(HostIoResult::for_connect(addr_buffer))
    }

    #[cfg(windows)]
    pub(crate) fn make_accept_io_result(&self, addr_len: u32) -> AsyncHostResult<u64> {
        self.insert_io_result(HostIoResult::for_accept(addr_len)?)
    }

    #[cfg(windows)]
    pub(crate) fn make_read_dir_changes_io_result(
        &self,
        buffer_handle: u64,
    ) -> AsyncHostResult<u64> {
        let buffer = self.lease_windows_watcher_buffer(buffer_handle)?;
        self.insert_io_result(HostIoResult::for_read_dir_changes(buffer))
    }

    #[cfg(windows)]
    fn insert_io_result(&self, result: HostIoResult) -> AsyncHostResult<u64> {
        let key = self.handles.borrow_mut().insert(HandleKind::IoResult);
        let handle = handle_from_key(key);
        let overlapped = {
            let mut io_results = self.io_results.borrow_mut();
            io_results.io_results.insert(key, Box::new(result));
            io_results
                .io_results
                .get_mut(key)
                .ok_or(AsyncHostError::Badf)?
                .overlapped_addr()
        };
        self.io_results
            .borrow_mut()
            .io_results_by_overlapped
            .insert(overlapped, handle);
        Ok(handle)
    }

    #[cfg(windows)]
    pub(crate) fn free_io_result(&self, handle: u64) -> AsyncHostResult<()> {
        let mut handles = self.handles.borrow_mut();
        let key = handles.io_result(handle)?;
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
        handles.remove_io_result(handle)?;
        let overlapped = result.overlapped_addr();
        io_results.io_results_by_overlapped.remove(&overlapped);
        let buffer = result.read_dir_changes_buffer.take();
        drop(io_results);
        drop(handles);
        if let Some(buffer) = buffer {
            self.restore_windows_watcher_buffer(buffer);
        }
        Ok(())
    }

    #[cfg(windows)]
    pub(crate) fn io_result_get_event(&self, handle: u64) -> AsyncHostResult<i32> {
        let key = self.handles.borrow().io_result(handle)?;
        let io_results = self.io_results.borrow();
        let result = io_results.io_results.get(key).ok_or(AsyncHostError::Badf)?;
        Ok(result.event)
    }

    #[cfg(windows)]
    pub(crate) fn cancel_io_result(
        &self,
        result_handle: u64,
        fd_handle: HostHandle,
    ) -> AsyncHostResult<i32> {
        let handles = self.handles.borrow();
        let file = handles.resource(fd_handle)?;
        let result_key = handles.io_result(result_handle)?;
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

    #[cfg(windows)]
    pub(crate) fn io_result_get_status(
        &self,
        result_handle: u64,
        fd_handle: HostHandle,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::System::IO::GetOverlappedResult;

        let handles = self.handles.borrow();
        let file = handles.resource(fd_handle)?;
        let raw_handle = raw_overlapped_handle(file)?;
        let result_key = handles.io_result(result_handle)?;
        let mut io_results = self.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        result.validate_pending_resource(file)?;
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
            result.clear_pending();
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

    #[cfg(windows)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn io_result_copy_read(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        result_handle: u64,
        dst: u32,
        offset: u32,
        len: u32,
    ) -> AsyncHostResult<()> {
        let key = self.handles.borrow().io_result(result_handle)?;
        let io_results = self.io_results.borrow();
        let result = io_results.io_results.get(key).ok_or(AsyncHostError::Badf)?;
        result.copy_read_result(memory, dst, offset, len)
    }

    #[cfg(windows)]
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
        let key = self.handles.borrow().io_result(result_handle)?;
        let io_results = self.io_results.borrow();
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
        let mut io_results = self.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.is_pending() || result.event != IO_RESULT_READ_EVENT {
            return Err(AsyncHostError::Inval);
        }
        let file = result.kind.resource_ref(&handles, fd_handle)?;
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
            HostIoKind::Connect | HostIoKind::Accept | HostIoKind::ReadDirChanges => {
                return Err(AsyncHostError::Inval);
            }
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
            result.mark_pending(Arc::clone(file))?;
            Err(AsyncHostError::Native(errno))
        } else {
            Err(AsyncHostError::Native(errno))
        }
    }

    #[cfg(windows)]
    pub(crate) fn read_dir_changes_io_result(
        &self,
        fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        use crate::async_sys::fs::watch_windows::EventLayout;
        use windows_sys::Win32::Foundation::{ERROR_INVALID_FUNCTION, ERROR_IO_PENDING};
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
            FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, ReadDirectoryChangesW,
        };

        let handles = self.handles.borrow();
        let result_key = handles.io_result(result_handle)?;
        let mut io_results = self.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.kind != HostIoKind::ReadDirChanges || result.is_pending() {
            return Err(AsyncHostError::Inval);
        }
        let file = result.kind.resource_ref(&handles, fd_handle)?;
        let len = u32::try_from(result.read_dir_changes_len).map_err(|_| AsyncHostError::Fault)?;
        let buffer = result
            .read_dir_changes_buffer
            .as_mut()
            .ok_or(AsyncHostError::Inval)?
            .buffer_mut()
            .as_mut_slice()
            .as_mut_ptr();
        let notify_filter = FILE_NOTIFY_CHANGE_SIZE
            | FILE_NOTIFY_CHANGE_LAST_WRITE
            | FILE_NOTIFY_CHANGE_FILE_NAME
            | FILE_NOTIFY_CHANGE_DIR_NAME;
        let mut bytes_returned = 0;
        let extended = unsafe {
            crate::async_sys::fs::watch_windows::read_directory_changes_extended(
                file.as_file()?.as_raw_handle(),
                buffer.cast(),
                len,
                1,
                notify_filter,
                &mut bytes_returned,
                result.overlapped_ptr(),
            )
        };
        let (success, layout) = match extended {
            Some(success) if success != 0 => (success, EventLayout::Extended),
            Some(_) => {
                let error = last_native_error();
                if error
                    != AsyncHostError::Native(
                        i32::try_from(ERROR_INVALID_FUNCTION).expect("Windows error code fits i32"),
                    )
                {
                    return Err(error);
                }
                result.reset_overlapped();
                bytes_returned = 0;
                let success = unsafe {
                    ReadDirectoryChangesW(
                        file.as_file()?.as_raw_handle(),
                        buffer.cast(),
                        len,
                        1,
                        notify_filter,
                        &mut bytes_returned,
                        result.overlapped_ptr(),
                        None,
                    )
                };
                (success, EventLayout::Basic)
            }
            None => {
                let success = unsafe {
                    ReadDirectoryChangesW(
                        file.as_file()?.as_raw_handle(),
                        buffer.cast(),
                        len,
                        1,
                        notify_filter,
                        &mut bytes_returned,
                        result.overlapped_ptr(),
                        None,
                    )
                };
                (success, EventLayout::Basic)
            }
        };
        if success == 0 {
            return Err(last_native_error());
        }
        result
            .read_dir_changes_buffer
            .as_mut()
            .ok_or(AsyncHostError::Inval)?
            .buffer_mut()
            .begin_read(layout);

        // Directory change notification reports that the completion packet
        // was queued, not synchronous completion. Match native async by
        // exposing the operation as pending even though the call returned TRUE.
        result.mark_pending(Arc::clone(file))?;
        Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
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
        let mut io_results = self.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.is_pending() || result.event != IO_RESULT_WRITE_EVENT {
            return Err(AsyncHostError::Inval);
        }
        let file = result.kind.resource_ref(&handles, fd_handle)?;
        if result.kind == HostIoKind::SocketWithAddr {
            self.network.check_connect(&result.addr_buffer)?;
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
            HostIoKind::Connect | HostIoKind::Accept | HostIoKind::ReadDirChanges => {
                return Err(AsyncHostError::Inval);
            }
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
                result.mark_pending(Arc::clone(file))?;
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

        let handles = self.handles.borrow();
        let file = handles.resource_of_class(fd_handle, ResourceClass::TcpSocket)?;
        let raw_socket = file.as_socket()?.as_raw_socket();
        let result_key = handles.io_result(result_handle)?;
        let mut io_results = self.io_results.borrow_mut();
        let result = io_results
            .io_results
            .get_mut(result_key)
            .ok_or(AsyncHostError::Badf)?;
        if result.kind != HostIoKind::Connect || result.is_pending() {
            return Err(AsyncHostError::Inval);
        }
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
                result.mark_pending(
                    handles.acquire_resource_of_class(fd_handle, ResourceClass::TcpSocket)?,
                )?;
            }
            Err(AsyncHostError::Native(errno))
        }
    }

    #[cfg(windows)]
    pub(crate) fn setup_connected_socket(&self, fd_handle: HostHandle) -> AsyncHostResult<()> {
        use windows_sys::Win32::Networking::WinSock as ws;

        self.with_resource_of_class(fd_handle, ResourceClass::TcpSocket, |file| {
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
        })
    }

    #[cfg(windows)]
    pub(crate) fn accept_io_result(
        &self,
        server_fd_handle: HostHandle,
        conn_fd_handle: HostHandle,
        result_handle: u64,
    ) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Networking::WinSock as ws;

        let handles = self.handles.borrow();
        let server_file = handles.resource_of_class(server_fd_handle, ResourceClass::TcpSocket)?;
        let conn_file = handles.resource_of_class(conn_fd_handle, ResourceClass::TcpSocket)?;
        let server_socket = server_file.as_socket()?.as_raw_socket();
        let conn_socket = conn_file.as_socket()?.as_raw_socket();
        let result_key = handles.io_result(result_handle)?;
        let mut io_results = self.io_results.borrow_mut();
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
                result.mark_pending_with_close_guard(
                    handles
                        .acquire_resource_of_class(server_fd_handle, ResourceClass::TcpSocket)?,
                    handles.acquire_resource_of_class(conn_fd_handle, ResourceClass::TcpSocket)?,
                )?;
            }
            Err(AsyncHostError::Native(errno))
        }
    }

    #[cfg(windows)]
    pub(crate) fn get_accept_peer_addr(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        result_handle: u64,
        dst: u32,
        dst_len: u32,
    ) -> AsyncHostResult<()> {
        let key = self.handles.borrow().io_result(result_handle)?;
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

    #[cfg(windows)]
    pub(crate) fn setup_accepted_socket(
        &self,
        listen_fd_handle: HostHandle,
        accept_fd_handle: HostHandle,
    ) -> AsyncHostResult<()> {
        use windows_sys::Win32::Networking::WinSock as ws;

        let handles = self.handles.borrow();
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

    pub(crate) fn try_lock_file(&self, handle: HostHandle, exclusive: bool) -> AsyncHostResult<()> {
        self.with_resource_of_class(handle, ResourceClass::File, |file| {
            Self::check_file_lock_policy(&self.policy, Some(file), exclusive)?;
            crate::async_sys::fs::stub::try_lock_acquired_file(file, exclusive)
        })
    }

    pub(crate) fn unlock_file(&self, handle: HostHandle) -> AsyncHostResult<()> {
        self.with_resource_of_class(handle, ResourceClass::File, |file| {
            crate::async_sys::fs::stub::unlock_acquired_file(file)
        })
    }

    pub(crate) fn run_job(&self, handle: u64) -> AsyncHostResult<()> {
        let key = self.handles.borrow().job(handle)?;
        let mut job = self.jobs.borrow_mut().take_ready_job(key)?;
        Self::run_policy_checked_job(&self.policy, &self.process, &mut job);
        self.restore_job(key, job)
    }

    fn run_policy_checked_job(policy: &Policy, process: &HostProcess, job: &mut Job) {
        if let Err(error) = Self::check_job_policy(policy, process, job) {
            job.set_err(error.errno());
            return;
        }
        if let Ok(process_job) = job.process_mut()
            && let Err(error) = process.configure_job_for_execution(process_job)
        {
            job.set_err(error.errno());
            return;
        }
        thread_pool::run_host_job(job);
        if let Err(error) = Self::finish_process_job(process, job) {
            job.set_err(error.errno());
        }
    }

    fn check_job_policy(policy: &Policy, process: &HostProcess, job: &Job) -> AsyncHostResult<()> {
        match job.payload() {
            JobPayload::Filesystem(job) => job.check_policy(policy),
            JobPayload::Process(job) => process.check_job(job),
            _ => Ok(()),
        }
    }

    fn finish_process_job(process: &HostProcess, job: &Job) -> AsyncHostResult<()> {
        if let Ok(process_job) = job.process() {
            process.finish_job(process_job, job.ret(), job.err())?;
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn check_file_metadata_policy(policy: &Policy, file: Option<&Resource>) -> AsyncHostResult<()> {
        let file = file.ok_or(AsyncHostError::Badf)?;
        policy.stat_resource_path(file.policy_path())
    }

    fn check_file_lock_policy(
        policy: &Policy,
        file: Option<&Resource>,
        exclusive: bool,
    ) -> AsyncHostResult<()> {
        let file = file.ok_or(AsyncHostError::Badf)?;
        policy.lock_resource_path(file.policy_path(), exclusive)
    }

    pub(crate) fn spawn_worker(&self, completion_id: i32, job_handle: u64) -> AsyncHostResult<u64> {
        let completion_id = WorkerCompletionId::from_abi(completion_id);
        let job_key = self.handles.borrow().job(job_handle)?;
        #[cfg(unix)]
        let completion_notifier = self
            .thread_pool_completions
            .borrow()
            .notifier
            .clone()
            .ok_or(AsyncHostError::Badf)?;
        #[cfg(windows)]
        let completion_target = self
            .thread_pool_completions
            .borrow()
            .target
            .clone()
            .ok_or(AsyncHostError::Badf)?;

        let init_job = self.take_worker_job(completion_id, job_key)?;
        let worker = self.handles.borrow_mut().insert(HandleKind::Worker);
        #[cfg(unix)]
        {
            self.spawn_worker_thread(worker, init_job, move |completion_id| {
                let _ = completion_notifier.notify(completion_id.as_i32());
            })?;
        }
        #[cfg(windows)]
        {
            self.spawn_worker_thread(worker, init_job, move |completion_id| {
                let _ = poll::post_thread_pool_completion(
                    &completion_target.port,
                    completion_id.as_i32(),
                );
            })?;
        }
        Ok(handle_from_key(worker))
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
        let job = self.take_worker_job(completion_id, job_key)?;
        let replaced_job = self.workers.wake(worker_key, job)?;
        if let Some(replaced_job) = replaced_job {
            self.restore_unrun_worker_job(replaced_job);
        }
        Ok(())
    }

    pub(crate) fn worker_enter_idle(&self, worker_handle: u64) -> AsyncHostResult<()> {
        let worker_key = self.handles.borrow().worker(worker_handle)?;
        let replaced_job = self.workers.enter_idle(worker_key)?;
        if let Some(replaced_job) = replaced_job {
            self.restore_unrun_worker_job(replaced_job);
        }
        Ok(())
    }

    pub(crate) fn free_worker(&self, worker_handle: u64) -> AsyncHostResult<()> {
        let worker_key = self.handles.borrow().worker(worker_handle)?;
        let replaced_job = self.workers.free(worker_key)?;
        self.handles.borrow_mut().remove_worker(worker_handle)?;
        if let Some(replaced_job) = replaced_job {
            self.restore_unrun_worker_job(replaced_job);
        }
        self.restore_completed_worker_jobs();
        Ok(())
    }

    pub(crate) fn cancel_worker(&self, worker_handle: u64) -> AsyncHostResult<i32> {
        let worker_key = self.handles.borrow().worker(worker_handle)?;
        self.workers.cancel(worker_key)
    }

    #[cfg(unix)]
    pub(crate) fn thread_pool_child_signal_mask(&self) -> AsyncHostResult<libc::sigset_t> {
        self.thread_pool_completions
            .borrow()
            .old_signal_mask
            .ok_or(AsyncHostError::Badf)
    }

    #[cfg(windows)]
    pub(crate) fn thread_pool_completion_target(&self) -> AsyncHostResult<poll::CompletionPort> {
        self.thread_pool_completions
            .borrow()
            .target
            .as_ref()
            .map(|target| target.port.clone())
            .ok_or(AsyncHostError::Badf)
    }

    pub(crate) fn get_read_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: u64,
        dst: u32,
        offset: u32,
        len: u32,
    ) -> AsyncHostResult<()> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        let filesystem_job = job.filesystem()?;
        filesystem_job.copy_read_result(job.err(), memory, dst, offset, len)
    }

    pub(crate) fn get_file_time_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: u64,
        dst: u32,
    ) -> AsyncHostResult<()> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        let filesystem_job = job.filesystem()?;
        filesystem_job.copy_file_time_result(job.err(), memory, dst)
    }

    pub(crate) fn get_stat_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: u64,
        dst: u32,
        dst_len: u32,
    ) -> AsyncHostResult<()> {
        let key = self.handles.borrow().job(handle)?;
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        let job_error = job.err();
        if job_error != 0 {
            return Ok(());
        }
        let filesystem_job = job.filesystem()?;
        filesystem_job.copy_stat_result(job_error, memory, dst, dst_len)
    }

    pub(crate) fn get_realpath_result(&self, handle: u64) -> AsyncHostResult<u64> {
        let key = self.handles.borrow().job(handle)?;
        let mut jobs = self.jobs.borrow_mut();
        let job = jobs.visible_job_mut(key)?;
        // Keep the mutable job borrow through publication so its ownership of
        // the resulting c_buffer changes atomically on the V8 thread.
        let job = job.filesystem_mut()?;
        job.publish_realpath_result(|buffer| {
            let buffer_key = self.handles.borrow_mut().insert(HandleKind::CBuffer);
            self.c_buffers
                .borrow_mut()
                .insert(buffer_key, HostCBuffer::Available(buffer));
            handle_from_key(buffer_key)
        })
    }

    #[cfg(unix)]
    pub(crate) fn thread_pool_notifier(
        &self,
    ) -> AsyncHostResult<Arc<ThreadPoolCompletionNotifier>> {
        self.thread_pool_completions
            .borrow()
            .notifier
            .clone()
            .ok_or(AsyncHostError::Badf)
    }

    #[cfg(unix)]
    pub(crate) fn fetch_completion(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        source_fd: HostHandle,
        dst: u32,
        max_jobs: u32,
    ) -> AsyncHostResult<i32> {
        let (completion_notifier, completion_source) = {
            let completions = self.thread_pool_completions.borrow();
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
        let max_bytes_u32 = u32::try_from(max_bytes).map_err(|_| AsyncHostError::Fault)?;
        let completions = memory.read_exact_mut(dst, max_bytes_u32)?;
        let bytes = completion_notifier.fetch(completions)?;
        self.restore_completed_worker_jobs();
        debug_assert_eq!(bytes % std::mem::size_of::<i32>(), 0);
        let bytes_i32 = i32::try_from(bytes).map_err(|_| AsyncHostError::Fault)?;
        Ok(bytes_i32)
    }

    pub(crate) fn tls_take_error(&self, handle: HostHandle) -> AsyncHostResult<HostHandle> {
        let message = self.with_tls_handle_mut(handle, |handle| {
            Ok(match handle {
                tls::TlsHandle::Connection(tls) => tls
                    .take_error()
                    .unwrap_or_else(|| "unknown TLS error".to_string()),
                tls::TlsHandle::Empty(pending) => pending
                    .take_error()
                    .unwrap_or_else(|| "unknown TLS error".to_string()),
            })
        })?;
        Ok(self.insert_c_buffer(error_message_buffer(message)))
    }

    pub(crate) fn tls_take_global_error(&self) -> HostHandle {
        let message = self
            .tls_error
            .borrow_mut()
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
        self.with_tls_handle_mut(handle, |handle| match handle {
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
        })
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
            if let Err(error) = self.policy.open_path(path.as_os_str(), 0, 0, false) {
                return self.with_tls_pending_mut(handle, |pending| {
                    pending.set_error(format!("failed to access {label} file: {error:?}"))
                });
            }
        }
        self.with_tls_handle_mut(handle, |handle| match handle {
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
        })
    }

    pub(crate) fn tls_set_server_pfx(
        &self,
        handle: HostHandle,
        pfx_content: Vec<u8>,
    ) -> AsyncHostResult<i32> {
        self.with_tls_handle_mut(handle, |handle| match handle {
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
        })
    }

    fn insert_tls_handle(&self, handle: tls::TlsHandle) -> HostHandle {
        let key = self.handles.borrow_mut().insert(HandleKind::TlsConnection);
        self.tls_connections.borrow_mut().insert(key, handle);
        handle_from_key(key)
    }

    pub(crate) fn tls_free(&self, handle: HostHandle) -> AsyncHostResult<()> {
        if handle == INVALID_HOST_HANDLE {
            return Ok(());
        }
        let key = self.handles.borrow_mut().remove_tls_connection(handle)?;
        self.tls_connections
            .borrow_mut()
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

    pub(crate) fn tls_bytes_read(&self, handle: HostHandle) -> AsyncHostResult<u32> {
        let bytes = self.with_tls_connection_mut(handle, 0, |tls| tls.bytes_read())?;
        u32::try_from(bytes).map_err(|_| AsyncHostError::Fault)
    }

    pub(crate) fn tls_bytes_to_write(&self, handle: HostHandle) -> AsyncHostResult<u32> {
        let bytes = self.with_tls_connection_mut(handle, 0, |tls| tls.bytes_to_write())?;
        u32::try_from(bytes).map_err(|_| AsyncHostError::Fault)
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
        match self.with_tls_connection_mut(handle, Err(()), |tls| tls.peer_certificate())? {
            Ok(Some(buffer)) => Ok(self.insert_c_buffer(buffer.into_boxed_slice())),
            // The guest reserves the invalid handle for TLS errors and uses a
            // valid zero-length buffer to represent an absent certificate.
            Ok(None) => Ok(self.insert_c_buffer(Box::default())),
            Err(()) => Ok(INVALID_HOST_HANDLE),
        }
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
        self.with_tls_handle_mut(handle, |handle| match handle {
            tls::TlsHandle::Connection(connection) => Ok(f(connection)),
            tls::TlsHandle::Empty(pending) => {
                pending.set_error("TLS handle is not configured".to_string());
                Ok(unconfigured_value)
            }
        })
    }

    fn with_tls_pending_mut<T>(
        &self,
        handle: HostHandle,
        f: impl FnOnce(&mut tls::TlsPending) -> T,
    ) -> AsyncHostResult<T> {
        self.with_tls_handle_mut(handle, |handle| match handle {
            tls::TlsHandle::Empty(pending) => Ok(f(pending)),
            tls::TlsHandle::Connection(_) => Err(AsyncHostError::Inval),
        })
    }

    fn with_tls_handle_mut<T>(
        &self,
        handle: HostHandle,
        f: impl FnOnce(&mut tls::TlsHandle) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let key = self.handles.borrow().tls_connection(handle)?;
        let mut tls_connections = self.tls_connections.borrow_mut();
        let handle = tls_connections.get_mut(key).ok_or(AsyncHostError::Badf)?;
        f(handle)
    }

    fn spawn_worker_thread(
        &self,
        worker: HandleKey,
        init_job: HostWorkerJob,
        complete_job: impl FnMut(WorkerCompletionId) + Send + 'static,
    ) -> AsyncHostResult<()> {
        let policy = Arc::clone(&self.policy);
        let process_for_runner = self.process.clone();
        self.workers.spawn(
            worker,
            init_job,
            move |worker_job| {
                Self::run_policy_checked_job(&policy, &process_for_runner, &mut worker_job.job);
            },
            complete_job,
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;
    use crate::filesystem::Job as FilesystemJob;
    use crate::process::{Job as ProcessJob, SpawnOptions};

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
        AsyncHost::new(Arc::new(Policy::from_file(path).unwrap()))
    }

    #[cfg(unix)]
    fn successful_process_job() -> Job {
        let mut child_signal_mask = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::sigemptyset(&mut child_signal_mask) }, 0);
        ProcessJob::spawn_unix(
            OsString::from("/usr/bin/true"),
            vec![OsString::from("/usr/bin/true")],
            Vec::new(),
            None,
            None,
            None,
            None,
            SpawnOptions { child_signal_mask },
        )
        .into()
    }

    #[cfg(windows)]
    fn successful_process_job() -> Job {
        ProcessJob::spawn_windows(
            OsString::from("cmd.exe /D /C exit 0"),
            vec![0, 0],
            None,
            None,
            None,
            None,
            SpawnOptions {
                no_console_window: true,
                is_orphan: false,
            },
        )
        .into()
    }

    #[test]
    fn no_policy_does_not_allocate_child_ownership_tracking() {
        let host = AsyncHost::default();

        assert!(!host.process.has_policy());
        host.check_owned_child_pid(i32::MAX).unwrap();
    }

    #[test]
    fn spawn_job_builder_only_mutates_ready_spawn_jobs() {
        let host = AsyncHost::default();
        #[cfg(unix)]
        let spawn_job = successful_process_job();
        #[cfg(windows)]
        let spawn_job = ProcessJob::spawn_windows(
            OsString::from("cmd.exe /D /C exit 0"),
            vec![0, 0],
            None,
            None,
            None,
            None,
            SpawnOptions {
                no_console_window: false,
                is_orphan: false,
            },
        );
        let handle = host.insert_job(spawn_job).unwrap();

        host.spawn_job_set_cwd(handle, OsString::from("working-directory"))
            .unwrap();
        #[cfg(windows)]
        host.spawn_job_set_no_console_window(handle).unwrap();

        {
            let jobs = host.jobs.borrow();
            let job = jobs.visible_job(job_key(&host, handle)).unwrap();
            let process_job = job.process().unwrap();
            assert_eq!(
                process_job.cwd().unwrap(),
                Some(std::ffi::OsStr::new("working-directory"))
            );
            #[cfg(windows)]
            assert!(process_job.no_console_window().unwrap());
        }

        let key = job_key(&host, handle);
        let job = host.jobs.borrow_mut().take_ready_job(key).unwrap();
        assert_eq!(
            host.spawn_job_set_cwd(handle, OsString::from("too-late")),
            Err(AsyncHostError::Badf)
        );
        assert!(host.jobs.borrow_mut().restore_unrun_job(key, job).is_none());

        let sleep = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        assert_eq!(
            host.spawn_job_set_cwd(sleep, OsString::from("wrong-job")),
            Err(AsyncHostError::Badf)
        );
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
            Some(host.acquire_resource(process_handle).unwrap())
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
                ProcessJob::wait_for_process(
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
        host.process.track_owned_child(checked_pid);
        host.process.track_owned_child(tracked_pid);
        let [process_handle, other] = host.pipe(false, false).unwrap();
        host.process
            .track_process_handle(process_handle, tracked_pid);
        let process_resource = host.acquire_resource(process_handle).unwrap();

        assert_eq!(
            host.check_process_handle_pid(process_handle, checked_pid),
            Err(AsyncHostError::PermissionDenied)
        );
        let wait_job = host
            .insert_job(
                ProcessJob::wait_for_process(
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
        host.process.track_owned_child(pid);
        let [first, second] = host.pipe(false, false).unwrap();
        host.process.track_process_handle(first, pid);
        host.process.track_process_handle(second, pid);

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
        host.process.track_owned_child(pid);
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

    #[cfg(unix)]
    #[test]
    fn process_env_builder_places_extra_entries_before_filtered_inherited_entries() {
        let mut builder = HostProcessEnvBuilder::new(vec![
            OsString::from("OVERRIDE=old"),
            OsString::from("KEEP=value"),
            OsString::from("WITHOUT_SEPARATOR"),
        ]);
        crate::async_sys::process::process_env_builder_add_entry(
            &mut builder,
            OsString::from("OVERRIDE"),
            OsString::from("new"),
        );

        assert_eq!(
            crate::async_sys::process::finish_process_env_builder(builder),
            vec![
                OsString::from("OVERRIDE=new"),
                OsString::from("KEEP=value"),
                OsString::from("WITHOUT_SEPARATOR"),
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn process_env_builder_filters_inherited_entries_case_insensitively() {
        fn block(entries: &[&str]) -> Vec<u16> {
            let mut block = Vec::new();
            for entry in entries {
                block.extend(entry.encode_utf16());
                block.push(0);
            }
            block.push(0);
            block
        }

        let mut builder = HostProcessEnvBuilder::new(vec![
            OsString::from("Path=old"),
            OsString::from("KEEP=value"),
            OsString::from("WITHOUT_SEPARATOR"),
            OsString::from("=PSEUDO"),
        ]);
        crate::async_sys::process::process_env_builder_add_entry(
            &mut builder,
            OsString::from("PATH"),
            OsString::from("new"),
        );

        assert_eq!(
            crate::async_sys::process::finish_process_env_builder(builder),
            block(&["PATH=new", "KEEP=value"])
        );
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
            host.process_envs
                .borrow()
                .get(host.process_env(dst).unwrap())
                .unwrap()
                .as_slice(),
            &[
                Some(
                    crate::async_sys::process::LegacyProcessEnvEntry::Materialized(OsString::from(
                        "A=B"
                    ))
                ),
                None,
            ]
        );
        #[cfg(windows)]
        assert_eq!(
            host.process_envs
                .borrow()
                .get(host.process_env(dst).unwrap())
                .unwrap()
                .as_slice(),
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
    fn legacy_process_spawn_inputs_use_inherited_count_to_build_environment() {
        let host = AsyncHost::default();
        let argv = host.insert_process_argv(2).unwrap();
        host.process_argv_add_entry(argv, 0, OsString::from("command"))
            .unwrap();
        host.process_argv_add_entry(argv, 1, OsString::from("argument"))
            .unwrap();
        let env = host.insert_process_env(vec![
            Some(OsString::from("OVERRIDE=old")),
            Some(OsString::from("KEEP=value")),
            Some(OsString::from("OVERRIDE=new")),
        ]);

        let (args, entries) = host.take_legacy_process_spawn_inputs(argv, env, 2).unwrap();

        assert_eq!(
            args,
            vec![OsString::from("command"), OsString::from("argument")]
        );
        assert_eq!(
            entries,
            vec![OsString::from("OVERRIDE=new"), OsString::from("KEEP=value")]
        );
        assert!(matches!(host.process_argv(argv), Err(AsyncHostError::Badf)));
        assert!(matches!(host.process_env(env), Err(AsyncHostError::Badf)));
    }

    #[cfg(unix)]
    #[test]
    fn legacy_process_spawn_inputs_apply_current_nul_handling() {
        let host = AsyncHost::default();
        let argv = host.insert_process_argv(1).unwrap();
        host.process_argv_add_entry(argv, 0, OsString::from("command"))
            .unwrap();
        let env = host.insert_process_env(vec![None, None]);
        host.process_env_add_entry(
            env,
            0,
            OsString::from("BAD=PREFIX\0KEY"),
            OsString::from("value"),
        )
        .unwrap();
        host.process_env_add_entry(
            env,
            1,
            OsString::from("GOOD"),
            OsString::from("before\0INJECTED=value"),
        )
        .unwrap();

        let (_, entries) = host.take_legacy_process_spawn_inputs(argv, env, 0).unwrap();

        assert_eq!(entries, vec![OsString::from("GOOD=before")]);
    }

    #[cfg(unix)]
    #[test]
    fn invalid_legacy_process_spawn_inherited_count_does_not_consume_buffers() {
        let host = AsyncHost::default();
        let argv = host.insert_process_argv(1).unwrap();
        host.process_argv_add_entry(argv, 0, OsString::from("command"))
            .unwrap();
        let env = host.insert_process_env(vec![Some(OsString::from("A=B"))]);

        assert_eq!(
            host.take_legacy_process_spawn_inputs(argv, env, 2),
            Err(AsyncHostError::Inval)
        );
        assert!(host.process_argv(argv).is_ok());
        assert!(host.process_env(env).is_ok());
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
            host.take_legacy_process_spawn_inputs(argv, env, 0),
            Err(AsyncHostError::Inval)
        );
        assert!(host.process_argv(argv).is_ok());
        assert!(host.process_env(env).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn process_spawn_abis_reject_the_other_environment_handle_kind() {
        let host = AsyncHost::default();
        let argv = host.insert_process_argv(1).unwrap();
        host.process_argv_add_entry(argv, 0, OsString::from("command"))
            .unwrap();
        let builder = host.insert_process_env_builder(Vec::new());

        assert_eq!(
            host.take_legacy_process_spawn_inputs(argv, builder, 0),
            Err(AsyncHostError::Badf)
        );
        assert!(host.process_argv(argv).is_ok());
        assert!(host.handles.borrow().process_env_builder(builder).is_ok());

        let legacy = host.insert_process_env(vec![Some(OsString::from("A=B"))]);
        assert_eq!(
            host.take_process_spawn_inputs(argv, legacy),
            Err(AsyncHostError::Badf)
        );
        assert!(host.process_argv(argv).is_ok());
        assert!(host.process_env(legacy).is_ok());
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
    fn process_spawn_abis_reject_the_other_environment_handle_kind() {
        let host = AsyncHost::default();
        let builder = host.insert_process_env_builder(Vec::new());
        assert_eq!(host.take_process_env(builder), Err(AsyncHostError::Badf));
        assert!(host.handles.borrow().process_env_builder(builder).is_ok());

        let legacy = host.insert_process_env(vec![0, 0]);
        assert_eq!(
            host.take_process_env_builder(legacy),
            Err(AsyncHostError::Badf)
        );
        assert!(host.process_env(legacy).is_ok());
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

        AsyncHost::finish_process_job(&host.process, &job).unwrap();

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
    fn resource_class_rejects_file_as_socket() {
        let host = AsyncHost::default();
        let [read, write] = host.pipe(true, true).unwrap();

        assert_eq!(
            host.acquire_socket_resource(read).unwrap_err(),
            AsyncHostError::Inval
        );
        assert!(
            host.acquire_resource_of_class(read, ResourceClass::File)
                .is_ok()
        );

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
            let read_fd = host
                .acquire_resource(read)
                .unwrap()
                .as_fd()
                .unwrap()
                .as_raw_fd();
            let write_fd = host
                .acquire_resource(write)
                .unwrap()
                .as_fd()
                .unwrap()
                .as_raw_fd();
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
        let tcp = host.make_tcp_socket(4).unwrap();
        let udp = host.make_udp_socket(4, false).unwrap();

        assert!(
            host.with_raw_resource_class(tcp, ResourceClass::TcpSocket, |_| Ok(()))
                .is_ok()
        );
        assert_eq!(host.acquire_resource(tcp).unwrap().socket_family(), Some(4));
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
            .acquire_resource(completion_source)
            .unwrap()
            .as_fd()
            .unwrap()
            .as_raw_fd();
        {
            let polls = host.polls.borrow();
            let poll = polls.polls.get(poll_key(&host, poll)).unwrap();
            assert!(poll.registered_fds.contains(&raw_completion_fd));
        }

        {
            let completions = host.thread_pool_completions.borrow();
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
        let job = FilesystemJob::read(Arc::new(Resource::invalid()), 3, -1);
        let job_handle = host.insert_job(job).unwrap();
        {
            let mut jobs = host.jobs.borrow_mut();
            let job = jobs.visible_job_mut(job_key(&host, job_handle)).unwrap();
            let job = job.filesystem_mut().unwrap();
            job.set_read_result(b"abc".to_vec()).unwrap();
            host.thread_pool_completions
                .borrow()
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
    fn failed_stat_job_does_not_require_a_filesystem_payload() {
        let host = AsyncHost::default();
        let error = AsyncHostError::Inval.errno();
        let job = host
            .insert_job(thread_pool::make_failed_job(error))
            .unwrap();
        let mut memory = [0xaa; 8];

        host.run_job(job).unwrap();
        assert_eq!(host.job_get_err(job).unwrap(), error);
        host.get_stat_result(memory.as_mut_slice(), job, 0, 8)
            .unwrap();

        assert_eq!(memory, [0xaa; 8]);
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

    #[test]
    fn readdir_job_exclusively_leases_and_restores_c_buffer() {
        let host = AsyncHost::default();
        let handle = host.insert_c_buffer(b"abcd".to_vec().into_boxed_slice());
        let key = host.handles.borrow().c_buffer(handle).unwrap();
        assert!(matches!(
            host.c_buffers.borrow().get(key),
            Some(HostCBuffer::Available(_))
        ));

        let lease = host.lease_c_buffer(handle).unwrap();
        assert!(matches!(
            host.c_buffers.borrow().get(key),
            Some(HostCBuffer::Leased)
        ));
        assert_eq!(
            host.with_c_buffer(handle, |_| Ok(())).unwrap_err(),
            AsyncHostError::Badf
        );
        assert_eq!(
            host.lease_c_buffer(handle).unwrap_err(),
            AsyncHostError::Badf
        );

        let job = FilesystemJob::readdir(Arc::new(Resource::invalid()), lease, 4, false);
        let job = host.insert_job(job).unwrap();
        host.run_job(job).unwrap();

        assert!(matches!(
            host.c_buffers.borrow().get(key),
            Some(HostCBuffer::Available(_))
        ));
        host.with_c_buffer(handle, |buffer| {
            assert_eq!(buffer, b"abcd");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn freeing_unrun_readdir_job_restores_c_buffer() {
        let host = AsyncHost::default();
        let handle = host.insert_c_buffer(b"abcd".to_vec().into_boxed_slice());
        let lease = host.lease_c_buffer(handle).unwrap();
        let job = FilesystemJob::readdir(Arc::new(Resource::invalid()), lease, 4, false);
        let job = host.insert_job(job).unwrap();

        host.free_job(job).unwrap();

        host.with_c_buffer(handle, |buffer| {
            assert_eq!(buffer, b"abcd");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn discarded_queued_readdir_job_restores_c_buffer() {
        let host = AsyncHost::default();
        let handle = host.insert_c_buffer(b"abcd".to_vec().into_boxed_slice());
        let lease = host.lease_c_buffer(handle).unwrap();
        let job = FilesystemJob::readdir(Arc::new(Resource::invalid()), lease, 4, false);
        let job = host.insert_job(job).unwrap();
        let worker_job = host
            .take_worker_job(WorkerCompletionId::from_abi(1), job_key(&host, job))
            .unwrap();

        host.free_job(job).unwrap();
        assert_eq!(
            host.with_c_buffer(handle, |_| Ok(())).unwrap_err(),
            AsyncHostError::Badf
        );
        host.restore_unrun_worker_job(worker_job);

        host.with_c_buffer(handle, |buffer| {
            assert_eq!(buffer, b"abcd");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn freed_c_buffer_is_not_restored_by_its_readdir_job() {
        let host = AsyncHost::default();
        let handle = host.insert_c_buffer(b"old".to_vec().into_boxed_slice());
        let lease = host.lease_c_buffer(handle).unwrap();
        let job = FilesystemJob::readdir(Arc::new(Resource::invalid()), lease, 3, false);
        let job = host.insert_job(job).unwrap();

        host.free_c_buffer(handle).unwrap();
        let replacement = host.insert_c_buffer(b"new".to_vec().into_boxed_slice());
        host.free_job(job).unwrap();

        assert_eq!(
            host.with_c_buffer(handle, |_| Ok(())).unwrap_err(),
            AsyncHostError::Badf
        );
        host.with_c_buffer(replacement, |buffer| {
            assert_eq!(buffer, b"new");
            Ok(())
        })
        .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn realpath_result_is_registered_c_buffer_cleaned_up_with_job() {
        let host = AsyncHost::default();
        let job_handle = host
            .insert_job(FilesystemJob::realpath(std::ffi::OsString::from(
                "/tmp/example",
            )))
            .unwrap();
        {
            let mut jobs = host.jobs.borrow_mut();
            let job = jobs.visible_job_mut(job_key(&host, job_handle)).unwrap();
            let job = job.filesystem_mut().unwrap();
            job.set_realpath_result(b"/tmp/example\0".to_vec().into_boxed_slice())
                .unwrap();
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
            let completions = host.thread_pool_completions.borrow();
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
            .insert_job(FilesystemJob::open_legacy(
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
        assert_eq!(host.open_job_get_kind(job).unwrap(), 1);
        host.open_job_get_dev_id(job).unwrap();
        host.open_job_get_file_id(job).unwrap();

        let opened = host.open_job_get_fd(job).unwrap();
        assert_eq!(host.open_job_get_fd(job).unwrap(), opened);
        assert_eq!(host.run_job(job), Err(AsyncHostError::Badf));
        assert_eq!(resource_count(&host), 1);
        assert!(host.acquire_resource(opened).is_ok());

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
            .insert_job(FilesystemJob::open_legacy(
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
            .insert_job(FilesystemJob::realpath(
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
            .insert_job(FilesystemJob::open_legacy(
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
            .open_path(link.as_os_str(), 0, 0, false)
            .unwrap();
        let job = host
            .insert_job(FilesystemJob::open_legacy(
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
            .insert_job(FilesystemJob::remove(
                denied_link.as_os_str().to_os_string(),
            ))
            .unwrap();
        let rename_job = host
            .insert_job(FilesystemJob::rename(
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
            .insert_job(FilesystemJob::file_kind_by_path(
                None,
                denied_link.as_os_str().to_os_string(),
                false,
            ))
            .unwrap();
        let time_job = host
            .insert_job(FilesystemJob::file_time_by_path(
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
            .insert_job(FilesystemJob::open_legacy(
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
        let parent = host.acquire_resource(parent_fd).unwrap();
        let allowed_link_job = host
            .insert_job(FilesystemJob::file_kind_by_path(
                Some(Arc::clone(&parent)),
                std::ffi::OsString::from("link.txt"),
                false,
            ))
            .unwrap();
        let denied_link_job = host
            .insert_job(FilesystemJob::file_kind_by_path(
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
            .insert_job(FilesystemJob::open_legacy(
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
        let resource = host.acquire_resource(fd).unwrap();
        let size_job = host
            .insert_job(FilesystemJob::file_size(Arc::clone(&resource)))
            .unwrap();
        let time_job = host
            .insert_job(FilesystemJob::file_time(Arc::clone(&resource)))
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

    #[cfg(target_os = "macos")]
    #[test]
    fn kqueue_watcher_registration_requires_read_policy() {
        let tmp = tempfile::tempdir().unwrap();
        let writable = tmp.path().join("writable");
        std::fs::create_dir(&writable).unwrap();
        let file = writable.join("data.txt");
        std::fs::write(&file, "secret").unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nwrite = [\"writable\"]\n").unwrap();
        let host = host_with_policy(&policy_file);
        let open_job = host
            .insert_job(FilesystemJob::open_legacy(
                file.as_os_str().to_os_string(),
                1,
                0,
                false,
                0,
                0,
            ))
            .unwrap();
        host.run_job(open_job).unwrap();
        let file_handle = host.open_job_get_fd(open_job).unwrap();
        let kqueue_handle =
            host.insert_file_resource(crate::async_sys::fs::watch_kqueue::create().unwrap());

        assert_eq!(
            host.kqueue_watcher_add_file(kqueue_handle, file_handle, false),
            Err(AsyncHostError::PermissionDenied)
        );

        host.close_fd(kqueue_handle).unwrap();
        host.close_fd(file_handle).unwrap();
        host.free_job(open_job).unwrap();
    }

    #[test]
    fn open_stat_identity_uses_open_policy_but_extra_metadata_requires_read_policy() {
        let tmp = tempfile::tempdir().unwrap();
        let writable = tmp.path().join("writable");
        std::fs::create_dir(&writable).unwrap();
        let file = writable.join("data.txt");
        std::fs::write(&file, "secret").unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nwrite = [\"writable\"]\n").unwrap();
        let host = host_with_policy(&policy_file);
        let identity_job = host
            .insert_job(
                FilesystemJob::open(
                    file.as_os_str().to_os_string(),
                    1,
                    0,
                    false,
                    0,
                    0,
                    crate::filesystem::STAT_OPEN_IDENTITY,
                    32,
                )
                .unwrap(),
            )
            .unwrap();
        let metadata_job = host
            .insert_job(
                FilesystemJob::open(
                    file.as_os_str().to_os_string(),
                    1,
                    0,
                    false,
                    0,
                    0,
                    crate::filesystem::STAT_OPEN_IDENTITY | 0x0002,
                    40,
                )
                .unwrap(),
            )
            .unwrap();

        host.run_job(identity_job).unwrap();
        host.run_job(metadata_job).unwrap();

        assert_eq!(host.job_get_err(identity_job).unwrap(), 0);
        assert_eq!(
            host.job_get_err(metadata_job).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        let fd = host.open_job_get_fd(identity_job).unwrap();
        host.close_fd(fd).unwrap();
        host.free_job(metadata_job).unwrap();
        host.free_job(identity_job).unwrap();
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
            .insert_job(FilesystemJob::open_legacy(
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
            .insert_job(FilesystemJob::open_legacy(
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
        let resource = host.acquire_resource(fd).unwrap();
        let flock_job = host
            .insert_job(FilesystemJob::flock(Arc::clone(&resource), true))
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
            .insert_job(FilesystemJob::open_legacy(
                path.as_os_str().to_os_string(),
                2,
                3,
                false,
                0,
                0o600,
            ))
            .unwrap();
        let key = job_key(&host, job_handle);
        let mut job = host.jobs.borrow_mut().take_ready_job(key).unwrap();

        thread_pool::run_host_job(&mut job);

        assert_eq!(job.err(), 0);
        assert!(matches!(
            job.filesystem().unwrap().open_result().unwrap().resource,
            ResourcePublication::Unpublished(_)
        ));
        assert_eq!(resource_count(&host), 0);
        host.jobs.borrow_mut().jobs.remove(key);

        {
            assert_eq!(host.restore_job(key, job), Err(AsyncHostError::Badf));
            assert_eq!(resource_count(&host), 0);
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn drop_destroys_pool_even_when_worker_holds_state() {
        let policy = Arc::new(Policy::allow_all());
        let policy_weak = Arc::downgrade(&policy);
        let host = AsyncHost::new(policy);
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

        assert!(policy_weak.upgrade().is_none());
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
    fn free_running_worker_job_detaches_its_result() {
        let host = AsyncHost::default();
        let first_job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let first_key = job_key(&host, first_job);
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let (completion_sender, completion_receiver) = std::sync::mpsc::channel();
        let worker_key = host.handles.borrow_mut().insert(HandleKind::Worker);
        host.workers
            .spawn(
                worker_key,
                host.take_worker_job(WorkerCompletionId::from_abi(1), first_key)
                    .unwrap(),
                move |worker_job| {
                    started_sender.send(worker_job.completion_id).unwrap();
                    release_receiver.recv().unwrap();
                    thread_pool::run_host_job(&mut worker_job.job);
                },
                move |completion_id| completion_sender.send(completion_id).unwrap(),
            )
            .unwrap();
        let worker = handle_from_key(worker_key);

        assert_eq!(
            started_receiver.recv().unwrap(),
            WorkerCompletionId::from_abi(1)
        );
        host.free_job(first_job).unwrap();
        let replacement_job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();

        release_sender.send(()).unwrap();
        assert_eq!(
            completion_receiver.recv().unwrap(),
            WorkerCompletionId::from_abi(1)
        );
        host.restore_completed_worker_jobs();
        assert_eq!(host.job_get_ret(first_job), Err(AsyncHostError::Badf));
        host.run_job(replacement_job).unwrap();
        assert_eq!(host.job_get_ret(replacement_job), Ok(0));
        host.free_job(replacement_job).unwrap();

        host.free_worker(worker).unwrap();
        host.destroy_thread_pool();
    }

    #[test]
    fn free_queued_worker_job_detaches_without_cancelling_it() {
        let host = AsyncHost::default();
        let first_job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let first_key = job_key(&host, first_job);
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let (completion_sender, completion_receiver) = std::sync::mpsc::channel();
        let worker_key = host.handles.borrow_mut().insert(HandleKind::Worker);
        host.workers
            .spawn(
                worker_key,
                host.take_worker_job(WorkerCompletionId::from_abi(1), first_key)
                    .unwrap(),
                move |worker_job| {
                    started_sender.send(worker_job.completion_id).unwrap();
                    if worker_job.job_key == first_key {
                        release_receiver.recv().unwrap();
                    }
                    thread_pool::run_host_job(&mut worker_job.job);
                },
                move |completion_id| completion_sender.send(completion_id).unwrap(),
            )
            .unwrap();
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
            .insert_job(FilesystemJob::remove(
                displaced_path.as_os_str().to_os_string(),
            ))
            .unwrap();
        let queued_path = std::env::temp_dir().join(format!(
            "moonrun-cancelled-queued-worker-job-{}",
            std::process::id()
        ));
        std::fs::write(&queued_path, b"queued").unwrap();
        let queued_job = host
            .insert_job(FilesystemJob::remove(
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
            WorkerCompletionId::from_abi(1)
        );
        assert_eq!(
            completion_receiver
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            WorkerCompletionId::from_abi(3)
        );
        host.restore_completed_worker_jobs();
        host.free_job(first_job).unwrap();
        assert!(!queued_path.exists());
        assert_eq!(host.job_get_ret(queued_job), Err(AsyncHostError::Badf));

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
            .insert_job(FilesystemJob::read(Arc::new(Resource::invalid()), 1, -1))
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
            .insert_job(FilesystemJob::read(Arc::new(Resource::invalid()), 1, -1))
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

        poll::post_thread_pool_completion(&completion, 42).unwrap();

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

        poll::post_thread_pool_completion(&completion, 42).unwrap();
    }

    #[test]
    fn stale_worker_handle_is_rejected_after_free() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let job = host
            .insert_job(FilesystemJob::read(Arc::new(Resource::invalid()), 1, -1))
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
        let file = host.acquire_resource(read).unwrap();

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
            .insert_job(FilesystemJob::read(
                host.acquire_resource(read).unwrap(),
                1,
                -1,
            ))
            .unwrap();

        host.close_fd(read).unwrap();
        let fd = host
            .acquire_resource(write)
            .unwrap()
            .as_fd()
            .unwrap()
            .as_raw_fd();
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
    fn read_dir_changes_io_result_leases_buffer_until_free() {
        let host = AsyncHost::default();
        let buffer = host.insert_windows_watcher_buffer();
        let result = host.make_read_dir_changes_io_result(buffer).unwrap();

        assert_eq!(
            host.with_windows_watcher_buffer(buffer, |_| Ok(())),
            Err(AsyncHostError::Badf)
        );

        host.free_io_result(result).unwrap();
        assert_eq!(
            host.with_windows_watcher_buffer(buffer, |buffer| Ok(buffer.capacity())),
            Ok(usize::try_from(crate::async_sys::fs::watch_windows::event_buffer_size()).unwrap())
        );
        host.free_windows_watcher_buffer(buffer).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn read_dir_changes_rejects_a_generic_c_buffer() {
        let host = AsyncHost::default();
        let buffer = host.insert_c_buffer(vec![1, 2, 3].into_boxed_slice());

        assert_eq!(
            host.make_read_dir_changes_io_result(buffer),
            Err(AsyncHostError::Badf)
        );
        assert_eq!(
            host.with_c_buffer(buffer, |bytes| Ok(bytes.to_vec())),
            Ok(vec![1, 2, 3])
        );
        host.free_c_buffer(buffer).unwrap();
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

        let io_results = host.io_results.borrow();
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
            let read_file = host.acquire_resource(read).unwrap();
            let raw_read = read_file.raw_identity();
            host.io_results
                .borrow_mut()
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
            let mut io_results = host.io_results.borrow_mut();
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
            let read_file = host.acquire_resource(read).unwrap();
            host.io_results
                .borrow_mut()
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap()
                .mark_pending(read_file)
                .unwrap();
        }

        assert_eq!(host.cancel_io_result(result, read), Ok(0));
        {
            let io_results = host.io_results.borrow();
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
            let read_file = host.acquire_resource(read).unwrap();
            let raw_read = read_file.raw_identity();
            let mut io_results = host.io_results.borrow_mut();
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
            let mut io_results = host.io_results.borrow_mut();
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
            let read_file = host.acquire_resource(read).unwrap();
            let raw_read = read_file.raw_identity();
            host.io_results
                .borrow_mut()
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap()
                .mark_pending(read_file)
                .unwrap();
            raw_read
        };

        assert_eq!(host.close_fd(read), Err(AsyncHostError::Inval));
        {
            assert!(host.acquire_resource(read).is_ok());
            let mut io_results = host.io_results.borrow_mut();
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
            let read_file = host.acquire_resource(read).unwrap();
            let write_file = host.acquire_resource(write).unwrap();
            let raw_read = read_file.raw_identity();
            let raw_write = write_file.raw_identity();
            host.io_results
                .borrow_mut()
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
            assert!(host.acquire_resource(read).is_ok());
            assert!(host.acquire_resource(write).is_ok());
            let mut io_results = host.io_results.borrow_mut();
            let result = io_results
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap();
            assert_eq!(result.pending_resource_identity(), Some(raw_read));
            assert!(result.protects_pending_resource(&host.acquire_resource(write).unwrap()));
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
            let read_file = host.acquire_resource(read).unwrap();
            host.io_results
                .borrow_mut()
                .io_results
                .get_mut(io_result_key(&host, result))
                .unwrap()
                .mark_pending(read_file)
                .unwrap();
        }

        assert_eq!(host.free_io_result(result), Err(AsyncHostError::Inval));
        assert!(
            host.io_results
                .borrow()
                .io_results
                .contains_key(io_result_key(&host, result))
        );
        host.io_results
            .borrow_mut()
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
        let read_file = host.acquire_resource(read).unwrap();
        let raw_fd = read_file.raw_identity();
        let overlapped = {
            let mut io_results = host.io_results.borrow_mut();
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
                .borrow()
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
    fn unregistered_iocp_completion_key_is_returned() {
        use windows_sys::Win32::System::IO::PostQueuedCompletionStatus;

        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_port = {
            let polls = host.polls.borrow();
            let poll = polls.polls.get(poll_key(&host, poll)).unwrap();
            poll.instance.raw_fd()
        };
        let completion_key = 0x1234usize;
        let posted = unsafe {
            PostQueuedCompletionStatus(completion_port, 0, completion_key, std::ptr::null_mut())
        };
        assert_ne!(posted, 0);

        assert_eq!(host.poll_wait(poll, 1000).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();

        assert_eq!(host.poll_event_fd(event).unwrap(), completion_key as u64);
    }

    #[cfg(windows)]
    #[test]
    fn zero_iocp_completion_key_is_returned() {
        use windows_sys::Win32::System::IO::PostQueuedCompletionStatus;

        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let completion_port = {
            let polls = host.polls.borrow();
            let poll = polls.polls.get(poll_key(&host, poll)).unwrap();
            poll.instance.raw_fd()
        };
        let posted =
            unsafe { PostQueuedCompletionStatus(completion_port, 0, 0, std::ptr::null_mut()) };
        assert_ne!(posted, 0);

        assert_eq!(host.poll_wait(poll, 1000).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();

        assert_eq!(host.poll_event_fd(event).unwrap(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn close_fd_preserves_polled_iocp_resource_handle() {
        use windows_sys::Win32::System::IO::PostQueuedCompletionStatus;

        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let [read, write] = host.pipe(true, true).unwrap();
        host.poll_register(poll, read, true).unwrap();
        let completion_port = {
            let polls = host.polls.borrow();
            let poll = polls.polls.get(poll_key(&host, poll)).unwrap();
            poll.instance.raw_fd()
        };
        let posted = unsafe {
            PostQueuedCompletionStatus(
                completion_port,
                0,
                usize::try_from(read).unwrap(),
                std::ptr::null_mut(),
            )
        };
        assert_ne!(posted, 0);
        assert_eq!(host.poll_wait(poll, 1000).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();

        host.close_fd(read).unwrap();

        assert_eq!(host.poll_event_fd(event).unwrap(), read);
        assert_eq!(host.close_fd(read), Err(AsyncHostError::Badf));
        host.close_fd(write).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn poll_reports_registered_pipe_readiness_as_guest_fd() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let [read, write] = host.pipe(true, true).unwrap();
        host.poll_register(poll, read, true).unwrap();

        let fd = host
            .acquire_resource(write)
            .unwrap()
            .as_fd()
            .unwrap()
            .as_raw_fd();
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
    fn close_fd_preserves_polled_resource_handle() {
        let host = AsyncHost::default();
        let poll = host.poll_create().unwrap();
        let [read, write] = host.pipe(true, true).unwrap();
        host.poll_register(poll, read, true).unwrap();

        let fd = host
            .acquire_resource(write)
            .unwrap()
            .as_fd()
            .unwrap()
            .as_raw_fd();
        let byte = b"x";
        let ret = unsafe { libc::write(fd, byte.as_ptr().cast(), byte.len()) };
        assert_eq!(ret, 1);
        assert_eq!(host.poll_wait(poll, 100).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();

        host.close_fd(read).unwrap();

        assert_eq!(host.poll_event_fd(event).unwrap(), read);
        assert_eq!(host.close_fd(read), Err(AsyncHostError::Badf));
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
