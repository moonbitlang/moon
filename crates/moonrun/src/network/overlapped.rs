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

//! Network interpretation for Windows overlapped operations. The Async Host
//! supplies stable storage and retains Resources until completion.

use windows_sys::Win32::System::IO::OVERLAPPED;

use super::{AcceptBuffer, HostNetwork, os::overlapped as sys, raw_socket, raw_socket_of_class};
use crate::async_host::AsyncHostResult;
use crate::resource::{Resource, ResourceClass};

impl HostNetwork {
    /// # Safety
    /// Retain the socket, buffer, flags and OVERLAPPED until completion, without
    /// moving native output storage or accessing it while pending.
    pub(crate) unsafe fn recv_overlapped(
        &self,
        socket: &Resource,
        buffer: &mut [u8],
        flags: *mut u32,
        overlapped: *mut OVERLAPPED,
    ) -> AsyncHostResult<usize> {
        Ok(unsafe { sys::recv(raw_socket(socket)?, buffer, flags, overlapped)? })
    }

    /// # Safety
    /// Retain the socket, buffers, flags, address length and OVERLAPPED until
    /// completion, without moving native output storage or accessing it while pending.
    pub(crate) unsafe fn recv_from_overlapped(
        &self,
        socket: &Resource,
        buffer: &mut [u8],
        flags: *mut u32,
        addr: &mut [u8],
        addr_len: *mut i32,
        overlapped: *mut OVERLAPPED,
    ) -> AsyncHostResult<usize> {
        Ok(unsafe {
            sys::recv_from(
                raw_socket_of_class(socket, ResourceClass::UdpSocket)?,
                buffer,
                flags,
                addr,
                addr_len,
                overlapped,
            )?
        })
    }

    /// # Safety
    /// Retain the socket, buffer and OVERLAPPED until completion; do not modify
    /// the buffer while pending.
    pub(crate) unsafe fn send_overlapped(
        &self,
        socket: &Resource,
        buffer: &[u8],
        flags: u32,
        overlapped: *mut OVERLAPPED,
    ) -> AsyncHostResult<usize> {
        Ok(unsafe { sys::send(raw_socket(socket)?, buffer, flags, overlapped)? })
    }

    /// # Safety
    /// Retain the socket, buffers and OVERLAPPED until completion; do not modify
    /// input buffers while pending.
    pub(crate) unsafe fn send_to_overlapped(
        &self,
        socket: &Resource,
        buffer: &[u8],
        flags: u32,
        addr: &[u8],
        overlapped: *mut OVERLAPPED,
    ) -> AsyncHostResult<usize> {
        self.check_connect(addr)?;
        Ok(unsafe {
            sys::send_to(
                raw_socket_of_class(socket, ResourceClass::UdpSocket)?,
                buffer,
                flags,
                addr,
                overlapped,
            )?
        })
    }

    /// # Safety
    /// Retain the socket, address and OVERLAPPED until completion.
    pub(crate) unsafe fn connect_overlapped(
        &self,
        socket: &Resource,
        addr: &[u8],
        overlapped: *mut OVERLAPPED,
    ) -> AsyncHostResult<()> {
        self.check_connect(addr)?;
        unsafe {
            sys::connect(
                raw_socket_of_class(socket, ResourceClass::TcpSocket)?,
                addr,
                overlapped,
            )
        }
        .map_err(Into::into)
    }

    /// # Safety
    /// Retain both sockets, AcceptBuffer and OVERLAPPED until completion. Keep
    /// output storage at stable addresses and do not access it while pending.
    pub(crate) unsafe fn accept_overlapped(
        &self,
        server: &Resource,
        connection: &Resource,
        buffer: &mut AcceptBuffer,
        overlapped: *mut OVERLAPPED,
    ) -> AsyncHostResult<()> {
        unsafe {
            sys::accept(
                raw_socket_of_class(server, ResourceClass::TcpSocket)?,
                raw_socket_of_class(connection, ResourceClass::TcpSocket)?,
                buffer,
                overlapped,
            )
        }
        .map_err(Into::into)
    }

    pub(crate) fn setup_connected_socket(&self, socket: &Resource) -> AsyncHostResult<()> {
        Ok(sys::setup_connected_socket(raw_socket_of_class(
            socket,
            ResourceClass::TcpSocket,
        )?)?)
    }

    pub(crate) fn setup_accepted_socket(
        &self,
        listen: &Resource,
        accept: &Resource,
    ) -> AsyncHostResult<()> {
        Ok(sys::setup_accepted_socket(
            raw_socket_of_class(listen, ResourceClass::TcpSocket)?,
            raw_socket_of_class(accept, ResourceClass::TcpSocket)?,
        )?)
    }
}
