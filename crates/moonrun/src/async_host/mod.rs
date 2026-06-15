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
    self, HostFile, HostFileTable, HostProcess, HostProcessTable, HostWorkerHandle, HostWorkerJob,
    Job,
};

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
compile_error!("moonrun async wasm host currently supports only Linux, macOS, and Windows hosts");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncHostError {
    Fault,
    Inval,
    Badf,
    NotSupported,
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

    fn fill_exact(&mut self, offset: i32, len: i32, value: u8) -> AsyncHostResult<()> {
        let dst = self.read_exact_mut(offset, len)?;
        dst.fill(value);
        Ok(())
    }

    fn write_i32_le(&mut self, offset: i32, value: i32) -> AsyncHostResult<()> {
        self.write_exact(offset, &value.to_le_bytes())
    }
}

fn guest_bounds(offset: i32, len: i32) -> AsyncHostResult<(usize, usize)> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let end = offset.checked_add(len).ok_or(AsyncHostError::Fault)?;
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

    fn discard(&mut self, handle: i32) -> AsyncHostResult<()> {
        let (index, generation) = decode_handle(handle)?;
        let slot = self.slots.get_mut(index).ok_or(AsyncHostError::Badf)?;
        if slot.generation != generation {
            return Err(AsyncHostError::Badf);
        }
        // Worker jobs are removed from the table while the worker owns them.
        // Cancellation may still free the guest handle immediately; in that
        // case the worker drops the job when it later finishes.
        if slot.value.is_none() && !slot.reserved {
            return Err(AsyncHostError::Badf);
        }
        slot.value = None;
        slot.reserved = false;
        slot.generation = next_generation(slot.generation);
        Ok(())
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

fn encode_handle(index: usize, generation: u16) -> AsyncHostResult<i32> {
    if index >= 0x1_0000 {
        return Err(AsyncHostError::Fault);
    }
    if generation == 0 || generation > MAX_HANDLE_GENERATION {
        return Err(AsyncHostError::Fault);
    }
    Ok(((i32::from(generation)) << 16) | i32::try_from(index).unwrap())
}

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

fn next_generation(generation: u16) -> u16 {
    match generation.checked_add(1) {
        Some(next) if next <= MAX_HANDLE_GENERATION => next,
        _ => 1,
    }
}

#[derive(Default)]
struct AsyncHostState {
    errno: i32,
    jobs: HandleTable<Job>,
    files: HandleTable<HostFile>,
    processes: HandleTable<HostProcess>,
    workers: HandleTable<HostWorkerHandle>,
    completions: VecDeque<HostCompletion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HostCompletion {
    job_id: i32,
    job_handle: i32,
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
        let len = memory.read_exact(offset, len)?.len();
        i32::try_from(len).map_err(|_| AsyncHostError::Fault)
    }

    pub(crate) fn zero_guest(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        offset: i32,
        len: i32,
    ) -> AsyncHostResult<()> {
        memory.fill_exact(offset, len, 0)
    }

    pub(crate) fn insert_job(&self, job: Job) -> AsyncHostResult<i32> {
        self.state.lock().unwrap().jobs.insert(job)
    }

    pub(crate) fn free_job(&self, handle: i32) -> AsyncHostResult<()> {
        self.state.lock().unwrap().jobs.discard(handle)?;
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

    pub(crate) fn spawn_worker(&self, job_id: i32, job_handle: i32) -> AsyncHostResult<i32> {
        let worker = self.spawn_worker_thread(HostWorkerJob { job_id, job_handle });
        self.state.lock().unwrap().workers.insert(worker)
    }

    pub(crate) fn wake_worker(
        &self,
        worker_handle: i32,
        job_id: i32,
        job_handle: i32,
    ) -> AsyncHostResult<()> {
        let state = self.state.lock().unwrap();
        let worker = state.workers.get(worker_handle)?;
        thread_pool::wake_worker(worker, HostWorkerJob { job_id, job_handle });
        Ok(())
    }

    pub(crate) fn worker_enter_idle(&self, worker_handle: i32) -> AsyncHostResult<()> {
        let state = self.state.lock().unwrap();
        let worker = state.workers.get(worker_handle)?;
        thread_pool::worker_enter_idle(worker);
        Ok(())
    }

    pub(crate) fn free_worker(&self, worker_handle: i32) -> AsyncHostResult<()> {
        let worker = self.state.lock().unwrap().workers.remove(worker_handle)?;
        thread_pool::free_worker(worker);
        Ok(())
    }

    pub(crate) fn cancel_worker(&self, worker_handle: i32) -> AsyncHostResult<i32> {
        let state = self.state.lock().unwrap();
        let worker = state.workers.get(worker_handle)?;
        Ok(thread_pool::cancel_worker(worker))
    }

    pub(crate) fn fetch_completion(
        &self,
        memory: &mut (impl GuestMemory + ?Sized),
        dst: i32,
        max_jobs: i32,
    ) -> AsyncHostResult<i32> {
        let max_jobs = usize::try_from(max_jobs).map_err(|_| AsyncHostError::Fault)?;
        let mut state = self.state.lock().unwrap();
        let n = max_jobs.min(state.completions.len());
        if n == 0 {
            return Ok(0);
        }
        let bytes = n
            .checked_mul(std::mem::size_of::<i32>())
            .ok_or(AsyncHostError::Fault)?;
        let bytes_i32 = i32::try_from(bytes).map_err(|_| AsyncHostError::Fault)?;
        memory.read_exact(dst, bytes_i32)?;

        let completions = state
            .completions
            .iter()
            .take(n)
            .copied()
            .collect::<Vec<_>>();
        for completion in &completions {
            if let Ok(job) = state.jobs.get_mut(completion.job_handle) {
                thread_pool::complete_guest_job(job, memory)?;
            }
        }
        for (index, completion) in completions.into_iter().enumerate() {
            let removed = state.completions.pop_front().ok_or(AsyncHostError::Inval)?;
            debug_assert_eq!(removed, completion);
            let offset = dst
                .checked_add(i32::try_from(index * 4).map_err(|_| AsyncHostError::Fault)?)
                .ok_or(AsyncHostError::Fault)?;
            memory.write_i32_le(offset, completion.job_id)?;
        }
        Ok(bytes_i32)
    }

    fn spawn_worker_thread(&self, init_job: HostWorkerJob) -> HostWorkerHandle {
        let state = Arc::clone(&self.state);
        let run_state = Arc::clone(&state);
        thread_pool::spawn_worker(
            init_job,
            move |worker_job| {
                let Ok(mut job) = run_state.lock().unwrap().jobs.take(worker_job.job_handle) else {
                    return;
                };

                let mut files = SharedFileTable {
                    state: Arc::clone(&run_state),
                };
                thread_pool::run_host_job(&mut job, &mut files);

                let mut state = run_state.lock().unwrap();
                let _ = state.jobs.put(worker_job.job_handle, job);
            },
            move |worker_job| {
                // Even if cancellation discarded the job handle, the event loop
                // still needs the completion to move the worker out of running.
                state.lock().unwrap().completions.push_back(HostCompletion {
                    job_id: worker_job.job_id,
                    job_handle: worker_job.job_handle,
                });
            },
        )
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

pub(crate) fn checked_range(memory: &[u8], offset: i32, len: i32) -> AsyncHostResult<&[u8]> {
    memory.read_exact(offset, len)
}

pub(crate) fn checked_mut_range(
    memory: &mut [u8],
    offset: i32,
    len: i32,
) -> AsyncHostResult<&mut [u8]> {
    memory.read_exact_mut(offset, len)
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
    fn guest_memory_writes_fixed_little_endian_words() {
        let mut memory = [0; 16];

        memory.write_i32_le(2, 0x1020_3040).unwrap();

        assert_eq!(&memory[2..6], &[0x40, 0x30, 0x20, 0x10]);
        assert_eq!(memory.write_i32_le(14, 1), Err(AsyncHostError::Fault));
    }

    #[test]
    fn fetch_completion_copies_job_output_before_publishing_job_id() {
        let host = AsyncHost::default();
        let job = thread_pool::make_read_job(0, 8, 0, 3, -1);
        let job_handle = host.insert_job(job).unwrap();
        {
            let mut state = host.state.lock().unwrap();
            let job = state.jobs.get_mut(job_handle).unwrap();
            let thread_pool::JobPayload::Read { result, .. } = job.payload_mut() else {
                panic!("expected read job");
            };
            *result = Some(b"abc".to_vec());
            state.completions.push_back(HostCompletion {
                job_id: 42,
                job_handle,
            });
        }

        let mut memory = vec![0; 16];
        let bytes = host.fetch_completion(memory.as_mut_slice(), 0, 1).unwrap();

        assert_eq!(bytes, 4);
        assert_eq!(i32::from_le_bytes(memory[0..4].try_into().unwrap()), 42);
        assert_eq!(&memory[8..11], b"abc");
        assert!(host.state.lock().unwrap().completions.is_empty());
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
