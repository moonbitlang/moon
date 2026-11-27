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

//! Overlapped socket submission. Callers own the storage and completion lifecycle.

use std::io;
use windows_sys::Win32::Networking::WinSock as ws;
use windows_sys::Win32::System::IO::OVERLAPPED;

use crate::upstream::ported_fns;

use super::address::{sockaddr_family, sockaddr_len};
use super::{INVALID_ADDRESS, INVALID_INPUT, RawSocket, last_wsa_error};

/// Padded local and remote address storage for AcceptEx.
pub(crate) struct AcceptBuffer {
    buffer: Box<[u8]>,
    addr_len: usize,
    get_addrs: ws::LPFN_GETACCEPTEXSOCKADDRS,
}

impl AcceptBuffer {
    pub(crate) fn new(addr_len: u32) -> io::Result<Self> {
        let addr_len = i32::try_from(addr_len)
            .map_err(|_| io::Error::from_raw_os_error(INVALID_ADDRESS))?
            as usize;
        // AcceptEx reserves sixteen extra bytes for each of the two addresses.
        let buffer_len = addr_len
            .checked_add(16)
            .and_then(|len| len.checked_mul(2))
            .ok_or_else(|| io::Error::from_raw_os_error(INVALID_ADDRESS))?;
        Ok(Self {
            buffer: vec![0; buffer_len].into_boxed_slice(),
            addr_len,
            get_addrs: None,
        })
    }

