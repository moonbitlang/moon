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

use std::collections::VecDeque;
use std::fs::File;
use std::sync::{Arc, Mutex};

use crate::async_sys::internal::event_loop::thread_pool::{
    self, HostFile, HostFileTable, HostProcess, HostProcessTable, HostWorkerHandle, Job,
};

pub(crate) mod types;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
compile_error!("moonrun async wasm host currently supports only Linux, macOS, and Windows hosts");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncHostError {
    Fault,
    Inval,
    #[allow(dead_code)]
    Badf,
    NotSupported,
    #[allow(dead_code)]
    Native(i32),
}

pub(crate) type AsyncHostResult<T> = Result<T, AsyncHostError>;

#[cfg(unix)]
mod native_errno {
    pub(crate) const BADF: i32 = libc::EBADF;
    pub(crate) const FAULT: i32 = libc::EFAULT;
    pub(crate) const INVAL: i32 = libc::EINVAL;
    pub(crate) const NOT_SUPPORTED: i32 = libc::ENOSYS;
}

#[cfg(windows)]
mod native_errno {
    use windows_sys::Win32::Foundation::{
        ERROR_CALL_NOT_IMPLEMENTED, ERROR_INVALID_ADDRESS, ERROR_INVALID_HANDLE,
        ERROR_INVALID_PARAMETER,
    };

    pub(crate) const BADF: i32 = ERROR_INVALID_HANDLE as i32;
    pub(crate) const FAULT: i32 = ERROR_INVALID_ADDRESS as i32;
    pub(crate) const INVAL: i32 = ERROR_INVALID_PARAMETER as i32;
    pub(crate) const NOT_SUPPORTED: i32 = ERROR_CALL_NOT_IMPLEMENTED as i32;
}

