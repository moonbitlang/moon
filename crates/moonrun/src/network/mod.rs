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

//! Runtime-engine-neutral network operations for one moonrun Host.

use std::ffi::OsStr;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawSocket;
use std::sync::Arc;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::async_sys::socket as sys;
use crate::policy::Policy;
use crate::resource::{Resource, ResourceClass};

/// Permission-backed host networking shared by synchronous imports and Jobs.
///
/// `AsyncHost` owns this module together with Resource Handles and asynchronous
/// lifecycle. This module concentrates network authorization and the
/// policy-bearing socket operations that must not depend on a Wasm engine or
/// its guest-memory representation.
#[derive(Debug)]
pub(crate) struct HostNetwork {
    policy: Arc<Policy>,
}

impl HostNetwork {
    pub(crate) fn new(policy: Arc<Policy>) -> Self {
        Self { policy }
    }

    pub(crate) fn make_tcp_socket(&self, family: i32) -> AsyncHostResult<Resource> {
        let socket = sys::make_tcp_socket(family)?;
        Ok(Resource::tcp_socket(socket, family))
    }

    pub(crate) fn make_udp_socket(
        &self,
        family: i32,
        multicast: bool,
    ) -> AsyncHostResult<Resource> {
        let socket = sys::make_udp_socket(family, multicast)?;
        Ok(Resource::udp_socket(socket, family))
    }

    pub(crate) fn bind(&self, socket: &Resource, addr: &[u8]) -> AsyncHostResult<()> {
        self.check_bind(addr)?;
        sys::bind(raw_socket(socket)?, addr)
    }

    pub(crate) fn listen(&self, socket: &Resource) -> AsyncHostResult<()> {
        let mut local_addr = vec![0; socket_addr_buffer_len()];
        let raw_socket = raw_socket_of_class(socket, ResourceClass::TcpSocket)?;
        let implicit_addr = match sys::getsockname(raw_socket, &mut local_addr) {
            Ok(()) if socket_addr_port(&local_addr)? == 0 => Some(local_addr),
            Ok(()) => None,
            Err(error) => Some(listen_bind_addr_after_getsockname_error(socket, error)?),
        };
        if let Some(addr) = implicit_addr {
            self.check_bind(&addr)?;
        }
        sys::listen(raw_socket)
    }

    pub(crate) fn connect_udp(&self, socket: &Resource, addr: &[u8]) -> AsyncHostResult<()> {
        self.check_connect(addr)?;
        sys::udp_client_connect(raw_socket_of_class(socket, ResourceClass::UdpSocket)?, addr)
    }

    #[cfg(unix)]
    pub(crate) fn connect_tcp(&self, socket: &Resource, addr: &[u8]) -> AsyncHostResult<()> {
        self.check_connect(addr)?;
        sys::connect(raw_socket_of_class(socket, ResourceClass::TcpSocket)?, addr)
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
    }

    #[cfg(unix)]
    pub(crate) fn accept(&self, socket: &Resource, addr: &mut [u8]) -> AsyncHostResult<Resource> {
        let raw_socket = raw_socket_of_class(socket, ResourceClass::TcpSocket)?;
        let family = socket.socket_family().ok_or(AsyncHostError::Inval)?;
        let accepted = sys::accept(raw_socket, addr)?;
        Ok(Resource::tcp_socket(accepted, family))
    }

    pub(crate) fn check_bind(&self, addr: &[u8]) -> AsyncHostResult<()> {
        self.policy.check_bind(addr)
    }

    pub(crate) fn check_connect(&self, addr: &[u8]) -> AsyncHostResult<()> {
        self.policy.check_connect(addr)
    }

    pub(crate) fn check_dns(&self, host: &OsStr) -> AsyncHostResult<()> {
        self.policy.check_dns(host)
    }

    pub(crate) fn register_dns_result(
        &self,
        host: &OsStr,
        addrs: &[Box<[u8]>],
    ) -> AsyncHostResult<()> {
        self.policy.register_dns_result(host, addrs)
    }
}

#[cfg(unix)]
fn raw_socket(resource: &Resource) -> AsyncHostResult<sys::RawSocket> {
    if !resource.resource_class().is_socket() {
        return Err(AsyncHostError::Inval);
    }
    Ok(resource.as_fd()?.as_raw_fd())
}

#[cfg(windows)]
fn raw_socket(resource: &Resource) -> AsyncHostResult<sys::RawSocket> {
    if !resource.resource_class().is_socket() {
        return Err(AsyncHostError::Inval);
    }
    Ok(resource.as_socket()?.as_raw_socket())
}

fn raw_socket_of_class(
    resource: &Resource,
    class: ResourceClass,
) -> AsyncHostResult<sys::RawSocket> {
    if resource.resource_class() != class {
        return Err(AsyncHostError::Inval);
    }
    raw_socket(resource)
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
            let mut addr =
                vec![0; usize::try_from(sys::ipv4_addr_size()).map_err(|_| AsyncHostError::Fault)?];
            sys::init_ip_addr(&mut addr, 0, 0)?;
            Ok(addr)
        }
        6 => {
            let mut addr =
                vec![0; usize::try_from(sys::ipv6_addr_size()).map_err(|_| AsyncHostError::Fault)?];
            sys::init_ipv6_addr(&mut addr, &[0; 16], 0, 0)?;
            Ok(addr)
        }
        _ => Err(AsyncHostError::Inval),
    }
}

#[cfg(unix)]
fn socket_addr_buffer_len() -> usize {
    std::mem::size_of::<libc::sockaddr_storage>()
}

