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

//! Native socket operations, independent of guest handles and runtime policy.

use super::IpFamily;
#[cfg(windows)]
use super::last_wsa_error;
use std::io;
#[cfg(windows)]
use windows_sys::Win32::Networking::WinSock as ws;

use super::address::{read_sockaddr_in, read_sockaddr_in6, sockaddr_len};
use super::{INVALID_ADDRESS, RawSocket};
use crate::async_sys::ported_fns;

#[cfg(windows)]
fn socket(fd: RawSocket) -> ws::SOCKET {
    fd as usize
}

#[cfg(windows)]
fn raw_socket(socket: ws::SOCKET) -> RawSocket {
    socket as RawSocket
}

#[cfg(windows)]
fn socket_error(ret: i32) -> io::Result<()> {
    if ret == ws::SOCKET_ERROR {
        Err(last_wsa_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn set_socket_int(fd: RawSocket, level: i32, option: i32, value: i32) -> io::Result<()> {
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

ported_fns! {
    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_make_tcp_socket"
    )]
    pub(crate) fn make_tcp_socket(family: IpFamily) -> io::Result<RawSocket> {
        #[cfg(unix)]
        {
            let fd = unsafe { libc::socket(family.as_raw(), libc::SOCK_STREAM, 0) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(fd)
        }
        #[cfg(windows)]
        {
            let socket = unsafe {
                ws::WSASocketW(
                    family.as_raw(),
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
            if let Err(error) = set_socket_int(
                socket as RawSocket,
                ws::SOL_SOCKET,
                ws::SO_EXCLUSIVEADDRUSE,
                0,
            ) {
                unsafe {
                    ws::closesocket(socket);
                }
                return Err(error);
            }
            Ok(raw_socket(socket))
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_make_udp_socket"
    )]
    pub(in crate::network) fn make_udp_socket(
        family: IpFamily,
        multicast: bool,
    ) -> io::Result<RawSocket> {
        #[cfg(unix)]
        {
            let fd = unsafe { libc::socket(family.as_raw(), libc::SOCK_DGRAM, 0) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
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
                    let error = io::Error::last_os_error();
                    unsafe {
                        libc::close(fd);
                    }
                    return Err(error);
                }
            }
            Ok(fd)
        }
        #[cfg(windows)]
        {
            let socket = unsafe {
                ws::WSASocketW(
                    family.as_raw(),
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
                    socket as RawSocket,
                    ws::SOL_SOCKET,
                    ws::SO_REUSE_MULTICASTPORT,
                    1,
                )
            } else {
                set_socket_int(
                    socket as RawSocket,
                    ws::SOL_SOCKET,
                    ws::SO_EXCLUSIVEADDRUSE,
                    0,
                )
            };
            if let Err(error) = result {
                unsafe {
                    ws::closesocket(socket);
                }
                return Err(error);
            }
            Ok(raw_socket(socket))
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_join_multicast_group"
    )]
    pub(crate) fn join_multicast_group(
        fd: RawSocket,
        multi_addr: &[u8],
        local_addr: &[u8],
    ) -> io::Result<()> {
        #[cfg(unix)]
        {
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
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(windows)]
        {
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
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_join_multicast_group_v6"
    )]
    pub(crate) fn join_multicast_group_v6(
        fd: RawSocket,
        multi_addr: &[u8],
        interface_index: u32,
    ) -> io::Result<()> {
        #[cfg(unix)]
        {
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
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(windows)]
        {
            let multi_addr = read_sockaddr_in6(multi_addr)?;
            let mreq = ws::IPV6_MREQ {
                ipv6mr_multiaddr: multi_addr.sin6_addr,
                ipv6mr_interface: interface_index,
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
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_multicast_interface"
    )]
    pub(crate) fn set_multicast_interface(fd: RawSocket, local_addr: &[u8]) -> io::Result<()> {
        #[cfg(unix)]
        {
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
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(windows)]
        {
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
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_multicast_interface_v6"
    )]
    pub(crate) fn set_multicast_interface_v6(fd: RawSocket, interface_index: u32) -> io::Result<()> {
        #[cfg(unix)]
        {
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
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(windows)]
        {
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
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_multicast_ttl"
    )]
    pub(crate) fn set_multicast_ttl(fd: RawSocket, ttl: i32, family: IpFamily) -> io::Result<()> {
        #[cfg(unix)]
        {
            let (level, option) = match family {
                IpFamily::V4 => (libc::IPPROTO_IP, libc::IP_MULTICAST_TTL),
                IpFamily::V6 => (libc::IPPROTO_IPV6, libc::IPV6_MULTICAST_HOPS),
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
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(windows)]
        {
            let (level, option) = match family {
                IpFamily::V4 => (ws::IPPROTO_IP, ws::IP_MULTICAST_TTL),
                IpFamily::V6 => (ws::IPPROTO_IPV6, ws::IPV6_MULTICAST_HOPS),
            };
            set_socket_int(fd, level, option, ttl)
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_multicast_loopback"
    )]
    pub(crate) fn set_multicast_loopback(
        fd: RawSocket,
        enable: bool,
        family: IpFamily,
    ) -> io::Result<()> {
        #[cfg(unix)]
        {
            let (level, option) = match family {
                IpFamily::V4 => (libc::IPPROTO_IP, libc::IP_MULTICAST_LOOP),
                IpFamily::V6 => (libc::IPPROTO_IPV6, libc::IPV6_MULTICAST_LOOP),
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
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(windows)]
        {
            let (level, option) = match family {
                IpFamily::V4 => (ws::IPPROTO_IP, ws::IP_MULTICAST_LOOP),
                IpFamily::V6 => (ws::IPPROTO_IPV6, ws::IPV6_MULTICAST_LOOP),
            };
            set_socket_int(fd, level, option, i32::from(enable))
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_disable_nagle"
    )]
    pub(crate) fn disable_nagle(fd: RawSocket) -> io::Result<()> {
        #[cfg(unix)]
        {
            let enable: libc::c_int = 1;
            if unsafe {
                libc::setsockopt(
                    fd,
                    libc::IPPROTO_TCP,
                    libc::TCP_NODELAY,
                    (&enable as *const libc::c_int).cast(),
                    std::mem::size_of_val(&enable) as libc::socklen_t,
                )
            } < 0
            {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(windows)]
        {
            set_socket_int(fd, ws::IPPROTO_TCP, ws::TCP_NODELAY, 1)
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_allow_reuse_addr"
    )]
    pub(crate) fn allow_reuse_addr(fd: RawSocket) -> io::Result<()> {
        #[cfg(unix)]
        {
            let enable: libc::c_int = 1;
            if unsafe {
                libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_REUSEADDR,
                    (&enable as *const libc::c_int).cast(),
                    std::mem::size_of_val(&enable) as libc::socklen_t,
                )
            } < 0
            {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(windows)]
        {
            set_socket_int(fd, ws::SOL_SOCKET, ws::SO_REUSEADDR, 1)
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_allow_reuse_port"
    )]
    #[cfg(unix)]
    pub(crate) fn allow_reuse_port(fd: RawSocket) -> io::Result<()> {
        let enable: libc::c_int = 1;
        if unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEPORT,
                (&raw const enable).cast(),
                std::mem::size_of_val(&enable) as libc::socklen_t,
            )
        } < 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_allow_reuse_port"
    )]
    #[cfg(windows)]
    pub(crate) fn allow_reuse_port(_fd: RawSocket) -> io::Result<()> {
        // Native treats platforms without SO_REUSEPORT as a successful no-op.
        Ok(())
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_set_ipv6_only"
    )]
    pub(crate) fn set_ipv6_only(fd: RawSocket, ipv6_only: bool) -> io::Result<()> {
        #[cfg(unix)]
        {
            let value: libc::c_int = i32::from(ipv6_only);
            if unsafe {
                libc::setsockopt(
                    fd,
                    libc::IPPROTO_IPV6,
                    libc::IPV6_V6ONLY,
                    (&value as *const libc::c_int).cast(),
                    std::mem::size_of_val(&value) as libc::socklen_t,
                )
            } < 0
            {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(windows)]
        {
            set_socket_int(fd, ws::IPPROTO_IPV6, ws::IPV6_V6ONLY, i32::from(ipv6_only))
        }
    }

    #[ported(source = "src/socket/socket.c", original = "moonbitlang_async_listen")]
    pub(in crate::network) fn listen(fd: RawSocket) -> io::Result<()> {
        #[cfg(unix)]
        {
            if unsafe { libc::listen(fd, libc::SOMAXCONN) } < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(windows)]
        {
            socket_error(unsafe { ws::listen(socket(fd), ws::SOMAXCONN as i32) })
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_enable_keepalive"
    )]
    pub(crate) fn enable_keepalive(
        fd: RawSocket,
        keep_idle: i32,
        keep_count: i32,
        keep_intvl: i32,
    ) -> io::Result<()> {
        #[cfg(unix)]
        {
            let enable: libc::c_int = 1;
            if unsafe {
                libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_KEEPALIVE,
                    (&enable as *const libc::c_int).cast(),
                    std::mem::size_of_val(&enable) as libc::socklen_t,
                )
            } < 0
            {
                return Err(io::Error::last_os_error());
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
        #[cfg(windows)]
        {
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
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_getsockname"
    )]
    pub(crate) fn getsockname(fd: RawSocket, addr_out: &mut [u8]) -> io::Result<()> {
        #[cfg(unix)]
        {
            let mut len = libc::socklen_t::try_from(addr_out.len())
                .map_err(|_| io::Error::from_raw_os_error(INVALID_ADDRESS))?;
            if unsafe {
                libc::getsockname(fd, addr_out.as_mut_ptr().cast::<libc::sockaddr>(), &mut len)
            } < 0
            {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
        #[cfg(windows)]
        {
            let mut len = i32::try_from(addr_out.len())
                .map_err(|_| io::Error::from_raw_os_error(INVALID_ADDRESS))?;
            socket_error(unsafe {
                ws::getsockname(
                    socket(fd),
                    addr_out.as_mut_ptr().cast::<ws::SOCKADDR>(),
                    &mut len,
                )
            })
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_udp_client_connect"
    )]
    pub(in crate::network) fn udp_client_connect(fd: RawSocket, addr: &[u8]) -> io::Result<()> {
        #[cfg(unix)]
        {
            connect(fd, addr)
        }
        #[cfg(windows)]
        {
            let len = sockaddr_len(addr)?;
            socket_error(unsafe { ws::connect(socket(fd), addr.as_ptr().cast::<ws::SOCKADDR>(), len) })
        }
    }

    #[cfg(unix)]
    pub(in crate::network) fn bind(fd: RawSocket, addr: &[u8]) -> io::Result<()> {
        let len = sockaddr_len(addr)?;
        if unsafe { libc::bind(fd, addr.as_ptr().cast::<libc::sockaddr>(), len) } < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    #[cfg(windows)]
    pub(in crate::network) fn bind(fd: RawSocket, addr: &[u8]) -> io::Result<()> {
        let len = sockaddr_len(addr)?;
        socket_error(unsafe { ws::bind(socket(fd), addr.as_ptr().cast::<ws::SOCKADDR>(), len) })
    }

    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_connect"
    )]
    #[cfg(unix)]
    pub(in crate::network) fn connect(fd: RawSocket, addr: &[u8]) -> io::Result<()> {
        let len = sockaddr_len(addr)?;
        if unsafe { libc::connect(fd, addr.as_ptr().cast::<libc::sockaddr>(), len) } < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_getsockerr"
    )]
    #[cfg(unix)]
    pub(crate) fn getsockerr(fd: RawSocket) -> io::Result<i32> {
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
        } < 0
        {
            Err(io::Error::last_os_error())
        } else {
            Ok(err)
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_accept"
    )]
    #[cfg(unix)]
    pub(in crate::network) fn accept(fd: RawSocket, addr: &mut [u8]) -> io::Result<RawSocket> {
        let mut len = sockaddr_len(addr)?;
        let conn = unsafe { libc::accept(fd, addr.as_mut_ptr().cast::<libc::sockaddr>(), &mut len) };
        if conn < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(conn)
    }

    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_recvfrom"
    )]
    #[cfg(unix)]
    pub(in crate::network) fn recvfrom(
        fd: RawSocket,
        buf: &mut [u8],
        addr: &mut [u8],
    ) -> io::Result<usize> {
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
            Err(io::Error::last_os_error())
        } else {
            usize::try_from(ret).map_err(|_| io::Error::from_raw_os_error(INVALID_ADDRESS))
        }
    }

    #[ported(
        source = "src/internal/event_loop/io_unix.c",
        original = "moonbitlang_async_sendto"
    )]
    #[cfg(unix)]
    pub(in crate::network) fn sendto(fd: RawSocket, buf: &[u8], addr: &[u8]) -> io::Result<usize> {
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
            Err(io::Error::last_os_error())
        } else {
            usize::try_from(ret).map_err(|_| io::Error::from_raw_os_error(INVALID_ADDRESS))
        }
    }
}

