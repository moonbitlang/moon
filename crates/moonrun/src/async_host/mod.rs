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
use std::ffi::OsString;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(all(windows, test))]
use std::os::windows::io::AsRawHandle;
use std::rc::Rc;
use std::sync::Arc;

use slotmap::{Key, KeyData};

use crate::filesystem::HostFs;
use crate::filesystem::RawFile as RawFd;
use crate::guest_memory::{GuestMemory, GuestMemoryError};
use crate::network::HostNetwork;
#[cfg(windows)]
use crate::poll;
use crate::poll::PollTable;
use crate::process::{HostProcess, ProcessInputs};
use crate::resource::{Resource, ResourceClass, ResourcePublication, ResourceRef};
use crate::run_signal::SignalReceiver;
use crate::run_termination::TerminationRequest;
pub(crate) use crate::runtime::HostKey as HandleKey;
use crate::runtime::{
    Env, Handles, HostKeys, HostResourceKind as HandleKind, Stdio, StdioStream, null_handle,
};
#[cfg(unix)]
use crate::thread_pool::ThreadPoolCompletionNotifier;
use crate::thread_pool::{
    self, HostHandle, InstanceWorkers, Job, JobPayload, WorkerCompletionDestination,
    WorkerCompletionId, WorkerJob,
};
use crate::tls::HostTls;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
compile_error!("moonrun async wasm host currently supports only Linux, macOS, and Windows hosts");

#[cfg(not(target_endian = "little"))]
compile_error!("moonrun async wasm host requires little-endian host memory");

pub(crate) mod buffers;
use buffers::Buffers;

#[cfg(windows)]
mod windows_io;
#[cfg(windows)]
use windows_io::IoResultTable;

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

impl From<std::io::Error> for AsyncHostError {
    fn from(error: std::io::Error) -> Self {
        Self::Native(error.raw_os_error().unwrap_or(native_errno::INVAL))
    }
}

