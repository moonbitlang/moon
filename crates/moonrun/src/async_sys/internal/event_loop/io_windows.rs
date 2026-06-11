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
use crate::async_sys::ported_fns;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::IO::OVERLAPPED;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum IoResultKind {
    File = 0,
    Socket,
    SocketWithAddr,
    Connect,
    Accept,
    ReadDirChanges,
}

#[repr(C)]
#[allow(dead_code)]
pub(crate) struct IoResultHeader {
    overlapped: OVERLAPPED,
    kind: IoResultKind,
    job_id: i32,
}

#[allow(dead_code)]
pub(crate) struct FileIoResult {
    header: IoResultHeader,
    buf: Vec<u8>,
    offset: usize,
    len: usize,
}

#[allow(dead_code)]
pub(crate) struct SocketIoResult {
    header: IoResultHeader,
    buf: Vec<u8>,
    offset: usize,
    len: usize,
    flags: u32,
}

#[allow(dead_code)]
pub(crate) struct SocketWithAddrIoResult {
    header: IoResultHeader,
    buf: Vec<u8>,
    offset: usize,
    len: usize,
    flags: u32,
    addr: Vec<u8>,
    addr_len: i32,
}

#[allow(dead_code)]
pub(crate) struct ConnectIoResult {
    header: IoResultHeader,
    addr: Vec<u8>,
}

#[allow(dead_code)]
pub(crate) struct AcceptIoResult {
    header: IoResultHeader,
    bytes_received: u32,
    accept_buffer: [u8; ACCEPT_BUFFER_LEN],
}

#[allow(dead_code)]
pub(crate) struct ReadDirChangesIoResult {
    header: IoResultHeader,
    buf: Vec<u8>,
}

#[allow(dead_code)]
pub(crate) enum IoResult {
    File(FileIoResult),
    Socket(SocketIoResult),
    SocketWithAddr(SocketWithAddrIoResult),
    Connect(ConnectIoResult),
    Accept(AcceptIoResult),
    ReadDirChanges(ReadDirChangesIoResult),
}

const ACCEPT_BUFFER_LEN: usize =
    std::mem::size_of::<windows_sys::Win32::Networking::WinSock::SOCKADDR_STORAGE>() * 2;

impl IoResultHeader {
    fn new(job_id: i32, kind: IoResultKind) -> Self {
        Self {
            overlapped: unsafe { std::mem::zeroed() },
            kind,
            job_id,
        }
    }
}

impl IoResult {
    fn header(&self) -> &IoResultHeader {
        match self {
            Self::File(result) => &result.header,
            Self::Socket(result) => &result.header,
            Self::SocketWithAddr(result) => &result.header,
            Self::Connect(result) => &result.header,
            Self::Accept(result) => &result.header,
            Self::ReadDirChanges(result) => &result.header,
        }
    }