impl AsyncHostError {
    pub(crate) fn errno(self) -> i32 {
        match self {
            Self::Fault => native_errno::FAULT,
            Self::Inval => native_errno::INVAL,
            Self::Badf => native_errno::BADF,
            Self::NotSupported => native_errno::NOT_SUPPORTED,
            Self::Native(errno) => errno,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GuestRange {
    offset: usize,
    len: usize,
}

impl GuestRange {
    pub(crate) fn new(offset: i32, len: i32) -> AsyncHostResult<Self> {
        let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        Ok(Self { offset, len })
    }

    fn end(self) -> AsyncHostResult<usize> {
        self.offset
            .checked_add(self.len)
            .ok_or(AsyncHostError::Fault)
    }
}

#[allow(dead_code)]
pub(crate) trait GuestMemory {
    fn bytes(&self) -> &[u8];

    fn bytes_mut(&mut self) -> &mut [u8];

    fn read(&self, range: GuestRange) -> AsyncHostResult<&[u8]> {
        let end = range.end()?;
        self.bytes()
            .get(range.offset..end)
            .ok_or(AsyncHostError::Fault)
    }

    fn write(&mut self, range: GuestRange, data: &[u8]) -> AsyncHostResult<()> {
        if range.len != data.len() {
            return Err(AsyncHostError::Inval);
        }
        let end = range.end()?;
        let dst = self
            .bytes_mut()
            .get_mut(range.offset..end)
            .ok_or(AsyncHostError::Fault)?;
        dst.copy_from_slice(data);
        Ok(())
    }

    fn fill(&mut self, range: GuestRange, value: u8) -> AsyncHostResult<()> {
        let end = range.end()?;
        let dst = self
            .bytes_mut()
            .get_mut(range.offset..end)
            .ok_or(AsyncHostError::Fault)?;
        dst.fill(value);
        Ok(())
    }

    fn read_i32_le(&self, offset: i32) -> AsyncHostResult<i32> {
        let bytes = self.read(GuestRange::new(offset, 4)?)?;
        Ok(i32::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn write_i32_le(&mut self, offset: i32, value: i32) -> AsyncHostResult<()> {
        self.write(GuestRange::new(offset, 4)?, &value.to_le_bytes())
    }
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

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostResourceKind {
    File,
    Poll,
    Job,
    Worker,
    IoResult,
    RawFd,
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct HostResource {
    kind: HostResourceKind,
}

#[allow(dead_code)]
impl HostResource {
    pub(crate) fn new(kind: HostResourceKind) -> Self {
        Self { kind }
    }

    pub(crate) fn kind(&self) -> HostResourceKind {
        self.kind
    }
}

#[allow(dead_code)]
#[derive(Debug)]
struct ResourceSlot {
    generation: u16,
    resource: Option<HostResource>,
}

#[allow(dead_code)]
#[derive(Debug, Default)]
struct ResourceTable {
    slots: Vec<ResourceSlot>,
}

#[derive(Debug)]
struct HandleTable<T> {
    slots: Vec<HandleSlot<T>>,
}

#[derive(Debug)]
struct HandleSlot<T> {
    generation: u16,
    value: Option<T>,
    reserved: bool,
}

// Guest handles are signed i32 values, so the high generation bit must stay
// clear for every handle returned to MoonBit.
const MAX_HANDLE_GENERATION: u16 = 0x7fff;

impl<T> Default for HandleTable<T> {
    fn default() -> Self {
        Self { slots: Vec::new() }
    }
}

impl<T> HandleTable<T> {
    fn insert(&mut self, value: T) -> AsyncHostResult<i32> {
        if let Some((index, slot)) = self
            .slots
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.value.is_none() && !slot.reserved)
        {
            slot.value = Some(value);
            return encode_handle(index, slot.generation);
        }

        let index = self.slots.len();
        self.slots.push(HandleSlot {
            generation: 1,
            value: Some(value),
            reserved: false,
        });
        encode_handle(index, 1)
    }

    fn get(&self, handle: i32) -> AsyncHostResult<&T> {
        let (index, generation) = decode_handle(handle)?;
        let slot = self.slots.get(index).ok_or(AsyncHostError::Badf)?;
        if slot.generation != generation {
            return Err(AsyncHostError::Badf);
        }
        slot.value.as_ref().ok_or(AsyncHostError::Badf)
    }

    fn get_mut(&mut self, handle: i32) -> AsyncHostResult<&mut T> {
        let (index, generation) = decode_handle(handle)?;
        let slot = self.slots.get_mut(index).ok_or(AsyncHostError::Badf)?;
        if slot.generation != generation {
            return Err(AsyncHostError::Badf);
        }
        slot.value.as_mut().ok_or(AsyncHostError::Badf)
    }

    fn remove(&mut self, handle: i32) -> AsyncHostResult<T> {
        let (index, generation) = decode_handle(handle)?;
        let slot = self.slots.get_mut(index).ok_or(AsyncHostError::Badf)?;
        if slot.generation != generation {
            return Err(AsyncHostError::Badf);
        }
        let value = slot.value.take().ok_or(AsyncHostError::Badf)?;
        slot.reserved = false;
        slot.generation = next_generation(slot.generation);
        Ok(value)
    }

    fn take(&mut self, handle: i32) -> AsyncHostResult<T> {
        let (index, generation) = decode_handle(handle)?;
        let slot = self.slots.get_mut(index).ok_or(AsyncHostError::Badf)?;
        if slot.generation != generation {
            return Err(AsyncHostError::Badf);
        }
        let value = slot.value.take().ok_or(AsyncHostError::Badf)?;
        slot.reserved = true;
        Ok(value)
    }

    fn put(&mut self, handle: i32, value: T) -> AsyncHostResult<()> {
        let (index, generation) = decode_handle(handle)?;
        let slot = self.slots.get_mut(index).ok_or(AsyncHostError::Badf)?;
        if slot.generation != generation || slot.value.is_some() || !slot.reserved {
            return Err(AsyncHostError::Badf);
        }
        slot.value = Some(value);
        slot.reserved = false;
        Ok(())
    }
}

impl HostFileTable for HandleTable<HostFile> {
    fn insert_file(&mut self, file: File) -> AsyncHostResult<i32> {
        self.insert(HostFile::new(file))
    }

    fn with_file_mut<U>(
        &mut self,
        handle: i32,
        f: impl FnOnce(&mut File) -> AsyncHostResult<U>,
    ) -> AsyncHostResult<U> {
        f(self.get_mut(handle)?.file_mut())
    }

    fn with_host_file_mut<U>(
        &mut self,
        handle: i32,
        f: impl FnOnce(&mut HostFile) -> AsyncHostResult<U>,
    ) -> AsyncHostResult<U> {
        f(self.get_mut(handle)?)
    }
}

impl HostProcessTable for HandleTable<HostProcess> {
    fn insert_process(&mut self, process: HostProcess) -> AsyncHostResult<i32> {
        self.insert(process)
    }

    fn take_process(&mut self, handle: i32) -> AsyncHostResult<HostProcess> {
        self.remove(handle)
    }
}

#[allow(dead_code)]
impl ResourceTable {
    fn insert(&mut self, resource: HostResource) -> AsyncHostResult<i32> {
        if let Some((index, slot)) = self
            .slots
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.resource.is_none())
        {
            slot.resource = Some(resource);
            return encode_handle(index, slot.generation);
        }

        let index = self.slots.len();
        self.slots.push(ResourceSlot {
            generation: 1,
            resource: Some(resource),
        });
        encode_handle(index, 1)
    }

    fn get(&self, handle: i32) -> AsyncHostResult<&HostResource> {
        let (index, generation) = decode_handle(handle)?;
        let slot = self.slots.get(index).ok_or(AsyncHostError::Badf)?;
        if slot.generation != generation {
            return Err(AsyncHostError::Badf);
        }
        slot.resource.as_ref().ok_or(AsyncHostError::Badf)
    }

    fn remove(&mut self, handle: i32) -> AsyncHostResult<HostResource> {
        let (index, generation) = decode_handle(handle)?;
        let slot = self.slots.get_mut(index).ok_or(AsyncHostError::Badf)?;
        if slot.generation != generation {
            return Err(AsyncHostError::Badf);
        }
        let resource = slot.resource.take().ok_or(AsyncHostError::Badf)?;
        slot.generation = next_generation(slot.generation);
        Ok(resource)
    }
}

#[allow(dead_code)]
fn encode_handle(index: usize, generation: u16) -> AsyncHostResult<i32> {
    if index >= 0x1_0000 {
        return Err(AsyncHostError::Fault);
    }
    if generation == 0 || generation > MAX_HANDLE_GENERATION {
        return Err(AsyncHostError::Fault);
    }
    Ok(((i32::from(generation)) << 16) | i32::try_from(index).unwrap())
}

#[allow(dead_code)]
fn decode_handle(handle: i32) -> AsyncHostResult<(usize, u16)> {
    if handle <= 0 {
        return Err(AsyncHostError::Badf);
    }
    let index = (handle as u32 & 0xffff) as usize;
    let generation = ((handle as u32 >> 16) & 0xffff) as u16;
    if generation == 0 {
        return Err(AsyncHostError::Badf);
    }
    Ok((index, generation))
}

#[allow(dead_code)]
fn next_generation(generation: u16) -> u16 {
    match generation.checked_add(1) {
        Some(next) if next <= MAX_HANDLE_GENERATION => next,
        _ => 1,
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PendingGuestWrite {
    dst: GuestRange,
    data: Vec<u8>,
}

#[allow(dead_code)]
impl PendingGuestWrite {
    pub(crate) fn new(dst: GuestRange, data: Vec<u8>) -> Self {
        Self { dst, data }
    }

    pub(crate) fn complete(self, memory: &mut (impl GuestMemory + ?Sized)) -> AsyncHostResult<()> {
        memory.write(self.dst, &self.data)
    }
}

#[derive(Default)]
struct AsyncHostState {
    errno: i32,
    #[allow(dead_code)]
    resources: ResourceTable,
    jobs: HandleTable<Job>,
    files: HandleTable<HostFile>,
    processes: HandleTable<HostProcess>,
    workers: HandleTable<HostWorkerHandle>,
    completions: VecDeque<i32>,
}

#[derive(Default)]
pub(crate) struct AsyncHost {
    state: Arc<Mutex<AsyncHostState>>,
}

impl AsyncHost {
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

    pub(crate) fn unsupported_return(&self) -> i32 {
        self.record_error(AsyncHostError::NotSupported);
        -1
    }

    pub(crate) fn copy_from_guest_len(
        &self,
        memory: &(impl GuestMemory + ?Sized),
        offset: i32,
        len: i32,
    ) -> AsyncHostResult<i32> {
        let len = memory.read(GuestRange::new(offset, len)?)?.len();
        i32::try_from(len).map_err(|_| AsyncHostError::Fault)
    }

    pub(crate) fn zero_guest(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        offset: i32,
        len: i32,
    ) -> AsyncHostResult<()> {
        memory.fill(GuestRange::new(offset, len)?, 0)
    }

    #[allow(dead_code)]
    pub(crate) fn insert_resource(&self, resource: HostResource) -> AsyncHostResult<i32> {
        self.state.lock().unwrap().resources.insert(resource)
    }

    #[allow(dead_code)]
    pub(crate) fn resource_kind(&self, handle: i32) -> AsyncHostResult<HostResourceKind> {
        Ok(self.state.lock().unwrap().resources.get(handle)?.kind())
    }

    #[allow(dead_code)]
    pub(crate) fn remove_resource(&self, handle: i32) -> AsyncHostResult<HostResource> {
        self.state.lock().unwrap().resources.remove(handle)
    }

    pub(crate) fn insert_job(&self, job: Job) -> AsyncHostResult<i32> {
        self.state.lock().unwrap().jobs.insert(job)
    }

    pub(crate) fn free_job(&self, handle: i32) -> AsyncHostResult<()> {
        self.state.lock().unwrap().jobs.remove(handle)?;
        Ok(())
    }

    pub(crate) fn job_get_ret(&self, handle: i32) -> AsyncHostResult<i64> {
        Ok(
            crate::async_sys::internal::event_loop::thread_pool::job_get_ret(
                self.state.lock().unwrap().jobs.get(handle)?,
            ),
        )
    }

    pub(crate) fn job_get_err(&self, handle: i32) -> AsyncHostResult<i32> {
        Ok(
            crate::async_sys::internal::event_loop::thread_pool::job_get_err(
                self.state.lock().unwrap().jobs.get(handle)?,
            ),
        )
    }

    pub(crate) fn open_job_get_fd(&self, handle: i32) -> AsyncHostResult<i32> {
        let state = self.state.lock().unwrap();
        let result = thread_pool::open_job_result(state.jobs.get(handle)?)?;
        Ok(thread_pool::open_job_get_fd(result))
    }

    pub(crate) fn open_job_get_kind(&self, handle: i32) -> AsyncHostResult<i32> {
        let state = self.state.lock().unwrap();
        let result = thread_pool::open_job_result(state.jobs.get(handle)?)?;
        Ok(thread_pool::open_job_get_kind(result))
    }

    pub(crate) fn open_job_get_dev_id(&self, handle: i32) -> AsyncHostResult<u64> {
        let state = self.state.lock().unwrap();
        let result = thread_pool::open_job_result(state.jobs.get(handle)?)?;
        Ok(thread_pool::open_job_get_dev_id(result))
    }

    pub(crate) fn open_job_get_file_id(&self, handle: i32) -> AsyncHostResult<u64> {
        let state = self.state.lock().unwrap();
        let result = thread_pool::open_job_result(state.jobs.get(handle)?)?;
        Ok(thread_pool::open_job_get_file_id(result))
    }

    pub(crate) fn get_file_size_result(&self, handle: i32) -> AsyncHostResult<i64> {
        crate::async_sys::internal::event_loop::thread_pool::get_file_size_result(
            self.state.lock().unwrap().jobs.get(handle)?,
        )
    }

    pub(crate) fn close_fd(&self, handle: i32) -> AsyncHostResult<()> {
        self.state.lock().unwrap().files.remove(handle)?;
        Ok(())
    }

    pub(crate) fn pipe(&self) -> AsyncHostResult<[i32; 2]> {
        crate::async_sys::internal::fd_util::stub::pipe_host_files(
            &mut self.state.lock().unwrap().files,
        )
    }

    pub(crate) fn try_lock_file(&self, handle: i32, exclusive: bool) -> AsyncHostResult<()> {
        let mut state = self.state.lock().unwrap();
        crate::async_sys::fs::stub::try_lock_host_file(state.files.get_mut(handle)?, exclusive)
    }

    pub(crate) fn unlock_file(&self, handle: i32) -> AsyncHostResult<()> {
        let mut state = self.state.lock().unwrap();
        crate::async_sys::fs::stub::unlock_host_file(state.files.get_mut(handle)?)
    }

    pub(crate) fn spawn_process(
        &self,
        command: String,
        args: Vec<String>,
        stdin: i32,
        stdout: i32,
        stderr: i32,
    ) -> AsyncHostResult<i32> {
        let mut state = self.state.lock().unwrap();
        let AsyncHostState {
            files, processes, ..
        } = &mut *state;
        thread_pool::spawn_process(files, processes, command, args, stdin, stdout, stderr)
    }

    pub(crate) fn make_wait_for_process_job(&self, process: i32) -> AsyncHostResult<i32> {
        let mut state = self.state.lock().unwrap();
        let job =
            thread_pool::make_wait_for_process_job_from_handle(&mut state.processes, process)?;
        state.jobs.insert(job)
    }

    pub(crate) fn run_job(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: i32,
    ) -> AsyncHostResult<()> {
        let mut job = self.state.lock().unwrap().jobs.take(handle)?;
        let mut files = SharedFileTable {
            state: Arc::clone(&self.state),
        };
        thread_pool::run_host_job(&mut job, &mut files);
        thread_pool::complete_guest_job(&mut job, memory)?;
        self.state.lock().unwrap().jobs.put(handle, job)?;
        Ok(())
    }

    pub(crate) fn complete_job(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        handle: i32,
    ) -> AsyncHostResult<()> {
        let mut state = self.state.lock().unwrap();
        let job = state.jobs.get_mut(handle)?;
        thread_pool::complete_guest_job(job, memory)
    }

    pub(crate) fn spawn_worker(&self, job_id: i32, job_handle: i32) -> AsyncHostResult<i32> {
        let worker = self.spawn_worker_thread();
        worker
            .run(job_id, job_handle)
            .map_err(|_| AsyncHostError::Badf)?;
        self.state.lock().unwrap().workers.insert(worker)
    }

    pub(crate) fn wake_worker(
        &self,
        worker_handle: i32,
        job_id: i32,
        job_handle: i32,
    ) -> AsyncHostResult<()> {
        self.state
            .lock()
            .unwrap()
            .workers
            .get(worker_handle)?
            .run(job_id, job_handle)
            .map_err(|_| AsyncHostError::Badf)
    }

    pub(crate) fn worker_enter_idle(&self, worker_handle: i32) -> AsyncHostResult<()> {
        self.state.lock().unwrap().workers.get(worker_handle)?;
        Ok(())
    }

    pub(crate) fn free_worker(&self, worker_handle: i32) -> AsyncHostResult<()> {
        let mut worker = self.state.lock().unwrap().workers.remove(worker_handle)?;
        worker.join();
        Ok(())
    }

    pub(crate) fn cancel_worker(&self, worker_handle: i32) -> AsyncHostResult<i32> {
        self.state.lock().unwrap().workers.get(worker_handle)?;

        #[cfg(unix)]
        {
            // The wasm wrapper interprets 0 as coroutine-level NoWait. The
            // host worker may still complete later; the event loop drains that
            // completion independently.
            Ok(0)
        }

        #[cfg(windows)]
        {
            // Native Windows uses CancelSynchronousIo and then waits for the
            // completion. Keep the same surface until the Windows worker
            // cancellation path is ported fully.
            Ok(1)
        }

        #[cfg(not(any(unix, windows)))]
        {
            Err(AsyncHostError::NotSupported)
        }
    }

    pub(crate) fn fetch_completion(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        dst: i32,
        max_jobs: i32,
    ) -> AsyncHostResult<i32> {
        let max_jobs = usize::try_from(max_jobs).map_err(|_| AsyncHostError::Fault)?;
        let mut state = self.state.lock().unwrap();
        let mut output = vec![0; max_jobs];
        let bytes = thread_pool::fetch_completion(&mut state.completions, &mut output)?;
        let n =
            usize::try_from(bytes).map_err(|_| AsyncHostError::Fault)? / std::mem::size_of::<i32>();
        for (index, job_id) in output.into_iter().take(n).enumerate() {
            let offset = dst
                .checked_add(i32::try_from(index * 4).map_err(|_| AsyncHostError::Fault)?)
                .ok_or(AsyncHostError::Fault)?;
            memory.write_i32_le(offset, job_id)?;
        }
        Ok(bytes)
    }

    fn spawn_worker_thread(&self) -> HostWorkerHandle {
        let state = Arc::clone(&self.state);
        HostWorkerHandle::spawn(move |worker_job| {
            let Ok(mut job) = state.lock().unwrap().jobs.take(worker_job.job_handle) else {
                return;
            };

            let mut files = SharedFileTable {
                state: Arc::clone(&state),
            };
            thread_pool::run_host_job(&mut job, &mut files);

            let mut state = state.lock().unwrap();
            if state.jobs.put(worker_job.job_handle, job).is_ok() {
                state.completions.push_back(worker_job.job_id);
            }
        })
    }
}

struct SharedFileTable {
    state: Arc<Mutex<AsyncHostState>>,
}

impl HostFileTable for SharedFileTable {
    fn insert_file(&mut self, file: File) -> AsyncHostResult<i32> {
        self.state.lock().unwrap().files.insert(HostFile::new(file))
    }

    fn with_file_mut<U>(
        &mut self,
        handle: i32,
        f: impl FnOnce(&mut File) -> AsyncHostResult<U>,
    ) -> AsyncHostResult<U> {
        let mut file = {
            let mut state = self.state.lock().unwrap();
            state
                .files
                .get_mut(handle)?
                .file_mut()
                .try_clone()
                .map_err(native_io_error)?
        };
        f(&mut file)
    }

    fn with_host_file_mut<U>(
        &mut self,
        handle: i32,
        f: impl FnOnce(&mut HostFile) -> AsyncHostResult<U>,
    ) -> AsyncHostResult<U> {
        let mut state = self.state.lock().unwrap();
        f(state.files.get_mut(handle)?)
    }
}

fn native_io_error(error: std::io::Error) -> AsyncHostError {
    AsyncHostError::Native(
        error
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}

#[allow(dead_code)]
pub(crate) fn checked_range(memory: &[u8], offset: i32, len: i32) -> AsyncHostResult<&[u8]> {
    memory.read(GuestRange::new(offset, len)?)
}

#[allow(dead_code)]
pub(crate) fn checked_mut_range(
    memory: &mut [u8],
    offset: i32,
    len: i32,
) -> AsyncHostResult<&mut [u8]> {
    let range = GuestRange::new(offset, len)?;
    let end = range.end()?;
    memory
        .get_mut(range.offset..end)
        .ok_or(AsyncHostError::Fault)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_range_accepts_in_bounds_access() {
        let memory = [1, 2, 3, 4];

        assert_eq!(checked_range(&memory, 1, 2).unwrap(), &[2, 3]);
        assert!(checked_range(&memory, 4, 0).unwrap().is_empty());
    }

    #[test]
    fn checked_range_rejects_out_of_bounds_access() {
        let memory = [0; 4];

        for (offset, len) in [(-1, 1), (0, -1), (3, 2), (i32::MAX, 1), (2, i32::MAX)] {
            assert_eq!(
                checked_range(&memory, offset, len),
                Err(AsyncHostError::Fault)
            );
        }
    }

    #[test]
    fn checked_mut_range_accepts_in_bounds_access() {
        let mut memory = [1, 2, 3, 4];

        checked_mut_range(&mut memory, 1, 2).unwrap().fill(9);

        assert_eq!(memory, [1, 9, 9, 4]);
    }

    #[test]
    fn checked_mut_range_rejects_out_of_bounds_access() {
        let mut memory = [0; 4];

        for (offset, len) in [(-1, 1), (0, -1), (3, 2), (i32::MAX, 1), (2, i32::MAX)] {
            assert_eq!(
                checked_mut_range(&mut memory, offset, len),
                Err(AsyncHostError::Fault)
            );
        }
    }

    #[test]
    fn guest_memory_reads_and_writes_fixed_little_endian_records() {
        let mut memory = [0; 8];

        memory.write_i32_le(2, 0x1020_3040).unwrap();

        assert_eq!(memory.read_i32_le(2).unwrap(), 0x1020_3040);
        assert_eq!(&memory[2..6], &[0x40, 0x30, 0x20, 0x10]);
        assert_eq!(memory.write_i32_le(6, 1), Err(AsyncHostError::Fault));
    }

    #[test]
    fn pending_guest_write_reacquires_current_memory() {
        let pending = PendingGuestWrite::new(GuestRange::new(4, 3).unwrap(), b"abc".to_vec());
        let mut grown_memory = vec![0; 16];

        pending.complete(grown_memory.as_mut_slice()).unwrap();

        assert_eq!(&grown_memory[4..7], b"abc");
    }

    #[test]
    fn process_spawn_and_wait_uses_host_process_table() {
        let host = AsyncHost::default();
        let process = host
            .spawn_process("true".to_string(), Vec::new(), -1, -1, -1)
            .unwrap();
        let job = host.make_wait_for_process_job(process).unwrap();
        let mut memory = [];

        host.run_job(&mut memory, job).unwrap();

        assert_eq!(host.job_get_ret(job).unwrap(), 0);
    }

    #[test]
    fn resource_handles_reject_invalid_and_stale_values() {
        let host = AsyncHost::default();
        let handle = host
            .insert_resource(HostResource::new(HostResourceKind::File))
            .unwrap();

        assert_eq!(host.resource_kind(handle), Ok(HostResourceKind::File));
        assert_eq!(
            host.remove_resource(handle).unwrap().kind(),
            HostResourceKind::File
        );
        assert_eq!(host.resource_kind(handle), Err(AsyncHostError::Badf));
        assert!(matches!(host.remove_resource(0), Err(AsyncHostError::Badf)));
    }

    #[test]
    fn table_reuse_keeps_handles_positive_after_generation_wrap() {
        let mut table = HandleTable::default();
        let mut handle = table.insert(1).unwrap();

        for _ in 0..=MAX_HANDLE_GENERATION {
            assert!(handle > 0);
            assert_eq!(table.remove(handle), Ok(1));
            handle = table.insert(1).unwrap();
        }

        assert!(handle > 0);
        let (_, generation) = decode_handle(handle).unwrap();
        assert!((1..=MAX_HANDLE_GENERATION).contains(&generation));
    }

    #[test]
    fn unsupported_records_native_errno() {
        let host = AsyncHost::default();

        assert_eq!(host.unsupported_return(), -1);
        assert_eq!(host.get_errno(), AsyncHostError::NotSupported.errno());
    }
}