impl From<crate::filesystem::FsError> for AsyncHostError {
    fn from(error: crate::filesystem::FsError) -> Self {
        match error {
            crate::filesystem::FsError::Denied => Self::PermissionDenied,
            crate::filesystem::FsError::Io(error) => error.into(),
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

fn handle_from_key(key: HandleKey) -> HostHandle {
    key.data().as_ffi()
}

/// Decode an untrusted Handle; the owning table validates the resulting key.
fn key_from_handle(handle: HostHandle) -> HandleKey {
    KeyData::from_ffi(handle).into()
}

struct ResourceHandles {
    resources: Handles<ResourceRef>,
    invalid_resource: HandleKey,
    stdio_resources: [HandleKey; 3],
}

impl ResourceHandles {
    fn new(keys: Rc<RefCell<HostKeys>>, stdio_raw_handles: [Option<RawFd>; 3]) -> Self {
        let mut resources = Handles::new(keys, HandleKind::Resource);
        let invalid_resource = resources.insert(Arc::new(Resource::invalid()));

        let mut stdio_resources = [invalid_resource; 3];
        for (index, raw) in stdio_raw_handles.iter().enumerate() {
            let Some(raw) = raw else {
                continue;
            };
            if let Some(previous) = stdio_raw_handles[..index]
                .iter()
                .position(|previous| previous.as_ref() == Some(raw))
            {
                stdio_resources[index] = stdio_resources[previous];
            } else {
                let key = resources.insert(Arc::new(Resource::stdio_file(*raw)));
                stdio_resources[index] = key;
            }
        }

        Self {
            resources,
            invalid_resource,
            stdio_resources,
        }
    }

    fn invalid_fd(&self) -> HostHandle {
        handle_from_key(self.invalid_resource)
    }

    fn resource_ref(&self, handle: HostHandle) -> AsyncHostResult<&ResourceRef> {
        let key = key_from_handle(handle);
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
        let key = key_from_handle(handle);
        if key == self.invalid_resource || self.stdio_resources.contains(&key) {
            return Err(AsyncHostError::Badf);
        }
        self.resources.remove(key).ok_or(AsyncHostError::Badf)
    }

    fn insert_resource(&mut self, resource: Resource) -> HostHandle {
        let key = self.resources.insert(Arc::new(resource));
        handle_from_key(key)
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

    fn leak_summary(&self) -> Option<String> {
        let mut leaks = Vec::new();
        if !self
            .resources
            .get(self.invalid_resource)
            .is_some_and(|resource| resource.is_invalid())
        {
            leaks.push("invalid_resource=invalid".to_string());
        }
        let resources = self.resource_count_excluding_reserved();
        if resources != 0 {
            leaks.push(format!("resources={resources}"));
        }
        (!leaks.is_empty()).then(|| leaks.join(", "))
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
    jobs: Handles<HostJobState>,
}

impl JobTable {
    fn new(keys: Rc<RefCell<HostKeys>>) -> Self {
        Self {
            jobs: Handles::new(keys, HandleKind::Job),
        }
    }

    fn insert_job(&mut self, job: Job) -> HandleKey {
        self.jobs.insert(HostJobState::Ready(job))
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

// One registration binds the owning Poll and platform completion state.
// Unix signal-mask ownership lasts until this registration is removed.
struct ThreadPoolRegistration {
    poll: HandleKey,
    #[cfg(unix)]
    notifier: Arc<ThreadPoolCompletionNotifier>,
    #[cfg(unix)]
    source: HostHandle,
    #[cfg(unix)]
    old_signal_mask: libc::sigset_t,
    #[cfg(windows)]
    port: poll::CompletionPort,
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

pub(crate) struct AsyncHost {
    // V8 enters this host synchronously on one thread. Cell and RefCell encode
    // that ownership; worker threads own their Jobs and return them through the
    // completion channel instead of sharing the host's tables.
    filesystem: Arc<HostFs>,
    network: HostNetwork,
    errno: Cell<i32>,
    addr_infos: RefCell<Handles<HostAddrInfo>>,
    buffers: Buffers,
    process_inputs: ProcessInputs,
    process: HostProcess,
    #[cfg(unix)]
    child_signal_mask: libc::sigset_t,
    #[cfg(windows)]
    io_results: RefCell<IoResultTable>,
    jobs: RefCell<JobTable>,
    workers: InstanceWorkers,
    polls: RefCell<PollTable>,
    thread_pool_registration: RefCell<Option<ThreadPoolRegistration>>,
    signals: SignalReceiver,
    #[cfg(unix)]
    signal_handler: RefCell<Option<crate::run_signal::SignalTargetGuard>>,
    resources: RefCell<ResourceHandles>,
    tls: HostTls,
}

impl AsyncHost {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        environment: Arc<Env>,
        stdio: &Stdio,
        filesystem: Arc<HostFs>,
        network: HostNetwork,
        process: HostProcess,
        keys: Rc<RefCell<HostKeys>>,
        signals: SignalReceiver,
        #[cfg(unix)] child_signal_mask: libc::sigset_t,
    ) -> Self {
        Self {
            tls: HostTls::new(Arc::clone(&filesystem), keys.clone()),
            filesystem,
            network,
            errno: Cell::new(0),
            addr_infos: RefCell::new(Handles::new(keys.clone(), HandleKind::AddrInfo)),
            buffers: Buffers::new(keys.clone()),
            process_inputs: ProcessInputs::new(environment, keys.clone()),
            process,
            #[cfg(unix)]
            child_signal_mask,
            #[cfg(windows)]
            io_results: RefCell::new(IoResultTable::new(keys.clone())),
            jobs: RefCell::new(JobTable::new(keys.clone())),
            workers: InstanceWorkers::new(keys.clone()),
            polls: RefCell::new(PollTable::new(keys.clone())),
            thread_pool_registration: RefCell::new(None),
            signals,
            #[cfg(unix)]
            signal_handler: RefCell::new(None),
            resources: RefCell::new(ResourceHandles::new(keys, stdio.raw_handles())),
        }
    }

    pub(crate) fn invalid_fd(&self) -> HostHandle {
        self.resources.borrow().invalid_fd()
    }

    pub(crate) fn std_handle(&self, id: i32) -> AsyncHostResult<HostHandle> {
        let stdio = match id {
            STDIN_ID => StdioStream::Stdin,
            STDOUT_ID => StdioStream::Stdout,
            STDERR_ID => StdioStream::Stderr,
            _ => return Err(AsyncHostError::Inval),
        };
        let handles = self.resources.borrow();
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
    ) -> AsyncHostResult<WorkerJob> {
        let job = self.jobs.borrow_mut().take_ready_job(key)?;
        Ok(WorkerJob::new(completion_id, key, job))
    }

    fn restore_unrun_worker_job(&self, worker_job: WorkerJob) {
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
        self.buffers().restore(buffer);
    }

    fn revoke_unclaimed_spawn(process: &HostProcess, job: &Job) {
        if let Ok(process_job) = job.process() {
            process.revoke_unclaimed_spawn(process_job, job.ret(), job.err());
        }
    }

    fn publish_open_job_result(&self, key: HandleKey) -> AsyncHostResult<HostHandle> {
        let mut jobs = self.jobs.borrow_mut();
        let placeholder = ResourcePublication::Published(self.invalid_fd());
        let job = jobs.visible_job_mut(key)?;
        let result = job.filesystem_mut()?.open_result_mut()?;
        let fd = match std::mem::replace(&mut result.resource, placeholder) {
            ResourcePublication::Published(fd) => fd,
            ResourcePublication::Unpublished(file) => {
                self.resources.borrow_mut().insert_resource(file)
            }
        };
        result.resource = ResourcePublication::Published(fd);
        Ok(fd)
    }

    /// Describe live async payloads without inspecting another domain's keys.
    pub(crate) fn leak_summary(&self) -> Option<String> {
        {
            let mut leaks = Vec::new();

            self.buffers.append_leaks(&mut leaks);
            {
                let addr_infos = self.addr_infos.borrow();
                if !addr_infos.is_empty() {
                    leaks.push(format!("addr_infos={}", addr_infos.len()));
                }
            }
            #[cfg(windows)]
            {
                let io_results = self.io_results.borrow();
                if let Some(summary) = io_results.leak_summary() {
                    leaks.push(summary);
                }
            }
            {
                let jobs = self.jobs.borrow();
                if !jobs.jobs.is_empty() {
                    leaks.push(format!("jobs={}", jobs.jobs.len()));
                }
            }
            if let Some(summary) = self.polls.borrow().leak_summary() {
                leaks.push(summary);
            }
            if let Some(summary) = self.process_inputs.leak_summary() {
                leaks.push(summary);
            }
            {
                let completions = self.thread_pool_registration.borrow();
                #[cfg(unix)]
                {
                    if completions.is_some() {
                        leaks.push("completion_notifier=1".to_string());
                        leaks.push("completion_source=1".to_string());
                    }
                }
                #[cfg(windows)]
                {
                    if completions.is_some() {
                        leaks.push("completion_port=1".to_string());
                    }
                }
            }
            if let Some(summary) = self.tls.leak_summary() {
                leaks.push(summary);
            }
            if let Some(resources) = self.resources.borrow().leak_summary() {
                leaks.push(resources);
            }
            if self.workers.len() != 0 {
                leaks.push(format!("workers={}", self.workers.len()));
            }
            (!leaks.is_empty()).then(|| leaks.join(", "))
        }
    }

    pub(crate) fn poll_create(&self) -> AsyncHostResult<u64> {
        self.polls.borrow_mut().create()
    }

    pub(crate) fn poll_destroy(&self, handle: u64) -> AsyncHostResult<()> {
        let poll = self.polls.borrow_mut().remove(handle)?;
        let registration = self
            .thread_pool_registration
            .borrow_mut()
            .take_if(|registration| registration.poll == key_from_handle(handle));
        if let Some(registration) = registration {
            self.unregister_thread_pool(registration);
        }
        poll.destroy();
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
        let handles = self.resources.borrow();
        let resource = handles.resource_ref(fd_handle)?;
        #[cfg(windows)]
        if self
            .io_results
            .borrow()
            .has_pending_io_for_resource(resource)
        {
            return Err(AsyncHostError::Inval);
        }
        #[cfg(unix)]
        let raw_fd = resource.as_file()?.as_raw_fd();
        let mut polls = self.polls.borrow_mut();
        let poll = polls.get_mut(poll_handle)?;
        #[cfg(unix)]
        let registered = poll.register(raw_fd, fd_handle, read_only)?;
        #[cfg(windows)]
        let registered = poll.register(resource, fd_handle, read_only)?;
        #[cfg(windows)]
        self.io_results
            .borrow_mut()
            .register(resource, poll.completion_port());
        Ok(registered)
    }

    pub(crate) fn poll_register_legacy(
        &self,
        poll_handle: u64,
        fd_handle: HostHandle,
        read_only: bool,
    ) -> AsyncHostResult<()> {
        // Preserve the old syscall behavior as well as its 0/-1 ABI result.
        #[cfg(unix)]
        {
            let handles = self.resources.borrow();
            let resource = handles.resource_ref(fd_handle)?;
            let raw_fd = resource.as_file()?.as_raw_fd();
            self.polls
                .borrow_mut()
                .get_mut(poll_handle)?
                .register_legacy(raw_fd, fd_handle, read_only)
        }
        #[cfg(windows)]
        self.poll_register(poll_handle, fd_handle, read_only)
            .map(|_| ())
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn poll_register_pid(&self, poll_handle: u64, pid: i32) -> AsyncHostResult<i32> {
        self.polls
            .borrow_mut()
            .register_pid(poll_handle, pid, &self.process)
    }

    pub(crate) fn poll_wait(
        &self,
        poll_handle: u64,
        timeout_ms: i32,
        termination: &TerminationRequest,
    ) -> AsyncHostResult<i32> {
        let result = self.polls.borrow_mut().wait(
            poll_handle,
            timeout_ms,
            termination,
            #[cfg(windows)]
            |overlapped| self.io_results.borrow_mut().complete(overlapped),
        )?;
        let Some(count) = result else { return Ok(0) };
        self.restore_completed_worker_jobs();
        Ok(count)
    }

    pub(crate) fn poll_get_event(&self, poll_handle: u64, index: u32) -> AsyncHostResult<u64> {
        self.polls.borrow().get_event(poll_handle, index)
    }

    pub(crate) fn poll_event_fd(&self, event_handle: u64) -> AsyncHostResult<HostHandle> {
        self.polls.borrow().event_fd(event_handle)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn poll_event_pid(&self, event_handle: u64) -> AsyncHostResult<i32> {
        self.polls.borrow().event_pid(event_handle)
    }

    #[cfg(unix)]
    pub(crate) fn poll_event_events(&self, event_handle: u64) -> AsyncHostResult<i32> {
        self.polls.borrow().event_events(event_handle)
    }

    #[cfg(windows)]
    pub(crate) fn poll_event_io_result(&self, event_handle: u64) -> AsyncHostResult<u64> {
        let key = self.polls.borrow().event_io_result(event_handle)?;
        self.io_results.borrow().event_result(key)
    }

    #[cfg(windows)]
    pub(crate) fn poll_event_bytes_transferred(&self, event_handle: u64) -> AsyncHostResult<u32> {
        self.polls.borrow().event_bytes_transferred(event_handle)
    }

    pub(crate) fn set_cancellation_signals(
        &self,
        all_signals: &[i32],
        signals: &[i32],
    ) -> AsyncHostResult<()> {
        crate::async_sys::signal::set_global_cancellation_signals(
            &self.signals,
            all_signals,
            signals,
        )
    }

    #[cfg(unix)]
    pub(crate) fn start_signal_handler(&self) -> AsyncHostResult<()> {
        if self.signal_handler.borrow().is_none() {
            let handler = crate::async_sys::signal::start_signal_handler(
                &self.signals,
                self.thread_pool_notifier()?,
            )?;
            *self.signal_handler.borrow_mut() = Some(handler);
        }
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn terminate_signal_handler(&self) {
        crate::async_sys::signal::terminate_signal_handler(
            &self.signals,
            self.signal_handler.borrow_mut().take(),
        );
    }

    #[cfg(unix)]
    pub(crate) fn make_sigwait_job(&self, signals: Vec<i32>) -> AsyncHostResult<HostHandle> {
        let notifier = self.thread_pool_notifier()?;
        let job = crate::async_sys::signal::make_sigwait_job(&self.signals, &signals, notifier)?;
        self.insert_job(job)
    }

    #[cfg(windows)]
    pub(crate) fn set_signal_delivery_enabled(&self, enabled: bool) -> AsyncHostResult<i32> {
        let completion_target = if enabled {
            Some(self.thread_pool_completion_target()?)
        } else {
            None
        };
        crate::async_sys::signal::set_console_control_handler(
            &self.signals,
            enabled,
            completion_target,
        )
    }

    pub(crate) fn init_thread_pool(&self, poll_handle: u64) -> AsyncHostResult<HostHandle> {
        let poll_key = key_from_handle(poll_handle);
        let mut polls = self.polls.borrow_mut();
        let poll = polls.get_mut(poll_handle)?;
        let mut registration = self.thread_pool_registration.borrow_mut();
        if registration.is_some() {
            return Err(AsyncHostError::Inval);
        }
        #[cfg(unix)]
        {
            let old_signal_mask = crate::async_sys::signal::init_thread_pool_signal_mask()?;
            let signal_mask_guard = ThreadPoolSignalMaskGuard::new(old_signal_mask);
            let (completion_notifier, event_fd) = ThreadPoolCompletionNotifier::new()?;
            let completion_notifier = Arc::new(completion_notifier);
            let source = self
                .resources
                .borrow_mut()
                .insert_resource(Resource::new(event_fd));
            let signal_fd = completion_notifier.signal_fd();
            // Both sources use the same guest handle. fetch_completion merges
            // pending Run signals with worker IDs without a forwarding thread.
            if let Err(error) = poll.register_completion_sources([event_fd, signal_fd], source) {
                let _ = self.resources.borrow_mut().remove_resource(source);
                return Err(error);
            }
            // Publish the complete registration after both poll sources exist.
            *registration = Some(ThreadPoolRegistration {
                poll: poll_key,
                notifier: completion_notifier,
                source,
                old_signal_mask: signal_mask_guard.commit(),
            });
            Ok(source)
        }
        #[cfg(windows)]
        {
            let completion_port = poll.completion_port();
            *registration = Some(ThreadPoolRegistration {
                poll: poll_key,
                port: completion_port,
            });
            raw_fd_to_guest(windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE)
        }
    }

    pub(crate) fn destroy_thread_pool(&self) {
        #[cfg(unix)]
        self.terminate_signal_handler();
        for unrun_job in self.workers.destroy() {
            self.restore_unrun_worker_job(unrun_job);
        }
        self.restore_completed_worker_jobs();
        let registration = self.thread_pool_registration.borrow_mut().take();
        if let Some(registration) = registration {
            self.unregister_thread_pool(registration);
        }
    }

    fn unregister_thread_pool(&self, registration: ThreadPoolRegistration) {
        #[cfg(unix)]
        {
            self.terminate_signal_handler();
            // The source may already have been removed by close_fd. Retain a
            // surviving resource until every poll has released its descriptor.
            let source = self
                .resources
                .borrow_mut()
                .remove_resource(registration.source)
                .ok();
            let source_fd = source
                .as_ref()
                .and_then(|source| source.as_fd().ok())
                .map(|source| source.as_raw_fd());
            let signal_fd = registration.notifier.signal_fd();
            let mut polls = self.polls.borrow_mut();
            for fd in [source_fd, Some(signal_fd)].into_iter().flatten() {
                polls.unregister_fd(fd);
            }
            let _ = crate::async_sys::signal::restore_thread_pool_signal_mask(
                &registration.old_signal_mask,
            );
        }
        #[cfg(windows)]
        let _ = self.set_signal_delivery_enabled(false);
        drop(registration);
    }

    pub(crate) fn buffers(&self) -> &Buffers {
        &self.buffers
    }

    pub(crate) fn insert_job(&self, job: impl Into<Job>) -> AsyncHostResult<u64> {
        let key = self.jobs.borrow_mut().insert_job(job.into());
        Ok(handle_from_key(key))
    }

    /// Domain adapters inspect their own payloads; the Async Host retains Job
    /// identity and rejects access while a worker owns the payload.
    /// Collect completed results first, including when a supplied pipe was
    /// read without polling or fetching the default completion source.
    pub(crate) fn with_job<T>(&self, handle: u64, f: impl FnOnce(&Job) -> T) -> AsyncHostResult<T> {
        self.restore_completed_worker_jobs();
        let key = key_from_handle(handle);
        let jobs = self.jobs.borrow();
        Ok(f(jobs.visible_job(key)?))
    }

    pub(crate) fn with_job_mut<T>(
        &self,
        handle: u64,
        f: impl FnOnce(&mut Job) -> T,
    ) -> AsyncHostResult<T> {
        self.restore_completed_worker_jobs();
        let key = key_from_handle(handle);
        let mut jobs = self.jobs.borrow_mut();
        Ok(f(jobs.visible_job_mut(key)?))
    }

    pub(crate) fn free_job(&self, handle: u64) -> AsyncHostResult<()> {
        let job = self
            .jobs
            .borrow_mut()
            .take_for_free(key_from_handle(handle))?;
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
            let _ = self.buffers().free(buffer_handle);
        }
        Ok(())
    }

    pub(crate) fn open_job_get_fd(&self, handle: u64) -> AsyncHostResult<HostHandle> {
        let key = key_from_handle(handle);
        self.publish_open_job_result(key)
    }

    pub(crate) fn open_job_get_kind(&self, handle: u64) -> AsyncHostResult<i32> {
        let key = key_from_handle(handle);
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        job.filesystem()?.open_result()?.file_kind()
    }

    pub(crate) fn open_job_get_dev_id(&self, handle: u64) -> AsyncHostResult<u64> {
        let key = key_from_handle(handle);
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        job.filesystem()?.open_result()?.device_id()
    }

    pub(crate) fn open_job_get_file_id(&self, handle: u64) -> AsyncHostResult<u64> {
        let key = key_from_handle(handle);
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        job.filesystem()?.open_result()?.file_id()
    }

    pub(crate) fn get_file_size_result(&self, handle: u64) -> AsyncHostResult<i64> {
        let key = key_from_handle(handle);
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        job.filesystem()?.file_size_result()
    }

    pub(crate) fn get_getaddrinfo_result(&self, handle: u64) -> AsyncHostResult<u64> {
        let addrs = {
            let key = key_from_handle(handle);
            let jobs = self.jobs.borrow();
            let job = jobs.visible_job(key)?;
            let JobPayload::Network(job) = job.payload() else {
                return Err(AsyncHostError::Badf);
            };
            self.network.getaddrinfo_result(job)?
        };
        let mut addr_infos = self.addr_infos.borrow_mut();
        let mut next = None;
        for addr in addrs.into_iter().rev() {
            let key = addr_infos.insert(HostAddrInfo { addr, next });
            next = Some(handle_from_key(key));
        }
        Ok(next.unwrap_or_else(null_handle))
    }

    pub(crate) fn get_spawn_job_result_handle(&self, handle: u64) -> AsyncHostResult<HostHandle> {
        let key = key_from_handle(handle);
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
        let process_pid = job.ret() as i32;
        let fd = self.resources.borrow_mut().insert_resource(resource);
        self.process.track_process_handle(fd, process_pid);
        job.process_mut()?
            .set_spawn_result(ResourcePublication::Published(fd))?;
        Ok(fd)
    }

    pub(crate) fn spawn_job_set_cwd(&self, handle: u64, cwd: OsString) -> AsyncHostResult<()> {
        let key = key_from_handle(handle);
        let mut jobs = self.jobs.borrow_mut();
        let job = jobs.ready_job_mut(key)?;
        job.process_mut()?.set_cwd(cwd)
    }

    #[cfg(windows)]
    pub(crate) fn spawn_job_set_no_console_window(&self, handle: u64) -> AsyncHostResult<()> {
        let key = key_from_handle(handle);
        let mut jobs = self.jobs.borrow_mut();
        let job = jobs.ready_job_mut(key)?;
        job.process_mut()?.set_no_console_window()
    }

    pub(crate) fn addrinfo_next(&self, handle: u64) -> AsyncHostResult<u64> {
        if handle == null_handle() {
            return Ok(null_handle());
        }
        let key = key_from_handle(handle);
        let addr_infos = self.addr_infos.borrow();
        let addrinfo = addr_infos.get(key).ok_or(AsyncHostError::Badf)?;
        Ok(addrinfo.next.unwrap_or_else(null_handle))
    }

    pub(crate) fn addrinfo_addr(&self, handle: u64) -> AsyncHostResult<Box<[u8]>> {
        let key = key_from_handle(handle);
        let addr_infos = self.addr_infos.borrow();
        let addrinfo = addr_infos.get(key).ok_or(AsyncHostError::Badf)?;
        Ok(addrinfo.addr.clone())
    }

    pub(crate) fn free_addrinfo(&self, handle: u64) -> AsyncHostResult<()> {
        if handle == null_handle() {
            return Ok(());
        }
        let mut current = Some(handle);
        while let Some(handle) = current {
            let mut addr_infos = self.addr_infos.borrow_mut();
            let addrinfo = addr_infos
                .remove(key_from_handle(handle))
                .ok_or(AsyncHostError::Badf)?;
            current = addrinfo.next;
        }
        Ok(())
    }

    pub(crate) fn close_fd(&self, handle: HostHandle) -> AsyncHostResult<()> {
        let file = {
            let mut handles = self.resources.borrow_mut();
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
            self.polls.borrow_mut().unregister_fd(raw_fd);
            let registration = self
                .thread_pool_registration
                .borrow_mut()
                .take_if(|registration| registration.source == handle);
            if let Some(registration) = registration {
                self.unregister_thread_pool(registration);
            }
        }
        Ok(())
    }

    pub(crate) fn make_tcp_socket(&self, family: i32) -> AsyncHostResult<HostHandle> {
        let socket = self.network.make_tcp_socket(family)?;
        Ok(self.resources.borrow_mut().insert_resource(socket))
    }

    pub(crate) fn make_udp_socket(
        &self,
        family: i32,
        multicast: bool,
    ) -> AsyncHostResult<HostHandle> {
        let socket = self.network.make_udp_socket(family, multicast)?;
        Ok(self.resources.borrow_mut().insert_resource(socket))
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
        Ok(self.resources.borrow_mut().insert_resource(socket))
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
        self.resources
            .borrow_mut()
            .insert_resource(Resource::new(raw_fd))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn kqueue_watcher_add_file(
        &self,
        kqueue_handle: HostHandle,
        file_handle: HostHandle,
    ) -> AsyncHostResult<()> {
        let kqueue = self.acquire_resource(kqueue_handle)?;
        let file = self.acquire_resource(file_handle)?;
        crate::filesystem::watch::add_file(&self.filesystem, &kqueue, &file, file_handle)
    }

    pub(crate) fn tls(&self) -> &HostTls {
        &self.tls
    }

    pub(crate) fn network(&self) -> &HostNetwork {
        &self.network
    }

    #[cfg(all(test, unix))]
    pub(crate) fn filesystem(&self) -> &HostFs {
        &self.filesystem
    }

    pub(crate) fn process(&self) -> &HostProcess {
        &self.process
    }

    pub(crate) fn process_inputs(&self) -> &ProcessInputs {
        &self.process_inputs
    }

    #[cfg(test)]
    pub(crate) fn check_owned_child_pid(&self, pid: i32) -> AsyncHostResult<()> {
        self.process.check_owned_child_pid(pid)
    }

    fn process_handle_pid(&self, handle: HostHandle) -> AsyncHostResult<Option<i32>> {
        if handle == self.invalid_fd() {
            return Ok(None);
        }
        self.process.process_handle_pid(handle)
    }

    pub(crate) fn open_process_handle(&self, pid: i32) -> AsyncHostResult<HostHandle> {
        match self.process.open_handle(pid)? {
            Some(resource) => {
                let handle = self.resources.borrow_mut().insert_resource(resource);
                self.process.track_process_handle(handle, pid);
                Ok(handle)
            }
            None => {
                // A PID-only target is a successful fallback, not an error.
                self.set_errno(0);
                Ok(self.invalid_fd())
            }
        }
    }

    pub(crate) fn read_process_exit(
        &self,
        handle: HostHandle,
        pid: i32,
        accept: impl FnOnce(i32) -> AsyncHostResult<()>,
    ) -> AsyncHostResult<()> {
        let resource = if handle == self.invalid_fd() {
            None
        } else {
            Some(self.acquire_resource(handle)?)
        };
        self.process.read_exit(
            pid,
            resource.as_deref().map(|resource| (handle, resource)),
            accept,
        )
    }

    pub(crate) fn make_wait_for_process_job(
        &self,
        handle: HostHandle,
        pid: i32,
    ) -> AsyncHostResult<HostHandle> {
        let tracked_pid = self.process_handle_pid(handle)?;
        let resource = if handle == self.invalid_fd() {
            None
        } else {
            Some(self.acquire_resource(handle)?)
        };
        let job = self.process.wait_job(resource, tracked_pid, pid)?;
        self.insert_job(job)
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

    pub(crate) fn with_resource<T>(
        &self,
        handle: HostHandle,
        f: impl FnOnce(&Resource) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let handles = self.resources.borrow();
        f(handles.resource(handle)?)
    }

    pub(crate) fn with_resource_of_class<T>(
        &self,
        handle: HostHandle,
        class: ResourceClass,
        f: impl FnOnce(&Resource) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let handles = self.resources.borrow();
        f(handles.resource_of_class(handle, class)?)
    }

    pub(crate) fn acquire_resource(&self, handle: HostHandle) -> AsyncHostResult<ResourceRef> {
        self.resources.borrow().acquire_resource(handle)
    }

    pub(crate) fn acquire_resource_of_class(
        &self,
        handle: HostHandle,
        class: ResourceClass,
    ) -> AsyncHostResult<ResourceRef> {
        self.resources
            .borrow()
            .acquire_resource_of_class(handle, class)
    }

    pub(crate) fn acquire_socket_resource(
        &self,
        handle: HostHandle,
    ) -> AsyncHostResult<ResourceRef> {
        self.resources.borrow().acquire_socket(handle)
    }

    pub(crate) fn pipe(
        &self,
        read_end_is_async: bool,
        write_end_is_async: bool,
    ) -> AsyncHostResult<[HostHandle; 2]> {
        let [read, write] = crate::filesystem::pipe(read_end_is_async, write_end_is_async)?;
        let read = Resource::from_file(read, crate::filesystem::FileGrant::UNTRACKED);
        let write = if write_end_is_async {
            Resource::async_pipe_writer(write)
        } else {
            Resource::from_file(write, crate::filesystem::FileGrant::UNTRACKED)
        };
        let mut resources = self.resources.borrow_mut();
        Ok([
            resources.insert_resource(read),
            resources.insert_resource(write),
        ])
    }

    pub(crate) fn stdio_kind(&self, handle: HostHandle) -> AsyncHostResult<i32> {
        self.with_resource(handle, |file| self.filesystem.resource_stdio_kind(file))
    }

    pub(crate) fn kind_of_fd(&self, handle: HostHandle) -> AsyncHostResult<i32> {
        self.with_resource(handle, |file| self.filesystem.resource_kind(file))
    }

    pub(crate) fn set_cloexec(&self, handle: HostHandle) -> AsyncHostResult<()> {
        self.with_resource(handle, |file| {
            #[cfg(unix)]
            {
                Ok(crate::filesystem::set_cloexec(file.as_file()?.as_raw_fd())?)
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
            self.filesystem
                .read(file, dst)
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
            self.filesystem
                .write(file, src)
                .and_then(|ret| i32::try_from(ret).map_err(|_| AsyncHostError::Fault))
        })
    }

    pub(crate) fn try_lock_file(&self, handle: HostHandle, exclusive: bool) -> AsyncHostResult<()> {
        self.with_resource_of_class(handle, ResourceClass::File, |file| {
            self.filesystem.try_lock_resource(file, exclusive)
        })
    }

    pub(crate) fn unlock_file(&self, handle: HostHandle) -> AsyncHostResult<()> {
        self.with_resource_of_class(handle, ResourceClass::File, |file| {
            self.filesystem.unlock_resource(file)
        })
    }

    pub(crate) fn run_job(&self, handle: u64) -> AsyncHostResult<()> {
        let key = key_from_handle(handle);
        let mut job = self.jobs.borrow_mut().take_ready_job(key)?;
        Self::run_policy_checked_job(&self.filesystem, &self.process, &mut job);
        self.restore_job(key, job)
    }

    fn run_policy_checked_job(filesystem: &HostFs, process: &HostProcess, job: &mut Job) {
        let preparation = match job.payload_mut() {
            JobPayload::Process(process_job) => process.prepare_job(process_job),
            _ => Ok(()),
        };
        if let Err(error) = preparation {
            job.set_err(error.errno());
            return;
        }
        thread_pool::run_host_job(job, filesystem);
        if let Err(error) = Self::finish_process_job(process, job) {
            job.set_err(error.errno());
        }
    }

    fn finish_process_job(process: &HostProcess, job: &Job) -> AsyncHostResult<()> {
        if let Ok(process_job) = job.process() {
            process.finish_job(process_job, job.ret(), job.err())?;
        }
        Ok(())
    }

    pub(crate) fn spawn_worker(&self, completion_id: i32, job_handle: u64) -> AsyncHostResult<u64> {
        let completion_id = WorkerCompletionId::from_abi(completion_id);
        let job_key = key_from_handle(job_handle);
        #[cfg(unix)]
        let completion_notifier = self.thread_pool_notifier()?;
        #[cfg(windows)]
        let completion_port = self.thread_pool_completion_target()?;

        let first_job = self.take_worker_job(completion_id, job_key)?;
        #[cfg(unix)]
        let completion = WorkerCompletionDestination::Pool(completion_notifier);
        #[cfg(windows)]
        let completion = WorkerCompletionDestination::Pool(completion_port);
        Ok(self.spawn_worker_thread(first_job, completion))
    }

    pub(crate) fn spawn_worker_with_pipe(
        &self,
        completion_id: i32,
        job_handle: u64,
        writer_handle: u64,
    ) -> AsyncHostResult<u64> {
        let job_key = key_from_handle(job_handle);
        // Validate and acquire the writer before consuming the one-shot Job.
        let notifier =
            crate::thread_pool::PipeCompletionNotifier::new(self.acquire_resource(writer_handle)?)?;
        let first_job =
            self.take_worker_job(WorkerCompletionId::from_abi(completion_id), job_key)?;
        Ok(self.spawn_worker_thread(first_job, WorkerCompletionDestination::Pipe(notifier)))
    }

    pub(crate) fn wake_worker(
        &self,
        worker_handle: u64,
        completion_id: i32,
        job_handle: u64,
    ) -> AsyncHostResult<()> {
        let completion_id = WorkerCompletionId::from_abi(completion_id);
        let worker_key = key_from_handle(worker_handle);
        let job_key = key_from_handle(job_handle);
        let unrun_job = self
            .workers
            .wake(worker_key, || self.take_worker_job(completion_id, job_key))?;
        if let Some(unrun_job) = unrun_job {
            self.restore_unrun_worker_job(unrun_job);
        }
        Ok(())
    }

    pub(crate) fn worker_enter_idle(&self, worker_handle: u64) -> AsyncHostResult<()> {
        let worker_key = key_from_handle(worker_handle);
        let unrun_job = self.workers.take_pending(worker_key)?;
        if let Some(unrun_job) = unrun_job {
            self.restore_unrun_worker_job(unrun_job);
        }
        Ok(())
    }

    pub(crate) fn free_worker(&self, worker_handle: u64) -> AsyncHostResult<()> {
        for unrun_job in self.workers.free(key_from_handle(worker_handle))? {
            self.restore_unrun_worker_job(unrun_job);
        }
        self.restore_completed_worker_jobs();
        Ok(())
    }

    pub(crate) fn cancel_worker(&self, worker_handle: u64) -> AsyncHostResult<i32> {
        let worker_key = key_from_handle(worker_handle);
        Ok(self.workers.cancel(worker_key)?.as_i32())
    }

    pub(crate) fn cancel_worker_with_retry(&self, worker_handle: u64) -> AsyncHostResult<i32> {
        let worker_key = key_from_handle(worker_handle);
        let status = self.workers.cancel_with_retry(
            worker_key,
            #[cfg(unix)]
            self.thread_pool_notifier()?,
        )?;
        // Waiting is published only after the worker sends its owned result.
        // Reacquire it even when this was originally a retry notification.
        if status == thread_pool::CancellationOutcome::JobFinished {
            self.restore_completed_worker_jobs();
        }
        Ok(status.as_i32())
    }

    #[cfg(unix)]
    pub(crate) fn thread_pool_child_signal_mask(&self) -> AsyncHostResult<libc::sigset_t> {
        self.thread_pool_registration
            .borrow()
            .is_some()
            .then_some(self.child_signal_mask)
            .ok_or(AsyncHostError::Badf)
    }

    #[cfg(windows)]
    pub(crate) fn thread_pool_completion_target(&self) -> AsyncHostResult<poll::CompletionPort> {
        self.thread_pool_registration
            .borrow()
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
        let key = key_from_handle(handle);
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key)?;
        let filesystem_job = job.filesystem()?;
        filesystem_job.copy_read_result(job.err(), memory, dst, offset, len)
    }

    pub(crate) fn get_read_console_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: u64,
        dst: u32,
        offset: u32,
        len: u32,
    ) -> AsyncHostResult<()> {
        let jobs = self.jobs.borrow();
        let job = jobs.visible_job(key_from_handle(handle))?;
        job.filesystem()?
            .copy_read_console_result(job.err(), memory, dst, offset, len)
    }

    pub(crate) fn get_file_time_result(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: u64,
        dst: u32,
    ) -> AsyncHostResult<()> {
        let key = key_from_handle(handle);
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
        let key = key_from_handle(handle);
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
        let key = key_from_handle(handle);
        let mut jobs = self.jobs.borrow_mut();
        let job = jobs.visible_job_mut(key)?;
        // Keep the mutable job borrow through publication so its ownership of
        // the resulting c_buffer changes atomically on the V8 thread.
        let job = job.filesystem_mut()?;
        job.publish_realpath_result(|buffer| self.buffers().publish(buffer))
    }

    #[cfg(unix)]
    fn thread_pool_notifier(&self) -> AsyncHostResult<Arc<ThreadPoolCompletionNotifier>> {
        self.thread_pool_registration
            .borrow()
            .as_ref()
            .map(|registration| Arc::clone(&registration.notifier))
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
            let completions = self.thread_pool_registration.borrow();
            let registration = completions.as_ref().ok_or(AsyncHostError::Badf)?;
            (Arc::clone(&registration.notifier), registration.source)
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

    fn spawn_worker_thread(
        &self,
        first_job: WorkerJob,
        completion: WorkerCompletionDestination,
    ) -> u64 {
        let filesystem = Arc::clone(&self.filesystem);
        let process_for_runner = self.process.clone();
        let worker = self.workers.spawn(
            first_job,
            move |worker_job| {
                Self::run_policy_checked_job(&filesystem, &process_for_runner, &mut worker_job.job);
            },
            completion,
        );
        handle_from_key(worker)
    }
}

impl Drop for AsyncHost {
    fn drop(&mut self) {
        self.destroy_thread_pool();
        #[cfg(windows)]
        self.io_results.get_mut().shutdown();
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
    #[cfg(unix)]
    use crate::filesystem::{FsOperation, FsPath};
    use crate::policy::{self, Policy};
    use crate::process::{Job as ProcessJob, SpawnOptions};
    use crate::runtime::WorkingDirectory;

    #[repr(align(2))]
    struct AlignedBytes<const N: usize>([u8; N]);

    fn resource_count(host: &AsyncHost) -> usize {
        host.resources.borrow().resource_count_excluding_reserved()
    }

    fn test_host(policy: Option<Policy>, environment: Env) -> AsyncHost {
        test_host_with_state(
            policy,
            environment,
            WorkingDirectory::Ambient,
            Rc::new(RefCell::new(HostKeys::default())),
        )
    }

    fn test_host_with_state(
        policy: Option<Policy>,
        environment: Env,
        working_directory: WorkingDirectory,
        keys: Rc<RefCell<HostKeys>>,
    ) -> AsyncHost {
        let environment = Arc::new(environment);
        let working_directory = Arc::new(working_directory);
        let filesystem = Arc::new(HostFs::with_policy(
            policy.as_ref().map(Policy::filesystem),
            Arc::clone(&environment),
            Arc::clone(&working_directory),
        ));
        let network = HostNetwork::new(policy.as_ref().map(Policy::network));
        let stdio = Arc::new(Stdio::Ambient);
        let process = HostProcess::new(
            policy,
            working_directory,
            Arc::clone(&stdio),
            None,
            Arc::clone(&environment),
        );
        #[cfg(unix)]
        let child_signal_mask = {
            let mut mask = unsafe { std::mem::zeroed::<libc::sigset_t>() };
            assert_eq!(unsafe { libc::sigemptyset(&mut mask) }, 0);
            mask
        };
        AsyncHost::new(
            environment,
            &stdio,
            filesystem,
            network,
            process,
            keys,
            crate::signal_channel().1,
            #[cfg(unix)]
            child_signal_mask,
        )
    }

    pub(super) fn default_host_with_keys(keys: Rc<RefCell<HostKeys>>) -> AsyncHost {
        test_host_with_state(None, Env::ambient(), WorkingDirectory::Ambient, keys)
    }

    pub(super) fn default_host() -> AsyncHost {
        test_host(None, Env::ambient())
    }

    #[cfg(unix)]
    #[test]
    fn poll_wait_without_external_termination_needs_no_spare_fds() {
        // Descriptor limits are process-global; isolate this test from other tests.
        const CHILD: &str = "MOONRUN_TEST_POLL_FD_LIMIT_CHILD";
        if std::env::var_os(CHILD).is_none() {
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "async_host::tests::poll_wait_without_external_termination_needs_no_spare_fds",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .timeout(std::time::Duration::from_secs(15))
                .assert()
                .success();
            return;
        }

        let host = default_host();
        let poll = host.poll_create().unwrap();
        let termination = TerminationRequest::default();
        let (_, receiver) = crate::termination_channel();
        let mut original = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        assert_eq!(
            unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut original) },
            0
        );
        // Existing descriptors remain valid, but no new pipe can be created.
        let limited = libc::rlimit {
            rlim_cur: 0,
            rlim_max: original.rlim_max,
        };
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limited) }, 0);
        let ordinary_wait = host.poll_wait(poll, 0, &termination);
        let interruptible_wait = host.poll_wait(poll, 0, &receiver.request);
        assert_eq!(
            unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &original) },
            0
        );

        assert_eq!(ordinary_wait, Ok(0));
        assert_eq!(
            interruptible_wait,
            Err(AsyncHostError::Native(libc::EMFILE))
        );
        // A failed wake-source allocation must leave binding retryable.
        assert_eq!(host.poll_wait(poll, 0, &receiver.request), Ok(0));
    }

    #[test]
    fn terminated_poll_does_not_publish_internal_wakeup() {
        let host = default_host();
        let poll = host.poll_create().unwrap();
        let (terminator, receiver) = crate::termination_channel();
        assert_eq!(host.poll_wait(poll, 0, &receiver.request), Ok(0));
        assert!(terminator.terminate(crate::RunTermination::Exit(7)));
        assert_eq!(host.poll_wait(poll, 0, &receiver.request), Ok(0));
        assert_eq!(host.poll_get_event(poll, 0), Err(AsyncHostError::Badf));
        host.poll_destroy(poll).unwrap();
    }

    #[test]
    fn async_resource_wait_requires_reserved_absence_sentinel() {
        let host = default_host();
        let job = host
            .make_wait_for_process_job(host.invalid_fd(), 1)
            .unwrap();
        host.free_job(job).unwrap();

        let buffer = host.buffers().publish(Box::default());
        for handle in [0, null_handle(), buffer, job] {
            assert_eq!(
                host.make_wait_for_process_job(handle, 1),
                Err(AsyncHostError::Badf)
            );
        }
        host.buffers().free(buffer).unwrap();
        assert!(host.leak_summary().is_none());
    }

    pub(super) fn host_with_policy(path: &std::path::Path) -> AsyncHost {
        let (policy, env) = policy::load_file(path).unwrap();
        test_host(Some(policy), env.realize().unwrap())
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
    fn no_policy_leaves_unknown_child_access_unrestricted() {
        let host = default_host();

        host.check_owned_child_pid(i32::MAX).unwrap();
    }

    #[test]
    fn spawn_job_builder_only_mutates_ready_spawn_jobs() {
        let host = default_host();
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
            let job = jobs.visible_job(key_from_handle(handle)).unwrap();
            let process_job = job.process().unwrap();
            assert_eq!(
                process_job.cwd().unwrap(),
                Some(std::ffi::OsStr::new("working-directory"))
            );
            #[cfg(windows)]
            assert!(process_job.no_console_window().unwrap());
        }

        let key = key_from_handle(handle);
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

        assert_eq!(host.with_job(job, |job| job.ret()).unwrap(), -1);
        assert_eq!(
            host.with_job(job, |job| job.err()).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        assert_eq!(
            host.check_owned_child_pid(i32::MAX),
            Err(AsyncHostError::PermissionDenied)
        );
        host.free_job(job).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn spawn_keeps_redirected_files_after_parent_handles_close() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input");
        let output = directory.path().join("output");
        let policy_file = directory.path().join("policy.json");
        std::fs::write(&input, b"delegated input").unwrap();

        for allowed in [false, true] {
            std::fs::write(&output, b"").unwrap();
            std::fs::write(
                &policy_file,
                serde_json::to_vec(&serde_json::json!({
                    "fs": {"read": ["*"], "write": ["*"]},
                    "process": {"spawn": allowed},
                }))
                .unwrap(),
            )
            .unwrap();
            let host = host_with_policy(&policy_file);
            let files = [(&input, 0), (&output, 1)].map(|(path, access)| {
                let open = host
                    .insert_job(FilesystemJob::open_legacy(
                        path.as_os_str().to_owned(),
                        access,
                        0,
                        false,
                        0,
                        0,
                    ))
                    .unwrap();
                host.run_job(open).unwrap();
                assert_eq!(host.with_job(open, |job| job.err()), Ok(0));
                let file = host.open_job_get_fd(open).unwrap();
                host.free_job(open).unwrap();
                file
            });
            let [stdin, stdout] = files.map(|file| host.acquire_resource(file).unwrap());
            let owners = [Arc::downgrade(&stdin), Arc::downgrade(&stdout)];
            let spawn = host
                .insert_job(ProcessJob::spawn_unix(
                    "/bin/cat".into(),
                    vec!["/bin/cat".into()],
                    Vec::new(),
                    Some(stdin),
                    Some(stdout),
                    None,
                    None,
                    SpawnOptions {
                        child_signal_mask: unsafe { std::mem::zeroed() },
                    },
                ))
                .unwrap();

            // The request owns acquired Resources before preparation or spawn.
            // Closing guest Handles must neither revoke those grants nor leave
            // the numeric Handles usable for another operation.
            for file in files {
                host.close_fd(file).unwrap();
                assert_eq!(
                    host.acquire_resource(file).unwrap_err(),
                    AsyncHostError::Badf
                );
            }
            assert!(owners.iter().all(|owner| owner.upgrade().is_some()));
            host.run_job(spawn).unwrap();
            if allowed {
                assert_eq!(host.with_job(spawn, |job| job.err()), Ok(0));
                let pid = host.with_job(spawn, |job| job.ret()).unwrap() as i32;
                let wait = host
                    .make_wait_for_process_job(host.invalid_fd(), pid)
                    .unwrap();
                host.run_job(wait).unwrap();
                assert_eq!(host.with_job(wait, |job| job.err()), Ok(0));
                assert_eq!(host.with_job(wait, |job| job.ret()), Ok(0));
                host.free_job(wait).unwrap();
            } else {
                assert_eq!(
                    host.with_job(spawn, |job| job.err()),
                    Ok(AsyncHostError::PermissionDenied.errno())
                );
            }
            host.free_job(spawn).unwrap();
            assert!(owners.iter().all(|owner| owner.upgrade().is_none()));
            assert_eq!(resource_count(&host), 0);
            assert_eq!(
                std::fs::read(&output).unwrap(),
                if allowed {
                    b"delegated input".as_slice()
                } else {
                    b""
                }
            );
        }
    }

    #[test]
    fn failed_spawn_worker_reports_error_and_frees_without_a_result_handle() {
        let directory = tempfile::tempdir().unwrap();
        let policy_file = directory.path().join("deny.toml");
        std::fs::write(&policy_file, "").unwrap();
        let program = directory.path().join("missing-executable.exe");
        for denied in [true, false] {
            let host = if denied {
                host_with_policy(&policy_file)
            } else {
                default_host()
            };
            let poll = host.poll_create().unwrap();
            let completion_source = host.init_thread_pool(poll).unwrap();
            let resources = resource_count(&host);
            #[cfg(unix)]
            let spawn = ProcessJob::spawn_unix(
                program.as_os_str().to_owned(),
                vec![program.as_os_str().to_owned()],
                Vec::new(),
                None,
                None,
                None,
                None,
                SpawnOptions {
                    child_signal_mask: unsafe { std::mem::zeroed() },
                },
            );
            #[cfg(windows)]
            let spawn = ProcessJob::spawn_windows(
                format!("\"{}\"", program.display()).into(),
                vec![0, 0],
                None,
                None,
                None,
                None,
                SpawnOptions {
                    no_console_window: true,
                    is_orphan: true,
                },
            );
            let job = host.insert_job(spawn).unwrap();
            let worker = host.spawn_worker(42, job).unwrap();

            assert_eq!(host.poll_wait(poll, 1000, &Default::default()), Ok(1));
            #[cfg(unix)]
            {
                let mut memory = [0; 4];
                assert_eq!(
                    host.fetch_completion(memory.as_mut_slice(), completion_source, 0, 1),
                    Ok(4)
                );
                assert_eq!(i32::from_le_bytes(memory), 42);
            }
            #[cfg(windows)]
            {
                let event = host.poll_get_event(poll, 0).unwrap();
                assert_eq!(host.poll_event_fd(event).unwrap(), completion_source);
                assert_eq!(host.poll_event_bytes_transferred(event).unwrap(), 42);
            }

            let expected_error = if denied {
                AsyncHostError::PermissionDenied.errno()
            } else {
                #[cfg(unix)]
                let error = libc::ENOENT;
                #[cfg(windows)]
                let error = windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND as i32;
                error
            };
            // async's perform_job_in_worker raises on job.err before the spawn
            // wrapper reads a result handle. Its deferred free must work for
            // both preparation failure and failure in native process creation.
            assert_eq!(host.with_job(job, |job| job.err()), Ok(expected_error));
            assert_eq!(host.with_job(job, |job| job.ret()), Ok(-1));
            host.free_job(job).unwrap();
            assert_eq!(
                host.with_job(job, |job| job.err()),
                Err(AsyncHostError::Badf)
            );
            assert_eq!(resource_count(&host), resources);
            host.free_worker(worker).unwrap();
            host.destroy_thread_pool();
        }
    }

    #[cfg(unix)]
    #[test]
    fn process_spawn_uses_runtime_path_without_replacing_child_environment() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let caller_bin = directory.path().join("caller-bin");
        let child_bin = directory.path().join("child-bin");
        std::fs::create_dir(&caller_bin).unwrap();
        std::fs::create_dir(&child_bin).unwrap();
        let program = "moonrun-owned-path-probe";
        for (bin, code) in [(&caller_bin, 17), (&child_bin, 29)] {
            let executable = bin.join(program);
            std::fs::write(
                &executable,
                format!(
                    "#!/bin/sh\ntest \"$PATH\" = \"$EXPECTED_CHILD_PATH\" || exit 91\ntest \"$1\" = allowed || exit 92\nexit {code}\n"
                ),
            )
            .unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let environment = Env::owned([("PATH".into(), caller_bin.into_os_string())]).unwrap();
        let (policy, _) = policy::load_json(
            br#"{"process":{"allow":[{"program":"moonrun-owned-path-probe","args_prefix":["allowed"]}]}}"#,
            &environment,
        )
        .unwrap();
        let host = test_host(Some(policy), environment);
        let mut spawn: Job = ProcessJob::spawn_unix(
            program.into(),
            vec![program.into(), "allowed".into()],
            vec![
                format!("PATH={}", child_bin.display()).into(),
                format!("EXPECTED_CHILD_PATH={}", child_bin.display()).into(),
            ],
            None,
            None,
            None,
            None,
            SpawnOptions {
                child_signal_mask: unsafe { std::mem::zeroed() },
            },
        )
        .into();

        AsyncHost::run_policy_checked_job(&host.filesystem, &host.process, &mut spawn);
        assert_eq!(spawn.err(), 0, "spawn must search the Runtime's owned PATH");
        let mut wait: Job = host
            .process
            .wait_job(None, None, spawn.ret() as i32)
            .unwrap()
            .into();
        AsyncHost::run_policy_checked_job(&host.filesystem, &host.process, &mut wait);
        assert_eq!(wait.err(), 0);
        assert_eq!(wait.ret(), 17, "child PATH must not select the executable");
    }

    #[cfg(unix)]
    #[test]
    fn process_spawn_resolves_relative_paths_at_the_fixed_child_directory() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(directory.path()).unwrap();
        for (cwd, code) in [(root.clone(), 17), (root.join("child"), 29)] {
            std::fs::create_dir_all(cwd.join("bin")).unwrap();
            let executable = cwd.join("bin/moonrun-relative-path-probe");
            std::fs::write(
                &executable,
                format!("#!/bin/sh\ntest \"$PWD\" = \"$EXPECTED_CWD\" || exit 91\nexit {code}\n"),
            )
            .unwrap();
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let host = test_host_with_state(
            None,
            Env::owned([("PATH".into(), "bin".into())]).unwrap(),
            WorkingDirectory::Fixed(root.clone()),
            Rc::new(RefCell::new(HostKeys::default())),
        );
        for (program, child_cwd, expected_cwd, expected_code) in [
            ("moonrun-relative-path-probe", None, root.clone(), 17),
            (
                "moonrun-relative-path-probe",
                Some("child"),
                root.join("child"),
                29,
            ),
            (
                "bin/moonrun-relative-path-probe",
                Some("child"),
                root.join("child"),
                29,
            ),
        ] {
            let mut spawn: Job = ProcessJob::spawn_unix(
                program.into(),
                vec![program.into()],
                vec![format!("EXPECTED_CWD={}", expected_cwd.display()).into()],
                None,
                None,
                None,
                child_cwd.map(Into::into),
                SpawnOptions {
                    child_signal_mask: unsafe { std::mem::zeroed() },
                },
            )
            .into();
            AsyncHost::run_policy_checked_job(&host.filesystem, &host.process, &mut spawn);
            assert_eq!(spawn.err(), 0, "program={program}, child_cwd={child_cwd:?}");
            let mut wait: Job = host
                .process
                .wait_job(None, None, spawn.ret() as i32)
                .unwrap()
                .into();
            AsyncHost::run_policy_checked_job(&host.filesystem, &host.process, &mut wait);
            assert_eq!(wait.err(), 0);
            assert_eq!(wait.ret(), expected_code);
        }
    }

    #[test]
    fn process_spawn_denial_precedes_lookup_and_policy_transfer() {
        const CHILD: &str = "MOONRUN_DENIED_SPAWN_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let directory = tempfile::tempdir().unwrap();
            let missing = directory.path().join("missing");
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "async_host::tests::process_spawn_denial_precedes_lookup_and_policy_transfer",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env("TMPDIR", &missing)
                .env("TMP", &missing)
                .env("TEMP", &missing)
                .timeout(std::time::Duration::from_secs(10))
                .assert()
                .success();
            return;
        }

        // The request would fail both executable lookup and inheritance-file
        // creation. Neither failure may replace the earlier policy denial.
        let environment =
            Env::owned([("PATH".into(), std::env::temp_dir().into_os_string())]).unwrap();
        let (policy, _) = policy::load_json(b"{}", &environment).unwrap();
        let host = test_host(Some(policy), environment);
        #[cfg(unix)]
        let mut spawn: Job = ProcessJob::spawn_unix(
            "moonx".into(),
            vec!["moonx".into()],
            Vec::new(),
            None,
            None,
            None,
            Some(OsString::new()),
            SpawnOptions {
                child_signal_mask: unsafe { std::mem::zeroed() },
            },
        )
        .into();
        #[cfg(windows)]
        let mut spawn: Job = ProcessJob::spawn_windows(
            "moonx.exe".into(),
            vec![0, 0],
            None,
            None,
            None,
            Some(OsString::new()),
            SpawnOptions {
                no_console_window: true,
                is_orphan: true,
            },
        )
        .into();

        let resources = resource_count(&host);
        AsyncHost::run_policy_checked_job(&host.filesystem, &host.process, &mut spawn);
        assert_eq!(spawn.ret(), -1);
        assert_eq!(spawn.err(), AsyncHostError::PermissionDenied.errno());
        assert_eq!(resource_count(&host), resources);
    }

    #[cfg(windows)]
    #[test]
    fn process_spawn_windows_uses_caller_directory_before_child_directory() {
        use std::os::windows::ffi::OsStrExt;

        const CHILD: &str = "MOONRUN_WINDOWS_SPAWN_LOOKUP_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let expected = std::path::PathBuf::from(std::env::var_os(CHILD).unwrap());
            assert_eq!(
                dunce::canonicalize(std::env::current_exe().unwrap().parent().unwrap()).unwrap(),
                dunce::canonicalize(&expected).unwrap()
            );
            assert_eq!(
                dunce::canonicalize(std::env::current_dir().unwrap()).unwrap(),
                dunce::canonicalize(expected.join("child")).unwrap()
            );
            return;
        }

        let directory = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(directory.path()).unwrap();
        let child_directory = root.join("child");
        std::fs::create_dir(&child_directory).unwrap();
        for bin in [&root, &child_directory] {
            std::fs::copy(
                std::env::current_exe().unwrap(),
                bin.join("moonrun-caller-directory-probe.exe"),
            )
            .unwrap();
        }
        let host = test_host_with_state(
            None,
            Env::owned([]).unwrap(),
            WorkingDirectory::Fixed(root.clone()),
            Rc::new(RefCell::new(HostKeys::default())),
        );
        let env = [
            format!("{CHILD}={}", root.display()),
            format!("PATH={}", child_directory.display()),
        ]
        .into_iter()
        .flat_map(|entry| {
            OsString::from(entry)
                .encode_wide()
                .chain([0])
                .collect::<Vec<_>>()
        })
        .chain([0])
        .collect::<Vec<_>>();
        // Missing Runtime PATH still permits the platform's system-directory
        // search, but a child's cwd/PATH never replace the caller's search cwd.
        for (command_line, expected_code) in [
            ("cmd.exe /D /C exit 17".to_owned(), 17),
            ("moonrun-caller-directory-probe.exe --exact async_host::tests::process_spawn_windows_uses_caller_directory_before_child_directory --nocapture".to_owned(), 0),
            ("moonrun-caller-directory-probe --exact async_host::tests::process_spawn_windows_uses_caller_directory_before_child_directory --nocapture".to_owned(), 0),
            (".\\moonrun-caller-directory-probe.exe --exact async_host::tests::process_spawn_windows_uses_caller_directory_before_child_directory --nocapture".to_owned(), 0),
        ] {
            let mut spawn: Job = ProcessJob::spawn_windows(
                command_line.clone().into(),
                env.clone(),
                None,
                None,
                None,
                Some("child".into()),
                SpawnOptions { no_console_window: true, is_orphan: true },
            )
            .into();
            AsyncHost::run_policy_checked_job(&host.filesystem, &host.process, &mut spawn);
            assert_eq!(spawn.err(), 0, "command_line={command_line}");
            let Some(ResourcePublication::Unpublished(handle)) =
                spawn.process_mut().unwrap().take_spawn_result().unwrap()
            else {
                panic!("Windows spawn did not return a process handle");
            };
            let mut wait: Job = host
                .process
                .wait_job(Some(Arc::new(handle)), None, spawn.ret() as i32)
                .unwrap()
                .into();
            AsyncHost::run_policy_checked_job(&host.filesystem, &host.process, &mut wait);
            assert_eq!(wait.err(), 0);
            assert_eq!(wait.ret(), expected_code, "command_line={command_line}");
        }
    }

    #[test]
    fn native_process_result_checks_identity_and_revokes_before_copying() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[process]\nspawn = true\n").unwrap();
        let host = host_with_policy(&policy_file);
        assert_eq!(
            host.open_process_handle(std::process::id() as i32),
            Err(AsyncHostError::PermissionDenied)
        );
        let spawn = host.insert_job(successful_process_job()).unwrap();
        host.run_job(spawn).unwrap();
        assert_eq!(host.with_job(spawn, |job| job.err()).unwrap(), 0);
        let pid = host.with_job(spawn, |job| job.ret()).unwrap() as i32;
        let handle = host.get_spawn_job_result_handle(spawn).unwrap();
        assert_eq!(
            host.read_process_exit(handle, pid.wrapping_add(1), |_| panic!("wrong PID")),
            Err(AsyncHostError::PermissionDenied)
        );
        host.check_owned_child_pid(pid).unwrap();

        // Native waiting has already observed/reaped the child when the ABI
        // copies its result. A failed copy must not restore child authority.
        #[cfg(unix)]
        let pending = libc::EAGAIN;
        #[cfg(windows)]
        let pending = windows_sys::Win32::Foundation::ERROR_IO_PENDING as i32;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let result = host.read_process_exit(handle, pid, |code| {
                assert_eq!(code, 0);
                assert_eq!(
                    host.check_owned_child_pid(pid),
                    Err(AsyncHostError::PermissionDenied)
                );
                Err(AsyncHostError::Fault)
            });
            if result != Err(AsyncHostError::Native(pending)) {
                assert_eq!(result, Err(AsyncHostError::Fault));
                break;
            }
            host.check_owned_child_pid(pid).unwrap();
            assert!(std::time::Instant::now() < deadline, "child did not exit");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        if handle != host.invalid_fd() {
            host.close_fd(handle).unwrap();
        }
        host.free_job(spawn).unwrap();
    }

    #[test]
    fn process_policy_tracks_spawned_children_until_wait_completes() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[process]\nspawn = true\n").unwrap();
        let host = host_with_policy(&policy_file);
        let spawn_job = host.insert_job(successful_process_job()).unwrap();

        host.run_job(spawn_job).unwrap();

        assert_eq!(host.with_job(spawn_job, |job| job.err()).unwrap(), 0);
        let pid = host.with_job(spawn_job, |job| job.ret()).unwrap() as i32;
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
            crate::process::os::process_id(
                process_resource
                    .as_ref()
                    .unwrap()
                    .as_handle()
                    .unwrap()
                    .as_raw_handle()
            )
            .unwrap(),
            pid as u32
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

        assert_eq!(host.with_job(wait_job, |job| job.err()).unwrap(), 0);
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
            host.with_job(wait_job, |job| job.err()).unwrap(),
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
        host.process
            .prepare_job(job.process_mut().unwrap())
            .unwrap();
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
        let host = default_host();
        let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();

        assert_eq!(
            host.get_spawn_job_result_handle(job),
            Err(AsyncHostError::Badf)
        );
        host.free_job(job).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn process_policy_preserves_unsigned_windows_pid_bits() {
        let tmp = tempfile::tempdir().unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[process]\nspawn = true\n").unwrap();
        let host = host_with_policy(&policy_file);
        let mut job = successful_process_job();
        host.process
            .prepare_job(job.process_mut().unwrap())
            .unwrap();
        job.set_ret(i64::from(0x8000_0000u32));

        AsyncHost::finish_process_job(&host.process, &job).unwrap();

        host.check_owned_child_pid(i32::MIN).unwrap();
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
        let host = default_host();
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
        let host = default_host();

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
            for fd in [read_fd, write_fd] {
                let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
                assert!(flags >= 0);
                assert_ne!(flags & libc::FD_CLOEXEC, 0);
            }

            host.close_fd(read).unwrap();
            host.close_fd(write).unwrap();
        }
    }

    #[test]
    fn socket_options_reject_tcp_and_udp_mixups() {
        #[cfg(windows)]
        assert_eq!(crate::network::os::init_wsa(), 0);

        let host = default_host();
        let tcp = host.make_tcp_socket(4).unwrap();
        let udp = host.make_udp_socket(4, false).unwrap();

        assert!(
            host.with_resource(tcp, |socket| host.network().disable_nagle(socket))
                .is_ok()
        );
        assert_eq!(host.acquire_resource(tcp).unwrap().socket_family(), Some(4));
        assert_eq!(
            host.with_resource(tcp, |socket| host
                .network()
                .set_multicast_loopback(socket, true, 4)),
            Err(AsyncHostError::Inval)
        );
        assert!(
            host.with_resource(udp, |socket| host
                .network()
                .set_multicast_loopback(socket, true, 4))
                .is_ok()
        );
        assert_eq!(
            host.with_resource(udp, |socket| host.network().disable_nagle(socket)),
            Err(AsyncHostError::Inval)
        );

        host.close_fd(tcp).unwrap();
        host.close_fd(udp).unwrap();

        #[cfg(windows)]
        assert_eq!(crate::network::os::cleanup_wsa(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn signal_handler_reconfigures_restarts_and_detaches_with_the_pool() {
        let (sender, receiver) = crate::signal_channel();
        let mut host = default_host();
        host.signals = receiver;
        let poll = host.poll_create().unwrap();
        host.init_thread_pool(poll).unwrap();
        let notifier = host.thread_pool_notifier().unwrap();
        let all = [libc::SIGINT, libc::SIGTERM];
        host.set_cancellation_signals(&all, &[libc::SIGINT])
            .unwrap();
        assert_eq!(sender.send(libc::SIGINT), Ok(false));
        host.start_signal_handler().unwrap();
        host.start_signal_handler().unwrap();
        assert_eq!(host.workers.len(), 0);
        assert_eq!(sender.send(libc::SIGINT), Ok(true));
        let mut event = [0; 4];
        assert_eq!(notifier.fetch(&mut event), Ok(4));
        assert_eq!(i32::from_ne_bytes(event), libc::SIGINT | i32::MIN);

        host.set_cancellation_signals(&all, &[libc::SIGTERM])
            .unwrap();
        assert_eq!(sender.send(libc::SIGINT), Ok(false));
        assert_eq!(sender.send(libc::SIGTERM), Ok(true));
        assert_eq!(notifier.fetch(&mut event), Ok(4));
        assert_eq!(i32::from_ne_bytes(event), libc::SIGTERM | i32::MIN);

        host.terminate_signal_handler();
        assert_eq!(sender.send(libc::SIGTERM), Ok(false));
        host.set_cancellation_signals(&all, &all).unwrap();
        host.start_signal_handler().unwrap();
        assert_eq!(sender.send(libc::SIGINT), Ok(true));
        assert_eq!(notifier.fetch(&mut event), Ok(4));
        host.destroy_thread_pool();
        assert_eq!(sender.send(libc::SIGINT), Ok(false));
        assert!(host.signal_handler.borrow().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn signal_bursts_do_not_wait_for_worker_completions() {
        let (sender, receiver) = crate::signal_channel();
        let mut host = default_host();
        host.signals = receiver;
        let poll = host.poll_create().unwrap();
        let source = host.init_thread_pool(poll).unwrap();
        let signals = [libc::SIGINT, libc::SIGTERM];
        host.set_cancellation_signals(&signals, &signals).unwrap();
        host.start_signal_handler().unwrap();
        host.thread_pool_notifier()
            .unwrap()
            .fill_with_completions(17);

        let (finished, result) = std::sync::mpsc::channel();
        let broker = std::thread::spawn(move || {
            let delivered = (0..65536).try_for_each(|index| {
                sender
                    .send(signals[index % signals.len()])
                    .map(|accepted| assert!(accepted))
            });
            finished.send(delivered).unwrap();
        });
        let delivered = result.recv_timeout(std::time::Duration::from_secs(2));
        if delivered.is_err() {
            // Release a blocked writer so the regression fails without leaking a thread.
            host.destroy_thread_pool();
        }
        broker.join().unwrap();
        assert_eq!(
            delivered,
            Ok(Ok(())),
            "signal delivery blocked on the full worker pipe"
        );

        let mut memory = [0; 12];
        assert_eq!(
            host.fetch_completion(memory.as_mut_slice(), source, 0, 3)
                .unwrap(),
            12
        );
        let events: Vec<_> = memory
            .chunks_exact(4)
            .map(|bytes| i32::from_ne_bytes(bytes.try_into().unwrap()))
            .collect();
        assert_eq!(
            events,
            [libc::SIGINT | i32::MIN, libc::SIGTERM | i32::MIN, 17]
        );
        assert_eq!(host.workers.len(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn signal_notifications_remain_ready_until_fetched_and_are_discarded_on_detach() {
        let (sender, receiver) = crate::signal_channel();
        let mut host = default_host();
        host.signals = receiver;
        let poll = host.poll_create().unwrap();
        let source = host.init_thread_pool(poll).unwrap();
        let signals = [libc::SIGINT, libc::SIGTERM];
        host.set_cancellation_signals(&signals, &signals).unwrap();
        host.start_signal_handler().unwrap();
        for signal in signals {
            assert_eq!(sender.send(signal), Ok(true));
        }
        let mut memory = [0; 4];
        assert_eq!(host.poll_wait(poll, 1000, &Default::default()).unwrap(), 1);
        assert_eq!(
            host.poll_event_fd(host.poll_get_event(poll, 0).unwrap())
                .unwrap(),
            source
        );
        assert_eq!(
            host.fetch_completion(memory.as_mut_slice(), source, 0, 0)
                .unwrap(),
            0
        );
        for signal in signals {
            assert_eq!(host.poll_wait(poll, 0, &Default::default()).unwrap(), 1);
            assert_eq!(
                host.fetch_completion(memory.as_mut_slice(), source, 0, 1)
                    .unwrap(),
                4
            );
            assert_eq!(i32::from_ne_bytes(memory), signal | i32::MIN);
        }
        assert_eq!(host.poll_wait(poll, 0, &Default::default()).unwrap(), 0);

        assert_eq!(sender.send(libc::SIGTERM), Ok(true));
        host.terminate_signal_handler();
        assert_eq!(host.poll_wait(poll, 0, &Default::default()).unwrap(), 0);
        assert_eq!(sender.send(libc::SIGTERM), Ok(false));
        host.set_cancellation_signals(&signals, &signals).unwrap();
        host.start_signal_handler().unwrap();
        assert_eq!(
            host.fetch_completion(memory.as_mut_slice(), source, 0, 1)
                .unwrap(),
            0
        );
        assert_eq!(sender.send(libc::SIGINT), Ok(true));
        assert_eq!(host.poll_wait(poll, 1000, &Default::default()).unwrap(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn signal_delivery_detaches_when_its_poll_or_completion_source_closes() {
        for close_poll in [false, true] {
            let (sender, receiver) = crate::signal_channel();
            let mut host = default_host();
            host.signals = receiver;
            let poll = host.poll_create().unwrap();
            let source = host.init_thread_pool(poll).unwrap();
            host.set_cancellation_signals(&[libc::SIGINT], &[libc::SIGINT])
                .unwrap();
            host.start_signal_handler().unwrap();
            assert_eq!(sender.send(libc::SIGINT), Ok(true));
            if close_poll {
                host.poll_destroy(poll).unwrap();
            } else {
                host.close_fd(source).unwrap();
            }
            assert_eq!(sender.send(libc::SIGINT), Ok(false));
            assert!(host.signal_handler.borrow().is_none());
        }
    }

    #[cfg(unix)]
    #[test]
    fn thread_pool_exposes_its_concrete_child_signal_mask_only_while_active() {
        let mut child_signal_mask = unsafe { std::mem::zeroed::<libc::sigset_t>() };
        assert_eq!(unsafe { libc::sigemptyset(&mut child_signal_mask) }, 0);
        assert_eq!(
            unsafe { libc::sigaddset(&mut child_signal_mask, libc::SIGINT) },
            0
        );

        let mut host = default_host();
        host.child_signal_mask = child_signal_mask;
        assert!(matches!(
            host.thread_pool_child_signal_mask(),
            Err(AsyncHostError::Badf)
        ));

        let poll = host.poll_create().unwrap();
        host.init_thread_pool(poll).unwrap();

        let child = host.thread_pool_child_signal_mask().unwrap();
        assert_eq!(unsafe { libc::sigismember(&child, libc::SIGINT) }, 1);
        assert_eq!(unsafe { libc::sigismember(&child, libc::SIGTERM) }, 0);

        host.destroy_thread_pool();
        assert!(matches!(
            host.thread_pool_child_signal_mask(),
            Err(AsyncHostError::Badf)
        ));
    }

    #[test]
    fn dropping_host_retires_poll_handles() {
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let host = default_host_with_keys(keys.clone());
        let owner = host.poll_create().unwrap();
        let unrelated = host.poll_create().unwrap();
        host.init_thread_pool(owner).unwrap();
        assert_eq!(host.poll_wait(owner, 0, &Default::default()), Ok(0));

        for handle in [owner, unrelated] {
            assert_eq!(
                keys.borrow().kind(key_from_handle(handle)),
                Some(HandleKind::Poll)
            );
        }
        drop(host);
        for handle in [owner, unrelated] {
            assert_eq!(keys.borrow().kind(key_from_handle(handle)), None);
        }
    }

    #[test]
    fn dropping_host_retires_resource_handles_but_preserves_acquired_resources() {
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let host = default_host_with_keys(keys.clone());
        let [read, write] = host.pipe(false, false).unwrap();
        let acquired = host.acquire_resource(read).unwrap();
        let weak = Arc::downgrade(&acquired);
        let mut handles = vec![host.invalid_fd(), read, write];
        for id in [STDIN_ID, STDOUT_ID, STDERR_ID] {
            if let Ok(handle) = host.std_handle(id) {
                assert_eq!(host.close_fd(handle), Err(AsyncHostError::Badf));
                handles.push(handle);
            }
        }
        assert_eq!(host.close_fd(host.invalid_fd()), Err(AsyncHostError::Badf));

        drop(host);

        // The Runtime namespace may outlive a domain. Its old registrations
        // must disappear even when an acquired Resource still has an owner.
        for handle in handles {
            assert_eq!(keys.borrow().kind(key_from_handle(handle)), None);
        }
        assert_eq!(Arc::strong_count(&acquired), 1);
        assert!(weak.upgrade().is_some());
        drop(acquired);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn reserved_resources_share_stdio_aliases_and_reject_close() {
        let file = tempfile::tempfile().unwrap();
        #[cfg(unix)]
        let raw = file.as_raw_fd();
        #[cfg(windows)]
        let raw = file.as_raw_handle();
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let mut resources = ResourceHandles::new(keys.clone(), [Some(raw), Some(raw), None]);
        let [input, output, absent] = resources.stdio_resources;
        assert_eq!(input, output);
        assert_eq!(absent, resources.invalid_resource);
        assert!(resources.resource(handle_from_key(input)).is_ok());
        assert!(matches!(
            resources.resource(handle_from_key(absent)),
            Err(AsyncHostError::Badf)
        ));
        for key in [input, output, absent] {
            assert!(matches!(
                resources.remove_resource(handle_from_key(key)),
                Err(AsyncHostError::Badf)
            ));
        }
        assert!(resources.leak_summary().is_none());
        drop(resources);
        for key in [input, output, absent] {
            assert_eq!(keys.borrow().kind(key), None);
        }
        // Reserved entries borrow stdio; only their Handle registrations retire.
        assert!(file.metadata().is_ok());
    }

    #[test]
    fn poll_handles_reject_stale_and_other_family_handles() {
        let host = default_host();
        let old = host.poll_create().unwrap();
        host.poll_destroy(old).unwrap();
        let replacement = host.poll_create().unwrap();
        assert_ne!(old, replacement);
        let buffer = host
            .buffers()
            .publish(b"retained".to_vec().into_boxed_slice());
        host.init_thread_pool(replacement).unwrap();
        assert_eq!(host.poll_wait(replacement, 0, &Default::default()), Ok(0));

        // Polls require a live Handle, including for destruction.
        for handle in [0, null_handle(), host.invalid_fd(), buffer, old] {
            assert_eq!(host.poll_destroy(handle), Err(AsyncHostError::Badf));
            assert_eq!(
                host.poll_wait(handle, 0, &Default::default()),
                Err(AsyncHostError::Badf)
            );
            assert_eq!(host.poll_get_event(handle, 0), Err(AsyncHostError::Badf));
            assert_eq!(host.init_thread_pool(handle), Err(AsyncHostError::Badf));
        }
        assert_eq!(host.poll_wait(replacement, 0, &Default::default()), Ok(0));
        assert_eq!(
            host.init_thread_pool(replacement),
            Err(AsyncHostError::Inval)
        );
        host.buffers()
            .with_buffer(buffer, |bytes| {
                assert_eq!(bytes, b"retained");
                Ok(())
            })
            .unwrap();

        host.poll_destroy(replacement).unwrap();
        host.buffers().free(buffer).unwrap();
        assert!(host.leak_summary().is_none());
    }

    #[test]
    fn closing_unrelated_poll_preserves_thread_pool_registration() {
        let host = default_host();
        let owner = host.poll_create().unwrap();
        let unrelated = host.poll_create().unwrap();
        let source = host.init_thread_pool(owner).unwrap();
        assert_eq!(host.init_thread_pool(unrelated), Err(AsyncHostError::Inval));

        #[cfg(unix)]
        host.thread_pool_notifier().unwrap().notify(17).unwrap();
        #[cfg(windows)]
        poll::post_thread_pool_completion(&host.thread_pool_completion_target().unwrap(), 17)
            .unwrap();
        assert_eq!(host.poll_wait(owner, 1000, &Default::default()), Ok(1));
        let event = host.poll_get_event(owner, 0).unwrap();
        host.poll_destroy(unrelated).unwrap();
        assert_eq!(host.poll_get_event(owner, 0), Ok(event));
        assert_eq!(host.poll_event_fd(event), Ok(source));

        host.poll_destroy(owner).unwrap();
        assert_eq!(host.poll_event_fd(event), Err(AsyncHostError::Badf));
        let replacement = host.poll_create().unwrap();
        assert_eq!(host.poll_event_fd(event), Err(AsyncHostError::Badf));
        assert_eq!(host.poll_get_event(owner, 0), Err(AsyncHostError::Badf));
        host.init_thread_pool(replacement).unwrap();
        host.destroy_thread_pool();
        host.poll_destroy(replacement).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn closing_completion_poll_unregisters_source_from_other_polls() {
        let host = default_host();
        let owner = host.poll_create().unwrap();
        let observer = host.poll_create().unwrap();
        let source = host.init_thread_pool(owner).unwrap();
        assert_eq!(host.poll_register(observer, source, true), Ok(1));
        // Acquired references keep the pipe alive after its guest handle closes.
        let _reader = host.acquire_resource(source).unwrap();
        let notifier = host.thread_pool_notifier().unwrap();

        host.poll_destroy(owner).unwrap();
        notifier.notify(17).unwrap();
        assert_eq!(host.poll_wait(observer, 0, &Default::default()), Ok(0));
        assert!(matches!(
            host.acquire_resource(source),
            Err(AsyncHostError::Badf)
        ));
        host.poll_destroy(observer).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn completion_source_is_resource_handle() {
        let host = default_host();
        let poll = host.poll_create().unwrap();
        let completion_source = host.init_thread_pool(poll).unwrap();
        host.thread_pool_notifier().unwrap().notify(17).unwrap();
        assert_eq!(host.poll_wait(poll, 1000, &Default::default()).unwrap(), 1);
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
        let host = default_host();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let job = FilesystemJob::read(Arc::new(Resource::invalid()), 3, -1);
        let job_handle = host.insert_job(job).unwrap();
        {
            let mut jobs = host.jobs.borrow_mut();
            let job = jobs.visible_job_mut(key_from_handle(job_handle)).unwrap();
            let job = job.filesystem_mut().unwrap();
            job.set_read_result(b"abc".to_vec()).unwrap();
            host.thread_pool_notifier().unwrap().notify(42).unwrap();
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
        let host = default_host();
        let error = AsyncHostError::Inval.errno();
        let job = host
            .insert_job(thread_pool::make_failed_job(error))
            .unwrap();
        let mut memory = [0xaa; 8];

        host.run_job(job).unwrap();
        assert_eq!(host.with_job(job, |job| job.err()).unwrap(), error);
        host.get_stat_result(memory.as_mut_slice(), job, 0, 8)
            .unwrap();

        assert_eq!(memory, [0xaa; 8]);
    }

    #[test]
    fn address_info_handles_preserve_results_and_reject_other_families() {
        #[cfg(windows)]
        assert_eq!(crate::network::os::init_wsa(), 0);

        let host = default_host();
        let job = host
            .make_getaddrinfo_job(OsString::from("127.0.0.1"))
            .unwrap();
        host.run_job(job).unwrap();
        let expected = host
            .with_job(job, |job| {
                assert_eq!(job.err(), 0);
                assert_eq!(job.ret(), 0);
                let JobPayload::Network(job) = job.payload() else {
                    panic!()
                };
                host.network.getaddrinfo_result(job).unwrap()
            })
            .unwrap();
        assert!(!expected.is_empty());
        let head = host.get_getaddrinfo_result(job).unwrap();
        let buffer = host
            .buffers()
            .publish(b"buffer".to_vec().into_boxed_slice());

        assert_eq!(host.addrinfo_addr(buffer), Err(AsyncHostError::Badf));
        assert_eq!(host.addrinfo_next(buffer), Err(AsyncHostError::Badf));
        assert_eq!(host.free_addrinfo(buffer), Err(AsyncHostError::Badf));
        assert_eq!(
            host.buffers().with_buffer(head, |_| Ok(())),
            Err(AsyncHostError::Badf)
        );
        assert_eq!(
            host.buffers().with_buffer_mut(head, |_| Ok(())),
            Err(AsyncHostError::Badf)
        );
        assert!(matches!(
            host.buffers().lease(head),
            Err(AsyncHostError::Badf)
        ));
        assert_eq!(host.buffers().free(head), Err(AsyncHostError::Badf));

        let mut handles = Vec::new();
        let mut current = head;
        for addr in expected {
            assert_eq!(host.addrinfo_addr(current).unwrap(), addr);
            handles.push(current);
            current = host.addrinfo_next(current).unwrap();
        }
        assert_eq!(current, null_handle());
        host.free_addrinfo(head).unwrap();
        let replacement = host
            .buffers()
            .publish(b"replacement".to_vec().into_boxed_slice());
        for handle in handles {
            assert_eq!(host.addrinfo_addr(handle), Err(AsyncHostError::Badf));
            assert_eq!(host.free_addrinfo(handle), Err(AsyncHostError::Badf));
        }
        host.buffers()
            .with_buffer(replacement, |bytes| {
                assert_eq!(bytes, b"replacement");
                Ok(())
            })
            .unwrap();
        host.buffers().free(buffer).unwrap();
        host.buffers().free(replacement).unwrap();
        host.free_job(job).unwrap();
        assert!(host.leak_summary().is_none());

        #[cfg(windows)]
        assert_eq!(crate::network::os::cleanup_wsa(), 0);
    }

    #[test]
    fn nullable_objects_use_runtime_null_handle() {
        let host = default_host();
        let null = null_handle();
        host.buffers().free(null).unwrap();
        host.free_addrinfo(null).unwrap();
        assert_eq!(host.addrinfo_next(null), Ok(null));
        assert!(matches!(
            host.buffers().lease(null),
            Err(AsyncHostError::Badf)
        ));

        host.tls().free(null).unwrap();

        for handle in [0, host.invalid_fd()] {
            assert_eq!(host.buffers().free(handle), Err(AsyncHostError::Badf));
            assert_eq!(host.free_addrinfo(handle), Err(AsyncHostError::Badf));
            assert_eq!(host.addrinfo_next(handle), Err(AsyncHostError::Badf));
            assert_eq!(host.tls().free(handle), Err(AsyncHostError::Badf));
            #[cfg(windows)]
            assert_eq!(
                host.buffers().free_windows_watcher(handle),
                Err(AsyncHostError::Badf)
            );
        }
        #[cfg(windows)]
        host.buffers().free_windows_watcher(null).unwrap();
        assert!(host.leak_summary().is_none());
    }

    #[test]
    fn readdir_job_exclusively_leases_and_restores_c_buffer() {
        let host = default_host();
        let handle = host.buffers().publish(b"abcd".to_vec().into_boxed_slice());

        let lease = host.buffers().lease(handle).unwrap();
        assert_eq!(
            host.buffers().with_buffer(handle, |_| Ok(())).unwrap_err(),
            AsyncHostError::Badf
        );
        assert_eq!(
            host.buffers().lease(handle).unwrap_err(),
            AsyncHostError::Badf
        );

        let job = FilesystemJob::readdir(Arc::new(Resource::invalid()), lease, 4, false);
        let job = host.insert_job(job).unwrap();
        let key = key_from_handle(job);
        let mut completed = host.jobs.borrow_mut().take_ready_job(key).unwrap();
        AsyncHost::run_policy_checked_job(&host.filesystem, &host.process, &mut completed);
        // Finishing execution alone does not return access before collection.
        assert_eq!(host.buffers().get(handle, 0), Err(AsyncHostError::Badf));
        host.restore_job(key, completed).unwrap();

        host.buffers()
            .with_buffer(handle, |buffer| {
                assert_eq!(buffer, b"abcd");
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn freeing_unrun_readdir_job_restores_c_buffer() {
        let host = default_host();
        let handle = host.buffers().publish(b"abcd".to_vec().into_boxed_slice());
        let lease = host.buffers().lease(handle).unwrap();
        let job = FilesystemJob::readdir(Arc::new(Resource::invalid()), lease, 4, false);
        let job = host.insert_job(job).unwrap();

        host.free_job(job).unwrap();

        host.buffers()
            .with_buffer(handle, |buffer| {
                assert_eq!(buffer, b"abcd");
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn restored_unrun_readdir_job_keeps_its_buffer_lease() {
        let host = default_host();
        let handle = host.buffers().publish(b"abcd".to_vec().into_boxed_slice());
        let lease = host.buffers().lease(handle).unwrap();
        let job = host
            .insert_job(FilesystemJob::readdir(
                Arc::new(Resource::invalid()),
                lease,
                4,
                false,
            ))
            .unwrap();
        let worker_job = host
            .take_worker_job(WorkerCompletionId::from_abi(1), key_from_handle(job))
            .unwrap();

        // Returning an unrun Job to Ready retains its buffer for later execution.
        host.restore_unrun_worker_job(worker_job);
        assert_eq!(host.buffers().get(handle, 0), Err(AsyncHostError::Badf));
        host.free_job(job).unwrap();
        assert_eq!(host.buffers().get(handle, 0), Ok(b'a'));
        host.buffers().free(handle).unwrap();
    }

    #[test]
    fn discarded_queued_readdir_job_restores_c_buffer() {
        let host = default_host();
        let handle = host.buffers().publish(b"abcd".to_vec().into_boxed_slice());
        let lease = host.buffers().lease(handle).unwrap();
        let job = FilesystemJob::readdir(Arc::new(Resource::invalid()), lease, 4, false);
        let job = host.insert_job(job).unwrap();
        let worker_job = host
            .take_worker_job(WorkerCompletionId::from_abi(1), key_from_handle(job))
            .unwrap();

        host.free_job(job).unwrap();
        assert_eq!(
            host.buffers().with_buffer(handle, |_| Ok(())).unwrap_err(),
            AsyncHostError::Badf
        );
        host.restore_unrun_worker_job(worker_job);

        host.buffers()
            .with_buffer(handle, |buffer| {
                assert_eq!(buffer, b"abcd");
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn freed_c_buffer_is_not_restored_by_its_readdir_job() {
        let host = default_host();
        let handle = host.buffers().publish(b"old".to_vec().into_boxed_slice());
        let lease = host.buffers().lease(handle).unwrap();
        let job = FilesystemJob::readdir(Arc::new(Resource::invalid()), lease, 3, false);
        let job = host.insert_job(job).unwrap();

        host.buffers().free(handle).unwrap();
        let replacement = host.buffers().publish(b"new".to_vec().into_boxed_slice());
        host.free_job(job).unwrap();

        assert_eq!(
            host.buffers().with_buffer(handle, |_| Ok(())).unwrap_err(),
            AsyncHostError::Badf
        );
        host.buffers()
            .with_buffer(replacement, |buffer| {
                assert_eq!(buffer, b"new");
                Ok(())
            })
            .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn realpath_result_is_registered_c_buffer_cleaned_up_with_job() {
        let host = default_host();
        let job_handle = host
            .insert_job(FilesystemJob::realpath(std::ffi::OsString::from(
                "/tmp/example",
            )))
            .unwrap();
        {
            let mut jobs = host.jobs.borrow_mut();
            let job = jobs.visible_job_mut(key_from_handle(job_handle)).unwrap();
            let job = job.filesystem_mut().unwrap();
            job.set_realpath_result(b"/tmp/example\0".to_vec().into_boxed_slice())
                .unwrap();
        }

        let buffer_handle = host.get_realpath_result(job_handle).unwrap();
        assert_eq!(host.get_realpath_result(job_handle).unwrap(), buffer_handle);
        host.buffers()
            .with_buffer(buffer_handle, |buffer| {
                assert_eq!(buffer, b"/tmp/example\0");
                Ok(())
            })
            .unwrap();

        host.free_job(job_handle).unwrap();

        assert_eq!(
            host.buffers()
                .with_buffer(buffer_handle, |_| Ok(()))
                .unwrap_err(),
            AsyncHostError::Badf
        );
    }

    #[cfg(unix)]
    #[test]
    fn completion_source_remains_ready_until_all_ids_are_fetched() {
        let host = default_host();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        {
            let notifier = host.thread_pool_notifier().unwrap();
            notifier.notify(41).unwrap();
            notifier.notify(42).unwrap();
        }

        let mut memory = vec![0; 8];
        assert_eq!(host.poll_wait(poll, 0, &Default::default()).unwrap(), 1);
        let bytes = host
            .fetch_completion(memory.as_mut_slice(), completion_notifier, 0, 0)
            .unwrap();
        assert_eq!(bytes, 0);
        assert_eq!(host.poll_wait(poll, 0, &Default::default()).unwrap(), 1);

        let bytes = host
            .fetch_completion(memory.as_mut_slice(), completion_notifier, 0, 1)
            .unwrap();
        assert_eq!(bytes, 4);
        assert_eq!(i32::from_le_bytes(memory[0..4].try_into().unwrap()), 41);
        assert_eq!(host.poll_wait(poll, 0, &Default::default()).unwrap(), 1);

        let bytes = host
            .fetch_completion(memory.as_mut_slice(), completion_notifier, 4, 1)
            .unwrap();
        assert_eq!(bytes, 4);
        assert_eq!(i32::from_le_bytes(memory[4..8].try_into().unwrap()), 42);
        assert_eq!(host.poll_wait(poll, 0, &Default::default()).unwrap(), 0);
    }

    #[test]
    fn dropping_host_retires_ready_reserved_and_completed_job_handles() {
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let host = default_host_with_keys(keys.clone());
        let ready = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let reserved = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let completed = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let worker_job = host
            .take_worker_job(WorkerCompletionId::from_abi(1), key_from_handle(reserved))
            .unwrap();
        host.run_job(completed).unwrap();

        for handle in [ready, reserved, completed] {
            assert_eq!(
                keys.borrow().kind(key_from_handle(handle)),
                Some(HandleKind::Job)
            );
        }
        drop(host);
        for handle in [ready, reserved, completed] {
            assert_eq!(keys.borrow().kind(key_from_handle(handle)), None);
        }
        // Retiring the reservation does not require taking back the owned Job.
        drop(worker_job);
    }

    #[test]
    fn job_handles_reject_stale_and_other_family_handles() {
        let host = default_host();
        let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let buffer = host
            .buffers()
            .publish(b"retained".to_vec().into_boxed_slice());

        // Jobs require a live Handle, including for cleanup.
        for handle in [0, null_handle(), host.invalid_fd(), buffer] {
            assert_eq!(
                host.with_job(handle, |job| job.ret()),
                Err(AsyncHostError::Badf)
            );
            assert_eq!(host.run_job(handle), Err(AsyncHostError::Badf));
            assert_eq!(host.free_job(handle), Err(AsyncHostError::Badf));
        }
        host.buffers()
            .with_buffer(buffer, |bytes| {
                assert_eq!(bytes, b"retained");
                Ok(())
            })
            .unwrap();

        host.free_job(job).unwrap();
        let replacement = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        assert_ne!(job, replacement);

        assert_eq!(
            host.with_job(job, |job| job.ret()),
            Err(AsyncHostError::Badf)
        );
        assert_eq!(host.run_job(job), Err(AsyncHostError::Badf));
        assert_eq!(host.free_job(job), Err(AsyncHostError::Badf));

        host.run_job(replacement).unwrap();
        assert_eq!(host.with_job(replacement, |job| job.ret()), Ok(0));
        host.free_job(replacement).unwrap();
        host.buffers().free(buffer).unwrap();
        assert!(host.leak_summary().is_none());
    }

    #[test]
    fn open_job_get_fd_publishes_opened_resource_once() {
        let host = default_host();
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

        assert_eq!(host.with_job(job, |job| job.ret()).unwrap(), -1);
        assert_eq!(
            host.with_job(job, |job| job.err()).unwrap(),
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

        assert_eq!(host.with_job(job, |job| job.ret()).unwrap(), -1);
        assert_eq!(
            host.with_job(job, |job| job.err()).unwrap(),
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

        assert_eq!(host.poll_wait(poll, 1000, &Default::default()).unwrap(), 1);
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

        assert_eq!(host.with_job(job, |job| job.ret()).unwrap(), -1);
        assert_eq!(
            host.with_job(job, |job| job.err()).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        host.free_worker(worker).unwrap();
        host.free_job(job).unwrap();
        host.destroy_thread_pool();
    }

    #[cfg(unix)]
    #[test]
    fn run_job_rechecks_swapped_open_symlink_at_execution() {
        use crate::filesystem::{CreateMode, OpenAccess};
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

        host.filesystem()
            .authorize(FsOperation::Open {
                path: FsPath::Cwd(link.as_os_str()),
                access: Some(OpenAccess::Read),
                create: Some(CreateMode::Open),
                append: false,
                read_metadata: false,
            })
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

        assert_eq!(host.with_job(job, |job| job.ret()).unwrap(), -1);
        assert_eq!(
            host.with_job(job, |job| job.err()).unwrap(),
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

        assert_eq!(host.with_job(remove_job, |job| job.ret()).unwrap(), -1);
        assert_eq!(
            host.with_job(remove_job, |job| job.err()).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        assert_eq!(host.with_job(rename_job, |job| job.ret()).unwrap(), -1);
        assert_eq!(
            host.with_job(rename_job, |job| job.err()).unwrap(),
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

        assert_eq!(host.with_job(kind_job, |job| job.ret()).unwrap(), -1);
        assert_eq!(
            host.with_job(kind_job, |job| job.err()).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        assert_eq!(host.with_job(time_job, |job| job.ret()).unwrap(), -1);
        assert_eq!(
            host.with_job(time_job, |job| job.err()).unwrap(),
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

        assert_eq!(host.with_job(allowed_link_job, |job| job.ret()).unwrap(), 3);
        assert_eq!(host.with_job(denied_link_job, |job| job.ret()).unwrap(), -1);
        assert_eq!(
            host.with_job(denied_link_job, |job| job.err()).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        host.free_job(denied_link_job).unwrap();
        host.free_job(allowed_link_job).unwrap();
        host.close_fd(parent_fd).unwrap();
        host.free_job(parent_open_job).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn resource_io_and_metadata_keep_the_opening_grant() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("file");
        let moved = tmp.path().join("moved");
        std::fs::write(&file, b"keep").unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(&policy_file, "[fs]\nread = ['file']\nwrite = ['file']").unwrap();
        let host = host_with_policy(&policy_file);
        let open = host
            .insert_job(FilesystemJob::open_legacy(
                file.clone().into_os_string(),
                2,
                0,
                false,
                0,
                0,
            ))
            .unwrap();
        host.run_job(open).unwrap();
        let fd = host.open_job_get_fd(open).unwrap();
        let resource = host.acquire_resource(fd).unwrap();

        std::fs::rename(&file, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &file).unwrap();
        // The old path now resolves outside the configured rule. Both I/O and
        // metadata retain the authority captured when this file was opened.
        let metadata = host
            .insert_job(FilesystemJob::file_size(Arc::clone(&resource)))
            .unwrap();
        host.run_job(metadata).unwrap();
        assert_eq!(host.with_job(metadata, |job| job.err()).unwrap(), 0);
        assert_eq!(host.get_file_size_result(metadata).unwrap(), 4);

        let read = host
            .insert_job(FilesystemJob::read(Arc::clone(&resource), 4, 0))
            .unwrap();
        let write = host
            .insert_job(FilesystemJob::write(
                Arc::clone(&resource),
                b"edit".to_vec(),
                0,
            ))
            .unwrap();
        let flush = host
            .insert_job(FilesystemJob::fsync(resource, false))
            .unwrap();
        for (handle, expected) in [(read, 4), (write, 4), (flush, 0)] {
            host.run_job(handle).unwrap();
            assert_eq!(host.with_job(handle, |job| job.err()).unwrap(), 0);
            assert_eq!(host.with_job(handle, |job| job.ret()).unwrap(), expected);
        }
        let mut bytes = [0; 4];
        host.get_read_result(&mut bytes[..], read, 0, 0, 4).unwrap();
        assert_eq!(&bytes, b"keep");
        assert_eq!(std::fs::read(&moved).unwrap(), b"edit");
        // Unix event-loop I/O uses an immediate path rather than a Job.
        assert_eq!(host.read_fd(&mut bytes[..], fd, 0, 0, 4).unwrap(), 4);
        assert_eq!(&bytes, b"edit");
        let mut update = *b"next";
        assert_eq!(host.write_fd(&mut update[..], fd, 0, 0, 4).unwrap(), 4);
        assert_eq!(std::fs::read(&moved).unwrap(), b"editnext");
        for job in [open, metadata, read, write, flush] {
            host.free_job(job).unwrap();
        }
        host.close_fd(fd).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn chmod_job_requires_target_write_permission() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("file");
        std::fs::write(&file, b"keep").unwrap();
        let policy_file = tmp.path().join("policy.toml");
        for (permission, allowed) in [("read", false), ("write", true)] {
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
            std::fs::write(&policy_file, format!("[fs]\n{permission} = ['file']")).unwrap();
            let host = host_with_policy(&policy_file);
            let job = host
                .insert_job(FilesystemJob::chmod(file.clone().into_os_string(), 0o600))
                .unwrap();
            host.run_job(job).unwrap();
            assert_eq!(
                host.with_job(job, |job| job.err()).unwrap(),
                if allowed {
                    0
                } else {
                    AsyncHostError::PermissionDenied.errno()
                }
            );
            assert_eq!(
                std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
                if allowed { 0o600 } else { 0o644 }
            );
            host.free_job(job).unwrap();
        }
    }

    #[test]
    fn fd_metadata_beyond_kind_requires_read_policy() {
        const STAT_FILE_KIND: u32 = 0x0001;
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
        let kind_job = host
            .insert_job(FilesystemJob::fstatx(Arc::clone(&resource), STAT_FILE_KIND, 16).unwrap())
            .unwrap();
        let size_job = host
            .insert_job(FilesystemJob::file_size(Arc::clone(&resource)))
            .unwrap();
        let time_job = host
            .insert_job(FilesystemJob::file_time(Arc::clone(&resource)))
            .unwrap();

        host.run_job(kind_job).unwrap();
        host.run_job(size_job).unwrap();
        host.run_job(time_job).unwrap();

        assert_eq!(host.with_job(kind_job, |job| job.err()).unwrap(), 0);
        assert_eq!(host.with_job(size_job, |job| job.ret()).unwrap(), -1);
        assert_eq!(
            host.with_job(size_job, |job| job.err()).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        assert_eq!(host.with_job(time_job, |job| job.ret()).unwrap(), -1);
        assert_eq!(
            host.with_job(time_job, |job| job.err()).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        host.free_job(time_job).unwrap();
        host.free_job(size_job).unwrap();
        host.free_job(kind_job).unwrap();
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
        let kqueue_handle = host.insert_file_resource(crate::filesystem::watch::create().unwrap());

        assert_eq!(
            host.kqueue_watcher_add_file(kqueue_handle, file_handle),
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
                    None,
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
                    None,
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

        assert_eq!(host.with_job(identity_job, |job| job.err()).unwrap(), 0);
        assert_eq!(
            host.with_job(metadata_job, |job| job.err()).unwrap(),
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

        assert_eq!(host.with_job(flock_job, |job| job.ret()).unwrap(), -1);
        assert_eq!(
            host.with_job(flock_job, |job| job.err()).unwrap(),
            AsyncHostError::PermissionDenied.errno()
        );
        host.free_job(flock_job).unwrap();
        host.close_fd(fd).unwrap();
        host.free_job(open_job).unwrap();
    }

    #[test]
    fn dropping_host_retires_pending_process_inputs() {
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let host = default_host_with_keys(keys.clone());
        let env = host.process_inputs().make_env(false);
        let legacy = host.process_inputs().allocate_legacy_env(0).unwrap();
        #[cfg(unix)]
        let argv = host.process_inputs().allocate_argv(0).unwrap();

        assert_eq!(
            keys.borrow().kind(key_from_handle(env)),
            Some(HandleKind::ProcessEnvBuilder)
        );
        assert_eq!(
            keys.borrow().kind(key_from_handle(legacy)),
            Some(HandleKind::ProcessEnv)
        );
        #[cfg(unix)]
        assert_eq!(
            keys.borrow().kind(key_from_handle(argv)),
            Some(HandleKind::ProcessArgv)
        );
        assert!(
            host.leak_summary()
                .unwrap()
                .contains("process_env_builders=1")
        );
        drop(host);
        assert!(keys.borrow().kind(key_from_handle(env)).is_none());
        assert!(keys.borrow().kind(key_from_handle(legacy)).is_none());
        #[cfg(unix)]
        assert!(keys.borrow().kind(key_from_handle(argv)).is_none());
    }

    #[test]
    fn dropping_host_retires_pending_and_configured_tls_handles() {
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let host = default_host_with_keys(keys.clone());
        let pending = host.tls().new_connection();
        let configured = host.tls().new_connection();
        assert_eq!(
            host.tls().set_client(
                configured,
                "localhost",
                false,
                crate::tls::TlsTrust::NoVerification,
            ),
            Ok(0)
        );
        for handle in [pending, configured] {
            assert_eq!(
                keys.borrow().kind(key_from_handle(handle)),
                Some(HandleKind::TlsConnection)
            );
        }

        drop(host);

        for handle in [pending, configured] {
            assert_eq!(keys.borrow().kind(key_from_handle(handle)), None);
        }
    }

    #[test]
    fn discarded_completed_open_job_drops_unpublished_resource() {
        let host = default_host();
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
        let key = key_from_handle(job_handle);
        let mut job = host.jobs.borrow_mut().take_ready_job(key).unwrap();

        thread_pool::run_host_job(&mut job, &host.filesystem);

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
        let host = default_host();
        let filesystem_weak = Arc::downgrade(&host.filesystem);
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        host.spawn_worker(42, job).unwrap();

        host.poll_wait(poll, 1000, &Default::default()).unwrap();
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

        assert!(filesystem_weak.upgrade().is_none());
    }

    #[test]
    fn completion_pipe_validation_preserves_the_job() {
        let host = default_host();
        let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let [reader, writer] = host.pipe(true, false).unwrap();
        for invalid_writer in [reader, writer] {
            assert_eq!(
                host.spawn_worker_with_pipe(1, job, invalid_writer),
                Err(AsyncHostError::Inval),
            );
        }
        host.close_fd(writer).unwrap();
        assert_eq!(
            host.spawn_worker_with_pipe(1, job, writer),
            Err(AsyncHostError::Badf),
        );
        assert_eq!(host.workers.len(), 0);
        host.run_job(job).unwrap();
        assert_eq!(host.with_job(job, |job| job.ret()), Ok(0));
        host.free_job(job).unwrap();
        host.close_fd(reader).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn completion_pipe_cancellation_does_not_use_the_default_retry_source() {
        use std::time::Duration;

        let host = default_host();
        let poll = host.poll_create().unwrap();
        host.init_thread_pool(poll).unwrap();
        let [reader, writer] = host.pipe(true, true).unwrap();
        let notifier =
            crate::thread_pool::PipeCompletionNotifier::new(host.acquire_resource(writer).unwrap())
                .unwrap();
        let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let key = key_from_handle(job);
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker_key = host.workers.spawn(
            host.take_worker_job(WorkerCompletionId::from_abi(42), key)
                .unwrap(),
            move |worker_job| {
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                let error = thread_pool::with_cancellable_region(|| Ok(())).unwrap_err();
                worker_job.job.set_err(error.errno());
            },
            WorkerCompletionDestination::Pipe(notifier),
        );
        let worker = handle_from_key(worker_key);

        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let retry_status = host.cancel_worker_with_retry(worker);
        let legacy_status = host.cancel_worker(worker);
        release_tx.send(()).unwrap();
        // Cancellation happened before the blocking region. It must still be
        // acknowledged there, and only the finished Job goes to our pipe.
        let resource = host.acquire_resource(reader).unwrap();
        let mut ready = libc::pollfd {
            fd: resource.as_file().unwrap().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        assert_eq!(unsafe { libc::poll(&mut ready, 1, 5000) }, 1);
        let mut bytes = [0u8; 4];
        assert_eq!(
            unsafe { libc::read(ready.fd, bytes.as_mut_ptr().cast(), bytes.len()) },
            4
        );
        assert_eq!(i32::from_le_bytes(bytes), 42);
        assert_eq!(retry_status, Err(AsyncHostError::Inval));
        assert_eq!(legacy_status, Ok(0));
        assert_eq!(host.with_job(job, |job| job.err()), Ok(libc::EINTR));
        assert_eq!(host.cancel_worker(worker), Ok(1));
        assert_eq!(host.poll_wait(poll, 0, &Default::default()), Ok(0));

        host.free_worker(worker).unwrap();
        host.free_job(job).unwrap();
        host.close_fd(writer).unwrap();
        host.close_fd(reader).unwrap();
        host.destroy_thread_pool();
    }

    #[test]
    fn closed_completion_reader_does_not_block_worker_teardown() {
        let host = default_host();
        let [reader, writer] = host.pipe(true, true).unwrap();
        let writer_weak = Arc::downgrade(&host.acquire_resource(writer).unwrap());
        host.close_fd(reader).unwrap();
        let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let worker = host.spawn_worker_with_pipe(1, job, writer).unwrap();
        host.close_fd(writer).unwrap();
        // Publish the Job before freeing the Worker, so notification races a
        // closed reader rather than merely cancelling an unstarted Job.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while host.with_job(job, |job| job.ret()).is_err() {
            assert!(std::time::Instant::now() < deadline, "Job did not finish");
            std::thread::yield_now();
        }
        host.free_worker(worker).unwrap();
        assert!(writer_weak.upgrade().is_none());
        host.free_job(job).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn full_completion_pipe_does_not_block_worker_or_runtime_teardown() {
        use std::time::{Duration, Instant};

        for free_explicitly in [true, false] {
            // The timeout bounds a broken join without making the test suite
            // itself wait forever on a deliberately undrained pipe.
            let (finished, completion) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let host = default_host();
                let [reader, writer] = host.pipe(true, true).unwrap();
                let resource = host.acquire_resource(writer).unwrap();
                let fd = resource.as_file().unwrap().as_raw_fd();
                let bytes = [0u8; 4];
                loop {
                    let written = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
                    if written < 0 {
                        assert_eq!(
                            std::io::Error::last_os_error().raw_os_error(),
                            Some(libc::EAGAIN)
                        );
                        break;
                    }
                    assert_eq!(written, 4);
                }
                drop(resource);
                let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
                let worker = host.spawn_worker_with_pipe(1, job, writer).unwrap();
                host.close_fd(writer).unwrap();
                let deadline = Instant::now() + Duration::from_secs(1);
                while host.with_job(job, |job| job.ret()).is_err() {
                    assert!(Instant::now() < deadline, "Job did not finish");
                    std::thread::yield_now();
                }
                // The result is ready even though the notification cannot fit.
                if free_explicitly {
                    host.free_worker(worker).unwrap();
                    host.free_job(job).unwrap();
                    host.close_fd(reader).unwrap();
                }
                drop(host);
                finished.send(()).unwrap();
            });
            completion.recv_timeout(Duration::from_secs(3)).unwrap();
        }
    }

    #[test]
    fn worker_result_is_available_after_completion_event() {
        let host = default_host();
        let poll = host.poll_create().unwrap();
        let completion_source = host.init_thread_pool(poll).unwrap();
        let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let worker = host.spawn_worker(42, job).unwrap();

        assert_eq!(host.poll_wait(poll, 1000, &Default::default()).unwrap(), 1);
        assert_eq!(host.cancel_worker_with_retry(worker), Ok(2));
        assert_eq!(host.cancel_worker(worker), Ok(1));
        assert_eq!(host.with_job(job, |job| job.ret()).unwrap(), 0);
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

    #[cfg(unix)]
    #[test]
    fn worker_retry_rejects_closed_and_replaced_completion_sources() {
        for destroy_poll in [false, true] {
            let host = default_host();
            let poll = host.poll_create().unwrap();
            let source = host.init_thread_pool(poll).unwrap();
            let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
            let worker = host.spawn_worker(42, job).unwrap();
            assert_eq!(host.poll_wait(poll, 1000, &Default::default()), Ok(1));

            if destroy_poll {
                host.poll_destroy(poll).unwrap();
            } else {
                host.close_fd(source).unwrap();
            }
            assert_eq!(
                host.cancel_worker_with_retry(worker),
                Err(AsyncHostError::Badf)
            );

            let poll = if destroy_poll {
                host.poll_create().unwrap()
            } else {
                poll
            };
            host.init_thread_pool(poll).unwrap();
            // A replacement source must not validate retry checks for a Worker
            // whose finished notifications are still bound to the old source.
            assert_eq!(
                host.cancel_worker_with_retry(worker),
                Err(AsyncHostError::Badf)
            );
            assert_eq!(host.with_job(job, |job| job.err()), Ok(0));

            let next_job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
            let next_worker = host.spawn_worker(43, next_job).unwrap();
            assert_eq!(host.poll_wait(poll, 1000, &Default::default()), Ok(1));
            assert_eq!(host.cancel_worker_with_retry(next_worker), Ok(2));
            host.free_worker(worker).unwrap();
            host.free_worker(next_worker).unwrap();
            host.free_job(job).unwrap();
            host.free_job(next_job).unwrap();
            host.destroy_thread_pool();
        }
    }

    #[cfg(unix)]
    #[test]
    fn worker_teardown_does_not_require_guest_to_drain_completions() {
        use std::sync::mpsc;
        use std::time::{Duration, Instant};

        for destroy_pool in [false, true] {
            let (notifier_tx, notifier_rx) = mpsc::channel();
            let (finished_tx, finished_rx) = mpsc::channel();
            let run = std::thread::spawn(move || {
                let host = default_host();
                let poll = host.poll_create().unwrap();
                host.init_thread_pool(poll).unwrap();
                let notifier = host.thread_pool_notifier().unwrap();
                notifier.fill_with_completions(17);
                let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
                let worker = host.spawn_worker(23, job).unwrap();
                notifier_tx.send(notifier).unwrap();
                if destroy_pool {
                    host.destroy_thread_pool();
                } else {
                    host.free_worker(worker).unwrap();
                }
                assert_eq!(host.with_job(job, |job| job.ret()), Ok(0));
                host.free_job(job).unwrap();
                host.destroy_thread_pool();
                finished_tx.send(()).unwrap();
            });
            let notifier = notifier_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let finished_without_fetch = finished_rx.recv_timeout(Duration::from_secs(1)).is_ok();
            if !finished_without_fetch {
                // Keep a failing transport regression from hanging the test
                // process while the host is inside free_worker/destroy_thread_pool.
                let deadline = Instant::now() + Duration::from_secs(5);
                while !run.is_finished() && Instant::now() < deadline {
                    notifier.fetch(&mut [0; 4096]).unwrap();
                    std::thread::yield_now();
                }
                assert!(
                    run.is_finished(),
                    "teardown did not finish even after draining"
                );
            }
            run.join().unwrap();
            assert!(
                finished_without_fetch,
                "teardown required guest consumption (destroy_pool={destroy_pool})"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_status_waits_for_job_result_after_acknowledgement() {
        use std::time::Duration;

        let host = default_host();
        let poll = host.poll_create().unwrap();
        host.init_thread_pool(poll).unwrap();
        let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let key = key_from_handle(job);
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (ack_tx, ack_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let (completed_tx, completed_rx) = std::sync::mpsc::channel();
        let worker_key = host.workers.spawn(
            host.take_worker_job(WorkerCompletionId::from_abi(42), key)
                .unwrap(),
            move |worker_job| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                assert!(thread_pool::with_cancellable_region(|| Ok(())).is_err());
                ack_tx.send(()).unwrap();
                finish_rx.recv().unwrap();
                worker_job.job.set_ret(73);
            },
            WorkerCompletionDestination::Test(Box::new(move |_| completed_tx.send(()).unwrap())),
        );
        let worker = handle_from_key(worker_key);

        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(host.cancel_worker_with_retry(worker), Ok(1));
        release_tx.send(()).unwrap();
        ack_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(host.cancel_worker_with_retry(worker), Ok(1));
        assert!(matches!(
            host.jobs.borrow().jobs.get(key),
            Some(HostJobState::Reserved)
        ));

        finish_tx.send(()).unwrap();
        completed_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // No poll or fetch call has restored the result: the combined ABI
        // must make it available before reporting that the worker finished.
        assert!(matches!(
            host.jobs.borrow().jobs.get(key),
            Some(HostJobState::Reserved)
        ));
        assert_eq!(host.cancel_worker_with_retry(worker), Ok(2));
        assert!(matches!(
            host.jobs.borrow().jobs.get(key),
            Some(HostJobState::ResultReady(_))
        ));
        assert_eq!(host.with_job(job, |job| job.ret()), Ok(73));
        assert_eq!(host.cancel_worker(worker), Ok(1));

        host.free_worker(worker).unwrap();
        host.free_job(job).unwrap();
        host.destroy_thread_pool();
    }

    #[test]
    fn free_running_worker_job_detaches_its_result() {
        let host = default_host();
        let first_job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let first_key = key_from_handle(first_job);
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let (completion_sender, completion_receiver) = std::sync::mpsc::channel();
        let filesystem = Arc::clone(&host.filesystem);
        let worker_key = host.workers.spawn(
            host.take_worker_job(WorkerCompletionId::from_abi(1), first_key)
                .unwrap(),
            move |worker_job| {
                started_sender.send(worker_job.completion_id).unwrap();
                release_receiver.recv().unwrap();
                thread_pool::run_host_job(&mut worker_job.job, &filesystem);
            },
            WorkerCompletionDestination::Test(Box::new(move |completion_id| {
                completion_sender.send(completion_id).unwrap()
            })),
        );
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
        assert_eq!(
            host.with_job(first_job, |job| job.ret()),
            Err(AsyncHostError::Badf)
        );
        host.run_job(replacement_job).unwrap();
        assert_eq!(host.with_job(replacement_job, |job| job.ret()), Ok(0));
        host.free_job(replacement_job).unwrap();

        host.free_worker(worker).unwrap();
        host.destroy_thread_pool();
    }

    #[test]
    fn free_queued_worker_job_detaches_without_cancelling_it() {
        let host = default_host();
        let first_job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();
        let first_key = key_from_handle(first_job);
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let (completion_sender, completion_receiver) = std::sync::mpsc::channel();
        let filesystem = Arc::clone(&host.filesystem);
        let worker_key = host.workers.spawn(
            host.take_worker_job(WorkerCompletionId::from_abi(1), first_key)
                .unwrap(),
            move |worker_job| {
                started_sender.send(worker_job.completion_id).unwrap();
                if worker_job.job_key == first_key {
                    release_receiver.recv().unwrap();
                }
                thread_pool::run_host_job(&mut worker_job.job, &filesystem);
            },
            WorkerCompletionDestination::Test(Box::new(move |completion_id| {
                completion_sender.send(completion_id).unwrap()
            })),
        );
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
        assert_eq!(
            host.with_job(queued_job, |job| job.ret()),
            Err(AsyncHostError::Badf)
        );
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
        assert_eq!(
            host.with_job(queued_job, |job| job.ret()),
            Err(AsyncHostError::Badf)
        );

        host.free_worker(worker).unwrap();
        let _ = std::fs::remove_file(displaced_path);
        let _ = std::fs::remove_file(queued_path);
    }

    #[test]
    fn foreign_worker_handle_preserves_ready_job() {
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let host = default_host_with_keys(keys.clone());
        // A live Worker key from another owning table must not consume the Job.
        let mut other_workers = Handles::new(keys, HandleKind::Worker);
        let worker_key = other_workers.insert(());
        let job = host.insert_job(thread_pool::make_sleep_job(0)).unwrap();

        assert_eq!(
            host.wake_worker(handle_from_key(worker_key), 42, job),
            Err(AsyncHostError::Badf)
        );
        host.run_job(job).unwrap();
        assert_eq!(host.with_job(job, |job| job.err()), Ok(0));
        host.free_job(job).unwrap();
    }

    #[test]
    fn worker_handles_stay_stale_after_thread_pool_reinit() {
        let host = default_host();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let first_job = host
            .insert_job(FilesystemJob::read(Arc::new(Resource::invalid()), 1, -1))
            .unwrap();
        let old_worker = host.spawn_worker(42, first_job).unwrap();
        host.poll_wait(poll, 1000, &Default::default()).unwrap();
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
        let host = default_host();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let completion = host.thread_pool_completion_target().unwrap();

        poll::post_thread_pool_completion(&completion, 42).unwrap();

        assert_eq!(host.poll_wait(poll, 1000, &Default::default()).unwrap(), 1);
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
        let host = default_host();
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
        let host = default_host();
        let poll = host.poll_create().unwrap();
        let completion_notifier = host.init_thread_pool(poll).unwrap();
        let job = host
            .insert_job(FilesystemJob::read(Arc::new(Resource::invalid()), 1, -1))
            .unwrap();
        let worker = host.spawn_worker(42, job).unwrap();
        host.poll_wait(poll, 1000, &Default::default()).unwrap();
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
        let host = default_host();
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
        let host = default_host();
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

        assert_eq!(host.poll_wait(poll, 0, &Default::default()).unwrap(), 0);

        host.free_job(job).unwrap();
        host.close_fd(write).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn unregistered_iocp_completion_key_is_returned() {
        let host = default_host();
        let poll = host.poll_create().unwrap();
        let completion_port = {
            host.polls
                .borrow_mut()
                .get_mut(poll)
                .unwrap()
                .completion_port()
        };
        let completion_key = 0x1234usize;
        completion_port.post(0, completion_key).unwrap();

        assert_eq!(host.poll_wait(poll, 1000, &Default::default()).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();

        assert_eq!(host.poll_event_fd(event).unwrap(), completion_key as u64);
    }

    #[cfg(windows)]
    #[test]
    fn zero_iocp_completion_key_is_returned() {
        let host = default_host();
        let poll = host.poll_create().unwrap();
        let completion_port = {
            host.polls
                .borrow_mut()
                .get_mut(poll)
                .unwrap()
                .completion_port()
        };
        completion_port.post(0, 0).unwrap();

        assert_eq!(host.poll_wait(poll, 1000, &Default::default()).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();

        assert_eq!(host.poll_event_fd(event).unwrap(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn close_fd_preserves_polled_iocp_resource_handle() {
        let host = default_host();
        let poll = host.poll_create().unwrap();
        let [read, write] = host.pipe(true, true).unwrap();
        host.poll_register(poll, read, true).unwrap();
        let completion_port = {
            host.polls
                .borrow_mut()
                .get_mut(poll)
                .unwrap()
                .completion_port()
        };
        completion_port
            .post(0, usize::try_from(read).unwrap())
            .unwrap();
        assert_eq!(host.poll_wait(poll, 1000, &Default::default()).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();

        host.close_fd(read).unwrap();

        assert_eq!(host.poll_event_fd(event).unwrap(), read);
        assert_eq!(host.close_fd(read), Err(AsyncHostError::Badf));
        host.close_fd(write).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn poll_reports_registered_pipe_readiness_as_guest_fd() {
        let (_, receiver) = crate::termination_channel();
        for termination in [TerminationRequest::default(), receiver.request.clone()] {
            let host = default_host();
            let poll = host.poll_create().unwrap();
            let [read, write] = host.pipe(true, true).unwrap();
            host.poll_register(poll, read, true).unwrap();

            let fd = host
                .acquire_resource(write)
                .unwrap()
                .as_fd()
                .unwrap()
                .as_raw_fd();
            // The same byte as the private wake source remains ordinary guest data.
            let byte = b"x";
            let ret = unsafe { libc::write(fd, byte.as_ptr().cast(), byte.len()) };
            assert_eq!(ret, 1);

            assert_eq!(host.poll_wait(poll, 100, &termination).unwrap(), 1);
            let event = host.poll_get_event(poll, 0).unwrap();
            assert_eq!(event, 0);
            assert_eq!(host.poll_get_event(poll, 0).unwrap(), event);
            assert_eq!(host.poll_event_fd(event).unwrap(), read);
            assert_eq!(host.poll_event_events(event).unwrap() & 1, 1);
            let fd = host
                .acquire_resource(read)
                .unwrap()
                .as_fd()
                .unwrap()
                .as_raw_fd();
            let mut data = [0; 1];
            assert_eq!(unsafe { libc::read(fd, data.as_mut_ptr().cast(), 1) }, 1);
            assert_eq!(&data, byte);
            assert_eq!(host.poll_wait(poll, 0, &termination).unwrap(), 0);

            host.close_fd(read).unwrap();
            host.close_fd(write).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn close_fd_preserves_polled_resource_handle() {
        let host = default_host();
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
        assert_eq!(host.poll_wait(poll, 100, &Default::default()).unwrap(), 1);
        let event = host.poll_get_event(poll, 0).unwrap();

        host.close_fd(read).unwrap();

        assert_eq!(host.poll_event_fd(event).unwrap(), read);
        assert_eq!(host.close_fd(read), Err(AsyncHostError::Badf));
        host.close_fd(write).unwrap();
    }

    #[test]
    fn stale_file_handle_is_rejected_after_close() {
        let host = default_host();
        let [read, write] = host.pipe(true, true).unwrap();

        host.close_fd(read).unwrap();

        let buffer = host.buffers().publish(vec![1].into_boxed_slice());
        for handle in [read, buffer, null_handle(), 0] {
            assert!(matches!(
                host.acquire_resource(handle),
                Err(AsyncHostError::Badf)
            ));
            assert_eq!(host.close_fd(handle), Err(AsyncHostError::Badf));
        }
        host.buffers().free(buffer).unwrap();
        host.close_fd(write).unwrap();
    }
}
