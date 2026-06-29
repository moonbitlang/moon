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
use crate::async_host::AsyncHostError;
use crate::async_host::{AsyncHostResult, GuestMemory, read_u16};
use crate::async_sys::socket as sys;

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
#[ported(source = "src/socket/socket.c")]
pub(super) fn ipv4_addr_size(_context: &mut ImportContext<'_, '_>) -> i32 {
    sys::ipv4_addr_size()
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn ipv6_addr_size(_context: &mut ImportContext<'_, '_>) -> i32 {
    sys::ipv6_addr_size()
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn init_ip_addr(
    context: &mut ImportContext<'_, '_>,
    addr: i32,
    ip: i32,
    port: i32,
) -> AsyncHostResult<()> {
    context.with_memory_mut(|memory| {
        let addr_len = sys::ipv4_addr_size();
        let addr = memory.read_exact_mut(addr, addr_len)?;
        sys::init_ip_addr(addr, ip, port)
    })
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn init_ipv6_addr(
    context: &mut ImportContext<'_, '_>,
    addr: i32,
    ip: i32,
    port: i32,
    scope_id: i32,
) -> AsyncHostResult<()> {
    context.with_memory_mut(|memory| {
        let ip = memory.read_exact(ip, 16)?.to_vec();
        let addr_len = sys::ipv6_addr_size();
        let addr = memory.read_exact_mut(addr, addr_len)?;
        sys::init_ipv6_addr(addr, &ip, port, scope_id)
    })
}

#[ported(source = "src/internal/event_loop/network.mbt", original = "gai_strerror")]
#[cfg(unix)]
pub(super) fn gai_strerror(context: &mut ImportContext<'_, '_>, code: i32) -> u64 {
    context.host.insert_c_buffer(sys::gai_strerror(code))
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn ip_addr_get_ip(
    context: &mut ImportContext<'_, '_>,
    addr: i32,
    addr_len: i32,
) -> AsyncHostResult<i32> {
    context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?;
        sys::ip_addr_get_ip(addr)
    })
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn ip_addr_get_port(
    context: &mut ImportContext<'_, '_>,
    addr: i32,
    addr_len: i32,
) -> AsyncHostResult<i32> {
    context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?;
        sys::ip_addr_get_port(addr)
    })
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn addr_is_ipv6(
    context: &mut ImportContext<'_, '_>,
    addr: i32,
    addr_len: i32,
) -> AsyncHostResult<i32> {
    context
        .with_memory_mut(|memory| {
            let addr = memory.read_exact(addr, addr_len)?;
            sys::addr_is_ipv6(addr)
        })
        .map(i32::from)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn addr_is_multicast(
    context: &mut ImportContext<'_, '_>,
    addr: i32,
    addr_len: i32,
) -> AsyncHostResult<i32> {
    context
        .with_memory_mut(|memory| {
            let addr = memory.read_exact(addr, addr_len)?;
            sys::addr_is_multicast(addr)
        })
        .map(i32::from)
}

#[ported(
    source = "src/socket/socket.c",
    original = "moonbitlang_async_addr_get_ipv6_bytes"
)]
pub(super) fn addr_copy_ipv6_bytes(
    context: &mut ImportContext<'_, '_>,
    addr: i32,
    out: i32,
    addr_len: i32,
    len: i32,
) -> AsyncHostResult<()> {
    context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?.to_vec();
        let out = memory.read_exact_mut(out, len)?;
        sys::addr_copy_ipv6_bytes(&addr, out)
    })
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn addr_get_ipv6_scope_id(
    context: &mut ImportContext<'_, '_>,
    addr: i32,
    addr_len: i32,
) -> AsyncHostResult<i32> {
    context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?;
        sys::addr_get_ipv6_scope_id(addr)
    })
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn addr_is_ipv6_wildcard(
    context: &mut ImportContext<'_, '_>,
    addr: i32,
    addr_len: i32,
) -> AsyncHostResult<i32> {
    context
        .with_memory_mut(|memory| {
            let addr = memory.read_exact(addr, addr_len)?;
            sys::addr_is_ipv6_wildcard(addr)
        })
        .map(i32::from)
}

pub(super) fn addrinfo_is_null(_context: &mut ImportContext<'_, '_>, addrinfo: u64) -> i32 {
    i32::from(addrinfo == crate::async_host::INVALID_HOST_HANDLE)
}

