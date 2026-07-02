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
use openssl::x509::{X509, X509Ref, store::X509StoreBuilder, verify::X509CheckFlags};

use super::{
    TLS_CLOSED_STATUS, TLS_ERROR_STATUS, TLS_RENEGOTIATION_STATUS, TLS_WOULD_BLOCK_STATUS,
    TlsConfig, TlsFileType, TlsTrust,
};

pub(crate) struct TlsConnection {
    stream: Option<SslStream<QueueStream>>,
    pending_ssl: Option<Ssl>,
    state: Arc<Mutex<BioState>>,
    mode: TlsMode,
    shutdown_started: bool,
    shutdown_complete: bool,
    last_error: Option<String>,
    last_want_read: bool,
    last_want_write: bool,
    last_bytes_read: usize,
    last_bytes_to_write: usize,
}

impl TlsConnection {
    pub(crate) fn client(host: &str, sni: bool, config: TlsConfig) -> Result<Self, String> {
        let TlsConfig::Client {
            trust,
            root_certificates,
        } = config
        else {
            return Err("TLS client requires client configuration".to_string());
        };
        if trust != TlsTrust::CustomRoot && !root_certificates.is_empty() {
            return Err("TLS root certificates require custom root trust".to_string());
        }
        let mut ctx = SslContextBuilder::new(SslMethod::tls())
            .map_err(|error| format!("failed to create TLS client context: {error}"))?;
        ctx.set_mode(SslMode::ENABLE_PARTIAL_WRITE | SslMode::ACCEPT_MOVING_WRITE_BUFFER);
        match trust {
            TlsTrust::NoVerification => ctx.set_verify(SslVerifyMode::NONE),
            TlsTrust::SystemRoot => {
                ctx.set_verify(SslVerifyMode::PEER);
                load_system_root_paths(&mut ctx)
                    .map_err(|error| format!("failed to load system TLS roots: {error}"))?;
            }
            TlsTrust::CustomRoot => {
                ctx.set_verify(SslVerifyMode::PEER);
            }
        }

        let mut ssl = Ssl::new(&ctx.build())
            .map_err(|error| format!("failed to create TLS client: {error}"))?;
        if trust == TlsTrust::CustomRoot {
            configure_custom_root_store(&mut ssl, &root_certificates)?;
        }
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

        Self::new(ssl, TlsMode::Client)
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
            TlsConfig::ServerPfx {
                pfx_content: _pfx_content,
            } => {
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
        ctx.set_mode(SslMode::ENABLE_PARTIAL_WRITE | SslMode::ACCEPT_MOVING_WRITE_BUFFER);
        if certificate_type == TlsFileType::Pem {
            ctx.set_certificate_chain_file(&certificate_file)
                .map_err(|error| format!("failed to load TLS certificate chain: {error}"))?;
        } else {
            ctx.set_certificate_file(&certificate_file, openssl_file_type(certificate_type))
                .map_err(|error| format!("failed to load TLS certificate: {error}"))?;
        }
        ctx.set_private_key_file(&private_key_file, openssl_file_type(private_key_type))
            .map_err(|error| format!("failed to load TLS private key: {error}"))?;

        let mut ssl = Ssl::new(&ctx.build())
            .map_err(|error| format!("failed to create TLS server: {error}"))?;
        ssl.set_accept_state();
        Self::new(ssl, TlsMode::Server)
    }

    fn new(ssl: Ssl, mode: TlsMode) -> Result<Self, String> {
        let state = Arc::new(Mutex::new(BioState::new()));
        Ok(Self {
            stream: None,
            pending_ssl: Some(ssl),
            state,
            mode,
            shutdown_started: false,
            shutdown_complete: false,
            last_error: None,
            last_want_read: false,
            last_want_write: true,
            last_bytes_read: 0,
            last_bytes_to_write: 0,
        })
    }

    pub(crate) fn read_plain(
        &mut self,
        input: &mut [u8],
        dst: &mut [u8],
        output: &mut [u8],
    ) -> i32 {
        self.with_scoped_io(input, output, |tls| tls.read_plain_scoped(dst))
    }

    fn read_plain_scoped(&mut self, dst: &mut [u8]) -> i32 {
        if dst.is_empty() {
            return 0;
        }
        if !self.is_init_finished() {
            self.last_want_read = !self.output_pending();
            self.last_want_write = self.output_pending();
            return TLS_RENEGOTIATION_STATUS;
        }
        let result = match self.stream_mut() {
            Ok(stream) => stream.ssl_read(dst),
            Err(error) => return self.error(error),
        };
        match result {
            Ok(0) => TLS_CLOSED_STATUS,
            Ok(read) => i32::try_from(read)
                .unwrap_or_else(|_| self.error("TLS plaintext read size overflow".to_string())),
            Err(error) => self.handle_ssl_data_error(error, "TLS plaintext read failed"),
        }
    }

    pub(crate) fn write_plain(&mut self, input: &mut [u8], src: &[u8], output: &mut [u8]) -> i32 {
        self.with_scoped_io(input, output, |tls| tls.write_plain_scoped(src))
    }

    fn write_plain_scoped(&mut self, src: &[u8]) -> i32 {
        if src.is_empty() {
            return 0;
        }
        if self.shutdown_started {
            self.last_want_read = false;
            self.last_want_write = self.output_pending();
            return TLS_CLOSED_STATUS;
        }
        if !self.is_init_finished() {
            self.last_want_read = false;
            self.last_want_write = self.output_pending();
            return TLS_RENEGOTIATION_STATUS;
        }
        let result = match self.stream_mut() {
            Ok(stream) => stream.ssl_write(src),
            Err(error) => return self.error(error),
        };
        match result {
            Ok(written) => {
                self.last_want_read = false;
                self.last_want_write = self.output_pending();
                i32::try_from(written)
                    .unwrap_or_else(|_| self.error("TLS plaintext write size overflow".to_string()))
            }
            Err(error) => self.handle_ssl_data_error(error, "TLS plaintext write failed"),
        }
    }

    pub(crate) fn wants_read(&self) -> bool {
        self.last_want_read
    }

    pub(crate) fn wants_write(&self) -> bool {
        self.last_want_write
    }

    pub(crate) fn is_handshaking(&self) -> bool {
        !self.is_init_finished()
    }

    pub(crate) fn shutdown(&mut self) -> i32 {
        self.shutdown_started = true;
        0
    }

    pub(crate) fn connect(&mut self, input: &mut [u8], output: &mut [u8]) -> i32 {
        if self.mode != TlsMode::Client {
            return self.error("TLS connect requires a client handle".to_string());
        }
        self.advance_state(input, output)
    }

    pub(crate) fn accept(&mut self, input: &mut [u8], output: &mut [u8]) -> i32 {
        if self.mode != TlsMode::Server {
            return self.error("TLS accept requires a server handle".to_string());
        }
        self.advance_state(input, output)
    }

    pub(crate) fn bytes_read(&self) -> i32 {
        self.last_bytes_read as i32
    }

    pub(crate) fn bytes_to_write(&self) -> i32 {
        self.last_bytes_to_write as i32
    }

    fn advance_state(&mut self, input: &mut [u8], output: &mut [u8]) -> i32 {
        self.with_scoped_io(input, output, |tls| {
            if tls.shutdown_started {
                tls.advance_shutdown()
            } else {
                tls.advance_handshake()
            }
        })
    }

    fn with_scoped_io(
        &mut self,
        input: &mut [u8],
        output: &mut [u8],
        f: impl FnOnce(&mut Self) -> i32,
    ) -> i32 {
        self.last_bytes_read = 0;
        self.last_bytes_to_write = 0;
        let scoped = self
            .state
            .lock()
            .map_err(|_| "TLS BIO state lock poisoned".to_string())
            .and_then(|mut state| state.begin_scoped(input, output));
        if let Err(error) = scoped {
            return self.error(error);
        }

        let status = f(self);

        let counters = self
            .state
            .lock()
            .map_err(|_| "TLS BIO state lock poisoned".to_string())
            .and_then(|mut state| state.end_scoped());
        match counters {
            Ok((bytes_read, bytes_to_write)) => {
                self.last_bytes_read = bytes_read;
                self.last_bytes_to_write = bytes_to_write;
                if bytes_to_write > 0
                    && (status == TLS_WOULD_BLOCK_STATUS || status == TLS_RENEGOTIATION_STATUS)
                {
                    self.last_want_write = true;
                }
                status
            }
            Err(error) => self.error(error),
        }
    }

    fn advance_shutdown(&mut self) -> i32 {
        if self.shutdown_complete {
            self.last_want_read = false;
            self.last_want_write = self.output_pending();
            return 0;
        }
        let result = match self.stream_mut() {
            Ok(stream) => stream.shutdown(),
            Err(error) => return self.error(error),
        };
        match result {
            Ok(_) => {
                self.shutdown_complete = true;
                self.last_want_read = false;
                self.last_want_write = self.output_pending();
                0
            }
            Err(error) => self.handle_ssl_error(error, "TLS shutdown failed"),
        }
    }

    pub(crate) fn peer_certificate(&mut self) -> Result<Option<Vec<u8>>, ()> {
        match self.peer_certificate_der() {
            Ok(certificate) => Ok(certificate),
            Err(error) => {
                self.error(error);
                Err(())
            }
        }
    }

    pub(crate) fn unique_channel_binding(&mut self) -> Result<Option<Vec<u8>>, ()> {
        match self.unique_channel_binding_bytes() {
            Ok(binding) => Ok(binding),
            Err(error) => {
                self.error(error);
                Err(())
            }
        }
    }

    pub(crate) fn server_endpoint_channel_binding(&mut self) -> Result<Option<Vec<u8>>, ()> {
        match self.server_endpoint_channel_binding_bytes() {
            Ok(binding) => Ok(binding),
            Err(error) => {
                self.error(error);
                Err(())
            }
        }
    }

    pub(crate) fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    fn advance_handshake(&mut self) -> i32 {
        if self.is_init_finished() {
            self.last_want_read = false;
            self.last_want_write = self.output_pending();
            return 0;
        }
        let mode = self.mode;
        let result = match self.stream_mut() {
            Ok(stream) => match mode {
                TlsMode::Client => stream.connect(),
                TlsMode::Server => stream.accept(),
            },
            Err(error) => return self.error(error),
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

    fn handle_ssl_data_error(&mut self, error: SslError, context: &str) -> i32 {
        let status = self.handle_ssl_error(error, context);
        if status == TLS_WOULD_BLOCK_STATUS && self.is_handshaking() {
            TLS_RENEGOTIATION_STATUS
        } else {
            status
        }
    }

    fn handle_ssl_error(&mut self, error: SslError, context: &str) -> i32 {
        match error.code() {
            ErrorCode::WANT_READ => {
                self.last_want_read = true;
                self.last_want_write = self.output_pending();
                TLS_WOULD_BLOCK_STATUS
            }
            ErrorCode::WANT_WRITE => {
                self.last_want_read = false;
                self.last_want_write = true;
                TLS_WOULD_BLOCK_STATUS
            }
            ErrorCode::ZERO_RETURN => {
                self.last_want_read = false;
                self.last_want_write = self.output_pending();
                TLS_CLOSED_STATUS
            }
            ErrorCode::SYSCALL
                if error
                    .io_error()
                    .is_some_and(|io| io.kind() == io::ErrorKind::WouldBlock) =>
            {
                match self.last_bio_blocked() {
                    Some(BioBlocked::Write) => {
                        self.last_want_read = false;
                        self.last_want_write = true;
                    }
                    Some(BioBlocked::Read) | None => {
                        self.last_want_read = true;
                        self.last_want_write = self.output_pending();
                    }
                }
                TLS_WOULD_BLOCK_STATUS
            }
            _ => self.error(format!("{context}: {error}")),
        }
    }

    fn peer_certificate_der(&self) -> Result<Option<Vec<u8>>, String> {
        let Some(stream) = &self.stream else {
            return Ok(None);
        };
        stream
            .ssl()
            .peer_certificate()
            .map(|certificate| {
                certificate
                    .to_der()
                    .map_err(|error| format!("failed to encode TLS peer certificate: {error}"))
            })
            .transpose()
    }

    fn unique_channel_binding_bytes(&self) -> Result<Option<Vec<u8>>, String> {
        let Some(stream) = &self.stream else {
            return Ok(None);
        };
        let ssl = stream.ssl();
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

    fn server_endpoint_channel_binding_bytes(&self) -> Result<Option<Vec<u8>>, String> {
        let Some(stream) = &self.stream else {
            return Ok(None);
        };
        match self.mode {
            TlsMode::Client => stream
                .ssl()
                .peer_certificate()
                .as_deref()
                .map(server_endpoint_certificate_hash)
                .transpose(),
            TlsMode::Server => stream
                .ssl()
                .certificate()
                .map(server_endpoint_certificate_hash)
                .transpose(),
        }
    }

    fn is_init_finished(&self) -> bool {
        self.stream
            .as_ref()
            .is_some_and(|stream| stream.ssl().is_init_finished())
    }

    fn stream_mut(&mut self) -> Result<&mut SslStream<QueueStream>, String> {
        if self.stream.is_none() {
            let ssl = self
                .pending_ssl
                .take()
                .ok_or_else(|| "TLS stream is not initialized".to_string())?;
            let stream = QueueStream {
                state: Arc::clone(&self.state),
            };
            self.stream = Some(
                SslStream::new(ssl, stream)
                    .map_err(|error| format!("failed to create TLS stream: {error}"))?,
            );
        }
        self.stream
            .as_mut()
            .ok_or_else(|| "TLS stream is not initialized".to_string())
    }

    fn output_pending(&self) -> bool {
        false
    }

    fn error(&mut self, message: String) -> i32 {
        self.last_error = Some(message);
        TLS_ERROR_STATUS
    }

    fn last_bio_blocked(&self) -> Option<BioBlocked> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.last_blocked())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TlsMode {
    Client,
    Server,
}

#[derive(Debug)]
struct BioState {
    scoped: Option<ScopedBio>,
    last_blocked: Option<BioBlocked>,
}

impl BioState {
    fn new() -> Self {
        Self {
            scoped: None,
            last_blocked: None,
        }
    }

    fn begin_scoped(&mut self, input: &mut [u8], output: &mut [u8]) -> Result<(), String> {
        if self.scoped.is_some() {
            return Err("TLS BIO state already has scoped buffers".to_string());
        }
        self.last_blocked = None;
        self.scoped = Some(ScopedBio::new(input, output));
        Ok(())
    }

    fn end_scoped(&mut self) -> Result<(usize, usize), String> {
        self.scoped
            .take()
            .map(|scoped| (scoped.input_pos, scoped.output_pos))
            .ok_or_else(|| "TLS BIO state has no scoped buffers".to_string())
    }

    fn read_input(&mut self, dst: &mut [u8]) -> usize {
        if let Some(scoped) = &mut self.scoped
            && scoped.input_pos < scoped.input_len
        {
            self.last_blocked = None;
            return scoped.read_input(dst);
        }
        0
    }

    fn input_pending(&self) -> bool {
        self.scoped
            .as_ref()
            .is_some_and(|scoped| scoped.input_pos < scoped.input_len)
    }

    fn write_output(&mut self, src: &[u8]) -> usize {
        if let Some(scoped) = &mut self.scoped {
            let len = scoped.write_output(src);
            if len > 0 || src.is_empty() {
                self.last_blocked = None;
            }
            return len;
        }
        0
    }

    fn block_on_read(&mut self) {
        self.last_blocked = Some(BioBlocked::Read);
    }

    fn block_on_write(&mut self) {
        self.last_blocked = Some(BioBlocked::Write);
    }

    fn last_blocked(&self) -> Option<BioBlocked> {
        self.last_blocked
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BioBlocked {
    Read,
    Write,
}

#[derive(Debug)]
struct ScopedBio {
    input_ptr: *const u8,
    input_len: usize,
    input_pos: usize,
    output_ptr: *mut u8,
    output_len: usize,
    output_pos: usize,
}

// The raw pointers are borrowed guest-memory slices registered only while a
// single synchronous OpenSSL call is active. `QueueStream` reaches them through
// the same mutex and `advance_state` always clears them before returning.
unsafe impl Send for ScopedBio {}

impl ScopedBio {
    fn new(input: &mut [u8], output: &mut [u8]) -> Self {
        Self {
            input_ptr: input.as_ptr(),
            input_len: input.len(),
            input_pos: 0,
            output_ptr: output.as_mut_ptr(),
            output_len: output.len(),
            output_pos: 0,
        }
    }

    fn read_input(&mut self, dst: &mut [u8]) -> usize {
        let len = dst.len().min(self.input_len - self.input_pos);
        if len > 0 {
            // SAFETY: `input_ptr` points to the scoped input slice registered
            // by `begin_scoped`; `input_pos + len` is bounded above.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.input_ptr.add(self.input_pos),
                    dst.as_mut_ptr(),
                    len,
                );
            }
            self.input_pos += len;
        }
        len
    }

    fn write_output(&mut self, src: &[u8]) -> usize {
        let len = src.len().min(self.output_len - self.output_pos);
        if len > 0 {
            // SAFETY: `output_ptr` points to the scoped output slice registered
            // by `begin_scoped`; `output_pos + len` is bounded above.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    src.as_ptr(),
                    self.output_ptr.add(self.output_pos),
                    len,
                );
            }
            self.output_pos += len;
        }
        len
    }
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
        if !state.input_pending() {
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

fn load_system_root_paths(ctx: &mut SslContextBuilder) -> Result<(), openssl::error::ErrorStack> {
    let mut loaded = false;
    let probe = openssl_probe::probe();

    #[cfg(target_os = "macos")]
    let cert_file = probe.cert_file.or_else(|| {
        let cert_file = std::path::PathBuf::from("/etc/ssl/cert.pem");
        cert_file.exists().then_some(cert_file)
    });

    #[cfg(not(target_os = "macos"))]
    let cert_file = probe.cert_file;

    if let Some(cert_file) = cert_file {
        ctx.set_ca_file(&cert_file)?;
        loaded = true;
    }
    for cert_dir in probe.cert_dir {
        ctx.load_verify_locations(None, Some(&cert_dir))?;
        loaded = true;
    }

    if loaded {
        Ok(())
    } else {
        ctx.set_default_verify_paths()
    }
}

fn configure_custom_root_store(ssl: &mut Ssl, roots: &[Vec<u8>]) -> Result<(), String> {
    let mut store = X509StoreBuilder::new()
        .map_err(|error| format!("failed to create TLS root store: {error}"))?;
    for root in roots {
        let cert = X509::from_der(root)
            .map_err(|error| format!("failed to parse TLS root certificate: {error}"))?;
        store
            .add_cert(cert)
            .map_err(|error| format!("failed to add TLS root certificate: {error}"))?;
    }
    ssl.set_verify_cert_store(store.build())
        .map_err(|error| format!("failed to configure TLS root store: {error}"))
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
