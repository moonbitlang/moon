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

//! DNS authorization and published address-list ownership.

use std::ffi::{OsStr, OsString};

use slotmap::{Key, KeyData};

use super::{HostNetwork, Job, os::address};
use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::runtime::null_handle;

#[derive(Debug)]
pub(super) struct HostAddrInfo {
    addr: Box<[u8]>,
    next: Option<u64>,
}

impl HostNetwork {
    pub(crate) fn make_getaddrinfo_job(&self, host: OsString) -> AsyncHostResult<Job> {
        self.check_dns(&host)?;
        Ok(Job::getaddrinfo(host))
    }

    pub(crate) fn publish_getaddrinfo_result(&self, job: &Job) -> AsyncHostResult<u64> {
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
        // Each publication owns a fresh list, independent of the Job and any
        // earlier publication. Freeing the list does not revoke DNS authority.
        let mut addr_infos = self.addr_infos.borrow_mut();
        let mut next = None;
        for addr in addrs.iter().rev() {
            let key = addr_infos.insert(HostAddrInfo {
                addr: addr.clone(),
                next,
            });
            next = Some(key.data().as_ffi());
        }
        Ok(next.unwrap_or_else(null_handle))
    }

    pub(crate) fn addrinfo_next(&self, handle: u64) -> AsyncHostResult<u64> {
        if handle == null_handle() {
            return Ok(null_handle());
        }
        let key = KeyData::from_ffi(handle).into();
        let addr_infos = self.addr_infos.borrow();
        let addrinfo = addr_infos.get(key).ok_or(AsyncHostError::Badf)?;
        Ok(addrinfo.next.unwrap_or_else(null_handle))
    }

    pub(crate) fn addrinfo_addr(&self, handle: u64) -> AsyncHostResult<Box<[u8]>> {
        let key = KeyData::from_ffi(handle).into();
        let addr_infos = self.addr_infos.borrow();
        let addrinfo = addr_infos.get(key).ok_or(AsyncHostError::Badf)?;
        Ok(addrinfo.addr.clone())
    }

    pub(crate) fn free_addrinfo(&self, handle: u64) -> AsyncHostResult<()> {
        if handle == null_handle() {
            return Ok(());
        }
        let mut addr_infos = self.addr_infos.borrow_mut();
        let mut current = Some(handle);
        while let Some(handle) = current {
            let addrinfo = addr_infos
                .remove(KeyData::from_ffi(handle).into())
                .ok_or(AsyncHostError::Badf)?;
            current = addrinfo.next;
        }
        Ok(())
    }

    fn check_dns(&self, host: &OsStr) -> AsyncHostResult<()> {
        self.policy
            .as_ref()
            .map_or(Ok(()), |policy| policy.check_dns(host))
    }

    pub(crate) fn append_leaks(&self, leaks: &mut Vec<String>) {
        let addr_infos = self.addr_infos.borrow();
        if !addr_infos.is_empty() {
            leaks.push(format!("addr_infos={}", addr_infos.len()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::runtime::{Handles, HostKeys, HostResourceKind};

    #[test]
    fn published_lists_outlive_the_job_and_retire_independently() {
        #[cfg(windows)]
        assert_eq!(crate::network::os::init_wsa(), 0);

        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let network = HostNetwork::new(None, keys.clone());
        let mut buffers = Handles::new(keys.clone(), HostResourceKind::CBuffer);
        let buffer = buffers.insert(());
        let mut job = network
            .make_getaddrinfo_job(OsString::from("127.0.0.1"))
            .unwrap();
        assert_eq!(job.run().unwrap(), 0);
        let expected = job.getaddrinfo_result().unwrap().1.to_vec();
        assert!(!expected.is_empty());

        let first = network.publish_getaddrinfo_result(&job).unwrap();
        let second = network.publish_getaddrinfo_result(&job).unwrap();
        assert_ne!(first, second);
        drop(job);

        let wrong_kind = buffer.data().as_ffi();
        assert_eq!(network.addrinfo_addr(wrong_kind), Err(AsyncHostError::Badf));
        assert_eq!(network.addrinfo_next(wrong_kind), Err(AsyncHostError::Badf));
        assert_eq!(network.free_addrinfo(wrong_kind), Err(AsyncHostError::Badf));
        assert_eq!(buffers.get(KeyData::from_ffi(first).into()), None);

        let mut first_handles = Vec::new();
        let mut current = first;
        for addr in &expected {
            assert_eq!(&network.addrinfo_addr(current).unwrap(), addr);
            first_handles.push(current);
            current = network.addrinfo_next(current).unwrap();
        }
        assert_eq!(current, null_handle());
        network.free_addrinfo(first).unwrap();
        let replacement = buffers.insert(());
        for handle in first_handles {
            assert_eq!(network.addrinfo_addr(handle), Err(AsyncHostError::Badf));
            assert_eq!(network.addrinfo_next(handle), Err(AsyncHostError::Badf));
            assert_eq!(network.free_addrinfo(handle), Err(AsyncHostError::Badf));
        }

        let mut second_keys = Vec::new();
        current = second;
        for addr in &expected {
            assert_eq!(&network.addrinfo_addr(current).unwrap(), addr);
            second_keys.push(KeyData::from_ffi(current).into());
            current = network.addrinfo_next(current).unwrap();
        }
        assert_eq!(current, null_handle());
        let mut leaks = Vec::new();
        network.append_leaks(&mut leaks);
        assert_eq!(leaks, [format!("addr_infos={}", expected.len())]);
        drop(network);
        for key in second_keys {
            assert_eq!(keys.borrow().kind(key), None);
        }
        assert_eq!(buffers.get(buffer), Some(&()));
        assert_eq!(buffers.get(replacement), Some(&()));

        #[cfg(windows)]
        assert_eq!(crate::network::os::cleanup_wsa(), 0);
    }

    #[test]
    fn publication_records_authority_that_survives_list_free() {
        #[cfg(windows)]
        assert_eq!(crate::network::os::init_wsa(), 0);

        let dir = tempfile::tempdir().unwrap();
        let policy_file = dir.path().join("network.toml");
        std::fs::write(&policy_file, "[net]\nconnect = [\"localhost:443\"]\n").unwrap();
        let (policy, _) = crate::policy::load_file(&policy_file).unwrap();
        let first = HostNetwork::new(Some(policy.network()), Rc::default());
        let second = HostNetwork::new(Some(policy.network()), Rc::default());
        let mut job = first
            .make_getaddrinfo_job(OsString::from("localhost"))
            .unwrap();
        assert_eq!(job.run().unwrap(), 0);
        let addr = &job.getaddrinfo_result().unwrap().1[0];
        let mut connect_addr = vec![0; addr.len()];
        address::addrinfo_fill_addr(addr, &mut connect_addr, 443).unwrap();
        assert_eq!(
            first.check_connect(&connect_addr),
            Err(AsyncHostError::PermissionDenied)
        );

        let list = first.publish_getaddrinfo_result(&job).unwrap();
        assert_eq!(first.check_connect(&connect_addr), Ok(()));
        first.free_addrinfo(list).unwrap();
        assert_eq!(first.check_connect(&connect_addr), Ok(()));
        assert_eq!(
            second.check_connect(&connect_addr),
            Err(AsyncHostError::PermissionDenied)
        );

        #[cfg(windows)]
        assert_eq!(crate::network::os::cleanup_wsa(), 0);
    }
}