    /// # Safety
    /// Native access must have ended and the buffer must satisfy its stored
    /// parser's input contract. For AcceptEx, this requires successful completion.
    pub(crate) unsafe fn parse_peer_addr(&self) -> io::Result<&[u8]> {
        let get_addrs = self
            .get_addrs
            .ok_or_else(|| io::Error::from_raw_os_error(INVALID_INPUT))?;
        let region_len = (self.addr_len + 16) as u32;
        let mut local = std::ptr::null_mut();
        let mut remote = std::ptr::null_mut();
        let mut local_len = 0;
        let mut remote_len = 0;
        // The extra space belongs to the provider's internal format. Use its
        // parser with the same buffer and region lengths as the submission.
        unsafe {
            get_addrs(
                self.buffer.as_ptr().cast(),
                0,
                region_len,
                region_len,
                &mut local,
                &mut local_len,
                &mut remote,
                &mut remote_len,
            );
        }
        // Borrow from our allocation, rather than constructing a slice from
        // an unchecked provider pointer or interpreting its padding as data.
        let offset = (remote as usize)
            .checked_sub(self.buffer.as_ptr() as usize)
            .ok_or_else(|| io::Error::from_raw_os_error(INVALID_ADDRESS))?;
        let len = usize::try_from(remote_len)
            .map_err(|_| io::Error::from_raw_os_error(INVALID_ADDRESS))?;
        self.buffer
            .get(offset..)
            .and_then(|remaining| remaining.get(..len))
            .ok_or_else(|| io::Error::from_raw_os_error(INVALID_ADDRESS))
    }
}

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_read"
    )]
    /// Flags are read and updated only during this call, not on deferred completion.
    ///
    /// # Safety
    /// The socket, buffer and OVERLAPPED must remain valid until completion.
    /// Writable output must not be accessed while the operation is pending.
    pub(in crate::network) unsafe fn recv(
        socket: RawSocket,
        buffer: &mut [u8],
        flags: &mut u32,
        overlapped: *mut OVERLAPPED,
    ) -> io::Result<usize> {
        let buffer = socket_buffer(buffer.as_mut_ptr(), buffer.len())?;
        let mut bytes = 0;
        if unsafe {
            ws::WSARecv(
                socket as usize,
                &buffer,
                1,
                &mut bytes,
                flags,
                overlapped,
                None,
            )
        } == 0
        {
            Ok(bytes as usize)
        } else {
            Err(last_wsa_error())
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_read"
    )]
    /// Flags are read and updated only during this call, not on deferred completion.
    ///
    /// # Safety
    /// The socket, buffers, address length and OVERLAPPED must remain valid
    /// until completion. Writable output must not be accessed while pending.
    pub(in crate::network) unsafe fn recv_from(
        socket: RawSocket,
        buffer: &mut [u8],
        flags: &mut u32,
        addr: &mut [u8],
        addr_len: *mut i32,
        overlapped: *mut OVERLAPPED,
    ) -> io::Result<usize> {
        let buffer = socket_buffer(buffer.as_mut_ptr(), buffer.len())?;
        // Winsock can write the length after this function returns; use the owner's
        // stable output scalar rather than a local variable.
        unsafe {
            *addr_len =
                i32::try_from(addr.len()).map_err(|_| io::Error::from_raw_os_error(INVALID_ADDRESS))?
        };
        let mut bytes = 0;
        if unsafe {
            ws::WSARecvFrom(
                socket as usize,
                &buffer,
                1,
                &mut bytes,
                flags,
                addr.as_mut_ptr().cast(),
                addr_len,
                overlapped,
                None,
            )
        } == 0
        {
            Ok(bytes as usize)
        } else {
            Err(last_wsa_error())
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_write"
    )]
    /// # Safety
    /// The socket, buffer and OVERLAPPED must remain valid until completion.
    /// The buffer must not be modified while the operation is pending.
    pub(in crate::network) unsafe fn send(
        socket: RawSocket,
        buffer: &[u8],
        flags: u32,
        overlapped: *mut OVERLAPPED,
    ) -> io::Result<usize> {
        let buffer = socket_buffer(buffer.as_ptr().cast_mut(), buffer.len())?;
        let mut bytes = 0;
        if unsafe {
            ws::WSASend(
                socket as usize,
                &buffer,
                1,
                &mut bytes,
                flags,
                overlapped,
                None,
            )
        } == 0
        {
            Ok(bytes as usize)
        } else {
            Err(last_wsa_error())
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_write"
    )]
    /// # Safety
    /// The socket, buffers and OVERLAPPED must remain valid until completion.
    /// Input buffers must not be modified while the operation is pending.
    pub(in crate::network) unsafe fn send_to(
        socket: RawSocket,
        buffer: &[u8],
        flags: u32,
        addr: &[u8],
        overlapped: *mut OVERLAPPED,
    ) -> io::Result<usize> {
        let buffer = socket_buffer(buffer.as_ptr().cast_mut(), buffer.len())?;
        let addr_len =
            i32::try_from(addr.len()).map_err(|_| io::Error::from_raw_os_error(INVALID_ADDRESS))?;
        let mut bytes = 0;
        if unsafe {
            ws::WSASendTo(
                socket as usize,
                &buffer,
                1,
                &mut bytes,
                flags,
                addr.as_ptr().cast(),
                addr_len,
                overlapped,
                None,
            )
        } == 0
        {
            Ok(bytes as usize)
        } else {
            Err(last_wsa_error())
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = ["moonbitlang_async_connect", "get_WSA_extension"]
    )]
    /// # Safety
    /// The socket, address and OVERLAPPED must remain valid until completion.
    pub(in crate::network) unsafe fn connect(
        socket: RawSocket,
        addr: &[u8],
        overlapped: *mut OVERLAPPED,
    ) -> io::Result<()> {
        bind_any_for_connect(socket, addr)?;
        let connect_ex = get_wsa_extension::<ws::LPFN_CONNECTEX>(socket, &ws::WSAID_CONNECTEX)?
            .ok_or_else(|| io::Error::from_raw_os_error(INVALID_INPUT))?;
        let addr_len = sockaddr_len(addr)?;
        if unsafe {
            connect_ex(
                socket as usize,
                addr.as_ptr().cast(),
                addr_len,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                overlapped,
            )
        } != 0
        {
            Ok(())
        } else {
            Err(last_wsa_error())
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = ["moonbitlang_async_accept", "get_WSA_extension"]
    )]
    /// # Safety
    /// Retain both sockets until completion. Keep the address storage and OVERLAPPED
    /// at stable addresses until completion; do not access the storage while pending.
    pub(in crate::network) unsafe fn accept(
        server: RawSocket,
        connection: RawSocket,
        buffer: &mut AcceptBuffer,
        overlapped: *mut OVERLAPPED,
    ) -> io::Result<()> {
        let accept_ex = get_wsa_extension::<ws::LPFN_ACCEPTEX>(server, &ws::WSAID_ACCEPTEX)?
            .ok_or_else(|| io::Error::from_raw_os_error(INVALID_INPUT))?;
        // Resolve the producer and parser through the same socket's provider.
        buffer.get_addrs = Some(
            get_wsa_extension::<ws::LPFN_GETACCEPTEXSOCKADDRS>(
                server,
                &ws::WSAID_GETACCEPTEXSOCKADDRS,
            )?
            .ok_or_else(|| io::Error::from_raw_os_error(INVALID_INPUT))?,
        );
        let addr_len = (buffer.addr_len + 16) as u32;
        // AcceptEx only writes this on synchronous completion. Pending requests
        // report their byte count through the completion notification instead.
        let mut bytes_received = 0;
        if unsafe {
            accept_ex(
                server as usize,
                connection as usize,
                buffer.buffer.as_mut_ptr().cast(),
                0,
                addr_len,
                addr_len,
                &mut bytes_received,
                overlapped,
            )
        } != 0
        {
            Ok(())
        } else {
            Err(last_wsa_error())
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_setup_connected_socket"
    )]
    pub(in crate::network) fn setup_connected_socket(socket: RawSocket) -> io::Result<()> {
        if unsafe {
            ws::setsockopt(
                socket as usize,
                ws::SOL_SOCKET,
                ws::SO_UPDATE_CONNECT_CONTEXT,
                std::ptr::null(),
                0,
            )
        } == ws::SOCKET_ERROR
        {
            Err(last_wsa_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_windows.c",
        original = "moonbitlang_async_setup_accepted_socket"
    )]
    pub(in crate::network) fn setup_accepted_socket(
        listen: RawSocket,
        accept: RawSocket,
    ) -> io::Result<()> {
        let listen = listen as usize;
        if unsafe {
            ws::setsockopt(
                accept as usize,
                ws::SOL_SOCKET,
                ws::SO_UPDATE_ACCEPT_CONTEXT,
                (&listen as *const usize).cast(),
                std::mem::size_of_val(&listen) as i32,
            )
        } == ws::SOCKET_ERROR
        {
            Err(last_wsa_error())
        } else {
            Ok(())
        }
    }
}