    fn overlapped_mut(&mut self) -> *mut OVERLAPPED {
        match self {
            Self::File(result) => &mut result.header.overlapped,
            Self::Socket(result) => &mut result.header.overlapped,
            Self::SocketWithAddr(result) => &mut result.header.overlapped,
            Self::Connect(result) => &mut result.header.overlapped,
            Self::Accept(result) => &mut result.header.overlapped,
            Self::ReadDirChanges(result) => &mut result.header.overlapped,
        }
    }
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_init_WSA"
    )]
    #[allow(dead_code)]
    pub(crate) fn init_wsa() -> i32 {
        use windows_sys::Win32::Networking::WinSock::{WSADATA, WSAStartup};

        let mut data = unsafe { std::mem::zeroed::<WSADATA>() };
        unsafe { WSAStartup(0x0202, &mut data) }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_cleanup_WSA"
    )]
    #[allow(dead_code)]
    pub(crate) fn cleanup_wsa() -> i32 {
        unsafe { windows_sys::Win32::Networking::WinSock::WSACleanup() }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_make_file_io_result"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_file_io_result(
        job_id: i32,
        buf: Vec<u8>,
        offset: i32,
        len: i32,
        position: i64,
    ) -> AsyncHostResult<IoResult> {
        let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        checked_range(&buf, offset, len)?;
        let mut header = IoResultHeader::new(job_id, IoResultKind::File);
        header.overlapped.Anonymous.Anonymous.Offset = position as u32;
        header.overlapped.Anonymous.Anonymous.OffsetHigh = (position >> 32) as u32;
        Ok(IoResult::File(FileIoResult {
            header,
            buf,
            offset,
            len,
        }))
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_make_socket_io_result"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_socket_io_result(
        job_id: i32,
        buf: Vec<u8>,
        offset: i32,
        len: i32,
        flags: i32,
    ) -> AsyncHostResult<IoResult> {
        let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        checked_range(&buf, offset, len)?;
        Ok(IoResult::Socket(SocketIoResult {
            header: IoResultHeader::new(job_id, IoResultKind::Socket),
            buf,
            offset,
            len,
            flags: flags as u32,
        }))
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_make_socket_with_addr_io_result"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_socket_with_addr_io_result(
        job_id: i32,
        buf: Vec<u8>,
        offset: i32,
        len: i32,
        flags: i32,
        addr: Vec<u8>,
    ) -> AsyncHostResult<IoResult> {
        let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        checked_range(&buf, offset, len)?;
        let addr_len = sockaddr_len(&addr)?;
        Ok(IoResult::SocketWithAddr(SocketWithAddrIoResult {
            header: IoResultHeader::new(job_id, IoResultKind::SocketWithAddr),
            buf,
            offset,
            len,
            flags: flags as u32,
            addr,
            addr_len,
        }))
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_make_connect_io_result"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_connect_io_result(job_id: i32, addr: Vec<u8>) -> AsyncHostResult<IoResult> {
        sockaddr_len(&addr)?;
        Ok(IoResult::Connect(ConnectIoResult {
            header: IoResultHeader::new(job_id, IoResultKind::Connect),
            addr,
        }))
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_make_accept_io_result"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_accept_io_result(job_id: i32) -> IoResult {
        IoResult::Accept(AcceptIoResult {
            header: IoResultHeader::new(job_id, IoResultKind::Accept),
            bytes_received: 0,
            accept_buffer: [0; ACCEPT_BUFFER_LEN],
        })
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_make_read_dir_changes_io_result"
    )]
    #[allow(dead_code)]
    pub(crate) fn make_read_dir_changes_io_result(job_id: i32, buf: Vec<u8>, len: i32) -> AsyncHostResult<IoResult> {
        let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
        buf.get(..len).ok_or(AsyncHostError::Fault)?;
        Ok(IoResult::ReadDirChanges(ReadDirChangesIoResult {
            header: IoResultHeader::new(job_id, IoResultKind::ReadDirChanges),
            buf,
        }))
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_free_io_result"
    )]
    #[allow(dead_code)]
    pub(crate) fn free_io_result(result: IoResult) {
        drop(result);
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_io_result_get_job_id"
    )]
    #[allow(dead_code)]
    pub(crate) fn io_result_get_job_id(result: &IoResult) -> i32 {
        result.header().job_id
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_io_result_get_status"
    )]
    #[allow(dead_code)]
    pub(crate) fn io_result_get_status(result: &mut IoResult, handle: HANDLE) -> AsyncHostResult<i32> {
        use windows_sys::Win32::System::IO::GetOverlappedResult;

        let mut bytes_transferred = 0;
        if unsafe { GetOverlappedResult(handle, result.overlapped_mut(), &mut bytes_transferred, 0) } != 0 {
            i32::try_from(bytes_transferred).map_err(|_| AsyncHostError::Fault)
        } else {
            Err(last_native_error())
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_cancel_io_result"
    )]
    #[allow(dead_code)]
    pub(crate) fn cancel_io_result(result: &mut IoResult, handle: HANDLE) -> i32 {
        use windows_sys::Win32::Foundation::{ERROR_IO_INCOMPLETE, ERROR_NOT_FOUND, GetLastError};
        use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult};

        if unsafe { CancelIoEx(handle, result.overlapped_mut()) } == 0 {
            return if unsafe { GetLastError() } == ERROR_NOT_FOUND { 0 } else { -1 };
        }

        let mut bytes_transferred = 0;
        if unsafe { GetOverlappedResult(handle, result.overlapped_mut(), &mut bytes_transferred, 0) } != 0 {
            return 0;
        }
        if unsafe { GetLastError() } == ERROR_IO_INCOMPLETE { 1 } else { 0 }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_errno_is_read_EOF"
    )]
    #[allow(dead_code)]
    pub(crate) fn errno_is_read_eof(errno: i32) -> bool {
        use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF};

        errno == ERROR_HANDLE_EOF as i32 || errno == ERROR_BROKEN_PIPE as i32
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_read"
    )]
    #[allow(dead_code)]
    pub(crate) fn read(handle: HANDLE, result: &mut IoResult) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Foundation::ERROR_HANDLE_EOF;
        use windows_sys::Win32::Networking::WinSock::{SOCKET, WSARecv, WSARecvFrom, WSABUF};
        use windows_sys::Win32::Storage::FileSystem::ReadFile;

        let mut n_read = 0;
        let success = match result {
            IoResult::File(result) => unsafe {
                ReadFile(
                    handle,
                    result.buf.as_mut_ptr().add(result.offset),
                    result.len as u32,
                    &mut n_read,
                    &mut result.header.overlapped,
                ) != 0
            },
            IoResult::Socket(result) => {
                let buf = WSABUF {
                    len: result.len as u32,
                    buf: unsafe { result.buf.as_mut_ptr().add(result.offset).cast() },
                };
                unsafe {
                    WSARecv(
                        handle as SOCKET,
                        &buf,
                        1,
                        &mut n_read,
                        &mut result.flags,
                        &mut result.header.overlapped,
                        None,
                    ) == 0
                }
            }
            IoResult::SocketWithAddr(result) => {
                let buf = WSABUF {
                    len: result.len as u32,
                    buf: unsafe { result.buf.as_mut_ptr().add(result.offset).cast() },
                };
                unsafe {
                    WSARecvFrom(
                        handle as SOCKET,
                        &buf,
                        1,
                        &mut n_read,
                        &mut result.flags,
                        result.addr.as_mut_ptr().cast(),
                        &mut result.addr_len,
                        &mut result.header.overlapped,
                        None,
                    ) == 0
                }
            }
            _ => return Err(AsyncHostError::Inval),
        };

        if success {
            i32::try_from(n_read).map_err(|_| AsyncHostError::Fault)
        } else if last_errno() == ERROR_HANDLE_EOF as i32 {
            Ok(0)
        } else {
            Err(last_native_error())
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_write"
    )]
    #[allow(dead_code)]
    pub(crate) fn write(handle: HANDLE, result: &mut IoResult) -> AsyncHostResult<i32> {
        use windows_sys::Win32::Networking::WinSock::{SOCKET, WSABUF, WSASend, WSASendTo};
        use windows_sys::Win32::Storage::FileSystem::WriteFile;

        let mut n_written = 0;
        let success = match result {
            IoResult::File(result) => unsafe {
                WriteFile(
                    handle,
                    result.buf.as_ptr().add(result.offset),
                    result.len as u32,
                    &mut n_written,
                    &mut result.header.overlapped,
                ) != 0
            },
            IoResult::Socket(result) => {
                let buf = WSABUF {
                    len: result.len as u32,
                    buf: unsafe { result.buf.as_mut_ptr().add(result.offset).cast() },
                };
                unsafe {
                    WSASend(
                        handle as SOCKET,
                        &buf,
                        1,
                        &mut n_written,
                        result.flags,
                        &mut result.header.overlapped,
                        None,
                    ) == 0
                }
            }
            IoResult::SocketWithAddr(result) => {
                let buf = WSABUF {
                    len: result.len as u32,
                    buf: unsafe { result.buf.as_mut_ptr().add(result.offset).cast() },
                };
                unsafe {
                    WSASendTo(
                        handle as SOCKET,
                        &buf,
                        1,
                        &mut n_written,
                        result.flags,
                        result.addr.as_ptr().cast(),
                        result.addr_len,
                        &mut result.header.overlapped,
                        None,
                    ) == 0
                }
            }
            _ => return Err(AsyncHostError::Inval),
        };

        if success {
            i32::try_from(n_written).map_err(|_| AsyncHostError::Fault)
        } else {
            Err(last_native_error())
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_connect"
    )]
    #[allow(dead_code)]
    pub(crate) fn connect(handle: HANDLE, result: &mut IoResult) -> AsyncHostResult<()> {
        use windows_sys::Win32::Networking::WinSock::{
            AF_INET, AF_INET6, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6, SOCKET, bind,
        };

        let IoResult::Connect(result) = result else {
            return Err(AsyncHostError::Inval);
        };
        let Some(connect_ex) = get_connect_ex(handle) else {
            return Err(AsyncHostError::Native(last_errno()));
        };
        let family = sockaddr_family(&result.addr)?;
        let bind_result = if family == AF_INET {
            let addr = SOCKADDR_IN {
                sin_family: AF_INET,
                sin_port: 0,
                sin_addr: unsafe { std::mem::zeroed() },
                sin_zero: [0; 8],
            };
            unsafe {
                bind(
                    handle as SOCKET,
                    (&addr as *const SOCKADDR_IN).cast::<SOCKADDR>(),
                    std::mem::size_of::<SOCKADDR_IN>() as i32,
                )
            }
        } else if family == AF_INET6 {
            let addr = SOCKADDR_IN6 {
                sin6_family: AF_INET6,
                sin6_port: 0,
                sin6_flowinfo: 0,
                sin6_addr: unsafe { std::mem::zeroed() },
                Anonymous: unsafe { std::mem::zeroed() },
            };
            unsafe {
                bind(
                    handle as SOCKET,
                    (&addr as *const SOCKADDR_IN6).cast::<SOCKADDR>(),
                    std::mem::size_of::<SOCKADDR_IN6>() as i32,
                )
            }
        } else {
            return Err(AsyncHostError::Inval);
        };
        if bind_result != 0 {
            return Err(last_native_error());
        }

        let ok = unsafe {
            connect_ex(
                handle as SOCKET,
                result.addr.as_ptr().cast(),
                sockaddr_len(&result.addr)?,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                &mut result.header.overlapped,
            )
        };
        if ok != 0 { Ok(()) } else { Err(last_native_error()) }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_setup_connected_socket"
    )]
    #[allow(dead_code)]
    pub(crate) fn setup_connected_socket(handle: HANDLE) -> AsyncHostResult<()> {
        use windows_sys::Win32::Networking::WinSock::{
            SO_UPDATE_CONNECT_CONTEXT, SOCKET, SOL_SOCKET, setsockopt,
        };

        let yes = 1u32;
        let ret = unsafe {
            setsockopt(
                handle as SOCKET,
                SOL_SOCKET,
                SO_UPDATE_CONNECT_CONTEXT,
                (&yes as *const u32).cast(),
                std::mem::size_of::<u32>() as i32,
            )
        };
        if ret == 0 { Ok(()) } else { Err(last_native_error()) }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_accept"
    )]
    #[allow(dead_code)]
    pub(crate) fn accept(handle: HANDLE, conn_sock: HANDLE, result: &mut IoResult) -> AsyncHostResult<()> {
        use windows_sys::Win32::Networking::WinSock::SOCKET;

        let IoResult::Accept(result) = result else {
            return Err(AsyncHostError::Inval);
        };
        let Some(accept_ex) = get_accept_ex(handle) else {
            return Err(AsyncHostError::Native(last_errno()));
        };
        let ok = unsafe {
            accept_ex(
                handle as SOCKET,
                conn_sock as SOCKET,
                result.accept_buffer.as_mut_ptr().cast(),
                0,
                std::mem::size_of::<windows_sys::Win32::Networking::WinSock::SOCKADDR_STORAGE>() as u32,
                std::mem::size_of::<windows_sys::Win32::Networking::WinSock::SOCKADDR_STORAGE>() as u32,
                &mut result.bytes_received,
                &mut result.header.overlapped,
            )
        };
        if ok != 0 { Ok(()) } else { Err(last_native_error()) }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_setup_accepted_socket"
    )]
    #[allow(dead_code)]
    pub(crate) fn setup_accepted_socket(listen_sock: HANDLE, accept_sock: HANDLE) -> AsyncHostResult<()> {
        use windows_sys::Win32::Networking::WinSock::{
            SO_UPDATE_ACCEPT_CONTEXT, SOCKET, SOL_SOCKET, setsockopt,
        };

        let ret = unsafe {
            setsockopt(
                accept_sock as SOCKET,
                SOL_SOCKET,
                SO_UPDATE_ACCEPT_CONTEXT,
                (&listen_sock as *const HANDLE).cast(),
                std::mem::size_of::<usize>() as i32,
            )
        };
        if ret == 0 { Ok(()) } else { Err(last_native_error()) }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_get_std_handle"
    )]
    #[allow(dead_code)]
    pub(crate) fn get_std_handle(id: i32) -> HANDLE {
        unsafe { windows_sys::Win32::System::Console::GetStdHandle(id as u32) }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_read_dir_changes"
    )]
    #[allow(dead_code)]
    pub(crate) fn read_dir_changes(dir: HANDLE, result: &mut IoResult) -> AsyncHostResult<()> {
        use windows_sys::Win32::Foundation::{ERROR_IO_PENDING, SetLastError};
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_NOTIFY_CHANGE_CREATION, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
            FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, ReadDirectoryChangesW,
        };

        let IoResult::ReadDirChanges(result) = result else {
            return Err(AsyncHostError::Inval);
        };
        let mut bytes_returned = 0;
        let ret = unsafe {
            ReadDirectoryChangesW(
                dir,
                result.buf.as_mut_ptr().cast(),
                result.buf.len() as u32,
                1,
                FILE_NOTIFY_CHANGE_SIZE
                    | FILE_NOTIFY_CHANGE_LAST_WRITE
                    | FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_DIR_NAME
                    | FILE_NOTIFY_CHANGE_CREATION,
                &mut bytes_returned,
                &mut result.header.overlapped,
                None,
            )
        };
        if ret == 0 {
            return Err(last_native_error());
        }
        unsafe { SetLastError(ERROR_IO_PENDING) };
        Err(AsyncHostError::Native(ERROR_IO_PENDING as i32))
    }
}

