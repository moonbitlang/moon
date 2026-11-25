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

//! TLS connection ownership and native-library implementations.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use slotmap::{Key, KeyData};

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::filesystem::{FsOperation, FsPath, HostFs};
use crate::runtime::{Handles, HostKeys, HostResourceKind, null_handle};

#[cfg(not(windows))]
mod openssl;
#[cfg(windows)]
mod schannel;

#[cfg(not(windows))]
use self::openssl::TlsConnection;
#[cfg(windows)]
use self::schannel::TlsConnection;

/// TLS connection state and operations for one Runtime. The guest drives
/// transport using per-call input/output slices and the reported byte counts;
/// connections do not retain these borrows beyond the current call.
pub(crate) struct HostTls {
    filesystem: Arc<HostFs>,
    connections: RefCell<Handles<TlsHandle>>,
}

impl HostTls {
    pub(crate) fn new(filesystem: Arc<HostFs>, keys: Rc<RefCell<HostKeys>>) -> Self {
        Self {
            filesystem,
            connections: RefCell::new(Handles::new(keys, HostResourceKind::TlsConnection)),
        }
    }

    pub(crate) fn leak_summary(&self) -> Option<String> {
        let count = self.connections.borrow().len();
        (count != 0).then(|| format!("tls_connections={count}"))
    }

    pub(crate) fn take_error(&self, handle: u64) -> AsyncHostResult<Option<String>> {
        self.with_handle_mut(handle, |handle| {
            Ok(match handle {
                TlsHandle::Connection(connection) => connection.take_error(),
                TlsHandle::Empty(pending) => pending.take_error(),
            })
        })
    }

    pub(crate) fn new_connection(&self) -> u64 {
        let key = self
            .connections
            .borrow_mut()
            .insert(TlsHandle::Empty(TlsPending::new()));
        key.data().as_ffi()
    }

    pub(crate) fn set_client(
        &self,
        handle: u64,
        host: &str,
        sni: bool,
        trust: TlsTrust,
    ) -> AsyncHostResult<i32> {
        self.with_handle_mut(handle, |handle| match handle {
            TlsHandle::Empty(pending) => {
                match TlsConnection::client(host, sni, pending.client_config(trust)) {
                    Ok(connection) => {
                        *handle = TlsHandle::Connection(Box::new(connection));
                        Ok(TLS_SUCCESS_STATUS)
                    }
                    Err(message) => Ok(pending.set_error(message)),
                }
            }
            TlsHandle::Connection(_) => Err(AsyncHostError::Inval),
        })
    }

    pub(crate) fn add_root_certificate(&self, handle: u64, root: &[u8]) -> AsyncHostResult<i32> {
        self.with_pending_mut(handle, |pending| pending.add_root_certificate(root))
    }

    pub(crate) fn set_server_files(
        &self,
        handle: u64,
        private_key_file: PathBuf,
        private_key_type: TlsFileType,
        certificate_file: PathBuf,
        certificate_type: TlsFileType,
    ) -> AsyncHostResult<i32> {
        let private_key_file =
            PathBuf::from(self.filesystem.resolve_path(private_key_file.as_os_str()));
        let certificate_file =
            PathBuf::from(self.filesystem.resolve_path(certificate_file.as_os_str()));
        for (label, path) in [
            ("TLS private key", private_key_file.as_path()),
            ("TLS certificate", certificate_file.as_path()),
        ] {
            if let Err(error) = self
                .filesystem
                .authorize(FsOperation::Read(FsPath::Cwd(path.as_os_str())))
            {
                return self.with_pending_mut(handle, |pending| {
                    pending.set_error(format!(
                        "failed to access {label} file: {:?}",
                        AsyncHostError::from(error)
                    ))
                });
            }
        }
        self.with_handle_mut(handle, |handle| match handle {
            TlsHandle::Empty(pending) => {
                if pending.has_root_certificates() {
                    return Ok(pending.set_error(
                        "TLS root certificates require client custom root trust".to_string(),
                    ));
                }
                match TlsConnection::server(TlsConfig::ServerFiles {
                    private_key_file,
                    private_key_type,
                    certificate_file,
                    certificate_type,
                }) {
                    Ok(connection) => {
                        *handle = TlsHandle::Connection(Box::new(connection));
                        Ok(TLS_SUCCESS_STATUS)
                    }
                    Err(message) => Ok(pending.set_error(message)),
                }
            }
            TlsHandle::Connection(_) => Err(AsyncHostError::Inval),
        })
    }