#[cfg(windows)]
fn socket_addr_buffer_len() -> usize {
    use windows_sys::Win32::Networking::WinSock as ws;

    std::mem::size_of::<ws::SOCKADDR_IN6>()
}

#[cfg(unix)]
fn socket_addr_port(addr: &[u8]) -> AsyncHostResult<u16> {
    if addr.len() < std::mem::size_of::<libc::sockaddr>() {
        return Err(AsyncHostError::Fault);
    }
    let family = unsafe { addr.as_ptr().cast::<libc::sockaddr>().read_unaligned() }.sa_family;
    match i32::from(family) {
        libc::AF_INET => {
            if addr.len() < std::mem::size_of::<libc::sockaddr_in>() {
                return Err(AsyncHostError::Fault);
            }
            let addr = unsafe { addr.as_ptr().cast::<libc::sockaddr_in>().read_unaligned() };
            Ok(u16::from_be(addr.sin_port))
        }
        libc::AF_INET6 => {
            if addr.len() < std::mem::size_of::<libc::sockaddr_in6>() {
                return Err(AsyncHostError::Fault);
            }
            let addr = unsafe { addr.as_ptr().cast::<libc::sockaddr_in6>().read_unaligned() };
            Ok(u16::from_be(addr.sin6_port))
        }
        _ => Err(AsyncHostError::Inval),
    }
}

#[cfg(windows)]
fn socket_addr_port(addr: &[u8]) -> AsyncHostResult<u16> {
    use windows_sys::Win32::Networking::WinSock as ws;

    if addr.len() < std::mem::size_of::<ws::SOCKADDR>() {
        return Err(AsyncHostError::Fault);
    }
    let family = unsafe { addr.as_ptr().cast::<ws::SOCKADDR>().read_unaligned() }.sa_family;
    match family {
        ws::AF_INET => {
            if addr.len() < std::mem::size_of::<ws::SOCKADDR_IN>() {
                return Err(AsyncHostError::Fault);
            }
            let addr = unsafe { addr.as_ptr().cast::<ws::SOCKADDR_IN>().read_unaligned() };
            Ok(u16::from_be(addr.sin_port))
        }
        ws::AF_INET6 => {
            if addr.len() < std::mem::size_of::<ws::SOCKADDR_IN6>() {
                return Err(AsyncHostError::Fault);
            }
            let addr = unsafe { addr.as_ptr().cast::<ws::SOCKADDR_IN6>().read_unaligned() };
            Ok(u16::from_be(addr.sin6_port))
        }
        _ => Err(AsyncHostError::Inval),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_addr_port_reads_ipv4_port() {
        let mut addr = vec![0; usize::try_from(sys::ipv4_addr_size()).unwrap()];

        sys::init_ip_addr(&mut addr, 0x7f000001, 1234).unwrap();

        assert_eq!(socket_addr_port(&addr).unwrap(), 1234);
    }

    #[test]
    fn tcp_and_udp_operations_reject_the_other_resource_class() {
        #[cfg(windows)]
        assert_eq!(crate::async_sys::internal::event_loop::io::init_wsa(), 0);

        let network = HostNetwork::new(Arc::new(Policy::allow_all()));
        let tcp = network.make_tcp_socket(4).unwrap();
        let udp = network.make_udp_socket(4, false).unwrap();
        let mut addr = vec![0; usize::try_from(sys::ipv4_addr_size()).unwrap()];
        sys::init_ip_addr(&mut addr, 0x7f000001, 1234).unwrap();

        assert_eq!(network.listen(&udp), Err(AsyncHostError::Inval));
        assert_eq!(network.connect_udp(&tcp, &addr), Err(AsyncHostError::Inval));

        drop((tcp, udp));
        #[cfg(windows)]
        assert_eq!(crate::async_sys::internal::event_loop::io::cleanup_wsa(), 0);
    }

    #[test]
    fn bind_propagates_socket_errors() {
        #[cfg(windows)]
        assert_eq!(crate::async_sys::internal::event_loop::io::init_wsa(), 0);

        let network = HostNetwork::new(Arc::new(Policy::allow_all()));
        let first = network.make_tcp_socket(4).unwrap();
        let second = network.make_tcp_socket(4).unwrap();
        let mut addr = vec![0; usize::try_from(sys::ipv4_addr_size()).unwrap()];
        sys::init_ip_addr(&mut addr, 0x7f000001, 0).unwrap();
        network.bind(&first, &addr).unwrap();
        sys::getsockname(raw_socket(&first).unwrap(), &mut addr).unwrap();

        assert!(network.bind(&second, &addr).is_err());

        drop((first, second));
        #[cfg(windows)]
        assert_eq!(crate::async_sys::internal::event_loop::io::cleanup_wsa(), 0);
    }

    #[test]
    fn listen_checks_bind_policy_for_unbound_socket() {
        #[cfg(windows)]
        assert_eq!(crate::async_sys::internal::event_loop::io::init_wsa(), 0);

        let dir = tempfile::tempdir().unwrap();
        let policy_file = dir.path().join("deny-all.toml");
        std::fs::write(&policy_file, "").unwrap();
        let network = HostNetwork::new(Arc::new(Policy::from_file(&policy_file).unwrap()));
        let socket = network.make_tcp_socket(4).unwrap();

        assert_eq!(
            network.listen(&socket),
            Err(AsyncHostError::PermissionDenied)
        );

        drop(socket);
        #[cfg(windows)]
        assert_eq!(crate::async_sys::internal::event_loop::io::cleanup_wsa(), 0);
    }
}
