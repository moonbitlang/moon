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

use std::os::windows::io::{
    AsRawHandle, AsRawSocket, BorrowedSocket, FromRawHandle, OwnedHandle, RawHandle,
};
use std::sync::Arc;

use crate::filesystem::RawFile as RawFd;
use crate::upstream::ported_fns;
use std::io;

use super::{EVENT_BUFFER_SIZE, PollEvent, PollInstance, last_errno, last_native_error};

#[derive(Debug, Clone)]
pub(crate) struct CompletionPort(Arc<OwnedHandle>);

// A completion port handle may be used from worker threads to post completion
// packets. Share ownership with PollInstance so a worker cannot post through a
// stale handle if the guest destroys the poll instance before the worker exits.

impl CompletionPort {
    pub(in crate::poll) fn from_poll(poll: &PollInstance) -> Self {
        Self(Arc::clone(&poll.fd))
    }

    pub(crate) fn post(&self, bytes: u32, user_data: usize) -> io::Result<()> {
        if unsafe {
            windows_sys::Win32::System::IO::PostQueuedCompletionStatus(
                self.0.as_raw_handle(),
                bytes,
                user_data,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(last_native_error());
        }
        Ok(())
    }

    /// Dequeue one completion during shutdown, including failed I/O results.
    /// Callers retain all request allocations until their packets are drained.
    pub(crate) fn wait_for_io(
        &self,
    ) -> io::Result<*mut windows_sys::Win32::System::IO::OVERLAPPED> {
        let mut entry = empty_overlapped_entry();
        let mut count = 0;
        if unsafe {
            windows_sys::Win32::System::IO::GetQueuedCompletionStatusEx(
                self.0.as_raw_handle(),
                &mut entry,
                1,
                &mut count,
                windows_sys::Win32::System::Threading::INFINITE,
                0,
            )
        } == 0
        {
            return Err(last_native_error());
        }
        Ok(entry.lpOverlapped)
    }
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_bus_create"
    )]
    pub(in crate::poll) fn poll_create() -> io::Result<PollInstance> {
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::System::IO::CreateIoCompletionPort;

        let fd = unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, std::ptr::null_mut(), 0, 0) };
        if fd.is_null() {
            Err(last_native_error())
        } else {
            Ok(PollInstance {
                fd: Arc::new(unsafe { OwnedHandle::from_raw_handle(fd) }),
                raw_events: vec![empty_overlapped_entry(); EVENT_BUFFER_SIZE].into_boxed_slice(),
                event_count: 0,
            })
        }
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_bus_destroy"
    )]
    pub(in crate::poll) fn poll_destroy(instance: PollInstance) {
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
        user_data: usize,
    ) -> io::Result<bool> {
        use windows_sys::Win32::Storage::FileSystem::SetFileCompletionNotificationModes;
        use windows_sys::Win32::System::IO::CreateIoCompletionPort;
        use windows_sys::Win32::System::WindowsProgramming::FILE_SKIP_COMPLETION_PORT_ON_SUCCESS;

        let _ = read_only;
        if unsafe { SetFileCompletionNotificationModes(fd, FILE_SKIP_COMPLETION_PORT_ON_SUCCESS as u8) } == 0 {
            return Err(last_native_error());
        }
        let completion_key = user_data;
        let registered =
            unsafe { CreateIoCompletionPort(fd, instance.raw_fd(), completion_key, 0) };
        if registered.is_null() {
            Err(last_native_error())
        } else {
            Ok(true)
        }
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_bus_wait"
    )]
    pub(in crate::poll) fn poll_wait(instance: &mut PollInstance, timeout: i32) -> io::Result<i32> {
        use windows_sys::Win32::Foundation::WAIT_TIMEOUT;
        use windows_sys::Win32::System::IO::GetQueuedCompletionStatusEx;
        use windows_sys::Win32::System::Threading::INFINITE;

        let mut count = 0;
        let ok = unsafe {
            GetQueuedCompletionStatusEx(
                instance.raw_fd(),
                instance.raw_events.as_mut_ptr(),
                EVENT_BUFFER_SIZE as u32,
                &mut count,
                if timeout < 0 { INFINITE } else { timeout as u32 },
                0,
            )
        };
        if ok == 0 {
            if last_errno() == WAIT_TIMEOUT as i32 {
                instance.event_count = 0;
                return Ok(0);
            }
            return Err(last_native_error());
        }
        instance.event_count = count as usize;
        Ok(count as i32)
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_list_get"
    )]
    pub(in crate::poll) fn event_list_get(instance: &PollInstance, index: usize) -> Option<&PollEvent> {
        instance.raw_events.get(..instance.event_count)?.get(index)
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_get_fd"
    )]
    pub(in crate::poll) fn event_get_fd(event: &PollEvent) -> u64 {
        event.lpCompletionKey as u64
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_get_io_result"
    )]
    pub(in crate::poll) fn event_get_io_result(
        event: &PollEvent,
    ) -> *mut windows_sys::Win32::System::IO::OVERLAPPED {
        event.lpOverlapped
    }

    #[ported(
        source = "src/internal/event_loop/iocp.c",
        original = "moonbitlang_async_event_get_bytes_transferred"
    )]
    pub(in crate::poll) fn event_get_bytes_transferred(event: &PollEvent) -> u32 {
        event.dwNumberOfBytesTransferred
    }
}

pub(in crate::poll) fn poll_register_file(
    instance: &PollInstance,
    handle: RawHandle,
    read_only: bool,
    user_data: usize,
) -> io::Result<bool> {
    poll_register(instance, handle, read_only, user_data)
}

pub(in crate::poll) fn poll_register_socket(
    instance: &PollInstance,
    socket: BorrowedSocket<'_>,
    read_only: bool,
    user_data: usize,
) -> io::Result<bool> {
    // IOCP accepts a socket value in the HANDLE parameter. Keep that Windows
    // ABI conversion inside the IOCP adapter rather than the resource model.
    poll_register(
        instance,
        socket.as_raw_socket() as RawFd,
        read_only,
        user_data,
    )
}

fn empty_overlapped_entry() -> windows_sys::Win32::System::IO::OVERLAPPED_ENTRY {
    unsafe { std::mem::zeroed() }
}
