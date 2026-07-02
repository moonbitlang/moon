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

use std::io::{self, Read, Write};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::ssl::{
    Error as SslError, ErrorCode, Ssl, SslContextBuilder, SslFiletype, SslMethod, SslMode,
    SslStream, SslVerifyMode,
};
use openssl::x509::{X509, X509Ref, verify::X509CheckFlags};

use super::{
    RingBuffer, TLS_ENCRYPTED_INPUT_LIMIT, TLS_ENCRYPTED_OUTPUT_LIMIT, TlsConfig, TlsFileType,
    TlsStatus, TlsTrust,
};

pub(crate) struct TlsConnection {
    stream: SslStream<QueueStream>,
    state: Arc<Mutex<BioState>>,
    mode: TlsMode,
    last_error: Option<String>,
    last_want_read: bool,
    last_want_write: bool,
}

impl TlsConnection {
    pub(crate) fn client(host: &str, sni: bool, config: TlsConfig) -> Result<Self, String> {
        let TlsConfig::Client { trust, roots } = config else {
            return Err("TLS client requires client configuration".to_string());
        };
        let mut ctx = SslContextBuilder::new(SslMethod::tls())
            .map_err(|error| format!("failed to create TLS client context: {error}"))?;
        ctx.set_mode(SslMode::ENABLE_PARTIAL_WRITE);
        match trust {
            TlsTrust::NoVerification => ctx.set_verify(SslVerifyMode::NONE),
            TlsTrust::SystemRoot => {
                ctx.set_verify(SslVerifyMode::PEER);
                ctx.set_default_verify_paths()
                    .map_err(|error| format!("failed to load system TLS roots: {error}"))?;
            }
            TlsTrust::CustomRoot => {
                ctx.set_verify(SslVerifyMode::PEER);
                for root in roots {
                    let cert = X509::from_der(&root).map_err(|error| {
                        format!("failed to parse TLS root certificate: {error}")
                    })?;
                    ctx.cert_store_mut()
                        .add_cert(cert)
                        .map_err(|error| format!("failed to add TLS root certificate: {error}"))?;
                }
            }
        }

        let mut ssl = Ssl::new(&ctx.build())
            .map_err(|error| format!("failed to create TLS client: {error}"))?;
        ssl.set_connect_state();
        if !host.is_empty() {
            if sni {
                ssl.set_hostname(host)
                    .map_err(|error| format!("failed to configure TLS SNI: {error}"))?;
            }
            if trust != TlsTrust::NoVerification {
                configure_hostname_verification(&mut ssl, host).map_err(|error| {
                    format!("failed to configure TLS host verification: {error}")
                })?;
            }
        }

        let mut connection = Self::new(ssl, TlsMode::Client)?;
        let status = connection.drive_handshake();
        if status == TlsStatus::Error.code() || status == TlsStatus::Closed.code() {
            Err(connection
                .take_error()
                .unwrap_or_else(|| "TLS client handshake closed".to_string()))
        } else {
            Ok(connection)
        }
    }

    pub(crate) fn server(config: TlsConfig) -> Result<Self, String> {
        let (private_key_file, private_key_type, certificate_file, certificate_type) = match config
        {
            TlsConfig::ServerFiles {
                private_key_file,
                private_key_type,
                certificate_file,
                certificate_type,
            } => (
                private_key_file,
                private_key_type,
                certificate_file,
                certificate_type,
            ),
            TlsConfig::ServerPfx { pfx_content } => {
                drop(pfx_content);
                return Err(
                    "TLS PFX server certificates are supported only by SChannel on Windows"
                        .to_string(),
                );
            }
            TlsConfig::Client { .. } => {
                return Err("TLS server requires server configuration".to_string());
            }
        };
        let mut ctx = SslContextBuilder::new(SslMethod::tls())
            .map_err(|error| format!("failed to create TLS server context: {error}"))?;
        ctx.set_mode(SslMode::ENABLE_PARTIAL_WRITE);
        ctx.set_certificate_file(&certificate_file, openssl_file_type(certificate_type))
            .map_err(|error| format!("failed to load TLS certificate: {error}"))?;
        ctx.set_private_key_file(&private_key_file, openssl_file_type(private_key_type))
            .map_err(|error| format!("failed to load TLS private key: {error}"))?;

        let mut ssl = Ssl::new(&ctx.build())
            .map_err(|error| format!("failed to create TLS server: {error}"))?;
        ssl.set_accept_state();
        let mut connection = Self::new(ssl, TlsMode::Server)?;
        let status = connection.drive_handshake();
        if status == TlsStatus::Error.code() || status == TlsStatus::Closed.code() {
            Err(connection
                .take_error()
                .unwrap_or_else(|| "TLS server handshake closed".to_string()))
        } else {
            Ok(connection)
        }
    }

