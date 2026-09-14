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

//! Network-owned Job state submitted to moonrun's shared thread pool.

mod runner;

use std::ffi::{OsStr, OsString};

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::resource::ResourceRef;

#[derive(Debug)]
pub(crate) struct Job {
    kind: Kind,
}

#[derive(Debug)]
enum Kind {
    Bind {
        socket: Option<ResourceRef>,
        addr: Vec<u8>,
    },
    GetAddrInfo {
        host: OsString,
        result: Option<Vec<Box<[u8]>>>,
    },
}

impl Job {
    pub(super) fn bind(socket: ResourceRef, addr: Vec<u8>) -> Self {
        Self {
            kind: Kind::Bind {
                socket: Some(socket),
                addr,
            },
        }
    }

    pub(super) fn getaddrinfo(host: OsString) -> Self {
        Self {
            kind: Kind::GetAddrInfo { host, result: None },
        }
    }

    pub(crate) fn run(&mut self) -> AsyncHostResult<i64> {
        match &mut self.kind {
            Kind::Bind { socket, addr } => match socket.take() {
                Some(socket) => runner::bind(&socket, addr),
                None => Err(AsyncHostError::Badf),
            },
            Kind::GetAddrInfo { host, result } => runner::getaddrinfo(host.clone(), result),
        }
    }

    pub(super) fn getaddrinfo_result(&self) -> AsyncHostResult<(&OsStr, &[Box<[u8]>])> {
        match &self.kind {
            Kind::GetAddrInfo {
                host,
                result: Some(result),
            } => Ok((host.as_os_str(), result)),
            Kind::GetAddrInfo { .. } => Err(AsyncHostError::Inval),
            Kind::Bind { .. } => Err(AsyncHostError::Badf),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getaddrinfo_result_is_unavailable_before_the_job_runs() {
        let job = Job::getaddrinfo(OsString::from("localhost"));

        assert_eq!(job.getaddrinfo_result(), Err(AsyncHostError::Inval));
    }
}
