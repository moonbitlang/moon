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

#[cfg(unix)]
use std::ffi::CStr;
#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(windows)]
use windows_sys::Win32::Networking::WinSock as ws;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::internal::fd_util::stub::RawFd;
use crate::async_sys::ported_fns;

#[cfg(unix)]
pub(crate) type RawSocket = RawFd;
#[cfg(windows)]
pub(crate) type RawSocket = std::os::windows::io::RawSocket;

#[cfg(unix)]
fn last_native_error() -> AsyncHostError {
    AsyncHostError::Native(
        std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or_else(|| AsyncHostError::Inval.errno()),
    )
}

#[cfg(unix)]
fn sockaddr_len(addr: &[u8]) -> AsyncHostResult<libc::socklen_t> {
    let len = match sockaddr_family(addr)? {
        libc::AF_INET => std::mem::size_of::<libc::sockaddr_in>(),
        libc::AF_INET6 => std::mem::size_of::<libc::sockaddr_in6>(),
        _ => return Err(AsyncHostError::Inval),
    };
    if addr.len() < len {
        return Err(AsyncHostError::Fault);
    }
    libc::socklen_t::try_from(len).map_err(|_| AsyncHostError::Fault)
}

#[cfg(unix)]
fn sockaddr_family(addr: &[u8]) -> AsyncHostResult<i32> {
    if addr.len() < std::mem::size_of::<libc::sockaddr>() {
        return Err(AsyncHostError::Fault);
    }
    let family = unsafe { addr.as_ptr().cast::<libc::sockaddr>().read_unaligned() }.sa_family;
    Ok(i32::from(family))
}

#[cfg(unix)]
fn read_sockaddr_in(addr: &[u8]) -> AsyncHostResult<libc::sockaddr_in> {
    if addr.len() < std::mem::size_of::<libc::sockaddr_in>() {
        return Err(AsyncHostError::Fault);
    }
    Ok(unsafe { addr.as_ptr().cast::<libc::sockaddr_in>().read_unaligned() })
}

#[cfg(unix)]
fn read_sockaddr_in6(addr: &[u8]) -> AsyncHostResult<libc::sockaddr_in6> {
    if addr.len() < std::mem::size_of::<libc::sockaddr_in6>() {
        return Err(AsyncHostError::Fault);
    }
    Ok(unsafe { addr.as_ptr().cast::<libc::sockaddr_in6>().read_unaligned() })
}

#[cfg(unix)]
pub(crate) fn copy_sockaddrs_from_getaddrinfo(
    hostname: std::ffi::OsString,
) -> AsyncHostResult<(i32, Vec<Box<[u8]>>)> {
    let hostname = CString::new(hostname.into_vec()).map_err(|_| AsyncHostError::Inval)?;
    let mut hints = unsafe { std::mem::zeroed::<libc::addrinfo>() };
    hints.ai_flags = libc::AI_ADDRCONFIG;
    hints.ai_family = libc::AF_UNSPEC;

    let mut result = std::ptr::null_mut();
    let ret =
        unsafe { libc::getaddrinfo(hostname.as_ptr(), std::ptr::null(), &hints, &mut result) };
    if ret != 0 {
        if ret == libc::EAI_SYSTEM {
            return Err(last_native_error());
        }
        return Ok((ret, Vec::new()));
    }

    let mut addrs = Vec::new();
    let mut current = result;
    while !current.is_null() {
        let addrinfo = unsafe { &*current };
        let len = match addrinfo.ai_family {
            libc::AF_INET => std::mem::size_of::<libc::sockaddr_in>(),
            libc::AF_INET6 => std::mem::size_of::<libc::sockaddr_in6>(),
            _ => {
                current = addrinfo.ai_next;
                continue;
            }
        };
        if !addrinfo.ai_addr.is_null() && addrinfo.ai_addrlen as usize >= len {
            let addr = unsafe { std::slice::from_raw_parts(addrinfo.ai_addr.cast::<u8>(), len) };
            addrs.push(addr.into());
        }
        current = addrinfo.ai_next;
    }
    unsafe {
        libc::freeaddrinfo(result);
    }
    Ok((0, addrs))
}

#[cfg(unix)]
fn write_struct<T>(dst: &mut [u8], value: &T) -> AsyncHostResult<()> {
    let len = std::mem::size_of::<T>();
    if dst.len() < len {
        return Err(AsyncHostError::Fault);
    }
    let src = unsafe { std::slice::from_raw_parts((value as *const T).cast::<u8>(), len) };
    dst[..len].copy_from_slice(src);
    Ok(())
}

