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

use std::os::windows::io::{
    AsRawHandle, AsRawSocket, BorrowedSocket, FromRawHandle, OwnedHandle, RawHandle,
};
use std::sync::Arc;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::fd_util::stub::RawFd;
use crate::async_sys::ported_fns;

use super::{EVENT_BUFFER_SIZE, PollEvent, PollInstance, last_errno, last_native_error};

#[derive(Debug, Clone)]
pub(crate) struct CompletionPort(Arc<OwnedHandle>);

// A completion port handle may be used from worker threads to post completion
// packets. Share ownership with PollInstance so a worker cannot post through a
// stale handle if the guest destroys the poll instance before the worker exits.

impl CompletionPort {
    pub(crate) fn from_poll(poll: &PollInstance) -> Self {
        Self(Arc::clone(&poll.fd))
    }
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_bus_create"
    )]
    pub(crate) fn poll_create() -> AsyncHostResult<PollInstance> {
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::System::IO::CreateIoCompletionPort;

        let fd = unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, std::ptr::null_mut(), 0, 0) };
        if fd.is_null() {
            Err(last_native_error())
        } else {
            Ok(PollInstance {
                fd: Arc::new(unsafe { OwnedHandle::from_raw_handle(fd) }),
                events: Vec::new(),
            })
        }
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_bus_destroy"
    )]
    pub(crate) fn poll_destroy(instance: PollInstance) {
        drop(instance);
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_bus_register"
    )]
    fn poll_register(
        instance: &PollInstance,
        fd: RawFd,
        read_only: bool,
    ) -> AsyncHostResult<()> {
        use windows_sys::Win32::Storage::FileSystem::SetFileCompletionNotificationModes;
        use windows_sys::Win32::System::IO::CreateIoCompletionPort;
        use windows_sys::Win32::System::WindowsProgramming::FILE_SKIP_COMPLETION_PORT_ON_SUCCESS;

        let _ = read_only;
        if unsafe { SetFileCompletionNotificationModes(fd, FILE_SKIP_COMPLETION_PORT_ON_SUCCESS as u8) } == 0 {
            return Err(last_native_error());
        }
        let registered =
            unsafe { CreateIoCompletionPort(fd, instance.raw_fd(), fd as usize, 0) };
        if registered.is_null() {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_bus_wait"
    )]
    pub(crate) fn poll_wait(instance: &mut PollInstance, timeout: i32) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::WAIT_TIMEOUT;
        use windows_sys::Win32::System::IO::GetQueuedCompletionStatusEx;
        use windows_sys::Win32::System::Threading::INFINITE;

        let mut entries = vec![empty_overlapped_entry(); EVENT_BUFFER_SIZE];
        let mut count = 0;
        let ok = unsafe {
            GetQueuedCompletionStatusEx(
                instance.raw_fd(),
                entries.as_mut_ptr(),
                EVENT_BUFFER_SIZE as u32,
                &mut count,
                if timeout < 0 { INFINITE } else { timeout as u32 },
                0,
            )
        };
        if ok == 0 {
            if last_errno() == WAIT_TIMEOUT as i32 {
                instance.events.clear();
                return Ok(0);
            }
            return Err(last_native_error());
        }
        instance.events = entries
            .into_iter()
            .take(count as usize)
            .map(|entry| {
                let fd = entry.lpCompletionKey as RawFd;
                let worker_generation = if is_thread_pool_completion(fd) {
                    worker_generation_from_overlapped(entry.lpOverlapped)
                } else {
                    None
                };
                PollEvent {
                    fd: entry.lpCompletionKey,
                    events: 0,
                    // Worker completion packets use lpOverlapped only as a
                    // host generation token; guest-visible worker events match
                    // native and expose no IO result.
                    io_result: if worker_generation.is_some() {
                        0
                    } else {
                        entry.lpOverlapped as usize
                    },
                    bytes_transferred: entry.dwNumberOfBytesTransferred as i32,
                    worker_generation,
                }
            })
            .collect();
        i32::try_from(count).map_err(|_| AsyncHostError::Fault)
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_list_get"
    )]
    pub(crate) fn event_list_get(instance: &PollInstance, index: i32) -> AsyncHostResult<&PollEvent> {
        let index = usize::try_from(index).map_err(|_| AsyncHostError::Fault)?;
        instance.events.get(index).ok_or(AsyncHostError::Fault)
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_get_fd"
    )]
    pub(crate) fn event_get_fd(event: &PollEvent) -> RawFd {
        event.fd as RawFd
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_get_io_result"
    )]
    pub(crate) fn event_get_io_result(
        event: &PollEvent,
    ) -> *mut windows_sys::Win32::System::IO::OVERLAPPED {
        event.io_result as *mut windows_sys::Win32::System::IO::OVERLAPPED
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_get_bytes_transferred"
    )]
    pub(crate) fn event_get_bytes_transferred(event: &PollEvent) -> i32 {
        event.bytes_transferred
    }
}

pub(crate) fn poll_register_file(
    instance: &PollInstance,
    handle: RawHandle,
    read_only: bool,
) -> AsyncHostResult<()> {
    poll_register(instance, handle, read_only)
}

pub(crate) fn poll_register_socket(
    instance: &PollInstance,
    socket: BorrowedSocket<'_>,
    read_only: bool,
) -> AsyncHostResult<()> {
    // IOCP accepts a socket value in the HANDLE parameter. Keep that Windows
    // ABI conversion inside the IOCP adapter rather than the resource model.
    poll_register(instance, socket.as_raw_socket() as RawFd, read_only)
}

pub(crate) fn post_thread_pool_completion(
    completion_port: &CompletionPort,
    completion_id: i32,
    generation: usize,
) -> AsyncHostResult<()> {
    use windows_sys::Win32::Foundation::{GetLastError, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::IO::PostQueuedCompletionStatus;

    debug_assert_ne!(generation, 0);
    // Native thread_pool.c posts worker completions to the event bus IOCP with
    // INVALID_HANDLE_VALUE as the completion key and the native job id as
    // transferred bytes. Rust treats that value as an opaque completion id.
    if unsafe {
        PostQueuedCompletionStatus(
            completion_port.0.as_raw_handle(),
            completion_id as u32,
            INVALID_HANDLE_VALUE as usize,
            worker_generation_to_overlapped(generation),
        )
    } == 0
    {
        return Err(AsyncHostError::Native(unsafe { GetLastError() } as i32));
    }
    Ok(())
}

pub(crate) fn retain_current_thread_pool_completions(
    instance: &mut PollInstance,
    generation: Option<usize>,
) -> AsyncHostResult<i32> {
    instance.events.retain(|event| {
        event
            .worker_generation
            .is_none_or(|event_generation| Some(event_generation) == generation)
    });
    i32::try_from(instance.events.len()).map_err(|_| AsyncHostError::Fault)
}

fn is_thread_pool_completion(fd: RawFd) -> bool {
    fd == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE
}

fn worker_generation_to_overlapped(
    generation: usize,
) -> *mut windows_sys::Win32::System::IO::OVERLAPPED {
    generation as *mut windows_sys::Win32::System::IO::OVERLAPPED
}

fn worker_generation_from_overlapped(
    overlapped: *mut windows_sys::Win32::System::IO::OVERLAPPED,
) -> Option<usize> {
    let generation = overlapped as usize;
    (generation != 0).then_some(generation)
}

fn empty_overlapped_entry() -> windows_sys::Win32::System::IO::OVERLAPPED_ENTRY {
    unsafe { std::mem::zeroed() }
}