    fn new(ssl: Ssl, mode: TlsMode) -> Result<Self, String> {
        let state = Arc::new(Mutex::new(BioState::new()));
        let stream = QueueStream {
            state: Arc::clone(&state),
        };
        let stream = SslStream::new(ssl, stream)
            .map_err(|error| format!("failed to create TLS stream: {error}"))?;
        Ok(Self {
            stream,
            state,
            mode,
            last_error: None,
            last_want_read: false,
            last_want_write: false,
        })
    }

    pub(crate) fn read_tls(&mut self, src: &[u8]) -> i32 {
        let append_result = self
            .state
            .lock()
            .map_err(|_| "TLS BIO state lock poisoned".to_string())
            .and_then(|mut state| state.append_input(src));
        if let Err(error) = append_result {
            return self.error(error);
        }
        if !self.is_init_finished() {
            let status = self.drive_handshake();
            if status == TlsStatus::Error.code() || status == TlsStatus::Closed.code() {
                return status;
            }
        }
        i32::try_from(src.len())
            .unwrap_or_else(|_| self.error("TLS read size overflow".to_string()))
    }

    pub(crate) fn write_tls(&mut self, dst: &mut [u8]) -> i32 {
        if dst.is_empty() {
            self.last_want_write = self.output_pending() || self.last_want_write;
            return 0;
        }
        loop {
            // A previous WANT_WRITE may mean OpenSSL filled the output BIO
            // before finishing the operation. After MoonBit drains those
            // bytes, re-enter OpenSSL once so it can produce more output or
            // switch to WANT_READ.
            let was_want_write = self.last_want_write;
            let len = self
                .state
                .lock()
                .ok()
                .map(|mut state| state.drain_output(dst));
            let Some(len) = len else {
                return self.error("TLS BIO state lock poisoned".to_string());
            };
            let output_pending = self.output_pending();
            self.last_want_write = output_pending || was_want_write;
            if len > 0 || self.is_init_finished() || !was_want_write {
                return i32::try_from(len)
                    .unwrap_or_else(|_| self.error("TLS write size overflow".to_string()));
            }
            let status = self.drive_handshake();
            if status != 0 && !self.output_pending() {
                return status;
            }
        }
    }

    pub(crate) fn read_plain(&mut self, dst: &mut [u8]) -> i32 {
        if dst.is_empty() {
            return 0;
        }
        if !self.is_init_finished() {
            let status = self.drive_handshake();
            if status != 0 {
                return status;
            }
        }
        match self.stream.ssl_read(dst) {
            Ok(0) => TlsStatus::Closed.code(),
            Ok(read) => i32::try_from(read)
                .unwrap_or_else(|_| self.error("TLS plaintext read size overflow".to_string())),
            Err(error) => self.handle_ssl_error(error, "TLS plaintext read failed"),
        }
    }

    pub(crate) fn write_plain(&mut self, src: &[u8]) -> i32 {
        if !self.is_init_finished() {
            let status = self.drive_handshake();
            if status != 0 {
                return status;
            }
        }
        match self.stream.ssl_write(src) {
            Ok(written) => {
                self.last_want_read = false;
                self.last_want_write = self.output_pending();
                i32::try_from(written)
                    .unwrap_or_else(|_| self.error("TLS plaintext write size overflow".to_string()))
            }
            Err(error) => self.handle_ssl_error(error, "TLS plaintext write failed"),
        }
    }

    pub(crate) fn wants_read(&self) -> bool {
        self.last_want_read
    }

    pub(crate) fn wants_write(&self) -> bool {
        self.last_want_write || self.output_pending()
    }

    pub(crate) fn is_handshaking(&self) -> bool {
        !self.is_init_finished()
    }

    pub(crate) fn send_close_notify(&mut self) -> i32 {
        match self.stream.shutdown() {
            Ok(_) => {
                self.last_want_read = false;
                self.last_want_write = self.output_pending();
                0
            }
            Err(error) => self.handle_ssl_error(error, "TLS shutdown failed"),
        }
    }

