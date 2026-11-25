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

//! Native networking mechanisms. Runtime authority, guest encoding and async
//! request ownership belong to their callers; native failures use `io::Error`.

pub(crate) mod address;
pub(crate) mod dns;
pub(crate) mod interface;
#[cfg(windows)]
pub(super) mod overlapped;
pub(crate) mod socket;

#[cfg(unix)]
pub(crate) type RawSocket = std::os::fd::RawFd;
#[cfg(windows)]
pub(crate) type RawSocket = std::os::windows::io::RawSocket;

#[cfg(unix)]
const INVALID_INPUT: i32 = libc::EINVAL;
#[cfg(windows)]
const INVALID_INPUT: i32 = windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER as i32;
#[cfg(unix)]
const INVALID_ADDRESS: i32 = libc::EFAULT;
#[cfg(windows)]
const INVALID_ADDRESS: i32 = windows_sys::Win32::Foundation::ERROR_INVALID_ADDRESS as i32;

#[derive(Clone, Copy, Debug)]
pub(crate) enum IpFamily {
    V4,
    V6,
}

impl IpFamily {
    fn as_raw(self) -> i32 {
        #[cfg(unix)]
        match self {
            Self::V4 => libc::AF_INET,
            Self::V6 => libc::AF_INET6,
        }
        #[cfg(windows)]
        match self {
            Self::V4 => windows_sys::Win32::Networking::WinSock::AF_INET as i32,
            Self::V6 => windows_sys::Win32::Networking::WinSock::AF_INET6 as i32,
        }
    }
}

#[cfg(windows)]
fn last_wsa_error() -> std::io::Error {
    let errno = unsafe { windows_sys::Win32::Networking::WinSock::WSAGetLastError() };
    unsafe {
        windows_sys::Win32::Foundation::SetLastError(errno as u32);
    }
    std::io::Error::from_raw_os_error(errno)
}
