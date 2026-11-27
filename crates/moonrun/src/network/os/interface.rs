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

//! Native network interface names and indices.

#[cfg(unix)]
use std::ffi::{CStr, CString};
use std::ffi::{OsStr, OsString};
use std::io;
#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};
#[cfg(windows)]
use std::os::windows::ffi::{OsStrExt, OsStringExt};
#[cfg(windows)]
use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR, SetLastError, WIN32_ERROR};
#[cfg(windows)]
use windows_sys::Win32::NetworkManagement::IpHelper::{
    ConvertInterfaceAliasToLuid, ConvertInterfaceIndexToLuid, ConvertInterfaceLuidToAlias,
    ConvertInterfaceLuidToIndex, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
    GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses, IP_ADAPTER_NO_MULTICAST,
};
#[cfg(windows)]
use windows_sys::Win32::NetworkManagement::Ndis::{IfOperStatusUp, NET_LUID_LH};
#[cfg(windows)]
use windows_sys::Win32::Networking::WinSock as ws;

#[cfg(windows)]
use super::INVALID_ADDRESS;
#[cfg(unix)]
use super::INVALID_INPUT;
use crate::upstream::ported_fns;

#[cfg(windows)]
fn win32_error(errno: WIN32_ERROR) -> io::Error {
    unsafe {
        SetLastError(errno);
    }
    io::Error::from_raw_os_error(errno as i32)
}

