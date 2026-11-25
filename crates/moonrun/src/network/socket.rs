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

//! Socket creation, configuration and synchronous operations on Resources.

use super::os::{address, socket as sys};
use super::{HostNetwork, decode_ip_family, raw_socket, raw_socket_of_class};
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::resource::{Resource, ResourceClass};

impl HostNetwork {
    pub(crate) fn make_tcp_socket(&self, family: i32) -> AsyncHostResult<Resource> {
        let socket = sys::make_tcp_socket(decode_ip_family(family)?)?;
        Ok(Resource::tcp_socket(socket, family))
    }

    pub(crate) fn make_udp_socket(
        &self,
        family: i32,
        multicast: bool,
    ) -> AsyncHostResult<Resource> {
        let socket = sys::make_udp_socket(decode_ip_family(family)?, multicast)?;
        Ok(Resource::udp_socket(socket, family))
    }

    pub(crate) fn bind(&self, socket: &Resource, addr: &[u8]) -> AsyncHostResult<()> {
        self.check_bind(addr)?;
        sys::bind(raw_socket(socket)?, addr).map_err(Into::into)
    }

    pub(crate) fn listen(&self, socket: &Resource) -> AsyncHostResult<()> {
        let mut local_addr = vec![0; address::buffer_len()];
        let raw_socket = raw_socket_of_class(socket, ResourceClass::TcpSocket)?;
        let implicit_addr = match sys::getsockname(raw_socket, &mut local_addr) {
            Ok(()) if address::socket_addr(&local_addr)?.port() == 0 => Some(local_addr),
            Ok(()) => None,
            Err(error) => Some(listen_bind_addr_after_getsockname_error(
                socket,
                error.into(),
            )?),
        };
        if let Some(addr) = implicit_addr {
            self.check_bind(&addr)?;
        }
        sys::listen(raw_socket).map_err(Into::into)
    }

    pub(crate) fn connect_udp(&self, socket: &Resource, addr: &[u8]) -> AsyncHostResult<()> {
        self.check_connect(addr)?;
        sys::udp_client_connect(raw_socket_of_class(socket, ResourceClass::UdpSocket)?, addr)
            .map_err(Into::into)
    }

    #[cfg(unix)]
    pub(crate) fn connect_tcp(&self, socket: &Resource, addr: &[u8]) -> AsyncHostResult<()> {
        self.check_connect(addr)?;
        sys::connect(raw_socket_of_class(socket, ResourceClass::TcpSocket)?, addr)
            .map_err(Into::into)
    }

    #[cfg(unix)]
    pub(crate) fn recv_from(
        &self,
        socket: &Resource,
        data: &mut [u8],
        addr: &mut [u8],
    ) -> AsyncHostResult<usize> {
        sys::recvfrom(
            raw_socket_of_class(socket, ResourceClass::UdpSocket)?,
            data,
            addr,
        )
        .map_err(Into::into)
    }

    #[cfg(unix)]
    pub(crate) fn send_to(
        &self,
        socket: &Resource,
        data: &[u8],
        addr: &[u8],
    ) -> AsyncHostResult<usize> {
        self.check_connect(addr)?;
        sys::sendto(
            raw_socket_of_class(socket, ResourceClass::UdpSocket)?,
            data,
            addr,
        )
        .map_err(Into::into)
    }

    #[cfg(unix)]
    pub(crate) fn accept(&self, socket: &Resource, addr: &mut [u8]) -> AsyncHostResult<Resource> {
        let raw_socket = raw_socket_of_class(socket, ResourceClass::TcpSocket)?;
        let family = socket.socket_family().ok_or(AsyncHostError::Inval)?;
        let accepted = sys::accept(raw_socket, addr)?;
        Ok(Resource::tcp_socket(accepted, family))
    }

    pub(crate) fn join_multicast_group(
        &self,
        socket: &Resource,
        multi_addr: &[u8],
        local_addr: &[u8],
    ) -> AsyncHostResult<()> {
        sys::join_multicast_group(
            raw_socket_of_class(socket, ResourceClass::UdpSocket)?,
            multi_addr,
            local_addr,
        )
        .map_err(Into::into)
    }

    pub(crate) fn join_multicast_group_v6(
        &self,
        socket: &Resource,
        multi_addr: &[u8],
        interface_index: u32,
    ) -> AsyncHostResult<()> {
        sys::join_multicast_group_v6(
            raw_socket_of_class(socket, ResourceClass::UdpSocket)?,
            multi_addr,
            interface_index,
        )
        .map_err(Into::into)
    }

