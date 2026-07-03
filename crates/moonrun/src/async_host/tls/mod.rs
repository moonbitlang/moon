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

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::{AsyncHostError, AsyncHostResult};

#[cfg(not(windows))]
mod openssl;
#[cfg(windows)]
mod schannel;

#[cfg(not(windows))]
pub(crate) use self::openssl::TlsConnection;
#[cfg(windows)]
pub(crate) use self::schannel::TlsConnection;

// MoonBit owns the transport loop for the wasm TLS API. Every TLS step takes
// the current encrypted input and output buffers directly, then reports how
// many encrypted bytes were consumed or produced through `bytes_read` and
// `bytes_to_write`.
#[cfg(windows)]
const TLS_PLAINTEXT_INPUT_LIMIT: usize = 256 * 1024;

pub(crate) const TLS_ERROR_STATUS: i32 = -1;
pub(super) const TLS_CLOSED_STATUS: i32 = -2;
pub(super) const TLS_WOULD_BLOCK_STATUS: i32 = -3;
pub(super) const TLS_RENEGOTIATION_STATUS: i32 = -4;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TlsState {
    Completed = 0,
    WantRead = 1,
    WantWrite = 2,
    Error = 3,
    Eof = 4,
    ReNegotiation = 5,
}

impl TlsState {
    pub(crate) fn code(self) -> i32 {
        self as i32
    }

    pub(crate) fn from_status(status: i32, _wants_read: bool, wants_write: bool) -> Self {
        match status {
            0 => Self::Completed,
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
pub(crate) enum TlsConfig {
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

pub(crate) type TlsHandleRef = Arc<Mutex<TlsHandle>>;

// The wasm ABI exposes one TLS handle. New handles start empty and become a
// live connection as soon as a client/server setter has enough data.
pub(crate) enum TlsHandle {
    Empty(TlsPending),
    Connection(Box<TlsConnection>),
}

#[derive(Debug)]
pub(crate) struct TlsPending {
    root_certificates: Vec<Vec<u8>>,
    last_error: Option<String>,
}

impl TlsPending {
    pub(crate) fn new() -> Self {
        Self {
            root_certificates: Vec::new(),
            last_error: None,
        }
    }

    pub(crate) fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    pub(crate) fn set_error(&mut self, message: String) -> i32 {
        self.last_error = Some(message);
        TLS_ERROR_STATUS
    }

    pub(crate) fn add_root_certificate(&mut self, root: &[u8]) -> i32 {
        self.last_error = None;
        self.root_certificates.push(root.to_vec());
        0
    }

    pub(crate) fn client_config(&self, trust: TlsTrust) -> TlsConfig {
        TlsConfig::Client {
            trust,
            root_certificates: self.root_certificates.clone(),
        }
    }

    pub(crate) fn has_root_certificates(&self) -> bool {
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
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_preserves_order_across_wraparound() {
        let mut buffer = RingBuffer::new(5);
        buffer.push(&[1, 2, 3, 4]);

        let mut first = [0; 2];
        assert_eq!(buffer.pop(&mut first), 2);
        assert_eq!(first, [1, 2]);

        buffer.push(&[5, 6, 7]);
        assert_eq!(buffer.remaining(), 0);

        let mut second = [0; 5];
        assert_eq!(buffer.pop(&mut second), 5);
        assert_eq!(second, [3, 4, 5, 6, 7]);
        assert!(buffer.is_empty());
        assert_eq!(buffer.remaining(), 5);
    }

    #[test]
    fn ring_buffer_pop_is_limited_to_available_bytes() {
        let mut buffer = RingBuffer::new(4);
        buffer.push(&[9, 8]);

        let mut dst = [0; 4];
        assert_eq!(buffer.pop(&mut dst), 2);
        assert_eq!(dst, [9, 8, 0, 0]);
        assert!(buffer.is_empty());
    }
}