ported_fns! {
    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_if_nametoindex"
    )]
    pub(crate) fn if_nametoindex(name: &OsStr) -> io::Result<u32> {
        #[cfg(unix)]
        {
            let name = CString::new(name.as_bytes())
                .map_err(|_| io::Error::from_raw_os_error(INVALID_INPUT))?;
            let index = unsafe { libc::if_nametoindex(name.as_ptr()) };
            if index == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(index)
            }
        }
        #[cfg(windows)]
        {
            let name: Vec<u16> = name.encode_wide().chain(std::iter::once(0)).collect();
            let mut luid = unsafe { std::mem::zeroed::<NET_LUID_LH>() };
            let mut errno = unsafe { ConvertInterfaceAliasToLuid(name.as_ptr(), &mut luid) };
            if errno != NO_ERROR {
                return Err(win32_error(errno));
            }
            let mut index = 0;
            errno = unsafe { ConvertInterfaceLuidToIndex(&luid, &mut index) };
            if errno != NO_ERROR {
                return Err(win32_error(errno));
            }
            Ok(index)
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_if_indextoname"
    )]
    pub(crate) fn if_indextoname(index: u32) -> io::Result<OsString> {
        #[cfg(unix)]
        {
            let mut name = vec![0; libc::IF_NAMESIZE + 1];
            let ptr = unsafe { libc::if_indextoname(index, name.as_mut_ptr().cast()) };
            if ptr.is_null() {
                return Err(io::Error::last_os_error());
            }
            let bytes = unsafe { CStr::from_ptr(ptr) }.to_bytes();
            name.truncate(bytes.len());
            Ok(OsString::from_vec(name))
        }
        #[cfg(windows)]
        {
            let mut luid = unsafe { std::mem::zeroed::<NET_LUID_LH>() };
            let mut errno = unsafe { ConvertInterfaceIndexToLuid(index, &mut luid) };
            if errno != NO_ERROR {
                return Err(win32_error(errno));
            }
            let mut name = vec![0u16; 257];
            errno = unsafe { ConvertInterfaceLuidToAlias(&luid, name.as_mut_ptr(), name.len()) };
            if errno != NO_ERROR {
                return Err(win32_error(errno));
            }
            let len = name
                .iter()
                .position(|unit| *unit == 0)
                .ok_or(io::Error::from_raw_os_error(INVALID_ADDRESS))?;
            Ok(OsString::from_wide(&name[..len]))
        }
    }

    #[ported(
        source = "src/socket/socket.c",
        original = "moonbitlang_async_find_ipv6_test_interface"
    )]
    pub(crate) fn find_ipv6_test_interface() -> u32 {
        #[cfg(unix)]
        {
            let mut ifaddrs = std::ptr::null_mut();
            if unsafe { libc::getifaddrs(&mut ifaddrs) } < 0 {
                return 0;
            }
            let mut current = ifaddrs;
            let mut result = 0;
            while !current.is_null() {
                let ifaddr = unsafe { &*current };
                if !ifaddr.ifa_addr.is_null() {
                    let sockaddr = unsafe { &*ifaddr.ifa_addr };
                    let flags = ifaddr.ifa_flags as libc::c_uint;
                    #[cfg(target_os = "linux")]
                    let loopback_ok = (flags & libc::IFF_LOOPBACK as libc::c_uint) == 0;
                    #[cfg(target_os = "macos")]
                    let loopback_ok = (flags & libc::IFF_LOOPBACK as libc::c_uint) != 0;
                    if i32::from(sockaddr.sa_family) == libc::AF_INET6
                        && loopback_ok
                        && (flags & libc::IFF_UP as libc::c_uint) != 0
                        && (flags & libc::IFF_RUNNING as libc::c_uint) != 0
                        && (flags & libc::IFF_MULTICAST as libc::c_uint) != 0
                    {
                        let sockaddr_in6 = unsafe { &*(ifaddr.ifa_addr.cast::<libc::sockaddr_in6>()) };
                        let addr = sockaddr_in6.sin6_addr.s6_addr;
                        let is_link_local = addr[0] == 0xfe && (addr[1] & 0xc0) == 0x80;
                        if is_link_local {
                            let index = unsafe { libc::if_nametoindex(ifaddr.ifa_name) };
                            if index != 0 {
                                result = index;
                                break;
                            }
                        }
                    }
                }
                current = ifaddr.ifa_next;
            }
            unsafe {
                libc::freeifaddrs(ifaddrs);
            }
            result
        }
        #[cfg(windows)]
        {
            let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
            let mut buf_len = 16 * 1024u32;
            for _ in 0..3 {
                let word_len = (buf_len as usize).div_ceil(std::mem::size_of::<usize>());
                let mut storage = vec![0usize; word_len];
                let addrs = storage
                    .as_mut_ptr()
                    .cast::<windows_sys::Win32::NetworkManagement::IpHelper::IP_ADAPTER_ADDRESSES_LH>(
                );
                let err = unsafe {
                    GetAdaptersAddresses(
                        ws::AF_INET6 as u32,
                        flags,
                        std::ptr::null(),
                        addrs,
                        &mut buf_len,
                    )
                };
                if err == ERROR_BUFFER_OVERFLOW {
                    continue;
                }
                if err != NO_ERROR {
                    return 0;
                }
                let mut adapter = addrs;
                while !adapter.is_null() {
                    let adapter_ref = unsafe { &*adapter };
                    let adapter_flags = unsafe { adapter_ref.Anonymous2.Flags };
                    if adapter_ref.OperStatus == IfOperStatusUp
                        && adapter_ref.Ipv6IfIndex != 0
                        && (adapter_flags & IP_ADAPTER_NO_MULTICAST) == 0
                    {
                        let mut unicast = adapter_ref.FirstUnicastAddress;
                        while !unicast.is_null() {
                            let address = unsafe { (*unicast).Address };
                            if !address.lpSockaddr.is_null()
                                && unsafe { (*address.lpSockaddr).sa_family } == ws::AF_INET6
                            {
                                let sockaddr =
                                    unsafe { &*(address.lpSockaddr.cast::<ws::SOCKADDR_IN6>()) };
                                let bytes = unsafe { sockaddr.sin6_addr.u.Byte };
                                if bytes[0] == 0xfe && (bytes[1] & 0xc0) == 0x80 {
                                    return adapter_ref.Ipv6IfIndex;
                                }
                            }
                            unicast = unsafe { (*unicast).Next };
                        }
                    }
                    adapter = adapter_ref.Next;
                }
                return 0;
            }
            0
        }
    }
}
