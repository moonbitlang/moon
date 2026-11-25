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

//! Wasm-backend-neutral network operations owned by one moonrun Runtime.

mod job;
#[cfg(windows)]
mod overlapped;
mod socket;

#[cfg(windows)]
pub(crate) use os::overlapped::AcceptBuffer;
pub(crate) mod os;

use std::cell::RefCell;
use std::ffi::{OsStr, OsString};
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawSocket;

use self::os::IpFamily;
use self::os::address;
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::policy::{NetOperation, NetPolicy, ResolvedConnectRule};
use crate::resource::{Resource, ResourceClass, ResourceRef};

pub(crate) use job::Job;

/// Permission-backed host networking shared by imports and Jobs.
///
/// `AsyncHost` owns this module together with Resource Handles and asynchronous
/// lifecycle. This module concentrates network authorization and the
/// policy-bearing socket operations that must not depend on a Wasm engine or
/// its guest-memory representation.
#[derive(Debug)]
pub(crate) struct HostNetwork {
    policy: Option<NetPolicy>,
    // DNS results are Runtime provenance, not part of the cloneable policy.
    resolved_connect: RefCell<Vec<ResolvedConnectRule>>,
}

impl HostNetwork {
    pub(crate) fn new(policy: Option<NetPolicy>) -> Self {
        Self {
            policy,
            resolved_connect: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn make_bind_job(&self, socket: ResourceRef, addr: Vec<u8>) -> AsyncHostResult<Job> {
        self.check_bind(&addr)?;
        Ok(Job::bind(socket, addr))
    }

    pub(crate) fn make_getaddrinfo_job(&self, host: OsString) -> AsyncHostResult<Job> {
        self.check_dns(&host)?;
        Ok(Job::getaddrinfo(host))
    }

    pub(crate) fn getaddrinfo_result(&self, job: &Job) -> AsyncHostResult<Vec<Box<[u8]>>> {
        let (host, addrs) = job.getaddrinfo_result()?;
        if let Some(policy) = &self.policy {
            self.resolved_connect
                .borrow_mut()
                .extend(policy.resolved_connect_rules(
                    host,
                    addrs.iter().map(|addr| {
                        address::socket_addr(addr)
                            .map(|addr| addr.ip())
                            .map_err(Into::into)
                    }),
                )?);
        }
        Ok(addrs.to_vec())
    }

    fn check_bind(&self, addr: &[u8]) -> AsyncHostResult<()> {
        self.policy.as_ref().map_or(Ok(()), |policy| {
            policy.check_socket(
                NetOperation::Bind,
                address::socket_addr(addr)?,
                &self.resolved_connect.borrow(),
            )
        })
    }

    fn check_connect(&self, addr: &[u8]) -> AsyncHostResult<()> {
        self.policy.as_ref().map_or(Ok(()), |policy| {
            policy.check_socket(
                NetOperation::Connect,
                address::socket_addr(addr)?,
                &self.resolved_connect.borrow(),
            )
        })
    }

    fn check_dns(&self, host: &OsStr) -> AsyncHostResult<()> {
        self.policy
            .as_ref()
            .map_or(Ok(()), |policy| policy.check_dns(host))
    }
}

/// Decode the async IP family selector before invoking native socket operations.
fn decode_ip_family(family: i32) -> AsyncHostResult<IpFamily> {
    match family {
        4 => Ok(IpFamily::V4),
        6 => Ok(IpFamily::V6),
        _ => Err(AsyncHostError::Inval),
    }
}

#[cfg(unix)]
fn raw_socket(resource: &Resource) -> AsyncHostResult<os::RawSocket> {
    if !resource.resource_class().is_socket() {
        return Err(AsyncHostError::Inval);
    }
    Ok(resource.as_fd()?.as_raw_fd())
}

#[cfg(windows)]
fn raw_socket(resource: &Resource) -> AsyncHostResult<os::RawSocket> {
    if !resource.resource_class().is_socket() {
        return Err(AsyncHostError::Inval);
    }
    Ok(resource.as_socket()?.as_raw_socket())
}

fn raw_socket_of_class(
    resource: &Resource,
    class: ResourceClass,
) -> AsyncHostResult<os::RawSocket> {
    if resource.resource_class() != class {
        return Err(AsyncHostError::Inval);
    }
    raw_socket(resource)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::policy::{self, Policy};

    fn network(policy: Option<Policy>) -> HostNetwork {
        HostNetwork::new(policy.as_ref().map(Policy::network))
    }

    #[test]
    fn address_validation_stays_inside_policy_checks() {
        let unrestricted = network(None);
        assert_eq!(unrestricted.check_bind(&[]), Ok(()));
        assert_eq!(unrestricted.check_connect(&[]), Ok(()));

        let dir = tempfile::tempdir().unwrap();
        let policy_file = dir.path().join("deny-all.toml");
        std::fs::write(&policy_file, "").unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let restricted = network(Some(policy));
        assert_eq!(
            restricted.check_bind(&[]).unwrap_err().errno(),
            AsyncHostError::Fault.errno()
        );
        assert_eq!(
            restricted.check_connect(&[]).unwrap_err().errno(),
            AsyncHostError::Fault.errno()
        );

        let mut addr = vec![0; address::ipv6_addr_size()];
        address::init_ipv6_addr(&mut addr, &std::net::Ipv6Addr::LOCALHOST.octets(), 443, 0)
            .unwrap();
        // Address interpretation and authorization still precede Resource validation.
        assert_eq!(
            restricted.bind(&Resource::invalid(), &addr),
            Err(AsyncHostError::PermissionDenied)
        );
    }

    #[test]
    fn tcp_and_udp_operations_reject_the_other_resource_class() {
        #[cfg(windows)]
        assert_eq!(crate::network::os::init_wsa(), 0);

        let network = network(None);
        let tcp = network.make_tcp_socket(4).unwrap();
        let udp = network.make_udp_socket(4, false).unwrap();
        let mut addr = vec![0; address::ipv4_addr_size()];
        address::init_ip_addr(&mut addr, 0x7f000001, 1234).unwrap();

        assert_eq!(network.listen(&udp), Err(AsyncHostError::Inval));
        assert_eq!(network.connect_udp(&tcp, &addr), Err(AsyncHostError::Inval));

        drop((tcp, udp));
        #[cfg(windows)]
        assert_eq!(crate::network::os::cleanup_wsa(), 0);
    }

    #[test]
    fn bind_propagates_socket_errors() {
        #[cfg(windows)]
        assert_eq!(crate::network::os::init_wsa(), 0);

        let network = network(None);
        let first = network.make_tcp_socket(4).unwrap();
        let second = network.make_tcp_socket(4).unwrap();
        let mut addr = vec![0; address::ipv4_addr_size()];
        address::init_ip_addr(&mut addr, 0x7f000001, 0).unwrap();
        network.bind(&first, &addr).unwrap();
        network.getsockname(&first, &mut addr).unwrap();

        assert!(network.bind(&second, &addr).is_err());

        drop((first, second));
        #[cfg(windows)]
        assert_eq!(crate::network::os::cleanup_wsa(), 0);
    }

    #[test]
    fn listen_checks_bind_policy_for_unbound_socket() {
        #[cfg(windows)]
        assert_eq!(crate::network::os::init_wsa(), 0);

        let dir = tempfile::tempdir().unwrap();
        let policy_file = dir.path().join("deny-all.toml");
        std::fs::write(&policy_file, "").unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let network = network(Some(policy));
        let socket = network.make_tcp_socket(4).unwrap();

        assert_eq!(
            network.listen(&socket),
            Err(AsyncHostError::PermissionDenied)
        );

        drop(socket);
        #[cfg(windows)]
        assert_eq!(crate::network::os::cleanup_wsa(), 0);
    }

    #[test]
    fn job_preparation_checks_network_policy() {
        let dir = tempfile::tempdir().unwrap();
        let policy_file = dir.path().join("deny-all.toml");
        std::fs::write(&policy_file, "").unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let network = network(Some(policy));
        let mut addr = vec![0; address::ipv4_addr_size()];
        address::init_ip_addr(&mut addr, 0x7f000001, 1234).unwrap();

        assert_eq!(
            network
                .make_bind_job(Arc::new(Resource::invalid()), addr)
                .unwrap_err(),
            AsyncHostError::PermissionDenied
        );
        assert_eq!(
            network
                .make_getaddrinfo_job(OsString::from("localhost"))
                .unwrap_err(),
            AsyncHostError::PermissionDenied
        );
    }

    #[test]
    fn resolved_connect_authority_is_local_to_host_network() {
        let dir = tempfile::tempdir().unwrap();
        let policy_file = dir.path().join("network.toml");
        std::fs::write(&policy_file, "[net]\nconnect = [\"api.example.com:443\"]\n").unwrap();
        let (policy, _) = policy::load_file(&policy_file).unwrap();
        let policy = policy.network();
        let first = HostNetwork::new(Some(policy.clone()));
        let second = HostNetwork::new(Some(policy));
        let mut resolved_addr = vec![0; address::ipv4_addr_size()];
        address::init_ip_addr(&mut resolved_addr, 0x7f000001, 0).unwrap();
        let mut connect_addr = vec![0; address::ipv4_addr_size()];
        address::init_ip_addr(&mut connect_addr, 0x7f000001, 443).unwrap();

        let rules = first
            .policy
            .as_ref()
            .unwrap()
            .resolved_connect_rules(
                OsStr::new("api.example.com"),
                [address::socket_addr(&resolved_addr)
                    .map(|addr| addr.ip())
                    .map_err(Into::into)],
            )
            .unwrap();
        first.resolved_connect.borrow_mut().extend(rules);

        first.check_connect(&connect_addr).unwrap();
        assert_eq!(
            second.check_connect(&connect_addr),
            Err(AsyncHostError::PermissionDenied)
        );
    }
}