fn checked_range(buf: &[u8], offset: usize, len: usize) -> AsyncHostResult<&[u8]> {
    let end = offset.checked_add(len).ok_or(AsyncHostError::Fault)?;
    buf.get(offset..end).ok_or(AsyncHostError::Fault)
}

fn sockaddr_family(addr: &[u8]) -> AsyncHostResult<u16> {
    let bytes = addr.get(..2).ok_or(AsyncHostError::Fault)?;
    Ok(u16::from_ne_bytes([bytes[0], bytes[1]]))
}

fn sockaddr_len(addr: &[u8]) -> AsyncHostResult<i32> {
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6, SOCKADDR_IN, SOCKADDR_IN6};

    let family = sockaddr_family(addr)?;
    let len = if family == AF_INET {
        std::mem::size_of::<SOCKADDR_IN>()
    } else if family == AF_INET6 {
        std::mem::size_of::<SOCKADDR_IN6>()
    } else {
        return Err(AsyncHostError::Inval);
    };
    addr.get(..len).ok_or(AsyncHostError::Fault)?;
    Ok(len as i32)
}

fn get_connect_ex(handle: HANDLE) -> windows_sys::Win32::Networking::WinSock::LPFN_CONNECTEX {
    use windows_sys::Win32::Networking::WinSock::{LPFN_CONNECTEX, WSAID_CONNECTEX};

    let mut result: LPFN_CONNECTEX = None;
    if get_wsa_extension(
        handle,
        &WSAID_CONNECTEX,
        (&mut result as *mut LPFN_CONNECTEX).cast(),
    ) {
        result
    } else {
        None
    }
}