    pub(crate) fn peer_certificate_len(&mut self) -> i32 {
        match self.peer_certificate_der() {
            Ok(Some(certificate)) => i32::try_from(certificate.len())
                .unwrap_or_else(|_| self.error("TLS peer certificate size overflow".to_string())),
            Ok(None) => 0,
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn copy_peer_certificate(&mut self, dst: &mut [u8]) -> i32 {
        match self.peer_certificate_der() {
            Ok(Some(certificate)) if certificate.len() == dst.len() => {
                dst.copy_from_slice(&certificate);
                i32::try_from(dst.len()).unwrap_or_else(|_| {
                    self.error("TLS peer certificate size overflow".to_string())
                })
            }
            Ok(Some(_)) => {
                self.error("TLS peer certificate destination length mismatch".to_string())
            }
            Ok(None) => 0,
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn unique_channel_binding_len(&mut self) -> i32 {
        match self.unique_channel_binding() {
            Ok(Some(binding)) => i32::try_from(binding.len())
                .unwrap_or_else(|_| self.error("TLS channel binding size overflow".to_string())),
            Ok(None) => 0,
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn copy_unique_channel_binding(&mut self, dst: &mut [u8]) -> i32 {
        match self.unique_channel_binding() {
            Ok(Some(binding)) if binding.len() == dst.len() => {
                dst.copy_from_slice(&binding);
                i32::try_from(dst.len())
                    .unwrap_or_else(|_| self.error("TLS channel binding size overflow".to_string()))
            }
            Ok(Some(_)) => {
                self.error("TLS channel binding destination length mismatch".to_string())
            }
            Ok(None) => 0,
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn server_endpoint_channel_binding_len(&mut self) -> i32 {
        match self.server_endpoint_channel_binding() {
            Ok(Some(binding)) => i32::try_from(binding.len()).unwrap_or_else(|_| {
                self.error("TLS server endpoint channel binding size overflow".to_string())
            }),
            Ok(None) => 0,
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn copy_server_endpoint_channel_binding(&mut self, dst: &mut [u8]) -> i32 {
        match self.server_endpoint_channel_binding() {
            Ok(Some(binding)) if binding.len() == dst.len() => {
                dst.copy_from_slice(&binding);
                i32::try_from(dst.len()).unwrap_or_else(|_| {
                    self.error("TLS server endpoint channel binding size overflow".to_string())
                })
            }
            Ok(Some(_)) => self.error(
                "TLS server endpoint channel binding destination length mismatch".to_string(),
            ),
            Ok(None) => 0,
            Err(error) => self.error(error),
        }
    }

    pub(crate) fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    fn drive_handshake(&mut self) -> i32 {
        if self.is_init_finished() {
            self.last_want_read = false;
            self.last_want_write = self.output_pending();
            return 0;
        }
        let result = match self.mode {
            TlsMode::Client => self.stream.connect(),
            TlsMode::Server => self.stream.accept(),
        };
        match result {
            Ok(()) => {
                self.last_want_read = false;
                self.last_want_write = self.output_pending();
                0
            }
            Err(error) => self.handle_ssl_error(error, "TLS handshake failed"),
        }
    }

    fn handle_ssl_error(&mut self, error: SslError, context: &str) -> i32 {
        match error.code() {
            ErrorCode::WANT_READ => {
                self.last_want_read = true;
                self.last_want_write = self.output_pending();
                TlsStatus::WouldBlock.code()
            }
            ErrorCode::WANT_WRITE => {
                self.last_want_read = false;
                self.last_want_write = true;
                TlsStatus::WouldBlock.code()
            }
            ErrorCode::ZERO_RETURN => {
                self.last_want_read = false;
                self.last_want_write = self.output_pending();
                TlsStatus::Closed.code()
            }
            ErrorCode::SYSCALL
                if error
                    .io_error()
                    .is_some_and(|io| io.kind() == io::ErrorKind::WouldBlock) =>
            {
                self.last_want_read = true;
                self.last_want_write = self.output_pending();
                TlsStatus::WouldBlock.code()
            }
            _ => self.error(format!("{context}: {error}")),
        }
    }

    fn peer_certificate_der(&self) -> Result<Option<Vec<u8>>, String> {
        self.stream
            .ssl()
            .peer_certificate()
            .map(|certificate| {
                certificate
                    .to_der()
                    .map_err(|error| format!("failed to encode TLS peer certificate: {error}"))
            })
            .transpose()
    }

    fn unique_channel_binding(&self) -> Result<Option<Vec<u8>>, String> {
        let ssl = self.stream.ssl();
        let len = match self.mode {
            TlsMode::Client => ssl.finished(&mut []),
            TlsMode::Server => ssl.peer_finished(&mut []),
        };
        if len == 0 {
            return Ok(None);
        }
        let mut binding = vec![0; len];
        let actual_len = match self.mode {
            TlsMode::Client => ssl.finished(&mut binding),
            TlsMode::Server => ssl.peer_finished(&mut binding),
        };
        if actual_len != len {
            return Err("TLS channel binding size changed while copying".to_string());
        }
        Ok(Some(binding))
    }

    fn server_endpoint_channel_binding(&self) -> Result<Option<Vec<u8>>, String> {
        match self.mode {
            TlsMode::Client => self
                .stream
                .ssl()
                .peer_certificate()
                .as_deref()
                .map(server_endpoint_certificate_hash)
                .transpose(),
            TlsMode::Server => self
                .stream
                .ssl()
                .certificate()
                .map(server_endpoint_certificate_hash)
                .transpose(),
        }
    }

    fn is_init_finished(&self) -> bool {
        self.stream.ssl().is_init_finished()
    }

    fn output_pending(&self) -> bool {
        self.state
            .lock()
            .map(|state| !state.output.is_empty())
            .unwrap_or(true)
    }

    fn error(&mut self, message: String) -> i32 {
        self.last_error = Some(message);
        TlsStatus::Error.code()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TlsMode {
    Client,
    Server,
}

#[derive(Debug)]
struct BioState {
    input: RingBuffer,
    output: RingBuffer,
    last_blocked: Option<BioBlocked>,
}

impl BioState {
    fn new() -> Self {
        Self {
            input: RingBuffer::new(TLS_ENCRYPTED_INPUT_LIMIT),
            output: RingBuffer::new(TLS_ENCRYPTED_OUTPUT_LIMIT),
            last_blocked: None,
        }
    }

    fn append_input(&mut self, src: &[u8]) -> Result<(), String> {
        if src.len() > self.input.remaining() {
            return Err("TLS encrypted input buffer limit exceeded".to_string());
        }
        self.input.push(src);
        Ok(())
    }

    fn read_input(&mut self, dst: &mut [u8]) -> usize {
        self.last_blocked = None;
        self.input.pop(dst)
    }

    fn write_output(&mut self, src: &[u8]) -> usize {
        let len = src.len().min(self.output.remaining());
        self.output.push(&src[..len]);
        if len > 0 || src.is_empty() {
            self.last_blocked = None;
        }
        len
    }

    fn drain_output(&mut self, dst: &mut [u8]) -> usize {
        self.output.pop(dst)
    }

    fn block_on_read(&mut self) {
        self.last_blocked = Some(BioBlocked::Read);
    }

    fn block_on_write(&mut self) {
        self.last_blocked = Some(BioBlocked::Write);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BioBlocked {
    Read,
    Write,
}

#[derive(Debug, Clone)]
struct QueueStream {
    state: Arc<Mutex<BioState>>,
}

impl Read for QueueStream {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        if dst.is_empty() {
            return Ok(0);
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("TLS BIO state lock poisoned"))?;
        if state.input.is_empty() {
            state.block_on_read();
            Err(io::Error::from(io::ErrorKind::WouldBlock))
        } else {
            Ok(state.read_input(dst))
        }
    }
}

impl Write for QueueStream {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("TLS BIO state lock poisoned"))?;
        let written = state.write_output(src);
        if written == 0 && !src.is_empty() {
            state.block_on_write();
            Err(io::Error::from(io::ErrorKind::WouldBlock))
        } else {
            Ok(written)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn configure_hostname_verification(
    ssl: &mut Ssl,
    host: &str,
) -> Result<(), openssl::error::ErrorStack> {
    let param = ssl.param_mut();
    param.set_hostflags(X509CheckFlags::NO_PARTIAL_WILDCARDS);
    match host.parse::<IpAddr>() {
        Ok(ip) => param.set_ip(ip),
        Err(_) => param.set_host(host),
    }
}

fn openssl_file_type(file_type: TlsFileType) -> SslFiletype {
    match file_type {
        TlsFileType::Pem => SslFiletype::PEM,
        TlsFileType::Asn1 => SslFiletype::ASN1,
    }
}

fn server_endpoint_certificate_hash(certificate: &X509Ref) -> Result<Vec<u8>, String> {
    let signature_nid = certificate.signature_algorithm().object().nid();
    let digest_nid = signature_nid
        .signature_algorithms()
        .map(|algorithms| algorithms.digest)
        .ok_or_else(|| "tls-server-endpoint channel binding unavailable".to_string())?;
    let digest = if matches!(digest_nid, Nid::MD5 | Nid::SHA1) {
        MessageDigest::sha256()
    } else {
        MessageDigest::from_nid(digest_nid)
            .ok_or_else(|| "tls-server-endpoint channel binding unavailable".to_string())?
    };
    certificate
        .digest(digest)
        .map(|digest| digest.to_vec())
        .map_err(|error| format!("failed to hash TLS server endpoint certificate: {error}"))
}