    pub(crate) fn set_server_pfx(&self, handle: u64, pfx_content: Vec<u8>) -> AsyncHostResult<i32> {
        self.with_handle_mut(handle, |handle| match handle {
            TlsHandle::Empty(pending) => {
                if pending.has_root_certificates() {
                    return Ok(pending.set_error(
                        "TLS root certificates require client custom root trust".to_string(),
                    ));
                }
                match TlsConnection::server(TlsConfig::ServerPfx { pfx_content }) {
                    Ok(connection) => {
                        *handle = TlsHandle::Connection(Box::new(connection));
                        Ok(TLS_SUCCESS_STATUS)
                    }
                    Err(message) => Ok(pending.set_error(message)),
                }
            }
            TlsHandle::Connection(_) => Err(AsyncHostError::Inval),
        })
    }

    pub(crate) fn free(&self, handle: u64) -> AsyncHostResult<()> {
        if handle == null_handle() {
            return Ok(());
        }
        let connection = self
            .connections
            .borrow_mut()
            .remove(KeyData::from_ffi(handle).into())
            .ok_or(AsyncHostError::Badf)?;
        drop(connection);
        Ok(())
    }

    pub(crate) fn read_plain(
        &self,
        handle: u64,
        input: &mut [u8],
        plain: &mut [u8],
        output: &mut [u8],
    ) -> AsyncHostResult<i32> {
        self.with_connection_mut(handle, TLS_ERROR_STATUS, |tls| {
            tls.read_plain(input, plain, output)
        })
    }

    pub(crate) fn write_plain(
        &self,
        handle: u64,
        input: &mut [u8],
        plain: &[u8],
        output: &mut [u8],
    ) -> AsyncHostResult<i32> {
        self.with_connection_mut(handle, TLS_ERROR_STATUS, |tls| {
            tls.write_plain(input, plain, output)
        })
    }

    pub(crate) fn connect(
        &self,
        handle: u64,
        input: &mut [u8],
        output: &mut [u8],
    ) -> AsyncHostResult<i32> {
        self.with_connection_mut(handle, TlsState::Error.code(), |tls| {
            let status = tls.connect(input, output);
            TlsState::from_status(status, tls.wants_write()).code()
        })
    }

    pub(crate) fn accept(
        &self,
        handle: u64,
        input: &mut [u8],
        output: &mut [u8],
    ) -> AsyncHostResult<i32> {
        self.with_connection_mut(handle, TlsState::Error.code(), |tls| {
            let status = tls.accept(input, output);
            TlsState::from_status(status, tls.wants_write()).code()
        })
    }

    pub(crate) fn bytes_read(&self, handle: u64) -> AsyncHostResult<u32> {
        let bytes = self.with_connection_mut(handle, 0, |tls| tls.bytes_read())?;
        u32::try_from(bytes).map_err(|_| AsyncHostError::Fault)
    }

    pub(crate) fn bytes_to_write(&self, handle: u64) -> AsyncHostResult<u32> {
        let bytes = self.with_connection_mut(handle, 0, |tls| tls.bytes_to_write())?;
        u32::try_from(bytes).map_err(|_| AsyncHostError::Fault)
    }

    pub(crate) fn wants_read(&self, handle: u64) -> AsyncHostResult<i32> {
        self.with_connection_mut(handle, false, |tls| tls.wants_read())
            .map(i32::from)
    }