fn socket_buffer(buffer: *mut u8, len: usize) -> io::Result<ws::WSABUF> {
    // Winsock captures this descriptor during submission. The caller retains
    // its payload until completion, not the temporary WSABUF itself.
    Ok(ws::WSABUF {
        len: u32::try_from(len).map_err(|_| io::Error::from_raw_os_error(INVALID_ADDRESS))?,
        buf: buffer,
    })
}

fn bind_any_for_connect(socket: RawSocket, remote_addr: &[u8]) -> io::Result<()> {
    let result = match sockaddr_family(remote_addr)? as u16 {
        ws::AF_INET => {
            let mut addr = unsafe { std::mem::zeroed::<ws::SOCKADDR_IN>() };
            addr.sin_family = ws::AF_INET;
            unsafe {
                ws::bind(
                    socket as usize,
                    (&addr as *const ws::SOCKADDR_IN).cast(),
                    std::mem::size_of_val(&addr) as i32,
                )
            }
        }
        ws::AF_INET6 => {
            let mut addr = unsafe { std::mem::zeroed::<ws::SOCKADDR_IN6>() };
            addr.sin6_family = ws::AF_INET6;
            unsafe {
                ws::bind(
                    socket as usize,
                    (&addr as *const ws::SOCKADDR_IN6).cast(),
                    std::mem::size_of_val(&addr) as i32,
                )
            }
        }
        _ => return Err(io::Error::from_raw_os_error(INVALID_INPUT)),
    };
    // Untrusted guests can bind the socket before requesting a connection.
    // Preserve the native error instead of assuming the normal async sequence.
    if result == ws::SOCKET_ERROR {
        Err(last_wsa_error())
    } else {
        Ok(())
    }
}

