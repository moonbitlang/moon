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

//! Native-shaped worker operations for Network Jobs.

use std::ffi::OsString;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawSocket;

use crate::async_host::AsyncHostResult;
use crate::resource::Resource;

pub(super) fn bind(socket: &Resource, addr: &[u8]) -> AsyncHostResult<i64> {
    #[cfg(unix)]
    let socket = socket.as_fd()?.as_raw_fd();
    #[cfg(windows)]
    let socket = socket.as_socket()?.as_raw_socket();
    crate::async_sys::socket::bind(socket, addr)?;
    Ok(0)
}

pub(super) fn getaddrinfo(
    host: OsString,
    result: &mut Option<Vec<Box<[u8]>>>,
) -> AsyncHostResult<i64> {
    let (ret, addrs) = crate::async_sys::socket::copy_sockaddrs_from_getaddrinfo(host)?;
    *result = Some(addrs);
    Ok(i64::from(ret))
}