    pub(crate) fn wants_write(&self, handle: u64) -> AsyncHostResult<i32> {
        self.with_connection_mut(handle, false, |tls| tls.wants_write())
            .map(i32::from)
    }

    pub(crate) fn shutdown(&self, handle: u64) -> AsyncHostResult<i32> {
        self.with_connection_mut(handle, TLS_ERROR_STATUS, |tls| tls.shutdown())
    }

    // The outer result validates the Handle. A backend failure is reported
    // separately and leaves its diagnostic on the connection for take_error.
    pub(crate) fn peer_certificate(
        &self,
        handle: u64,
    ) -> AsyncHostResult<Result<Option<Vec<u8>>, ()>> {
        self.with_connection_mut(handle, Err(()), |connection| connection.peer_certificate())
    }

    pub(crate) fn unique_channel_binding(
        &self,
        handle: u64,
    ) -> AsyncHostResult<Result<Option<Vec<u8>>, ()>> {
        self.with_connection_mut(handle, Err(()), |connection| {
            connection.unique_channel_binding()
        })
    }

    pub(crate) fn server_endpoint_channel_binding(
        &self,
        handle: u64,
    ) -> AsyncHostResult<Result<Option<Vec<u8>>, ()>> {
        self.with_connection_mut(handle, Err(()), |connection| {
            connection.server_endpoint_channel_binding()
        })
    }

    fn with_connection_mut<T>(
        &self,
        handle: u64,
        unconfigured_value: T,
        f: impl FnOnce(&mut TlsConnection) -> T,
    ) -> AsyncHostResult<T> {
        self.with_handle_mut(handle, |handle| match handle {
            TlsHandle::Connection(connection) => Ok(f(connection)),
            TlsHandle::Empty(pending) => {
                pending.set_error("TLS handle is not configured".to_string());
                Ok(unconfigured_value)
            }
        })
    }

    fn with_pending_mut<T>(
        &self,
        handle: u64,
        f: impl FnOnce(&mut TlsPending) -> T,
    ) -> AsyncHostResult<T> {
        self.with_handle_mut(handle, |handle| match handle {
            TlsHandle::Empty(pending) => Ok(f(pending)),
            TlsHandle::Connection(_) => Err(AsyncHostError::Inval),
        })
    }

    fn with_handle_mut<T>(
        &self,
        handle: u64,
        f: impl FnOnce(&mut TlsHandle) -> AsyncHostResult<T>,
    ) -> AsyncHostResult<T> {
        let mut connections = self.connections.borrow_mut();
        let handle = connections
            .get_mut(KeyData::from_ffi(handle).into())
            .ok_or(AsyncHostError::Badf)?;
        f(handle)
    }
}

#[cfg(windows)]
const TLS_PLAINTEXT_INPUT_LIMIT: usize = 256 * 1024;

const TLS_SUCCESS_STATUS: i32 = 0;
const TLS_ERROR_STATUS: i32 = -1;
const TLS_CLOSED_STATUS: i32 = -2;
const TLS_WOULD_BLOCK_STATUS: i32 = -3;
const TLS_RENEGOTIATION_STATUS: i32 = -4;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TlsState {
    Completed = 0,
    WantRead = 1,
    WantWrite = 2,
    Error = 3,
    Eof = 4,
    ReNegotiation = 5,
}

impl TlsState {
    fn code(self) -> i32 {
        self as i32
    }