fn get_wsa_extension<T: Copy>(socket: RawSocket, guid: &windows_sys::core::GUID) -> io::Result<T> {
    debug_assert_eq!(
        std::mem::size_of::<T>(),
        std::mem::size_of::<*mut std::ffi::c_void>()
    );
    let mut extension = std::ptr::null_mut::<std::ffi::c_void>();
    let mut bytes_returned = 0;
    if unsafe {
        ws::WSAIoctl(
            socket as usize,
            ws::SIO_GET_EXTENSION_FUNCTION_POINTER,
            (guid as *const windows_sys::core::GUID).cast(),
            std::mem::size_of_val(guid) as u32,
            (&mut extension as *mut *mut std::ffi::c_void).cast(),
            std::mem::size_of_val(&extension) as u32,
            &mut bytes_returned,
            std::ptr::null_mut(),
            None,
        )
    } == ws::SOCKET_ERROR
    {
        Err(last_wsa_error())
    } else {
        Ok(unsafe { std::mem::transmute_copy(&extension) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "system" fn length_prefixed_addrs(
        output: *const std::ffi::c_void,
        received: u32,
        local_capacity: u32,
        _remote_capacity: u32,
        local: *mut *mut ws::SOCKADDR,
        local_len: *mut i32,
        remote: *mut *mut ws::SOCKADDR,
        remote_len: *mut i32,
    ) {
        unsafe {
            let local_start = output.cast::<u8>().add(received as usize);
            let remote_start = local_start.add(local_capacity as usize);
            *local_len = local_start.cast::<i32>().read_unaligned();
            *local = local_start.add(4).cast_mut().cast();
            *remote_len = remote_start.cast::<i32>().read_unaligned();
            *remote = remote_start.add(4).cast_mut().cast();
        }
    }

    #[test]
    fn accept_peer_address_uses_the_provider_layout() {
        for (family, addr_len) in [(2u16, 16usize), (23, 28)] {
            let mut expected = vec![0; addr_len];
            expected[..2].copy_from_slice(&family.to_le_bytes());
            expected[2..4].copy_from_slice(&13398u16.to_be_bytes());
            if family == 2 {
                expected[4..8].copy_from_slice(&[192, 0, 2, 123]);
            } else {
                expected[8..24]
                    .copy_from_slice(&[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
                expected[24..28].copy_from_slice(&7u32.to_le_bytes());
            }
            let mut buffer = AcceptBuffer::new(addr_len as u32).unwrap();
            buffer.get_addrs = Some(length_prefixed_addrs);
            // Replay Wine's fill_accept_output: each region starts with a
            // four-byte length, followed by the actual sockaddr.
            for start in [0, addr_len + 16] {
                buffer.buffer[start..start + 4].copy_from_slice(&(addr_len as i32).to_le_bytes());
                buffer.buffer[start + 4..start + 4 + addr_len].copy_from_slice(&expected);
            }
            // The fixture represents completed provider output.
            assert_eq!(unsafe { buffer.parse_peer_addr() }.unwrap(), expected);
        }
    }

    #[test]
    fn accept_peer_address_rejects_invalid_provider_lengths() {
        let mut buffer = AcceptBuffer::new(16).unwrap();
        buffer.get_addrs = Some(length_prefixed_addrs);
        let remote = 32;
        for len in [-1i32, i32::MAX] {
            buffer.buffer[remote..remote + 4].copy_from_slice(&len.to_le_bytes());
            // An inconsistent provider output must not escape as a slice.
            assert!(unsafe { buffer.parse_peer_addr() }.is_err(), "length {len}");
        }
    }
}
