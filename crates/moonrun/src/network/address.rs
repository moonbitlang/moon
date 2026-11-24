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

//! Native socket address encoding shared by network operations and async imports.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
#[cfg(windows)]
use windows_sys::Win32::Networking::WinSock as ws;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::ported_fns;

/// Interpret the IP and port used by authorization and implicit bind checks.
/// IPv6 scope and flow information stay in the native address for OS operations.
#[cfg(unix)]
pub(super) fn socket_addr(addr: &[u8]) -> AsyncHostResult<SocketAddr> {
    match sockaddr_family(addr)? {
        libc::AF_INET => {
            let addr = read_sockaddr_in(addr)?;
            Ok(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::from(u32::from_be(addr.sin_addr.s_addr))),
                u16::from_be(addr.sin_port),
            ))
        }
        libc::AF_INET6 => {
            let addr = read_sockaddr_in6(addr)?;
            Ok(SocketAddr::new(
                IpAddr::V6(Ipv6Addr::from(addr.sin6_addr.s6_addr)),
                u16::from_be(addr.sin6_port),
            ))
        }
        _ => Err(AsyncHostError::Inval),
    }
}

#[cfg(windows)]
pub(super) fn socket_addr(addr: &[u8]) -> AsyncHostResult<SocketAddr> {
    match sockaddr_family(addr)? as u16 {
        ws::AF_INET => {
            let addr = read_sockaddr_in(addr)?;
            Ok(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::from(u32::from_be(unsafe {
                    addr.sin_addr.S_un.S_addr
                }))),
                u16::from_be(addr.sin_port),
            ))
        }
        ws::AF_INET6 => {
            let addr = read_sockaddr_in6(addr)?;
            Ok(SocketAddr::new(
                IpAddr::V6(Ipv6Addr::from(unsafe { addr.sin6_addr.u.Byte })),
                u16::from_be(addr.sin6_port),
            ))
        }
        _ => Err(AsyncHostError::Inval),
    }
}

