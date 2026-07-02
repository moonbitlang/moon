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

use base64::Engine;

use super::{AsyncHostError, AsyncHostResult};

#[cfg(not(windows))]
mod openssl;
#[cfg(windows)]
mod schannel;

#[cfg(not(windows))]
pub(crate) use self::openssl::TlsConnection;
#[cfg(windows)]
pub(crate) use self::schannel::TlsConnection;

// MoonBit owns the transport loop for the wasm TLS API. Providers only buffer
// host-owned TLS bytes, advance their backend, and report status/wants flags:
// `wants_read` means MoonBit should read more encrypted transport input and
// call `read_tls`; pending output means MoonBit should drain `write_tls` and
// write those bytes to the underlying channel.
const TLS_ENCRYPTED_OUTPUT_LIMIT: usize = 64 * 1024;
const TLS_ENCRYPTED_INPUT_LIMIT: usize = 256 * 1024;
#[cfg(windows)]
const TLS_PLAINTEXT_INPUT_LIMIT: usize = 256 * 1024;
const PEM_BEGIN: &str = "-----BEGIN CERTIFICATE-----";
const PEM_END: &str = "-----END CERTIFICATE-----";

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TlsStatus {
    Error = -1,
    Closed = -2,
    WouldBlock = -3,
}

impl TlsStatus {
    pub(super) fn code(self) -> i32 {
        self as i32
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
        roots: Vec<Vec<u8>>,
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

pub(crate) fn decode_pem_certificates(pem: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let pem = std::str::from_utf8(pem)
        .map_err(|_| "invalid PEM certificate in custom root certificate file".to_string())?;
    let mut certs = Vec::new();
    for chunk in pem.split(PEM_BEGIN) {
        if let Some((base64_body, _)) = chunk.split_once(PEM_END) {
            let base64_body: String = base64_body
                .chars()
                .filter(|char| !char.is_ascii_whitespace())
                .collect();
            let cert = base64::engine::general_purpose::STANDARD
                .decode(base64_body)
                .map_err(|_| {
                    "invalid PEM certificate in custom root certificate file".to_string()
                })?;
            certs.push(cert);
        }
    }
    if certs.is_empty() {
        return Err("no PEM certificate found in custom root certificate file".to_string());
    }
    Ok(certs)
}

pub(crate) type TlsConnectionRef = Arc<Mutex<TlsConnection>>;

#[derive(Debug)]
struct RingBuffer {
    buf: Box<[u8]>,
    start: usize,
    len: usize,
}

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
