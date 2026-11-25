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

//! Native DNS lookup results retain resolver status separately from OS errors.

use std::ffi::OsString;
#[cfg(unix)]
use std::ffi::{CStr, CString};
use std::io;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use windows_sys::Win32::Networking::WinSock as ws;

#[cfg(unix)]
use super::INVALID_INPUT;
#[cfg(unix)]
use crate::async_sys::ported_fns;

#[cfg(unix)]
pub(in crate::network) fn copy_sockaddrs_from_getaddrinfo(
    hostname: OsString,
) -> io::Result<(i32, Vec<Box<[u8]>>)> {
    let hostname = CString::new(hostname.into_vec())
        .map_err(|_| io::Error::from_raw_os_error(INVALID_INPUT))?;
    let mut hints = unsafe { std::mem::zeroed::<libc::addrinfo>() };
    hints.ai_flags = libc::AI_ADDRCONFIG;
    hints.ai_family = libc::AF_UNSPEC;

    let mut result = std::ptr::null_mut();
    let ret =
        unsafe { libc::getaddrinfo(hostname.as_ptr(), std::ptr::null(), &hints, &mut result) };
    if ret != 0 {
        if ret == libc::EAI_SYSTEM {
            return Err(io::Error::last_os_error());
        }
        return Ok((ret, Vec::new()));
    }

    let mut addrs = Vec::new();
    let mut current = result;
    while !current.is_null() {
        let addrinfo = unsafe { &*current };
        let len = match addrinfo.ai_family {
            libc::AF_INET => std::mem::size_of::<libc::sockaddr_in>(),
            libc::AF_INET6 => std::mem::size_of::<libc::sockaddr_in6>(),
            _ => {
                current = addrinfo.ai_next;
                continue;
            }
        };
        if !addrinfo.ai_addr.is_null() && addrinfo.ai_addrlen as usize >= len {
            let addr = unsafe { std::slice::from_raw_parts(addrinfo.ai_addr.cast::<u8>(), len) };
            addrs.push(addr.into());
        }
        current = addrinfo.ai_next;
    }
    unsafe {
        libc::freeaddrinfo(result);
    }
    Ok((0, addrs))
}

#[cfg(windows)]
pub(in crate::network) fn copy_sockaddrs_from_getaddrinfo(
    hostname: OsString,
) -> io::Result<(i32, Vec<Box<[u8]>>)> {
    let hostname: Vec<u16> = hostname.encode_wide().chain(std::iter::once(0)).collect();
    let mut hints = unsafe { std::mem::zeroed::<ws::ADDRINFOW>() };
    hints.ai_flags = ws::AI_ADDRCONFIG as i32;
    hints.ai_family = ws::AF_UNSPEC as i32;

    let mut result = std::ptr::null_mut();
    let ret = unsafe { ws::GetAddrInfoW(hostname.as_ptr(), std::ptr::null(), &hints, &mut result) };
    if ret != 0 {
        return match ret {
            ws::WSATRY_AGAIN
            | ws::WSANO_RECOVERY
            | ws::WSAEAFNOSUPPORT
            | ws::WSAHOST_NOT_FOUND
            | ws::WSATYPE_NOT_FOUND
            | ws::WSAESOCKTNOSUPPORT => Ok((ret, Vec::new())),
            _ => Err(io::Error::from_raw_os_error(ret)),
        };
    }

    let mut addrs = Vec::new();
    let mut current = result;
    while !current.is_null() {
        let addrinfo = unsafe { &*current };
        let len = match addrinfo.ai_family as u16 {
            ws::AF_INET => std::mem::size_of::<ws::SOCKADDR_IN>(),
            ws::AF_INET6 => std::mem::size_of::<ws::SOCKADDR_IN6>(),
            _ => {
                current = addrinfo.ai_next;
                continue;
            }
        };
        if !addrinfo.ai_addr.is_null() && addrinfo.ai_addrlen >= len {
            let addr = unsafe { std::slice::from_raw_parts(addrinfo.ai_addr.cast::<u8>(), len) };
            addrs.push(addr.into());
        }
        current = addrinfo.ai_next;
    }
    unsafe {
        ws::FreeAddrInfoW(result);
    }
    Ok((0, addrs))
}

#[cfg(unix)]
ported_fns! {
    #[ported(
        source = "src/internal/event_loop/network.wasm.mbt",
        original = "gai_strerror"
    )]
    #[cfg(unix)]
    pub(crate) fn gai_strerror(code: i32) -> CString {
        let ptr = unsafe { libc::gai_strerror(code) };
        unsafe { CStr::from_ptr(ptr) }.to_owned()
    }
}
