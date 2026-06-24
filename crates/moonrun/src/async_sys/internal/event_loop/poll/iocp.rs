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

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::fd_util::stub::RawFd;
use crate::async_sys::ported_fns;

use super::{EVENT_BUFFER_SIZE, PollEvent, PollInstance, last_errno, last_native_error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompletionPort(RawFd);

// A completion port handle may be used from worker threads to post completion
// packets. PollInstance owns the actual handle lifetime; this value is only a
// typed copy used while the owning poll instance remains registered in AsyncHost.
unsafe impl Send for CompletionPort {}

impl CompletionPort {
    pub(crate) fn from_poll(poll: &PollInstance) -> Self {
        Self(poll.raw_fd())
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
                fd,
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
    pub(crate) fn poll_register(
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
        let registered = unsafe { CreateIoCompletionPort(fd, instance.fd, fd as usize, 0) };
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
                instance.fd,
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
            .map(|entry| PollEvent {
                fd: entry.lpCompletionKey as RawFd,
                events: 0,
                io_result: entry.lpOverlapped,
                bytes_transferred: entry.dwNumberOfBytesTransferred as i32,
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
        event.fd
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_get_io_result"
    )]
    pub(crate) fn event_get_io_result(
        event: &PollEvent,
    ) -> *mut windows_sys::Win32::System::IO::OVERLAPPED {
        event.io_result
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_get_bytes_transferred"
    )]
    pub(crate) fn event_get_bytes_transferred(event: &PollEvent) -> i32 {
        event.bytes_transferred
    }
}

pub(crate) fn post_thread_pool_completion(
    completion_port: CompletionPort,
    job_id: i32,
) -> AsyncHostResult<()> {
    use windows_sys::Win32::Foundation::{GetLastError, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::IO::PostQueuedCompletionStatus;

    // Native thread_pool.c posts worker completions to the event bus IOCP with
    // INVALID_HANDLE_VALUE as the completion key and the job id as transferred bytes.
    if unsafe {
        PostQueuedCompletionStatus(
            completion_port.0,
            job_id as u32,
            INVALID_HANDLE_VALUE as usize,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(AsyncHostError::Native(unsafe { GetLastError() } as i32));
    }
    Ok(())
}

fn empty_overlapped_entry() -> windows_sys::Win32::System::IO::OVERLAPPED_ENTRY {
    unsafe { std::mem::zeroed() }
}