fn get_accept_ex(handle: HANDLE) -> windows_sys::Win32::Networking::WinSock::LPFN_ACCEPTEX {
    use windows_sys::Win32::Networking::WinSock::{LPFN_ACCEPTEX, WSAID_ACCEPTEX};

    let mut result: LPFN_ACCEPTEX = None;
    if get_wsa_extension(
        handle,
        &WSAID_ACCEPTEX,
        (&mut result as *mut LPFN_ACCEPTEX).cast(),
    ) {
        result
    } else {
        None
    }
}

fn get_wsa_extension(
    handle: HANDLE,
    guid: &windows_sys::core::GUID,
    out: *mut std::ffi::c_void,
) -> bool {
    use windows_sys::Win32::Networking::WinSock::{
        SIO_GET_EXTENSION_FUNCTION_POINTER, SOCKET, WSAIoctl,
    };

    let mut pointer_size = 0;
    unsafe {
        WSAIoctl(
            handle as SOCKET,
            SIO_GET_EXTENSION_FUNCTION_POINTER,
            (guid as *const windows_sys::core::GUID).cast(),
            std::mem::size_of::<windows_sys::core::GUID>() as u32,
            out,
            std::mem::size_of::<usize>() as u32,
            &mut pointer_size,
            std::ptr::null_mut(),
            None,
        ) == 0
    }
}

fn last_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or_else(|| AsyncHostError::Inval.errno())
}

fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(last_errno())
}
