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

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::event_loop::thread_pool::{HostHandle, ResourceTable};
use crate::async_sys::ported_fns;
use crate::resource::Resource;

#[cfg(unix)]
pub(crate) type RawFd = std::os::fd::RawFd;

#[cfg(windows)]
pub(crate) type RawFd = windows_sys::Win32::Foundation::HANDLE;

ported_fns! {
    #[ported(
        source = "src/internal/fd_util/stub.c",
        original = "moonbitlang_async_pipe"
    )]
    #[cfg(unix)]
    pub(crate) fn pipe(
        read_end_is_async: bool,
        write_end_is_async: bool,
    ) -> AsyncHostResult<[RawFd; 2]> {
        let mut fds = [0, 0];
        if unsafe { libc::pipe(fds.as_mut_ptr()) } < 0 {
            return Err(last_native_error());
        }

        for (fd, is_async) in [(fds[0], read_end_is_async), (fds[1], write_end_is_async)] {
            if let Err(error) = setup_pipe_fd(fd, is_async) {
                for fd in fds {
                    unsafe {
                        libc::close(fd);
                    }
                }
                return Err(error);
            }
        }
        Ok(fds)
    }
}

#[cfg(unix)]
pub(crate) fn set_cloexec(fd: RawFd) -> AsyncHostResult<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    if flags < 0 {
        return Err(last_native_error());
    }
    if (flags & libc::FD_CLOEXEC) == 0 {
        let ret = unsafe { libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC) };
        if ret < 0 {
            return Err(last_native_error());
        }
    }
    Ok(())
}

#[cfg(unix)]
fn setup_pipe_fd(fd: RawFd, is_async: bool) -> AsyncHostResult<()> {
    set_cloexec(fd)?;
    if is_async {
        set_nonblocking(fd)?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_nonblocking(fd: RawFd) -> AsyncHostResult<()> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(last_native_error());
    }
    if (flags & libc::O_NONBLOCK) == 0 {
        let ret = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
        if ret < 0 {
            return Err(last_native_error());
        }
    }
    Ok(())
}

#[cfg(windows)]
fn create_named_pipe_server(name: &std::ffi::OsStr, is_async: bool) -> RawFd {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_OUTBOUND,
    };
    use windows_sys::Win32::System::Pipes::{
        CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

    let mut name: Vec<u16> = name.encode_wide().collect();
    name.push(0);
    let flags = PIPE_ACCESS_OUTBOUND
        | FILE_FLAG_FIRST_PIPE_INSTANCE
        | if is_async { FILE_FLAG_OVERLAPPED } else { 0 };
    unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            flags,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            1024,
            1024,
            0,
            std::ptr::null(),
        )
    }
}

#[cfg(windows)]
fn create_named_pipe_client(name: &std::ffi::OsStr, is_async: bool) -> RawFd {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::GENERIC_READ;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
    };

    let mut name: Vec<u16> = name.encode_wide().collect();
    name.push(0);
    unsafe {
        CreateFileW(
            name.as_ptr(),
            GENERIC_READ,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            if is_async { FILE_FLAG_OVERLAPPED } else { 0 },
            std::ptr::null_mut(),
        )
    }
}

pub(crate) fn pipe_resources(
    resources: &mut impl ResourceTable,
    read_end_is_async: bool,
    write_end_is_async: bool,
) -> AsyncHostResult<[HostHandle; 2]> {
    #[cfg(unix)]
    {
        let fds = pipe(read_end_is_async, write_end_is_async)?;
        let read = Resource::new(fds[0]);
        let write = if write_end_is_async {
            Resource::async_pipe_writer(fds[1])
        } else {
            Resource::new(fds[1])
        };
        let read = resources.insert_resource(read)?;
        let write = resources.insert_resource(write)?;
        Ok([read, write])
    }

    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::sync::atomic::{AtomicU64, Ordering};
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Threading::GetCurrentProcessId;

        static PIPE_ID: AtomicU64 = AtomicU64::new(0);

        let pipe_id = PIPE_ID.fetch_add(1, Ordering::Relaxed) + 1;
        let name = OsString::from(format!(
            r"\\.\pipe\moonbitlang_async.{}.{}",
            unsafe { GetCurrentProcessId() },
            pipe_id
        ));
        let write = create_named_pipe_server(&name, write_end_is_async);
        if write == INVALID_HANDLE_VALUE {
            return Err(last_native_error());
        }
        let read = create_named_pipe_client(&name, read_end_is_async);
        if read == INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(write);
            }
            return Err(last_native_error());
        }

        let read = Resource::new(read);
        let write = if write_end_is_async {
            Resource::async_pipe_writer(write)
        } else {
            Resource::new(write)
        };
        let read = resources.insert_resource(read)?;
        let write = resources.insert_resource(write)?;
        Ok([read, write])
    }
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}
