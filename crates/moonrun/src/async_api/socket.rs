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
use crate::guest_memory::GuestMemory;
#[cfg(unix)]
use crate::network::os::dns;
use crate::network::os::{address, interface};

use super::context::ImportContext;
use super::os_string;
use super::provenance::ported_imports;

// These codecs only validate native address buffers. Preserve their historical
// guest trap names while OS operations continue to return ordinary io::Error.
fn address_error(error: std::io::Error) -> AsyncHostError {
    match error.raw_os_error() {
        Some(errno) if errno == AsyncHostError::Fault.errno() => AsyncHostError::Fault,
        Some(errno) if errno == AsyncHostError::Inval.errno() => AsyncHostError::Inval,
        _ => error.into(),
    }
}

ported_imports! {
#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_init_WSA"
)]
#[cfg(windows)]
pub(super) fn init_wsa(_context: &mut ImportContext<'_>) -> i32 {
    crate::network::os::init_wsa()
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_cleanup_WSA"
)]
#[cfg(windows)]
pub(super) fn cleanup_wsa(_context: &mut ImportContext<'_>) -> i32 {
    crate::network::os::cleanup_wsa()
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn ipv4_addr_size(_context: &mut ImportContext<'_>) -> u32 {
    address::ipv4_addr_size() as u32
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn ipv6_addr_size(_context: &mut ImportContext<'_>) -> u32 {
    address::ipv6_addr_size() as u32
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn init_ip_addr(
    context: &mut ImportContext<'_>,
    addr: u32,
    ip: u32,
    port: u32,
) -> AsyncHostResult<()> {
    context.with_memory_mut(|memory| {
        let addr_len = address::ipv4_addr_size() as u32;
        let addr = memory.read_exact_mut(addr, addr_len)?;
        address::init_ip_addr(addr, ip, port as u16).map_err(address_error)
    })
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn init_ipv6_addr(
    context: &mut ImportContext<'_>,
    addr: u32,
    ip: u32,
    port: u32,
    scope_id: u32,
) -> AsyncHostResult<()> {
    context.with_memory_mut(|memory| {
        let ip: [u8; 16] = memory.read_exact(ip, 16)?
            .try_into()
            .map_err(|_| AsyncHostError::Fault)?;
        let addr_len = address::ipv6_addr_size() as u32;
        let addr = memory.read_exact_mut(addr, addr_len)?;
        address::init_ipv6_addr(addr, &ip, port as u16, scope_id).map_err(address_error)
    })
}

#[ported(source = "src/internal/event_loop/network.wasm.mbt", original = "gai_strerror")]
#[cfg(unix)]
pub(super) fn gai_strerror(context: &mut ImportContext<'_>, code: i32) -> u64 {
    context.host.buffers().publish(
        dns::gai_strerror(code).into_bytes_with_nul().into_boxed_slice(),
    )
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn ip_addr_get_ip(
    context: &mut ImportContext<'_>,
    addr: u32,
    addr_len: u32,
) -> AsyncHostResult<u32> {
    context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?;
        address::ip_addr_get_ip(addr).map_err(address_error)
    })
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn ip_addr_get_port(
    context: &mut ImportContext<'_>,
    addr: u32,
    addr_len: u32,
) -> AsyncHostResult<u32> {
    context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?;
        address::ip_addr_get_port(addr).map(u32::from).map_err(address_error)
    })
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn addr_is_ipv6(
    context: &mut ImportContext<'_>,
    addr: u32,
    addr_len: u32,
) -> AsyncHostResult<i32> {
    context
        .with_memory_mut(|memory| {
            let addr = memory.read_exact(addr, addr_len)?;
            address::addr_is_ipv6(addr).map_err(address_error)
        })
        .map(i32::from)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn addr_is_multicast(
    context: &mut ImportContext<'_>,
    addr: u32,
    addr_len: u32,
) -> AsyncHostResult<i32> {
    context
        .with_memory_mut(|memory| {
            let addr = memory.read_exact(addr, addr_len)?;
            address::addr_is_multicast(addr).map_err(address_error)
        })
        .map(i32::from)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn addr_get_ipv6_bytes_offset(_context: &mut ImportContext<'_>) -> u32 {
    address::addr_get_ipv6_bytes_offset() as u32
}

pub(super) fn addr_copy_ipv6_bytes(
    context: &mut ImportContext<'_>,
    addr: u32,
    out: u32,
    addr_len: u32,
    len: u32,
) -> AsyncHostResult<()> {
    context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?;
        let offset = address::addr_get_ipv6_bytes_offset();
        let ip = addr
            .get(offset..offset + 16)
            .ok_or(AsyncHostError::Fault)?
            .to_vec();
        let out = memory
            .read_exact_mut(out, len)?
            .get_mut(..16)
            .ok_or(AsyncHostError::Fault)?;
        out.copy_from_slice(&ip);
        Ok(())
    })
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn addr_get_ipv6_scope_id(
    context: &mut ImportContext<'_>,
    addr: u32,
    addr_len: u32,
) -> AsyncHostResult<u32> {
    context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?;
        address::addr_get_ipv6_scope_id(addr).map_err(address_error)
    })
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn addr_is_ipv6_wildcard(
    context: &mut ImportContext<'_>,
    addr: u32,
    addr_len: u32,
) -> AsyncHostResult<i32> {
    context
        .with_memory_mut(|memory| {
            let addr = memory.read_exact(addr, addr_len)?;
            address::addr_is_ipv6_wildcard(addr).map_err(address_error)
        })
        .map(i32::from)
}

pub(super) fn addrinfo_is_null(_context: &mut ImportContext<'_>, addrinfo: u64) -> i32 {
    i32::from(addrinfo == crate::runtime::null_handle())
}

pub(super) fn addrinfo_get_next(
    context: &mut ImportContext<'_>,
    addrinfo: u64,
) -> AsyncHostResult<u64> {
    context.host.addrinfo_next(addrinfo)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn addrinfo_addr_size(
    context: &mut ImportContext<'_>,
    addrinfo: u64,
) -> AsyncHostResult<u32> {
    if addrinfo == crate::runtime::null_handle() {
        return Ok(0);
    }
    let addr = context.host.addrinfo_addr(addrinfo)?;
    Ok(u32::try_from(addr.len()).unwrap_or(0))
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn addrinfo_fill_addr(
    context: &mut ImportContext<'_>,
    addrinfo: u64,
    out: u32,
    port: u32,
    out_len: u32,
) -> AsyncHostResult<()> {
    if addrinfo == crate::runtime::null_handle() {
        return Ok(());
    }
    let addr = context.host.addrinfo_addr(addrinfo)?;
    context.with_memory_mut(|memory| {
        let out = memory.read_exact_mut(out, out_len)?;
        address::addrinfo_fill_addr(&addr, out, port as u16).map_err(address_error)
    })
}

pub(super) fn addrinfo_free(context: &mut ImportContext<'_>, addrinfo: u64) -> AsyncHostResult<()> {
    context.host.free_addrinfo(addrinfo)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn make_tcp_socket(context: &mut ImportContext<'_>, family: i32) -> u64 {
    match context.host.make_tcp_socket(family) {
        Ok(socket) => socket,
        Err(error) => {
            context.host.record_error(error);
            context.host.invalid_fd()
        }
    }
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn make_udp_socket(context: &mut ImportContext<'_>, family: i32, multicast: i32) -> u64 {
    let multicast = multicast != 0;
    match context.host.make_udp_socket(family, multicast) {
        Ok(socket) => socket,
        Err(error) => {
            context.host.record_error(error);
            context.host.invalid_fd()
        }
    }
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn join_multicast_group(
    context: &mut ImportContext<'_>,
    fd: u64,
    multi_addr: u32,
    local_addr: u32,
    multi_addr_len: u32,
    local_addr_len: u32,
) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let multi_addr = memory.read_exact(multi_addr, multi_addr_len)?;
        let local_addr = memory.read_exact(local_addr, local_addr_len)?;
        host.with_resource(fd, |socket| {
            host.network().join_multicast_group(socket, multi_addr, local_addr)
        })
    });
    zero_or_minus_one(context, result)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn join_multicast_group_v6(
    context: &mut ImportContext<'_>,
    fd: u64,
    multi_addr: u32,
    interface_index: u32,
    multi_addr_len: u32,
) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let multi_addr = memory.read_exact(multi_addr, multi_addr_len)?;
        host.with_resource(fd, |socket| {
            host.network().join_multicast_group_v6(socket, multi_addr, interface_index)
        })
    });
    zero_or_minus_one(context, result)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn set_multicast_interface(
    context: &mut ImportContext<'_>,
    fd: u64,
    local_addr: u32,
    local_addr_len: u32,
) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let local_addr = memory.read_exact(local_addr, local_addr_len)?;
        host.with_resource(fd, |socket| {
            host.network().set_multicast_interface(socket, local_addr)
        })
    });
    zero_or_minus_one(context, result)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn set_multicast_interface_v6(
    context: &mut ImportContext<'_>,
    fd: u64,
    interface_index: u32,
) -> i32 {
    let host = context.host;
    zero_or_minus_one(
        context,
        host.with_resource(fd, |socket| {
            host.network().set_multicast_interface_v6(socket, interface_index)
        }),
    )
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn set_multicast_ttl(
    context: &mut ImportContext<'_>,
    fd: u64,
    ttl: i32,
    family: i32,
) -> i32 {
    let host = context.host;
    zero_or_minus_one(
        context,
        host.with_resource(fd, |socket| {
            host.network().set_multicast_ttl(socket, ttl, family)
        }),
    )
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn set_multicast_loopback(
    context: &mut ImportContext<'_>,
    fd: u64,
    enable: i32,
    family: i32,
) -> i32 {
    let host = context.host;
    zero_or_minus_one(
        context,
        host.with_resource(fd, |socket| {
            host.network().set_multicast_loopback(socket, enable != 0, family)
        }),
    )
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn disable_nagle(context: &mut ImportContext<'_>, fd: u64) -> i32 {
    let host = context.host;
    zero_or_minus_one(
        context,
        host.with_resource(fd, |socket| {
            host.network().disable_nagle(socket)
        }),
    )
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn allow_reuse_addr(context: &mut ImportContext<'_>, fd: u64) -> i32 {
    let host = context.host;
    zero_or_minus_one(context, host.with_resource(fd, |socket| {
        host.network().allow_reuse_addr(socket)
    }))
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn allow_reuse_port(context: &mut ImportContext<'_>, fd: u64) -> i32 {
    let host = context.host;
    zero_or_minus_one(context, host.with_resource(fd, |socket| {
        host.network().allow_reuse_port(socket)
    }))
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn set_ipv6_only(context: &mut ImportContext<'_>, fd: u64, ipv6_only: i32) -> i32 {
    let host = context.host;
    zero_or_minus_one(
        context,
        host.with_resource(fd, |socket| {
            host.network().set_ipv6_only(socket, ipv6_only != 0)
        }),
    )
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn listen(context: &mut ImportContext<'_>, fd: u64) -> i32 {
    let host = context.host;
    zero_or_minus_one(context, host.listen(fd))
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn enable_keepalive(
    context: &mut ImportContext<'_>,
    fd: u64,
    keep_idle: i32,
    keep_count: i32,
    keep_intvl: i32,
) -> i32 {
    let host = context.host;
    zero_or_minus_one(
        context,
        host.with_resource(fd, |socket| {
            host.network().enable_keepalive(socket, keep_idle, keep_count, keep_intvl)
        }),
    )
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn getsockname(context: &mut ImportContext<'_>, fd: u64, addr: u32, addr_len: u32) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let addr = memory.read_exact_mut(addr, addr_len)?;
        host.with_resource(fd, |socket| host.network().getsockname(socket, addr))
    });
    zero_or_minus_one(context, result)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn if_nametoindex(context: &mut ImportContext<'_>, name: u32, name_len: u32) -> u32 {
    let result = os_string::read_guest(context, name, name_len)
        .and_then(|name| interface::if_nametoindex(&name).map_err(Into::into));
    match result {
        Ok(index) => index,
        Err(error) => {
            context.host.record_error(error);
            0
        }
    }
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn if_indextoname(context: &mut ImportContext<'_>, index: u32) -> u64 {
    match interface::if_indextoname(index) {
        Ok(name) => {
            #[cfg(unix)]
            let bytes = {
                use std::os::unix::ffi::OsStringExt;
                let mut bytes = name.into_vec();
                bytes.push(0);
                bytes
            };
            #[cfg(windows)]
            let bytes = {
                use std::os::windows::ffi::OsStrExt;
                name.encode_wide().chain(std::iter::once(0))
                    .flat_map(u16::to_le_bytes).collect::<Vec<_>>()
            };
            context.host.buffers().publish(bytes.into_boxed_slice())
        },
        Err(error) => {
            context.host.record_error(error.into());
            crate::runtime::null_handle()
        }
    }
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn find_ipv6_test_interface(_context: &mut ImportContext<'_>) -> u32 {
    interface::find_ipv6_test_interface()
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn udp_client_connect(
    context: &mut ImportContext<'_>,
    fd: u64,
    addr: u32,
    addr_len: u32,
) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?;
        host.connect_udp(fd, addr)
    });
    zero_or_minus_one(context, result)
}

// Deprecated: upstream binds through thread_pool/make_bind_job.
pub(super) fn bind(context: &mut ImportContext<'_>, fd: u64, addr: u32, addr_len: u32) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?;
        host.bind(fd, addr)
    });
    zero_or_minus_one(context, result)
}

#[ported(source = "src/internal/event_loop/io_unix.c")]
#[cfg(unix)]
pub(super) fn recvfrom(
    context: &mut ImportContext<'_>,
    fd: u64,
    buf: u32,
    offset: u32,
    len: u32,
    addr: u32,
    addr_len: u32,
) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let buf = buf.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        memory.read_exact(buf, len)?;
        let mut data = vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?];
        let mut addr_data = memory.read_exact(addr, addr_len)?.to_vec();
        let n = host.recv_from(fd, &mut data, &mut addr_data)?;
        memory.write_exact(buf, &data[..n])?;
        memory.write_exact(addr, &addr_data)?;
        i32::try_from(n).map_err(|_| AsyncHostError::Fault)
    });
    match result {
        Ok(n) => n,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(source = "src/internal/event_loop/io_unix.c")]
#[cfg(unix)]
pub(super) fn sendto(
    context: &mut ImportContext<'_>,
    fd: u64,
    buf: u32,
    offset: u32,
    len: u32,
    addr: u32,
    addr_len: u32,
) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let buf = buf.checked_add(offset).ok_or(AsyncHostError::Fault)?;
        let data = memory.read_exact(buf, len)?;
        let addr = memory.read_exact(addr, addr_len)?;
        host.send_to(fd, data, addr)
            .and_then(|n| i32::try_from(n).map_err(|_| AsyncHostError::Fault))
    });
    match result {
        Ok(n) => n,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(source = "src/internal/event_loop/io_unix.c")]
#[cfg(unix)]
pub(super) fn connect(context: &mut ImportContext<'_>, fd: u64, addr: u32, addr_len: u32) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?;
        host.connect_tcp(fd, addr)
    });
    zero_or_minus_one(context, result)
}

#[ported(source = "src/internal/event_loop/io_unix.c")]
#[cfg(unix)]
pub(super) fn getsockerr(context: &mut ImportContext<'_>, fd: u64) -> i32 {
    let host = context.host;
    match host.with_resource(fd, |socket| host.network().getsockerr(socket)) {
        Ok(err) => err,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(source = "src/internal/event_loop/io_unix.c")]
#[cfg(unix)]
pub(super) fn accept(context: &mut ImportContext<'_>, fd: u64, addr: u32, addr_len: u32) -> u64 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let addr = memory.read_exact_mut(addr, addr_len)?;
        host.accept(fd, addr)
    });
    match result {
        Ok(socket) => socket,
        Err(error) => {
            context.host.record_error(error);
            context.host.invalid_fd()
        }
    }
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_connect"
)]
#[cfg(windows)]
pub(super) fn connect_io_result(context: &mut ImportContext<'_>, fd: u64, result: u64) -> i32 {
    match context.host.connect_io_result(fd, result) {
        Ok(ret) => ret,
        Err(error) => {
            context.host.record_error(error);
            0
        }
    }
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_setup_connected_socket"
)]
#[cfg(windows)]
pub(super) fn setup_connected_socket(context: &mut ImportContext<'_>, fd: u64) -> i32 {
    zero_or_minus_one(context, context.host.setup_connected_socket(fd))
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_accept"
)]
#[cfg(windows)]
pub(super) fn accept_io_result(
    context: &mut ImportContext<'_>,
    server_fd: u64,
    conn_fd: u64,
    result: u64,
) -> i32 {
    match context.host.accept_io_result(server_fd, conn_fd, result) {
        Ok(ret) => ret,
        Err(error) => {
            context.host.record_error(error);
            0
        }
    }
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_setup_accepted_socket"
)]
#[cfg(windows)]
pub(super) fn setup_accepted_socket(
    context: &mut ImportContext<'_>,
    listen_fd: u64,
    accept_fd: u64,
) -> i32 {
    zero_or_minus_one(
        context,
        context.host.setup_accepted_socket(listen_fd, accept_fd),
    )
}

fn zero_or_minus_one(context: &mut ImportContext<'_>, result: AsyncHostResult<()>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}
}