    pub(crate) fn set_multicast_interface(
        &self,
        socket: &Resource,
        local_addr: &[u8],
    ) -> AsyncHostResult<()> {
        sys::set_multicast_interface(
            raw_socket_of_class(socket, ResourceClass::UdpSocket)?,
            local_addr,
        )
        .map_err(Into::into)
    }

    pub(crate) fn set_multicast_interface_v6(
        &self,
        socket: &Resource,
        interface_index: u32,
    ) -> AsyncHostResult<()> {
        sys::set_multicast_interface_v6(
            raw_socket_of_class(socket, ResourceClass::UdpSocket)?,
            interface_index,
        )
        .map_err(Into::into)
    }

    pub(crate) fn set_multicast_ttl(
        &self,
        socket: &Resource,
        ttl: i32,
        family: i32,
    ) -> AsyncHostResult<()> {
        let socket = raw_socket_of_class(socket, ResourceClass::UdpSocket)?;
        sys::set_multicast_ttl(socket, ttl, decode_ip_family(family)?).map_err(Into::into)
    }

    pub(crate) fn set_multicast_loopback(
        &self,
        socket: &Resource,
        enable: bool,
        family: i32,
    ) -> AsyncHostResult<()> {
        let socket = raw_socket_of_class(socket, ResourceClass::UdpSocket)?;
        sys::set_multicast_loopback(socket, enable, decode_ip_family(family)?).map_err(Into::into)
    }

    pub(crate) fn disable_nagle(&self, socket: &Resource) -> AsyncHostResult<()> {
        sys::disable_nagle(raw_socket_of_class(socket, ResourceClass::TcpSocket)?)
            .map_err(Into::into)
    }

    pub(crate) fn allow_reuse_addr(&self, socket: &Resource) -> AsyncHostResult<()> {
        sys::allow_reuse_addr(raw_socket(socket)?).map_err(Into::into)
    }

    pub(crate) fn allow_reuse_port(&self, socket: &Resource) -> AsyncHostResult<()> {
        sys::allow_reuse_port(raw_socket(socket)?).map_err(Into::into)
    }

    pub(crate) fn set_ipv6_only(&self, socket: &Resource, ipv6_only: bool) -> AsyncHostResult<()> {
        sys::set_ipv6_only(raw_socket(socket)?, ipv6_only).map_err(Into::into)
    }

    pub(crate) fn enable_keepalive(
        &self,
        socket: &Resource,
        keep_idle: i32,
        keep_count: i32,
        keep_intvl: i32,
    ) -> AsyncHostResult<()> {
        sys::enable_keepalive(
            raw_socket_of_class(socket, ResourceClass::TcpSocket)?,
            keep_idle,
            keep_count,
            keep_intvl,
        )
        .map_err(Into::into)
    }

    pub(crate) fn getsockname(&self, socket: &Resource, addr: &mut [u8]) -> AsyncHostResult<()> {
        sys::getsockname(raw_socket(socket)?, addr).map_err(Into::into)
    }

    #[cfg(unix)]
    pub(crate) fn getsockerr(&self, socket: &Resource) -> AsyncHostResult<i32> {
        sys::getsockerr(raw_socket(socket)?).map_err(Into::into)
    }
}

#[cfg(unix)]
fn listen_bind_addr_after_getsockname_error(
    _socket: &Resource,
    error: AsyncHostError,
) -> AsyncHostResult<Vec<u8>> {
    Err(error)
}

#[cfg(windows)]
fn listen_bind_addr_after_getsockname_error(
    socket: &Resource,
    _error: AsyncHostError,
) -> AsyncHostResult<Vec<u8>> {
    let family = socket.socket_family().ok_or(AsyncHostError::Inval)?;
    match family {
        4 => {
            let mut addr = vec![0; address::ipv4_addr_size()];
            address::init_ip_addr(&mut addr, 0, 0)?;
            Ok(addr)
        }
        6 => {
            let mut addr = vec![0; address::ipv6_addr_size()];
            address::init_ipv6_addr(&mut addr, &[0; 16], 0, 0)?;
            Ok(addr)
        }
        _ => Err(AsyncHostError::Inval),
    }
}