#[cfg(windows)]
mod win {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::{
        ERROR_BUFFER_OVERFLOW, NO_ERROR, SetLastError, WIN32_ERROR,
    };
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        ConvertInterfaceAliasToLuid, ConvertInterfaceIndexToLuid, ConvertInterfaceLuidToAlias,
        ConvertInterfaceLuidToIndex, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
        GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses, IP_ADAPTER_NO_MULTICAST,
    };
    use windows_sys::Win32::NetworkManagement::Ndis::{IfOperStatusUp, NET_LUID_LH};
    use windows_sys::Win32::Networking::WinSock as ws;

    use super::{AsyncHostError, AsyncHostResult, RawFd, RawSocket};

    fn socket(fd: RawFd) -> ws::SOCKET {
        fd as usize
    }

    fn raw_socket(socket: ws::SOCKET) -> RawSocket {
        socket as RawSocket
    }

    fn last_wsa_error() -> AsyncHostError {
        let errno = unsafe { ws::WSAGetLastError() };
        unsafe {
            SetLastError(errno as WIN32_ERROR);
        }
        AsyncHostError::Native(errno)
    }

    fn win32_error(errno: WIN32_ERROR) -> AsyncHostError {
        unsafe {
            SetLastError(errno);
        }
        AsyncHostError::Native(errno as i32)
    }

    fn socket_error(ret: i32) -> AsyncHostResult<()> {
        if ret == ws::SOCKET_ERROR {
            Err(last_wsa_error())
        } else {
            Ok(())
        }
    }

    fn domain(family: i32) -> AsyncHostResult<i32> {
        match family {
            4 => Ok(ws::AF_INET as i32),
            6 => Ok(ws::AF_INET6 as i32),
            _ => Err(AsyncHostError::Inval),
        }
    }

    fn sockaddr_family(addr: &[u8]) -> AsyncHostResult<i32> {
        if addr.len() < std::mem::size_of::<ws::SOCKADDR>() {
            return Err(AsyncHostError::Fault);
        }
        let family = unsafe { addr.as_ptr().cast::<ws::SOCKADDR>().read_unaligned() }.sa_family;
        Ok(i32::from(family))
    }

    fn sockaddr_len(addr: &[u8]) -> AsyncHostResult<i32> {
        let len = match sockaddr_family(addr)? as u16 {
            ws::AF_INET => std::mem::size_of::<ws::SOCKADDR_IN>(),
            ws::AF_INET6 => std::mem::size_of::<ws::SOCKADDR_IN6>(),
            _ => return Err(AsyncHostError::Inval),
        };
        if addr.len() < len {
            return Err(AsyncHostError::Fault);
        }
        i32::try_from(len).map_err(|_| AsyncHostError::Fault)
    }

    fn read_sockaddr_in(addr: &[u8]) -> AsyncHostResult<ws::SOCKADDR_IN> {
        if addr.len() < std::mem::size_of::<ws::SOCKADDR_IN>() {
            return Err(AsyncHostError::Fault);
        }
        Ok(unsafe { addr.as_ptr().cast::<ws::SOCKADDR_IN>().read_unaligned() })
    }

    fn read_sockaddr_in6(addr: &[u8]) -> AsyncHostResult<ws::SOCKADDR_IN6> {
        if addr.len() < std::mem::size_of::<ws::SOCKADDR_IN6>() {
            return Err(AsyncHostError::Fault);
        }
        Ok(unsafe { addr.as_ptr().cast::<ws::SOCKADDR_IN6>().read_unaligned() })
    }

    fn write_struct<T>(dst: &mut [u8], value: &T) -> AsyncHostResult<()> {
        let len = std::mem::size_of::<T>();
        if dst.len() < len {
            return Err(AsyncHostError::Fault);
        }
        let src = unsafe { std::slice::from_raw_parts((value as *const T).cast::<u8>(), len) };
        dst[..len].copy_from_slice(src);
        Ok(())
    }

    fn set_socket_int(fd: RawFd, level: i32, option: i32, value: i32) -> AsyncHostResult<()> {
        socket_error(unsafe {
            ws::setsockopt(
                socket(fd),
                level,
                option,
                (&value as *const i32).cast(),
                std::mem::size_of_val(&value) as i32,
            )
        })
    }

    pub(super) fn copy_sockaddrs_from_getaddrinfo(
        hostname: OsString,
    ) -> AsyncHostResult<(i32, Vec<Box<[u8]>>)> {
        let hostname: Vec<u16> = hostname.encode_wide().chain(std::iter::once(0)).collect();
        let mut hints = unsafe { std::mem::zeroed::<ws::ADDRINFOW>() };
        hints.ai_flags = ws::AI_ADDRCONFIG as i32;
        hints.ai_family = ws::AF_UNSPEC as i32;

        let mut result = std::ptr::null_mut();
        let ret =
            unsafe { ws::GetAddrInfoW(hostname.as_ptr(), std::ptr::null(), &hints, &mut result) };
        if ret != 0 {
            return match ret {
                ws::WSATRY_AGAIN
                | ws::WSANO_RECOVERY
                | ws::WSAEAFNOSUPPORT
                | ws::WSAHOST_NOT_FOUND
                | ws::WSATYPE_NOT_FOUND
                | ws::WSAESOCKTNOSUPPORT => Ok((ret, Vec::new())),
                _ => Err(AsyncHostError::Native(ret)),
            };
        }

        let mut addrs = Vec::new();
        let mut current = result;
        while !current.is_null() {
            let addrinfo = unsafe { &*current };
            let len = match addrinfo.ai_family as u16 {
                ws::AF_INET => std::mem::size_of::<ws::SOCKADDR_IN>(),
                ws::AF_INET6 => std::mem::size_of::<ws::SOCKADDR_IN6>(),
                _ => {
                    current = addrinfo.ai_next;
                    continue;
                }
            };
            if !addrinfo.ai_addr.is_null() && addrinfo.ai_addrlen >= len {
                let addr =
                    unsafe { std::slice::from_raw_parts(addrinfo.ai_addr.cast::<u8>(), len) };
                addrs.push(addr.into());
            }
            current = addrinfo.ai_next;
        }
        unsafe {
            ws::FreeAddrInfoW(result);
        }
        Ok((0, addrs))
    }

    pub(super) fn ipv4_addr_size() -> i32 {
        std::mem::size_of::<ws::SOCKADDR_IN>() as i32
    }

    pub(super) fn ipv6_addr_size() -> i32 {
        std::mem::size_of::<ws::SOCKADDR_IN6>() as i32
    }

    pub(super) fn init_ip_addr(addr: &mut [u8], ip: i32, port: i32) -> AsyncHostResult<()> {
        let mut sockaddr = unsafe { std::mem::zeroed::<ws::SOCKADDR_IN>() };
        sockaddr.sin_family = ws::AF_INET;
        sockaddr.sin_port = (port as u16).to_be();
        sockaddr.sin_addr.S_un.S_addr = (ip as u32).to_be();
        write_struct(addr, &sockaddr)
    }

    pub(super) fn init_ipv6_addr(
        addr: &mut [u8],
        ip: &[u8],
        port: i32,
        scope_id: i32,
    ) -> AsyncHostResult<()> {
        let ip: [u8; 16] = ip
            .get(..16)
            .ok_or(AsyncHostError::Fault)?
            .try_into()
            .map_err(|_| AsyncHostError::Fault)?;
        let mut sockaddr = unsafe { std::mem::zeroed::<ws::SOCKADDR_IN6>() };
        sockaddr.sin6_family = ws::AF_INET6;
        sockaddr.sin6_port = (port as u16).to_be();
        sockaddr.sin6_addr.u.Byte = ip;
        sockaddr.Anonymous.sin6_scope_id = scope_id as u32;
        write_struct(addr, &sockaddr)
    }

    pub(super) fn ip_addr_get_ip(addr: &[u8]) -> AsyncHostResult<i32> {
        Ok(u32::from_be(unsafe { read_sockaddr_in(addr)?.sin_addr.S_un.S_addr }) as i32)
    }

    pub(super) fn ip_addr_get_port(addr: &[u8]) -> AsyncHostResult<i32> {
        Ok(i32::from(u16::from_be(read_sockaddr_in(addr)?.sin_port)))
    }

    pub(super) fn addr_is_ipv6(addr: &[u8]) -> AsyncHostResult<bool> {
        Ok(sockaddr_family(addr)? as u16 == ws::AF_INET6)
    }

    pub(super) fn addr_is_multicast(addr: &[u8]) -> AsyncHostResult<bool> {
        match sockaddr_family(addr)? as u16 {
            ws::AF_INET => {
                let first_octet =
                    u32::from_be(unsafe { read_sockaddr_in(addr)?.sin_addr.S_un.S_addr }) >> 24;
                Ok((224..240).contains(&first_octet))
            }
            ws::AF_INET6 => Ok(unsafe { read_sockaddr_in6(addr)?.sin6_addr.u.Byte[0] } == 0xff),
            _ => Ok(false),
        }
    }

    pub(super) fn addr_get_ipv6_scope_id(addr: &[u8]) -> AsyncHostResult<i32> {
        Ok(unsafe { read_sockaddr_in6(addr)?.Anonymous.sin6_scope_id } as i32)
    }

    pub(super) fn addr_is_ipv6_wildcard(addr: &[u8]) -> AsyncHostResult<bool> {
        Ok(unsafe { read_sockaddr_in6(addr)?.sin6_addr.u.Byte } == [0; 16])
    }

    pub(super) fn make_tcp_socket(family: i32) -> AsyncHostResult<RawSocket> {
        let socket = unsafe {
            ws::WSASocketW(
                domain(family)?,
                ws::SOCK_STREAM,
                0,
                std::ptr::null(),
                0,
                ws::WSA_FLAG_OVERLAPPED | ws::WSA_FLAG_NO_HANDLE_INHERIT,
            )
        };
        if socket == ws::INVALID_SOCKET {
            return Err(last_wsa_error());
        }
        if let Err(error) =
            set_socket_int(socket as RawFd, ws::SOL_SOCKET, ws::SO_EXCLUSIVEADDRUSE, 0)
        {
            unsafe {
                ws::closesocket(socket);
            }
            return Err(error);
        }
        Ok(raw_socket(socket))
    }

    pub(super) fn make_udp_socket(family: i32, multicast: bool) -> AsyncHostResult<RawSocket> {
        let socket = unsafe {
            ws::WSASocketW(
                domain(family)?,
                ws::SOCK_DGRAM,
                0,
                std::ptr::null(),
                0,
                ws::WSA_FLAG_OVERLAPPED | ws::WSA_FLAG_NO_HANDLE_INHERIT,
            )
        };
        if socket == ws::INVALID_SOCKET {
            return Err(last_wsa_error());
        }
        let result = if multicast {
            set_socket_int(
                socket as RawFd,
                ws::SOL_SOCKET,
                ws::SO_REUSE_MULTICASTPORT,
                1,
            )
        } else {
            set_socket_int(socket as RawFd, ws::SOL_SOCKET, ws::SO_EXCLUSIVEADDRUSE, 0)
        };
        if let Err(error) = result {
            unsafe {
                ws::closesocket(socket);
            }
            return Err(error);
        }
        Ok(raw_socket(socket))
    }

    pub(super) fn join_multicast_group(
        fd: RawFd,
        multi_addr: &[u8],
        local_addr: &[u8],
    ) -> AsyncHostResult<()> {
        let multi_addr = read_sockaddr_in(multi_addr)?;
        let local_addr = read_sockaddr_in(local_addr)?;
        let mreq = ws::IP_MREQ {
            imr_multiaddr: multi_addr.sin_addr,
            imr_interface: local_addr.sin_addr,
        };
        socket_error(unsafe {
            ws::setsockopt(
                socket(fd),
                ws::IPPROTO_IP,
                ws::IP_ADD_MEMBERSHIP,
                (&mreq as *const ws::IP_MREQ).cast(),
                std::mem::size_of_val(&mreq) as i32,
            )
        })
    }

    pub(super) fn join_multicast_group_v6(
        fd: RawFd,
        multi_addr: &[u8],
        interface_index: i32,
    ) -> AsyncHostResult<()> {
        let multi_addr = read_sockaddr_in6(multi_addr)?;
        let mreq = ws::IPV6_MREQ {
            ipv6mr_multiaddr: multi_addr.sin6_addr,
            ipv6mr_interface: interface_index as u32,
        };
        socket_error(unsafe {
            ws::setsockopt(
                socket(fd),
                ws::IPPROTO_IPV6,
                ws::IPV6_ADD_MEMBERSHIP,
                (&mreq as *const ws::IPV6_MREQ).cast(),
                std::mem::size_of_val(&mreq) as i32,
            )
        })
    }

    pub(super) fn set_multicast_interface(fd: RawFd, local_addr: &[u8]) -> AsyncHostResult<()> {
        let local_addr = read_sockaddr_in(local_addr)?;
        socket_error(unsafe {
            ws::setsockopt(
                socket(fd),
                ws::IPPROTO_IP,
                ws::IP_MULTICAST_IF,
                (&local_addr.sin_addr as *const ws::IN_ADDR).cast(),
                std::mem::size_of::<ws::IN_ADDR>() as i32,
            )
        })
    }

    pub(super) fn set_multicast_interface_v6(
        fd: RawFd,
        interface_index: i32,
    ) -> AsyncHostResult<()> {
        let interface_index = interface_index as u32;
        socket_error(unsafe {
            ws::setsockopt(
                socket(fd),
                ws::IPPROTO_IPV6,
                ws::IPV6_MULTICAST_IF,
                (&interface_index as *const u32).cast(),
                std::mem::size_of_val(&interface_index) as i32,
            )
        })
    }

    pub(super) fn set_multicast_ttl(fd: RawFd, ttl: i32, family: i32) -> AsyncHostResult<()> {
        let (level, option) = match family {
            4 => (ws::IPPROTO_IP, ws::IP_MULTICAST_TTL),
            6 => (ws::IPPROTO_IPV6, ws::IPV6_MULTICAST_HOPS),
            _ => return Err(AsyncHostError::Inval),
        };
        set_socket_int(fd, level, option, ttl)
    }

    pub(super) fn set_multicast_loopback(
        fd: RawFd,
        enable: bool,
        family: i32,
    ) -> AsyncHostResult<()> {
        let (level, option) = match family {
            4 => (ws::IPPROTO_IP, ws::IP_MULTICAST_LOOP),
            6 => (ws::IPPROTO_IPV6, ws::IPV6_MULTICAST_LOOP),
            _ => return Err(AsyncHostError::Inval),
        };
        set_socket_int(fd, level, option, i32::from(enable))
    }

    pub(super) fn disable_nagle(fd: RawFd) -> AsyncHostResult<()> {
        set_socket_int(fd, ws::IPPROTO_TCP, ws::TCP_NODELAY, 1)
    }

    pub(super) fn allow_reuse_addr(fd: RawFd) -> AsyncHostResult<()> {
        set_socket_int(fd, ws::SOL_SOCKET, ws::SO_REUSEADDR, 1)
    }

    pub(super) fn set_ipv6_only(fd: RawFd, ipv6_only: bool) -> AsyncHostResult<()> {
        set_socket_int(fd, ws::IPPROTO_IPV6, ws::IPV6_V6ONLY, i32::from(ipv6_only))
    }

    pub(super) fn listen(fd: RawFd) -> AsyncHostResult<()> {
        socket_error(unsafe { ws::listen(socket(fd), ws::SOMAXCONN as i32) })
    }

    pub(super) fn enable_keepalive(
        fd: RawFd,
        keep_idle: i32,
        keep_count: i32,
        keep_intvl: i32,
    ) -> AsyncHostResult<()> {
        set_socket_int(fd, ws::SOL_SOCKET, ws::SO_KEEPALIVE, 1)?;
        if keep_count > 0 {
            set_socket_int(fd, ws::IPPROTO_TCP, ws::TCP_KEEPCNT, keep_count)?;
        }
        if keep_idle > 0 {
            set_socket_int(fd, ws::IPPROTO_TCP, ws::TCP_KEEPIDLE, keep_idle)?;
        }
        if keep_intvl > 0 {
            set_socket_int(fd, ws::IPPROTO_TCP, ws::TCP_KEEPINTVL, keep_intvl)?;
        }
        Ok(())
    }

    pub(super) fn getsockname(fd: RawFd, addr_out: &mut [u8]) -> AsyncHostResult<()> {
        let mut len = i32::try_from(addr_out.len()).map_err(|_| AsyncHostError::Fault)?;
        socket_error(unsafe {
            ws::getsockname(
                socket(fd),
                addr_out.as_mut_ptr().cast::<ws::SOCKADDR>(),
                &mut len,
            )
        })
    }

    pub(super) fn if_nametoindex(name: &[u16]) -> AsyncHostResult<i32> {
        let name: Vec<u16> = name.iter().copied().chain(std::iter::once(0)).collect();
        let mut luid = unsafe { std::mem::zeroed::<NET_LUID_LH>() };
        let mut errno = unsafe { ConvertInterfaceAliasToLuid(name.as_ptr(), &mut luid) };
        if errno != NO_ERROR {
            return Err(win32_error(errno));
        }
        let mut index = 0;
        errno = unsafe { ConvertInterfaceLuidToIndex(&luid, &mut index) };
        if errno != NO_ERROR {
            return Err(win32_error(errno));
        }
        i32::try_from(index).map_err(|_| AsyncHostError::Fault)
    }

    pub(super) fn if_indextoname(index: i32) -> AsyncHostResult<Vec<u8>> {
        let mut luid = unsafe { std::mem::zeroed::<NET_LUID_LH>() };
        let mut errno = unsafe { ConvertInterfaceIndexToLuid(index as u32, &mut luid) };
        if errno != NO_ERROR {
            return Err(win32_error(errno));
        }
        let mut name = vec![0u16; 257];
        errno = unsafe { ConvertInterfaceLuidToAlias(&luid, name.as_mut_ptr(), name.len()) };
        if errno != NO_ERROR {
            return Err(win32_error(errno));
        }
        let len = name
            .iter()
            .position(|unit| *unit == 0)
            .ok_or(AsyncHostError::Fault)?;
        let mut bytes = Vec::with_capacity((len + 1) * std::mem::size_of::<u16>());
        for unit in &name[..=len] {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        Ok(bytes)
    }

    pub(super) fn find_ipv6_test_interface() -> i32 {
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        let mut buf_len = 16 * 1024u32;
        for _ in 0..3 {
            let word_len = (buf_len as usize).div_ceil(std::mem::size_of::<usize>());
            let mut storage = vec![0usize; word_len];
            let addrs = storage
                .as_mut_ptr()
                .cast::<windows_sys::Win32::NetworkManagement::IpHelper::IP_ADAPTER_ADDRESSES_LH>(
            );
            let err = unsafe {
                GetAdaptersAddresses(
                    ws::AF_INET6 as u32,
                    flags,
                    std::ptr::null(),
                    addrs,
                    &mut buf_len,
                )
            };
            if err == ERROR_BUFFER_OVERFLOW {
                continue;
            }
            if err != NO_ERROR {
                return 0;
            }
            let mut adapter = addrs;
            while !adapter.is_null() {
                let adapter_ref = unsafe { &*adapter };
                let adapter_flags = unsafe { adapter_ref.Anonymous2.Flags };
                if adapter_ref.OperStatus == IfOperStatusUp
                    && adapter_ref.Ipv6IfIndex != 0
                    && (adapter_flags & IP_ADAPTER_NO_MULTICAST) == 0
                {
                    let mut unicast = adapter_ref.FirstUnicastAddress;
                    while !unicast.is_null() {
                        let address = unsafe { (*unicast).Address };
                        if !address.lpSockaddr.is_null()
                            && unsafe { (*address.lpSockaddr).sa_family } == ws::AF_INET6
                        {
                            let sockaddr =
                                unsafe { &*(address.lpSockaddr.cast::<ws::SOCKADDR_IN6>()) };
                            let bytes = unsafe { sockaddr.sin6_addr.u.Byte };
                            if bytes[0] == 0xfe && (bytes[1] & 0xc0) == 0x80 {
                                return adapter_ref.Ipv6IfIndex as i32;
                            }
                        }
                        unicast = unsafe { (*unicast).Next };
                    }
                }
                adapter = adapter_ref.Next;
            }
            return 0;
        }
        0
    }

    pub(super) fn udp_client_connect(fd: RawFd, addr: &[u8]) -> AsyncHostResult<()> {
        connect(fd, addr)
    }

    pub(super) fn bind(fd: RawFd, addr: &[u8]) -> AsyncHostResult<()> {
        let len = sockaddr_len(addr)?;
        socket_error(unsafe { ws::bind(socket(fd), addr.as_ptr().cast::<ws::SOCKADDR>(), len) })
    }

    pub(super) fn connect(fd: RawFd, addr: &[u8]) -> AsyncHostResult<()> {
        let len = sockaddr_len(addr)?;
        socket_error(unsafe { ws::connect(socket(fd), addr.as_ptr().cast::<ws::SOCKADDR>(), len) })
    }

    pub(super) fn addrinfo_addr_size(addr: &[u8]) -> i32 {
        if addr.is_empty() {
            return 0;
        }
        i32::try_from(addr.len()).unwrap_or(0)
    }

    pub(super) fn addrinfo_fill_addr(
        addr: &[u8],
        out: &mut [u8],
        port: i32,
    ) -> AsyncHostResult<()> {
        match sockaddr_family(addr)? as u16 {
            ws::AF_INET => {
                let mut sockaddr = read_sockaddr_in(addr)?;
                sockaddr.sin_port = (port as u16).to_be();
                write_struct(out, &sockaddr)?;
            }
            ws::AF_INET6 => {
                let mut sockaddr = read_sockaddr_in6(addr)?;
                sockaddr.sin6_port = (port as u16).to_be();
                write_struct(out, &sockaddr)?;
            }
            _ => return Err(AsyncHostError::Inval),
        }
        Ok(())
    }
}