#[cfg(unix)]
pub(crate) fn sockaddr_len(addr: &[u8]) -> AsyncHostResult<libc::socklen_t> {
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
pub(crate) fn read_sockaddr_in(addr: &[u8]) -> AsyncHostResult<libc::sockaddr_in> {
    if addr.len() < std::mem::size_of::<libc::sockaddr_in>() {
        return Err(AsyncHostError::Fault);
    }
    Ok(unsafe { addr.as_ptr().cast::<libc::sockaddr_in>().read_unaligned() })
}

#[cfg(unix)]
pub(crate) fn read_sockaddr_in6(addr: &[u8]) -> AsyncHostResult<libc::sockaddr_in6> {
    if addr.len() < std::mem::size_of::<libc::sockaddr_in6>() {
        return Err(AsyncHostError::Fault);
    }
    Ok(unsafe { addr.as_ptr().cast::<libc::sockaddr_in6>().read_unaligned() })
}

#[cfg(windows)]
fn sockaddr_family(addr: &[u8]) -> AsyncHostResult<i32> {
    if addr.len() < std::mem::size_of::<ws::SOCKADDR>() {
        return Err(AsyncHostError::Fault);
    }
    let family = unsafe { addr.as_ptr().cast::<ws::SOCKADDR>().read_unaligned() }.sa_family;
    Ok(i32::from(family))
}

#[cfg(windows)]
pub(crate) fn sockaddr_len(addr: &[u8]) -> AsyncHostResult<i32> {
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

#[cfg(windows)]
pub(crate) fn read_sockaddr_in(addr: &[u8]) -> AsyncHostResult<ws::SOCKADDR_IN> {
    if addr.len() < std::mem::size_of::<ws::SOCKADDR_IN>() {
        return Err(AsyncHostError::Fault);
    }
    Ok(unsafe { addr.as_ptr().cast::<ws::SOCKADDR_IN>().read_unaligned() })
}

#[cfg(windows)]
pub(crate) fn read_sockaddr_in6(addr: &[u8]) -> AsyncHostResult<ws::SOCKADDR_IN6> {
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

ported_fns! {
    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ipv4_addr_size"
    )]
    #[cfg(unix)]
    pub(crate) fn ipv4_addr_size() -> u32 {
        u32::try_from(std::mem::size_of::<libc::sockaddr_in>())
            .expect("socket address size fits u32")
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ipv4_addr_size"
    )]
    #[cfg(windows)]
    pub(crate) fn ipv4_addr_size() -> u32 {
        u32::try_from(std::mem::size_of::<ws::SOCKADDR_IN>()).expect("socket address size fits u32")
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ipv6_addr_size"
    )]
    #[cfg(unix)]
    pub(crate) fn ipv6_addr_size() -> u32 {
        u32::try_from(std::mem::size_of::<libc::sockaddr_in6>())
            .expect("socket address size fits u32")
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ipv6_addr_size"
    )]
    #[cfg(windows)]
    pub(crate) fn ipv6_addr_size() -> u32 {
        u32::try_from(std::mem::size_of::<ws::SOCKADDR_IN6>())
            .expect("socket address size fits u32")
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_init_ip_addr"
    )]
    #[cfg(unix)]
    pub(crate) fn init_ip_addr(addr: &mut [u8], ip: u32, port: u32) -> AsyncHostResult<()> {
        let sockaddr = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: (port as u16).to_be(),
            sin_addr: libc::in_addr {
                s_addr: ip.to_be(),
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
    pub(crate) fn init_ip_addr(addr: &mut [u8], ip: u32, port: u32) -> AsyncHostResult<()> {
        let mut sockaddr = unsafe { std::mem::zeroed::<ws::SOCKADDR_IN>() };
        sockaddr.sin_family = ws::AF_INET;
        sockaddr.sin_port = (port as u16).to_be();
        sockaddr.sin_addr.S_un.S_addr = ip.to_be();
        write_struct(addr, &sockaddr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_init_ipv6_addr"
    )]
    #[cfg(unix)]
    pub(crate) fn init_ipv6_addr(
        addr: &mut [u8],
        ip: &[u8],
        port: u32,
        scope_id: u32,
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
            sin6_scope_id: scope_id,
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
        port: u32,
        scope_id: u32,
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
        sockaddr.Anonymous.sin6_scope_id = scope_id;
        write_struct(addr, &sockaddr)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ip_addr_get_ip"
    )]
    #[cfg(unix)]
    pub(crate) fn ip_addr_get_ip(addr: &[u8]) -> AsyncHostResult<u32> {
        Ok(u32::from_be(read_sockaddr_in(addr)?.sin_addr.s_addr))
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ip_addr_get_ip"
    )]
    #[cfg(windows)]
    pub(crate) fn ip_addr_get_ip(addr: &[u8]) -> AsyncHostResult<u32> {
        Ok(u32::from_be(unsafe {
            read_sockaddr_in(addr)?.sin_addr.S_un.S_addr
        }))
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ip_addr_get_port"
    )]
    #[cfg(unix)]
    pub(crate) fn ip_addr_get_port(addr: &[u8]) -> AsyncHostResult<u32> {
        Ok(u32::from(u16::from_be(read_sockaddr_in(addr)?.sin_port)))
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_ip_addr_get_port"
    )]
    #[cfg(windows)]
    pub(crate) fn ip_addr_get_port(addr: &[u8]) -> AsyncHostResult<u32> {
        Ok(u32::from(u16::from_be(read_sockaddr_in(addr)?.sin_port)))
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
        Ok(sockaddr_family(addr)? as u16 == ws::AF_INET6)
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

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_get_ipv6_bytes_offset"
    )]
    #[cfg(unix)]
    pub(crate) fn addr_get_ipv6_bytes_offset() -> u32 {
        u32::try_from(std::mem::offset_of!(libc::sockaddr_in6, sin6_addr))
            .expect("socket address offset fits u32")
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_get_ipv6_bytes_offset"
    )]
    #[cfg(windows)]
    pub(crate) fn addr_get_ipv6_bytes_offset() -> u32 {
        u32::try_from(std::mem::offset_of!(ws::SOCKADDR_IN6, sin6_addr))
            .expect("socket address offset fits u32")
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_get_ipv6_scope_id"
    )]
    #[cfg(unix)]
    pub(crate) fn addr_get_ipv6_scope_id(addr: &[u8]) -> AsyncHostResult<u32> {
        Ok(read_sockaddr_in6(addr)?.sin6_scope_id)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addr_get_ipv6_scope_id"
    )]
    #[cfg(windows)]
    pub(crate) fn addr_get_ipv6_scope_id(addr: &[u8]) -> AsyncHostResult<u32> {
        Ok(unsafe { read_sockaddr_in6(addr)?.Anonymous.sin6_scope_id })
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
        Ok(unsafe { read_sockaddr_in6(addr)?.sin6_addr.u.Byte } == [0; 16])
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addrinfo_addr_size"
    )]
    pub(crate) fn addrinfo_addr_size(addr: &[u8]) -> u32 {
        if addr.is_empty() {
            return 0;
        }
        u32::try_from(addr.len()).unwrap_or(0)
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_addrinfo_fill_addr"
    )]
    #[cfg(unix)]
    pub(crate) fn addrinfo_fill_addr(addr: &[u8], out: &mut [u8], port: u32) -> AsyncHostResult<()> {
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
    pub(crate) fn addrinfo_fill_addr(addr: &[u8], out: &mut [u8], port: u32) -> AsyncHostResult<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv4_address_decoding_accepts_unaligned_buffers() {
        let mut buffer = vec![0; ipv4_addr_size() as usize + 1];
        let addr = &mut buffer[1..];
        init_ip_addr(addr, 0x7f000001, 1234).unwrap();

        assert_eq!(
            socket_addr(addr).unwrap(),
            "127.0.0.1:1234".parse().unwrap()
        );
        assert_eq!(ip_addr_get_ip(addr).unwrap(), 0x7f000001);
        assert_eq!(ip_addr_get_port(addr).unwrap(), 1234);
        assert_eq!(sockaddr_len(addr).unwrap() as usize, addr.len());
    }

    #[test]
    fn changing_ipv6_port_preserves_ip_and_scope() {
        let ip = "fe80::1234".parse::<Ipv6Addr>().unwrap();
        let mut buffer = vec![0; ipv6_addr_size() as usize + 1];
        let addr = &mut buffer[1..];
        init_ipv6_addr(addr, &ip.octets(), 1234, 17).unwrap();
        let mut out = vec![0; ipv6_addr_size() as usize];
        addrinfo_fill_addr(addr, &mut out, 8443).unwrap();

        assert_eq!(socket_addr(addr).unwrap(), SocketAddr::from((ip, 1234)));
        assert_eq!(socket_addr(&out).unwrap(), SocketAddr::from((ip, 8443)));
        // Policy uses IP and port, while the OS still receives the scope.
        assert_eq!(addr_get_ipv6_scope_id(&out).unwrap(), 17);
        assert_eq!(sockaddr_len(&out).unwrap() as usize, out.len());
        let offset = addr_get_ipv6_bytes_offset() as usize;
        assert_eq!(&out[offset..offset + 16], &ip.octets());
    }

    #[test]
    fn malformed_addresses_preserve_validation_errors() {
        let mut ipv4 = vec![0; ipv4_addr_size() as usize];
        init_ip_addr(&mut ipv4, 0x7f000001, 1234).unwrap();
        let mut ipv6 = vec![0; ipv6_addr_size() as usize];
        init_ipv6_addr(&mut ipv6, &Ipv6Addr::LOCALHOST.octets(), 1234, 0).unwrap();
        let mut out = vec![0; ipv6.len()];
        for addr in [&ipv4, &ipv6] {
            for len in 0..addr.len() {
                assert_eq!(socket_addr(&addr[..len]), Err(AsyncHostError::Fault));
                assert_eq!(sockaddr_len(&addr[..len]), Err(AsyncHostError::Fault));
                assert_eq!(
                    addrinfo_fill_addr(&addr[..len], &mut out, 443),
                    Err(AsyncHostError::Fault)
                );
            }
            assert_eq!(
                addrinfo_fill_addr(addr, &mut [], 443),
                Err(AsyncHostError::Fault)
            );
        }

        let unknown = vec![0; ipv6.len()];
        assert_eq!(socket_addr(&unknown), Err(AsyncHostError::Inval));
        assert_eq!(sockaddr_len(&unknown), Err(AsyncHostError::Inval));
        // An invalid input family takes precedence over a short output buffer.
        assert_eq!(
            addrinfo_fill_addr(&unknown, &mut [], 443),
            Err(AsyncHostError::Inval)
        );
        assert_eq!(addr_is_ipv6(&unknown), Ok(false));
        assert_eq!(addr_is_multicast(&unknown), Ok(false));
        // Fixed-layout getters historically check size, without checking family.
        assert_eq!(ip_addr_get_ip(&unknown), Ok(0));
        assert_eq!(ip_addr_get_port(&unknown), Ok(0));
    }

    #[test]
    fn addr_get_ipv6_bytes_offset_uses_sin6_addr_offset() {
        let ip = [
            0x20, 0x01, 0x0d, 0xb8, 0x85, 0xa3, 0x00, 0x00, 0x00, 0x00, 0x8a, 0x2e, 0x03, 0x70,
            0x73, 0x34,
        ];
        let mut addr = vec![0; ipv6_addr_size() as usize];
        init_ipv6_addr(&mut addr, &ip, 443, 0).unwrap();

        #[cfg(unix)]
        let offset = std::mem::offset_of!(libc::sockaddr_in6, sin6_addr);
        #[cfg(windows)]
        let offset = std::mem::offset_of!(ws::SOCKADDR_IN6, sin6_addr);
        assert_eq!(addr_get_ipv6_bytes_offset(), u32::try_from(offset).unwrap());
        let len = ip.len();
        assert_eq!(&addr[offset..offset + len], &ip);
    }
}