    fn from_status(status: i32, wants_write: bool) -> Self {
        match status {
            TLS_SUCCESS_STATUS => Self::Completed,
            TLS_WOULD_BLOCK_STATUS => {
                if wants_write {
                    Self::WantWrite
                } else {
                    Self::WantRead
                }
            }
            TLS_CLOSED_STATUS => Self::Eof,
            TLS_RENEGOTIATION_STATUS => Self::ReNegotiation,
            _ => Self::Error,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TlsFileType {
    Pem,
    Asn1,
}

impl TlsFileType {
    const PEM_ABI: i32 = 1;
    const ASN1_ABI: i32 = 2;

    pub(crate) fn from_abi(value: i32) -> AsyncHostResult<Self> {
        match value {
            Self::PEM_ABI => Ok(Self::Pem),
            Self::ASN1_ABI => Ok(Self::Asn1),
            _ => Err(AsyncHostError::Inval),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TlsTrust {
    NoVerification,
    SystemRoot,
    CustomRoot,
}

impl TlsTrust {
    const NO_VERIFICATION_ABI: i32 = 0;
    const SYSTEM_ROOT_ABI: i32 = 1;
    const CUSTOM_ROOT_ABI: i32 = 2;

    pub(crate) fn from_abi(value: i32) -> AsyncHostResult<Self> {
        match value {
            Self::NO_VERIFICATION_ABI => Ok(Self::NoVerification),
            Self::SYSTEM_ROOT_ABI => Ok(Self::SystemRoot),
            Self::CUSTOM_ROOT_ABI => Ok(Self::CustomRoot),
            _ => Err(AsyncHostError::Inval),
        }
    }
}

#[derive(Debug, Clone)]
enum TlsConfig {
    Client {
        trust: TlsTrust,
        root_certificates: Vec<Vec<u8>>,
    },
    ServerFiles {
        private_key_file: PathBuf,
        private_key_type: TlsFileType,
        certificate_file: PathBuf,
        certificate_type: TlsFileType,
    },
    ServerPfx {
        pfx_content: Vec<u8>,
    },
}

// The wasm ABI exposes one TLS handle. New handles start empty and become a
// live connection as soon as a client/server setter has enough data.
enum TlsHandle {
    Empty(TlsPending),
    Connection(Box<TlsConnection>),
}

#[derive(Debug)]
struct TlsPending {
    root_certificates: Vec<Vec<u8>>,
    last_error: Option<String>,
}

impl TlsPending {
    fn new() -> Self {
        Self {
            root_certificates: Vec::new(),
            last_error: None,
        }
    }

    fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    fn set_error(&mut self, message: String) -> i32 {
        self.last_error = Some(message);
        TLS_ERROR_STATUS
    }

    fn add_root_certificate(&mut self, root: &[u8]) -> i32 {
        self.last_error = None;
        self.root_certificates.push(root.to_vec());
        TLS_SUCCESS_STATUS
    }

    fn client_config(&self, trust: TlsTrust) -> TlsConfig {
        TlsConfig::Client {
            trust,
            root_certificates: self.root_certificates.clone(),
        }
    }

    fn has_root_certificates(&self) -> bool {
        !self.root_certificates.is_empty()
    }
}

#[derive(Debug)]
#[cfg(any(windows, test))]
struct RingBuffer {
    buf: Box<[u8]>,
    start: usize,
    len: usize,
}

#[cfg(any(windows, test))]
impl RingBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0; capacity].into_boxed_slice(),
            start: 0,
            len: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn remaining(&self) -> usize {
        self.buf.len() - self.len
    }

    fn push(&mut self, src: &[u8]) {
        debug_assert!(src.len() <= self.remaining());
        let tail = (self.start + self.len) % self.buf.len();
        let first_len = src.len().min(self.buf.len() - tail);
        let second_len = src.len() - first_len;
        self.buf[tail..tail + first_len].copy_from_slice(&src[..first_len]);
        self.buf[..second_len].copy_from_slice(&src[first_len..]);
        self.len += src.len();
    }

    fn pop(&mut self, dst: &mut [u8]) -> usize {
        let len = dst.len().min(self.len);
        let first_len = len.min(self.buf.len() - self.start);
        let second_len = len - first_len;
        dst[..first_len].copy_from_slice(&self.buf[self.start..self.start + first_len]);
        dst[first_len..len].copy_from_slice(&self.buf[..second_len]);
        self.start = (self.start + len) % self.buf.len();
        self.len -= len;
        len
    }
}

#[cfg(test)]
mod tests;