#[cfg(windows)]
pub(crate) fn copy_sockaddrs_from_getaddrinfo(
    hostname: std::ffi::OsString,
) -> AsyncHostResult<(i32, Vec<Box<[u8]>>)> {
    win::copy_sockaddrs_from_getaddrinfo(hostname)
}

ported_fns! {
    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ipv4_addr_size"
    )]
    #[cfg(unix)]
    pub(crate) fn ipv4_addr_size() -> i32 {
        std::mem::size_of::<libc::sockaddr_in>() as i32
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ipv4_addr_size"
    )]
    #[cfg(windows)]
    pub(crate) fn ipv4_addr_size() -> i32 {
        win::ipv4_addr_size()
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ipv6_addr_size"
    )]
    #[cfg(unix)]
    pub(crate) fn ipv6_addr_size() -> i32 {
        std::mem::size_of::<libc::sockaddr_in6>() as i32
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ipv6_addr_size"
    )]
    #[cfg(windows)]
    pub(crate) fn ipv6_addr_size() -> i32 {
        win::ipv6_addr_size()
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_init_ip_addr"
    )]
    #[cfg(unix)]
    pub(crate) fn init_ip_addr(addr: &mut [u8], ip: i32, port: i32) -> AsyncHostResult<()> {
        let sockaddr = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: (port as u16).to_be(),
            sin_addr: libc::in_addr {
                s_addr: (ip as u32).to_be(),
            },
            ..unsafe { std::mem::zeroed() }
        };
        write_struct(addr, &sockaddr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_init_ip_addr"
    )]
    #[cfg(windows)]
    pub(crate) fn init_ip_addr(addr: &mut [u8], ip: i32, port: i32) -> AsyncHostResult<()> {
        win::init_ip_addr(addr, ip, port)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_init_ipv6_addr"
    )]
    #[cfg(unix)]
    pub(crate) fn init_ipv6_addr(
        addr: &mut [u8],
        ip: &[u8],
        port: i32,
        scope_id: i32,
    ) -> AsyncHostResult<()> {
        let ip: [u8; 16] = ip
            .get(..16)
            .ok_or(AsyncHostError::Fault)?
            .try_into()
            .map_err(|_| AsyncHostError::Fault)?;
        let sockaddr = libc::sockaddr_in6 {
            sin6_family: libc::AF_INET6 as libc::sa_family_t,
            sin6_port: (port as u16).to_be(),
            sin6_addr: libc::in6_addr { s6_addr: ip },
            sin6_scope_id: scope_id as u32,
            ..unsafe { std::mem::zeroed() }
        };
        write_struct(addr, &sockaddr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_init_ipv6_addr"
    )]
    #[cfg(windows)]
    pub(crate) fn init_ipv6_addr(
        addr: &mut [u8],
        ip: &[u8],
        port: i32,
        scope_id: i32,
    ) -> AsyncHostResult<()> {
        win::init_ipv6_addr(addr, ip, port, scope_id)
    }

    #[ported(
        source = "src/internal/event_loop/network.wasm.mbt",
        original = "gai_strerror"
    )]
    #[cfg(unix)]
    pub(crate) fn gai_strerror(code: i32) -> Box<[u8]> {
        let ptr = unsafe { libc::gai_strerror(code) };
        unsafe { CStr::from_ptr(ptr) }.to_bytes_with_nul().into()
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ip_addr_get_ip"
    )]
    #[cfg(unix)]
    pub(crate) fn ip_addr_get_ip(addr: &[u8]) -> AsyncHostResult<i32> {
        Ok(u32::from_be(read_sockaddr_in(addr)?.sin_addr.s_addr) as i32)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ip_addr_get_ip"
    )]
    #[cfg(windows)]
    pub(crate) fn ip_addr_get_ip(addr: &[u8]) -> AsyncHostResult<i32> {
        win::ip_addr_get_ip(addr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ip_addr_get_port"
    )]
    #[cfg(unix)]
    pub(crate) fn ip_addr_get_port(addr: &[u8]) -> AsyncHostResult<i32> {
        Ok(i32::from(u16::from_be(read_sockaddr_in(addr)?.sin_port)))
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ip_addr_get_port"
    )]
    #[cfg(windows)]
    pub(crate) fn ip_addr_get_port(addr: &[u8]) -> AsyncHostResult<i32> {
        win::ip_addr_get_port(addr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_is_ipv6"
    )]
    #[cfg(unix)]
    pub(crate) fn addr_is_ipv6(addr: &[u8]) -> AsyncHostResult<bool> {
        Ok(sockaddr_family(addr)? == libc::AF_INET6)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_is_ipv6"
    )]
    #[cfg(windows)]
    pub(crate) fn addr_is_ipv6(addr: &[u8]) -> AsyncHostResult<bool> {
        win::addr_is_ipv6(addr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_is_multicast"
    )]
    #[cfg(unix)]
    pub(crate) fn addr_is_multicast(addr: &[u8]) -> AsyncHostResult<bool> {
        match sockaddr_family(addr)? {
            libc::AF_INET => {
                let first_octet = u32::from_be(read_sockaddr_in(addr)?.sin_addr.s_addr) >> 24;
                Ok((224..240).contains(&first_octet))
            }
            libc::AF_INET6 => Ok(read_sockaddr_in6(addr)?.sin6_addr.s6_addr[0] == 0xff),
            _ => Ok(false),
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_is_multicast"
    )]
    #[cfg(windows)]
    pub(crate) fn addr_is_multicast(addr: &[u8]) -> AsyncHostResult<bool> {
        win::addr_is_multicast(addr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_get_ipv6_bytes_offset"
    )]
    #[cfg(unix)]
    pub(crate) fn addr_get_ipv6_bytes_offset() -> i32 {
        std::mem::offset_of!(libc::sockaddr_in6, sin6_addr) as i32
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_get_ipv6_bytes_offset"
    )]
    #[cfg(windows)]
    pub(crate) fn addr_get_ipv6_bytes_offset() -> i32 {
        std::mem::offset_of!(ws::SOCKADDR_IN6, sin6_addr) as i32
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_get_ipv6_scope_id"
    )]
    #[cfg(unix)]
    pub(crate) fn addr_get_ipv6_scope_id(addr: &[u8]) -> AsyncHostResult<i32> {
        Ok(read_sockaddr_in6(addr)?.sin6_scope_id as i32)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_get_ipv6_scope_id"
    )]
    #[cfg(windows)]
    pub(crate) fn addr_get_ipv6_scope_id(addr: &[u8]) -> AsyncHostResult<i32> {
        win::addr_get_ipv6_scope_id(addr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_is_ipv6_wildcard"
    )]
    #[cfg(unix)]
    pub(crate) fn addr_is_ipv6_wildcard(addr: &[u8]) -> AsyncHostResult<bool> {
        Ok(read_sockaddr_in6(addr)?.sin6_addr.s6_addr == [0; 16])
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_is_ipv6_wildcard"
    )]
    #[cfg(windows)]
    pub(crate) fn addr_is_ipv6_wildcard(addr: &[u8]) -> AsyncHostResult<bool> {
        win::addr_is_ipv6_wildcard(addr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_make_tcp_socket"
    )]
    #[cfg(unix)]
    pub(crate) fn make_tcp_socket(family: i32) -> AsyncHostResult<RawSocket> {
        let domain = match family {
            4 => libc::AF_INET,
            6 => libc::AF_INET6,
            _ => return Err(AsyncHostError::Inval),
        };
        let fd = unsafe { libc::socket(domain, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err(last_native_error());
        }
        Ok(fd)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_make_tcp_socket"
    )]
    #[cfg(windows)]
    pub(crate) fn make_tcp_socket(family: i32) -> AsyncHostResult<RawSocket> {
        win::make_tcp_socket(family)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_make_udp_socket"
    )]
    #[cfg(unix)]
    pub(crate) fn make_udp_socket(family: i32, multicast: bool) -> AsyncHostResult<RawSocket> {
        let domain = match family {
            4 => libc::AF_INET,
            6 => libc::AF_INET6,
            _ => return Err(AsyncHostError::Inval),
        };
        let fd = unsafe { libc::socket(domain, libc::SOCK_DGRAM, 0) };
        if fd < 0 {
            return Err(last_native_error());
        }
        if multicast {
            let enable: libc::c_int = 1;
            #[cfg(target_os = "linux")]
            let option = libc::SO_REUSEADDR;
            #[cfg(target_os = "macos")]
            let option = libc::SO_REUSEPORT;
            if unsafe {
                libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    option,
                    (&enable as *const libc::c_int).cast(),
                    std::mem::size_of_val(&enable) as libc::socklen_t,
                )
            } < 0
            {
                let error = last_native_error();
                unsafe {
                    libc::close(fd);
                }
                return Err(error);
            }
        }
        Ok(fd)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_make_udp_socket"
    )]
    #[cfg(windows)]
    pub(crate) fn make_udp_socket(family: i32, multicast: bool) -> AsyncHostResult<RawSocket> {
        win::make_udp_socket(family, multicast)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_join_multicast_group"
    )]
    #[cfg(unix)]
    pub(crate) fn join_multicast_group(
        fd: RawFd,
        multi_addr: &[u8],
        local_addr: &[u8],
    ) -> AsyncHostResult<()> {
        let multi_addr = read_sockaddr_in(multi_addr)?;
        let local_addr = read_sockaddr_in(local_addr)?;
        let mreq = libc::ip_mreq {
            imr_multiaddr: multi_addr.sin_addr,
            imr_interface: local_addr.sin_addr,
        };
        if unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_IP,
                libc::IP_ADD_MEMBERSHIP,
                (&mreq as *const libc::ip_mreq).cast(),
                std::mem::size_of_val(&mreq) as libc::socklen_t,
            )
        } < 0
        {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_join_multicast_group"
    )]
    #[cfg(windows)]
    pub(crate) fn join_multicast_group(
        fd: RawFd,
        multi_addr: &[u8],
        local_addr: &[u8],
    ) -> AsyncHostResult<()> {
        win::join_multicast_group(fd, multi_addr, local_addr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_join_multicast_group_v6"
    )]
    #[cfg(unix)]
    pub(crate) fn join_multicast_group_v6(
        fd: RawFd,
        multi_addr: &[u8],
        interface_index: i32,
    ) -> AsyncHostResult<()> {
        let multi_addr = read_sockaddr_in6(multi_addr)?;
        let mreq = libc::ipv6_mreq {
            ipv6mr_multiaddr: multi_addr.sin6_addr,
            ipv6mr_interface: interface_index as libc::c_uint,
        };
        #[cfg(target_os = "linux")]
        let option = libc::IPV6_ADD_MEMBERSHIP;
        #[cfg(target_os = "macos")]
        let option = libc::IPV6_JOIN_GROUP;
        if unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_IPV6,
                option,
                (&mreq as *const libc::ipv6_mreq).cast(),
                std::mem::size_of_val(&mreq) as libc::socklen_t,
            )
        } < 0
        {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_join_multicast_group_v6"
    )]
    #[cfg(windows)]
    pub(crate) fn join_multicast_group_v6(
        fd: RawFd,
        multi_addr: &[u8],
        interface_index: i32,
    ) -> AsyncHostResult<()> {
        win::join_multicast_group_v6(fd, multi_addr, interface_index)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_multicast_interface"
    )]
    #[cfg(unix)]
    pub(crate) fn set_multicast_interface(fd: RawFd, local_addr: &[u8]) -> AsyncHostResult<()> {
        let local_addr = read_sockaddr_in(local_addr)?;
        if unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_IP,
                libc::IP_MULTICAST_IF,
                (&local_addr.sin_addr as *const libc::in_addr).cast(),
                std::mem::size_of::<libc::in_addr>() as libc::socklen_t,
            )
        } < 0
        {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_multicast_interface"
    )]
    #[cfg(windows)]
    pub(crate) fn set_multicast_interface(fd: RawFd, local_addr: &[u8]) -> AsyncHostResult<()> {
        win::set_multicast_interface(fd, local_addr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_multicast_interface_v6"
    )]
    #[cfg(unix)]
    pub(crate) fn set_multicast_interface_v6(
        fd: RawFd,
        interface_index: i32,
    ) -> AsyncHostResult<()> {
        let interface_index = interface_index as libc::c_uint;
        if unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_IPV6,
                libc::IPV6_MULTICAST_IF,
                (&interface_index as *const libc::c_uint).cast(),
                std::mem::size_of_val(&interface_index) as libc::socklen_t,
            )
        } < 0
        {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_multicast_interface_v6"
    )]
    #[cfg(windows)]
    pub(crate) fn set_multicast_interface_v6(
        fd: RawFd,
        interface_index: i32,
    ) -> AsyncHostResult<()> {
        win::set_multicast_interface_v6(fd, interface_index)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_multicast_ttl"
    )]
    #[cfg(unix)]
    pub(crate) fn set_multicast_ttl(fd: RawFd, ttl: i32, family: i32) -> AsyncHostResult<()> {
        let (level, option) = match family {
            4 => (libc::IPPROTO_IP, libc::IP_MULTICAST_TTL),
            6 => (libc::IPPROTO_IPV6, libc::IPV6_MULTICAST_HOPS),
            _ => return Err(AsyncHostError::Inval),
        };
        let ttl: libc::c_int = ttl;
        if unsafe {
            libc::setsockopt(
                fd,
                level,
                option,
                (&ttl as *const libc::c_int).cast(),
                std::mem::size_of_val(&ttl) as libc::socklen_t,
            )
        } < 0
        {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_multicast_ttl"
    )]
    #[cfg(windows)]
    pub(crate) fn set_multicast_ttl(fd: RawFd, ttl: i32, family: i32) -> AsyncHostResult<()> {
        win::set_multicast_ttl(fd, ttl, family)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_multicast_loopback"
    )]
    #[cfg(unix)]
    pub(crate) fn set_multicast_loopback(
        fd: RawFd,
        enable: bool,
        family: i32,
    ) -> AsyncHostResult<()> {
        let (level, option) = match family {
            4 => (libc::IPPROTO_IP, libc::IP_MULTICAST_LOOP),
            6 => (libc::IPPROTO_IPV6, libc::IPV6_MULTICAST_LOOP),
            _ => return Err(AsyncHostError::Inval),
        };
        let enable: libc::c_int = i32::from(enable);
        if unsafe {
            libc::setsockopt(
                fd,
                level,
                option,
                (&enable as *const libc::c_int).cast(),
                std::mem::size_of_val(&enable) as libc::socklen_t,
            )
        } < 0
        {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_multicast_loopback"
    )]
    #[cfg(windows)]
    pub(crate) fn set_multicast_loopback(
        fd: RawFd,
        enable: bool,
        family: i32,
    ) -> AsyncHostResult<()> {
        win::set_multicast_loopback(fd, enable, family)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_disable_nagle"
    )]
    #[cfg(unix)]
    pub(crate) fn disable_nagle(fd: RawFd) -> AsyncHostResult<()> {
        let enable: libc::c_int = 1;
        if unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_TCP,
                libc::TCP_NODELAY,
                (&enable as *const libc::c_int).cast(),
                std::mem::size_of_val(&enable) as libc::socklen_t,
            )
        } < 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_disable_nagle"
    )]
    #[cfg(windows)]
    pub(crate) fn disable_nagle(fd: RawFd) -> AsyncHostResult<()> {
        win::disable_nagle(fd)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_allow_reuse_addr"
    )]
    #[cfg(unix)]
    pub(crate) fn allow_reuse_addr(fd: RawFd) -> AsyncHostResult<()> {
        let enable: libc::c_int = 1;
        if unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                (&enable as *const libc::c_int).cast(),
                std::mem::size_of_val(&enable) as libc::socklen_t,
            )
        } < 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_allow_reuse_addr"
    )]
    #[cfg(windows)]
    pub(crate) fn allow_reuse_addr(fd: RawFd) -> AsyncHostResult<()> {
        win::allow_reuse_addr(fd)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_ipv6_only"
    )]
    #[cfg(unix)]
    pub(crate) fn set_ipv6_only(fd: RawFd, ipv6_only: bool) -> AsyncHostResult<()> {
        let value: libc::c_int = i32::from(ipv6_only);
        if unsafe {
            libc::setsockopt(
                fd,
                libc::IPPROTO_IPV6,
                libc::IPV6_V6ONLY,
                (&value as *const libc::c_int).cast(),
                std::mem::size_of_val(&value) as libc::socklen_t,
            )
        } < 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_ipv6_only"
    )]
    #[cfg(windows)]
    pub(crate) fn set_ipv6_only(fd: RawFd, ipv6_only: bool) -> AsyncHostResult<()> {
        win::set_ipv6_only(fd, ipv6_only)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_listen"
    )]
    #[cfg(unix)]
    pub(crate) fn listen(fd: RawFd) -> AsyncHostResult<()> {
        if unsafe { libc::listen(fd, libc::SOMAXCONN) } < 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_listen"
    )]
    #[cfg(windows)]
    pub(crate) fn listen(fd: RawFd) -> AsyncHostResult<()> {
        win::listen(fd)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_enable_keepalive"
    )]
    #[cfg(unix)]
    pub(crate) fn enable_keepalive(
        fd: RawFd,
        keep_idle: i32,
        keep_count: i32,
        keep_intvl: i32,
    ) -> AsyncHostResult<()> {
        let enable: libc::c_int = 1;
        if unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_KEEPALIVE,
                (&enable as *const libc::c_int).cast(),
                std::mem::size_of_val(&enable) as libc::socklen_t,
            )
        } < 0 {
            return Err(last_native_error());
        }

        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            if keep_count > 0 {
                set_tcp_int(fd, libc::TCP_KEEPCNT, keep_count)?;
            }
            if keep_idle > 0 {
                set_tcp_int(fd, libc::TCP_KEEPIDLE, keep_idle)?;
            }
            if keep_intvl > 0 {
                set_tcp_int(fd, libc::TCP_KEEPINTVL, keep_intvl)?;
            }
        }

        #[cfg(target_os = "macos")]
        {
            if keep_count > 0 {
                set_tcp_int(fd, libc::TCP_KEEPCNT, keep_count)?;
            }
            if keep_idle > 0 {
                set_tcp_int(fd, libc::TCP_KEEPALIVE, keep_idle)?;
            }
            if keep_intvl > 0 {
                set_tcp_int(fd, libc::TCP_KEEPINTVL, keep_intvl)?;
            }
        }

        Ok(())
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_enable_keepalive"
    )]
    #[cfg(windows)]
    pub(crate) fn enable_keepalive(
        fd: RawFd,
        keep_idle: i32,
        keep_count: i32,
        keep_intvl: i32,
    ) -> AsyncHostResult<()> {
        win::enable_keepalive(fd, keep_idle, keep_count, keep_intvl)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_getsockname"
    )]
    #[cfg(unix)]
    pub(crate) fn getsockname(fd: RawFd, addr_out: &mut [u8]) -> AsyncHostResult<()> {
        let mut len = libc::socklen_t::try_from(addr_out.len()).map_err(|_| AsyncHostError::Fault)?;
        if unsafe {
            libc::getsockname(fd, addr_out.as_mut_ptr().cast::<libc::sockaddr>(), &mut len)
        } < 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_getsockname"
    )]
    #[cfg(windows)]
    pub(crate) fn getsockname(fd: RawFd, addr_out: &mut [u8]) -> AsyncHostResult<()> {
        win::getsockname(fd, addr_out)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_if_nametoindex"
    )]
    #[cfg(unix)]
    pub(crate) fn if_nametoindex(name: &[u16]) -> AsyncHostResult<i32> {
        let name = char::decode_utf16(name.iter().copied())
            .map(Result::unwrap)
            .collect::<String>();
        let name = std::ffi::CString::new(name).map_err(|_| AsyncHostError::Inval)?;
        let index = unsafe { libc::if_nametoindex(name.as_ptr()) };
        if index == 0 {
            Err(last_native_error())
        } else {
            Ok(index as i32)
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_if_nametoindex"
    )]
    #[cfg(windows)]
    pub(crate) fn if_nametoindex(name: &[u16]) -> AsyncHostResult<i32> {
        win::if_nametoindex(name)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_if_indextoname"
    )]
    #[cfg(unix)]
    pub(crate) fn if_indextoname(index: i32) -> AsyncHostResult<Vec<u8>> {
        let mut name = vec![0; libc::IF_NAMESIZE + 1];
        let ptr = unsafe { libc::if_indextoname(index as u32, name.as_mut_ptr().cast()) };
        if ptr.is_null() {
            return Err(last_native_error());
        }
        let bytes = unsafe { CStr::from_ptr(ptr) }.to_bytes_with_nul();
        name.truncate(bytes.len());
        Ok(name)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_if_indextoname"
    )]
    #[cfg(windows)]
    pub(crate) fn if_indextoname(index: i32) -> AsyncHostResult<Vec<u8>> {
        win::if_indextoname(index)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_find_ipv6_test_interface"
    )]
    #[cfg(unix)]
    pub(crate) fn find_ipv6_test_interface() -> i32 {
        let mut ifaddrs = std::ptr::null_mut();
        if unsafe { libc::getifaddrs(&mut ifaddrs) } < 0 {
            return 0;
        }
        let mut current = ifaddrs;
        let mut result = 0;
        while !current.is_null() {
            let ifaddr = unsafe { &*current };
            if !ifaddr.ifa_addr.is_null() {
                let sockaddr = unsafe { &*ifaddr.ifa_addr };
                let flags = ifaddr.ifa_flags as libc::c_uint;
                #[cfg(target_os = "linux")]
                let loopback_ok = (flags & libc::IFF_LOOPBACK as libc::c_uint) == 0;
                #[cfg(target_os = "macos")]
                let loopback_ok = (flags & libc::IFF_LOOPBACK as libc::c_uint) != 0;
                if i32::from(sockaddr.sa_family) == libc::AF_INET6
                    && loopback_ok
                    && (flags & libc::IFF_UP as libc::c_uint) != 0
                    && (flags & libc::IFF_RUNNING as libc::c_uint) != 0
                    && (flags & libc::IFF_MULTICAST as libc::c_uint) != 0
                {
                    let sockaddr_in6 =
                        unsafe { &*(ifaddr.ifa_addr.cast::<libc::sockaddr_in6>()) };
                    let addr = sockaddr_in6.sin6_addr.s6_addr;
                    let is_link_local = addr[0] == 0xfe && (addr[1] & 0xc0) == 0x80;
                    if is_link_local {
                        let index = unsafe { libc::if_nametoindex(ifaddr.ifa_name) };
                        if index != 0 {
                            result = index as i32;
                            break;
                        }
                    }
                }
            }
            current = ifaddr.ifa_next;
        }
        unsafe {
            libc::freeifaddrs(ifaddrs);
        }
        result
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_find_ipv6_test_interface"
    )]
    #[cfg(windows)]
    pub(crate) fn find_ipv6_test_interface() -> i32 {
        win::find_ipv6_test_interface()
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_udp_client_connect"
    )]
    #[cfg(unix)]
    pub(crate) fn udp_client_connect(fd: RawFd, addr: &[u8]) -> AsyncHostResult<()> {
        connect(fd, addr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_udp_client_connect"
    )]
    #[cfg(windows)]
    pub(crate) fn udp_client_connect(fd: RawFd, addr: &[u8]) -> AsyncHostResult<()> {
        win::udp_client_connect(fd, addr)
    }

    #[cfg(unix)]
    pub(crate) fn bind(fd: RawFd, addr: &[u8]) -> AsyncHostResult<()> {
        let len = sockaddr_len(addr)?;
        if unsafe { libc::bind(fd, addr.as_ptr().cast::<libc::sockaddr>(), len) } < 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[cfg(windows)]
    pub(crate) fn bind(fd: RawFd, addr: &[u8]) -> AsyncHostResult<()> {
        win::bind(fd, addr)
    }

    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_connect"
    )]
    #[cfg(unix)]
    pub(crate) fn connect(fd: RawFd, addr: &[u8]) -> AsyncHostResult<()> {
        let len = sockaddr_len(addr)?;
        if unsafe { libc::connect(fd, addr.as_ptr().cast::<libc::sockaddr>(), len) } < 0 {
            Err(last_native_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_getsockerr"
    )]
    #[cfg(unix)]
    pub(crate) fn getsockerr(fd: RawFd) -> AsyncHostResult<i32> {
        let mut err = 0;
        let mut len = std::mem::size_of_val(&err) as libc::socklen_t;
        if unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&mut err as *mut libc::c_int).cast(),
                &mut len,
            )
        } < 0 {
            Err(last_native_error())
        } else {
            Ok(err)
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_accept"
    )]
    #[cfg(unix)]
    pub(crate) fn accept(fd: RawFd, addr: &mut [u8]) -> AsyncHostResult<RawSocket> {
        let mut len = sockaddr_len(addr)?;
        let conn = unsafe { libc::accept(fd, addr.as_mut_ptr().cast::<libc::sockaddr>(), &mut len) };
        if conn < 0 {
            return Err(last_native_error());
        }
        Ok(conn)
    }

    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_recvfrom"
    )]
    #[cfg(unix)]
    pub(crate) fn recvfrom(
        fd: RawFd,
        buf: &mut [u8],
        addr: &mut [u8],
    ) -> AsyncHostResult<usize> {
        let mut len = sockaddr_len(addr)?;
        let ret = unsafe {
            libc::recvfrom(
                fd,
                buf.as_mut_ptr().cast(),
                buf.len(),
                0,
                addr.as_mut_ptr().cast::<libc::sockaddr>(),
                &mut len,
            )
        };
        if ret < 0 {
            Err(last_native_error())
        } else {
            usize::try_from(ret).map_err(|_| AsyncHostError::Fault)
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_sendto"
    )]
    #[cfg(unix)]
    pub(crate) fn sendto(fd: RawFd, buf: &[u8], addr: &[u8]) -> AsyncHostResult<usize> {
        let len = sockaddr_len(addr)?;
        let ret = unsafe {
            libc::sendto(
                fd,
                buf.as_ptr().cast(),
                buf.len(),
                0,
                addr.as_ptr().cast::<libc::sockaddr>(),
                len,
            )
        };
        if ret < 0 {
            Err(last_native_error())
        } else {
            usize::try_from(ret).map_err(|_| AsyncHostError::Fault)
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addrinfo_addr_size"
    )]
    #[cfg(unix)]
    pub(crate) fn addrinfo_addr_size(addr: &[u8]) -> i32 {
        if addr.is_empty() {
            return 0;
        }
        i32::try_from(addr.len()).unwrap_or(0)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addrinfo_addr_size"
    )]
    #[cfg(windows)]
    pub(crate) fn addrinfo_addr_size(addr: &[u8]) -> i32 {
        win::addrinfo_addr_size(addr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addrinfo_fill_addr"
    )]
    #[cfg(unix)]
    pub(crate) fn addrinfo_fill_addr(addr: &[u8], out: &mut [u8], port: i32) -> AsyncHostResult<()> {
        match sockaddr_family(addr)? {
            libc::AF_INET => {
                let mut sockaddr = read_sockaddr_in(addr)?;
                sockaddr.sin_port = (port as u16).to_be();
                write_struct(out, &sockaddr)?;
            }
            libc::AF_INET6 => {
                let mut sockaddr = read_sockaddr_in6(addr)?;
                sockaddr.sin6_port = (port as u16).to_be();
                write_struct(out, &sockaddr)?;
            }
            _ => return Err(AsyncHostError::Inval),
        }
        Ok(())
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addrinfo_fill_addr"
    )]
    #[cfg(windows)]
    pub(crate) fn addrinfo_fill_addr(addr: &[u8], out: &mut [u8], port: i32) -> AsyncHostResult<()> {
        win::addrinfo_fill_addr(addr, out, port)
    }
}