#[cfg(all(
    unix,
    any(target_os = "linux", target_os = "android", target_os = "macos")
))]
fn set_tcp_int(fd: RawSocket, option: libc::c_int, value: i32) -> io::Result<()> {
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
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn reuse_port_allows_two_listeners_on_the_same_address() {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

        let [first, second] = std::array::from_fn(|_| {
            let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
            assert!(fd >= 0);
            unsafe { OwnedFd::from_raw_fd(fd) }
        });
        let mut addr = unsafe { std::mem::zeroed::<libc::sockaddr_in>() };
        addr.sin_family = libc::AF_INET as _;
        addr.sin_addr.s_addr = u32::from_ne_bytes([127, 0, 0, 1]);
        let mut len = std::mem::size_of_val(&addr) as libc::socklen_t;
        allow_reuse_port(first.as_raw_fd()).unwrap();
        assert_eq!(
            unsafe { libc::bind(first.as_raw_fd(), (&raw const addr).cast(), len) },
            0
        );
        listen(first.as_raw_fd()).unwrap();
        assert_eq!(
            unsafe { libc::getsockname(first.as_raw_fd(), (&raw mut addr).cast(), &mut len) },
            0
        );
        assert_eq!(
            unsafe { libc::bind(second.as_raw_fd(), (&raw const addr).cast(), len) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EADDRINUSE)
        );
        allow_reuse_port(second.as_raw_fd()).unwrap();
        assert_eq!(
            unsafe { libc::bind(second.as_raw_fd(), (&raw const addr).cast(), len) },
            0
        );
        listen(second.as_raw_fd()).unwrap();
    }
}