pub(super) fn addrinfo_get_next(
    context: &mut ImportContext<'_, '_>,
    addrinfo: u64,
) -> AsyncHostResult<u64> {
    context.host.addrinfo_next(addrinfo)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn addrinfo_addr_size(
    context: &mut ImportContext<'_, '_>,
    addrinfo: u64,
) -> AsyncHostResult<i32> {
    if addrinfo == crate::async_host::INVALID_HOST_HANDLE {
        return Ok(0);
    }
    let addr = context.host.addrinfo_addr(addrinfo)?;
    Ok(sys::addrinfo_addr_size(&addr))
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn addrinfo_fill_addr(
    context: &mut ImportContext<'_, '_>,
    addrinfo: u64,
    out: i32,
    port: i32,
    out_len: i32,
) -> AsyncHostResult<()> {
    if addrinfo == crate::async_host::INVALID_HOST_HANDLE {
        return Ok(());
    }
    let addr = context.host.addrinfo_addr(addrinfo)?;
    context.with_memory_mut(|memory| {
        let out = memory.read_exact_mut(out, out_len)?;
        sys::addrinfo_fill_addr(&addr, out, port)
    })
}

pub(super) fn addrinfo_free(context: &mut ImportContext<'_, '_>, addrinfo: u64) -> AsyncHostResult<()> {
    context.host.free_addrinfo(addrinfo)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn make_tcp_socket(context: &mut ImportContext<'_, '_>, family: i32) -> u64 {
    match sys::make_tcp_socket(family) {
        Ok(fd) => context.host.insert_host_file(fd),
        Err(error) => {
            context.host.record_error(error);
            context.host.invalid_fd()
        }
    }
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn make_udp_socket(context: &mut ImportContext<'_, '_>, family: i32, multicast: i32) -> u64 {
    match sys::make_udp_socket(family, multicast != 0) {
        Ok(fd) => context.host.insert_host_file(fd),
        Err(error) => {
            context.host.record_error(error);
            context.host.invalid_fd()
        }
    }
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn join_multicast_group(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    multi_addr: i32,
    local_addr: i32,
    multi_addr_len: i32,
    local_addr_len: i32,
) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let multi_addr = memory.read_exact(multi_addr, multi_addr_len)?.to_vec();
        let local_addr = memory.read_exact(local_addr, local_addr_len)?.to_vec();
        host.with_raw_file(fd, |fd| {
            sys::join_multicast_group(fd, &multi_addr, &local_addr)
        })
    });
    zero_or_minus_one(context, result)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn join_multicast_group_v6(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    multi_addr: i32,
    interface_index: i32,
    multi_addr_len: i32,
) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let multi_addr = memory.read_exact(multi_addr, multi_addr_len)?.to_vec();
        host.with_raw_file(fd, |fd| {
            sys::join_multicast_group_v6(fd, &multi_addr, interface_index)
        })
    });
    zero_or_minus_one(context, result)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn set_multicast_interface(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    local_addr: i32,
    local_addr_len: i32,
) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let local_addr = memory.read_exact(local_addr, local_addr_len)?.to_vec();
        host.with_raw_file(fd, |fd| sys::set_multicast_interface(fd, &local_addr))
    });
    zero_or_minus_one(context, result)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn set_multicast_interface_v6(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    interface_index: i32,
) -> i32 {
    let host = context.host;
    zero_or_minus_one(
        context,
        host.with_raw_file(fd, |fd| {
            sys::set_multicast_interface_v6(fd, interface_index)
        }),
    )
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn set_multicast_ttl(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    ttl: i32,
    family: i32,
) -> i32 {
    let host = context.host;
    zero_or_minus_one(
        context,
        host.with_raw_file(fd, |fd| sys::set_multicast_ttl(fd, ttl, family)),
    )
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn set_multicast_loopback(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    enable: i32,
    family: i32,
) -> i32 {
    let host = context.host;
    zero_or_minus_one(
        context,
        host.with_raw_file(fd, |fd| {
            sys::set_multicast_loopback(fd, enable != 0, family)
        }),
    )
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn disable_nagle(context: &mut ImportContext<'_, '_>, fd: u64) -> i32 {
    let host = context.host;
    zero_or_minus_one(context, host.with_raw_file(fd, sys::disable_nagle))
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn allow_reuse_addr(context: &mut ImportContext<'_, '_>, fd: u64) -> i32 {
    let host = context.host;
    zero_or_minus_one(context, host.with_raw_file(fd, sys::allow_reuse_addr))
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn set_ipv6_only(context: &mut ImportContext<'_, '_>, fd: u64, ipv6_only: i32) -> i32 {
    let host = context.host;
    zero_or_minus_one(
        context,
        host.with_raw_file(fd, |fd| sys::set_ipv6_only(fd, ipv6_only != 0)),
    )
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn listen(context: &mut ImportContext<'_, '_>, fd: u64) -> i32 {
    let host = context.host;
    zero_or_minus_one(context, host.with_raw_file(fd, sys::listen))
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn enable_keepalive(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    keep_idle: i32,
    keep_count: i32,
    keep_intvl: i32,
) -> i32 {
    let host = context.host;
    zero_or_minus_one(
        context,
        host.with_raw_file(fd, |fd| {
            sys::enable_keepalive(fd, keep_idle, keep_count, keep_intvl)
        }),
    )
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn getsockname(context: &mut ImportContext<'_, '_>, fd: u64, addr: i32, addr_len: i32) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let addr = memory.read_exact_mut(addr, addr_len)?;
        host.with_raw_file(fd, |fd| sys::getsockname(fd, addr))
    });
    zero_or_minus_one(context, result)
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn if_nametoindex(context: &mut ImportContext<'_, '_>, name: i32, name_len: i32) -> i32 {
    let result = context.with_memory_mut(|memory| {
        let name = read_u16(memory, name, name_len)?;
        sys::if_nametoindex(name)
    });
    match result {
        Ok(index) => index,
        Err(error) => {
            context.host.record_error(error);
            0
        }
    }
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn if_indextoname(context: &mut ImportContext<'_, '_>, index: i32) -> u64 {
    match sys::if_indextoname(index) {
        Ok(name) => context.host.insert_c_buffer(name.into_boxed_slice()),
        Err(error) => {
            context.host.record_error(error);
            crate::async_host::INVALID_HOST_HANDLE
        }
    }
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn find_ipv6_test_interface(_context: &mut ImportContext<'_, '_>) -> i32 {
    sys::find_ipv6_test_interface()
}

#[ported(source = "src/socket/socket.c")]
pub(super) fn udp_client_connect(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    addr: i32,
    addr_len: i32,
) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?.to_vec();
        host.with_raw_file(fd, |fd| sys::udp_client_connect(fd, &addr))
    });
    zero_or_minus_one(context, result)
}

pub(super) fn bind(context: &mut ImportContext<'_, '_>, fd: u64, addr: i32, addr_len: i32) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?.to_vec();
        host.with_raw_file(fd, |fd| sys::bind(fd, &addr))
    });
    zero_or_minus_one(context, result)
}

#[ported(source = "src/internal/event_loop/io_unix.c")]
#[cfg(unix)]
pub(super) fn recvfrom(
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    buf: i32,
    offset: i32,
    len: i32,
    addr: i32,
    addr_len: i32,
) -> i32 {
    let offset_buf = match checked_add_i32(buf, offset) {
        Ok(offset_buf) => offset_buf,
        Err(error) => {
            context.host.record_error(error);
            return -1;
        }
    };
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        memory.read_exact(offset_buf, len)?;
        let mut data = vec![0; usize::try_from(len).map_err(|_| AsyncHostError::Fault)?];
        let mut addr_data = memory.read_exact(addr, addr_len)?.to_vec();
        let n = host.with_raw_file(fd, |fd| sys::recvfrom(fd, &mut data, &mut addr_data))?;
        memory.write_exact(offset_buf, &data[..n])?;
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
    context: &mut ImportContext<'_, '_>,
    fd: u64,
    buf: i32,
    offset: i32,
    len: i32,
    addr: i32,
    addr_len: i32,
) -> i32 {
    let offset_buf = match checked_add_i32(buf, offset) {
        Ok(offset_buf) => offset_buf,
        Err(error) => {
            context.host.record_error(error);
            return -1;
        }
    };
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let data = memory.read_exact(offset_buf, len)?.to_vec();
        let addr = memory.read_exact(addr, addr_len)?.to_vec();
        host.with_raw_file(fd, |fd| sys::sendto(fd, &data, &addr))
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
pub(super) fn connect(context: &mut ImportContext<'_, '_>, fd: u64, addr: i32, addr_len: i32) -> i32 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let addr = memory.read_exact(addr, addr_len)?;
        host.with_raw_file(fd, |fd| sys::connect(fd, addr))
    });
    zero_or_minus_one(context, result)
}

