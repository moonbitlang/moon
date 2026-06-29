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

use super::{HostFileTable, HostHandle};

ported_fns! {
    #[ported(
        source = "src/internal/event_loop/thread_pool.c",
        original = "bind_job_worker"
    )]
        pub(super) fn run_bind_job(
        files: &mut impl HostFileTable,
        socket: HostHandle,
        addr: &[u8],
    ) -> AsyncHostResult<i64> {
        #[cfg(unix)]
        {
            files.with_raw_file(socket, |socket| {
                crate::async_sys::socket::bind(socket, addr)?;
                Ok(0)
            })
        }
        #[cfg(windows)]
        {
            // `with_raw_file` duplicates with DuplicateHandle on Windows, which
            // does not produce a valid duplicate Winsock SOCKET.
            files.with_borrowed_raw_file(socket, |socket| {
                crate::async_sys::socket::bind(socket, addr)?;
                Ok(0)
            })
        }
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

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::async_host::AsyncHostResult;
    use crate::async_sys::internal::event_loop::thread_pool::HostFile;
    use crate::async_sys::internal::fd_util::stub::RawFd;

    struct TrackingFiles {
        borrowed: bool,
    }

    impl HostFileTable for TrackingFiles {
        fn insert_file(&mut self, _file: RawFd) -> AsyncHostResult<HostHandle> {
            unreachable!("bind job test does not insert files")
        }

        fn is_invalid_file_handle(&self, _handle: HostHandle) -> bool {
            unreachable!("bind job test does not query file validity")
        }

        fn with_borrowed_raw_file<T>(
            &mut self,
            _handle: HostHandle,
            f: impl FnOnce(RawFd) -> AsyncHostResult<T>,
        ) -> AsyncHostResult<T> {
            self.borrowed = true;
            f(windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE)
        }

        fn with_raw_file<T>(
            &mut self,
            _handle: HostHandle,
            _f: impl FnOnce(RawFd) -> AsyncHostResult<T>,
        ) -> AsyncHostResult<T> {
            panic!("Windows socket bind jobs must not duplicate SOCKET handles")
        }

        fn with_host_file_mut<T>(
            &mut self,
            _handle: HostHandle,
            _f: impl FnOnce(&mut HostFile) -> AsyncHostResult<T>,
        ) -> AsyncHostResult<T> {
            unreachable!("bind job test does not mutate host files")
        }
    }

    #[test]
    fn windows_bind_job_borrows_socket_handle() {
        let mut files = TrackingFiles { borrowed: false };
        let result = run_bind_job(&mut files, 1, &[]);
        assert!(result.is_err());
        assert!(files.borrowed);
    }
}