#[cfg(all(
    unix,
    any(target_os = "linux", target_os = "android", target_os = "macos")
))]
fn set_tcp_int(fd: RawFd, option: libc::c_int, value: i32) -> AsyncHostResult<()> {
    let value: libc::c_int = value;
    if unsafe {
        libc::setsockopt(
            fd,
            libc::IPPROTO_TCP,
            option,
            (&value as *const libc::c_int).cast(),
            std::mem::size_of_val(&value) as libc::socklen_t,
        )
    } < 0
    {
        Err(last_native_error())
    } else {
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn addr_get_ipv6_bytes_offset_uses_sin6_addr_offset() {
        let ip = [
            0x20, 0x01, 0x0d, 0xb8, 0x85, 0xa3, 0x00, 0x00, 0x00, 0x00, 0x8a, 0x2e, 0x03, 0x70,
            0x73, 0x34,
        ];
        let mut addr = vec![0; ipv6_addr_size() as usize];
        init_ipv6_addr(&mut addr, &ip, 443, 0).unwrap();

        let offset = std::mem::offset_of!(libc::sockaddr_in6, sin6_addr);
        assert_eq!(addr_get_ipv6_bytes_offset(), offset as i32);
        let len = std::mem::size_of::<libc::in6_addr>();
        assert_eq!(&addr[offset..offset + len], &ip);
    }
}
