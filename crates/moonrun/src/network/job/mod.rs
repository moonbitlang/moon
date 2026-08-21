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
