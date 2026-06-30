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

use std::ffi::OsString;

use crate::async_host::AsyncHostResult;
use crate::async_sys::ported_fns;

use super::FileResource;

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "bind_job_worker"
    )]
        pub(super) fn run_bind_job(
        socket: &FileResource,
        addr: &[u8],
    ) -> AsyncHostResult<i64> {
        crate::async_sys::socket::bind(socket.raw_fd(), addr)?;
        Ok(0)
    }

    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "getaddrinfo_job_worker"
    )]
        pub(super) fn run_getaddrinfo_job(
        host: OsString,
        result: &mut Option<Vec<Box<[u8]>>>,
    ) -> AsyncHostResult<i64> {
        let (ret, addrs) = crate::async_sys::socket::copy_sockaddrs_from_getaddrinfo(host)?;
        *result = Some(addrs);
        Ok(i64::from(ret))
    }
}