#[ported(source = "src/internal/event_loop/io_unix.c")]
#[cfg(unix)]
pub(super) fn getsockerr(context: &mut ImportContext<'_, '_>, fd: u64) -> i32 {
    let host = context.host;
    match host.with_raw_file(fd, sys::getsockerr) {
        Ok(err) => err,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[ported(source = "src/internal/event_loop/io_unix.c")]
#[cfg(unix)]
pub(super) fn accept(context: &mut ImportContext<'_, '_>, fd: u64, addr: i32, addr_len: i32) -> u64 {
    let host = context.host;
    let result = context.with_memory_mut(|memory| {
        let addr = memory.read_exact_mut(addr, addr_len)?;
        host.with_raw_file(fd, |fd| sys::accept(fd, addr))
    });
    match result {
        Ok(fd) => context.host.insert_host_file(fd),
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
pub(super) fn connect_io_result(context: &mut ImportContext<'_, '_>, fd: u64, result: u64) -> i32 {
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
pub(super) fn setup_connected_socket(context: &mut ImportContext<'_, '_>, fd: u64) -> i32 {
    zero_or_minus_one(context, context.host.setup_connected_socket(fd))
}

#[ported(
    source = "src/internal/event_loop/io_windows.c",
    original = "moonbitlang_async_accept"
)]
#[cfg(windows)]
pub(super) fn accept_io_result(
    context: &mut ImportContext<'_, '_>,
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
    context: &mut ImportContext<'_, '_>,
    listen_fd: u64,
    accept_fd: u64,
) -> i32 {
    zero_or_minus_one(
        context,
        context.host.setup_accepted_socket(listen_fd, accept_fd),
    )
}

fn zero_or_minus_one(context: &mut ImportContext<'_, '_>, result: AsyncHostResult<()>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(error) => {
            context.host.record_error(error);
            -1
        }
    }
}

#[cfg(unix)]
fn checked_add_i32(lhs: i32, rhs: i32) -> AsyncHostResult<i32> {
    lhs.checked_add(rhs).ok_or(AsyncHostError::Fault)
}
}
