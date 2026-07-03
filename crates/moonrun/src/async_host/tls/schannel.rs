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

use std::io;

use crate::async_sys::ported_fns;

use windows_sys::Win32::Foundation::{
    SEC_E_INCOMPLETE_MESSAGE, SEC_E_NO_CREDENTIALS, SEC_E_OK, SEC_I_CONTEXT_EXPIRED,
    SEC_I_CONTINUE_NEEDED, SEC_I_RENEGOTIATE,
};
use windows_sys::Win32::Security::Authentication::Identity::SECPKG_ATTR_ENDPOINT_BINDINGS;
use windows_sys::Win32::Security::Authentication::Identity::{
    ASC_REQ_CONFIDENTIALITY, ASC_REQ_INTEGRITY, AcceptSecurityContext, AcquireCredentialsHandleW,
    ApplyControlToken, DecryptMessage, DeleteSecurityContext, EncryptMessage, FreeContextBuffer,
    FreeCredentialsHandle, ISC_REQ_CONFIDENTIALITY, ISC_REQ_INTEGRITY, InitializeSecurityContextW,
    QueryContextAttributesW, SCH_CRED_FORMAT_CERT_HASH_STORE, SCH_CRED_IGNORE_NO_REVOCATION_CHECK,
    SCH_CRED_MANUAL_CRED_VALIDATION, SCH_CRED_NO_DEFAULT_CREDS, SCH_CREDENTIALS,
    SCH_CREDENTIALS_VERSION, SCH_USE_STRONG_CRYPTO, SCHANNEL_SHUTDOWN, SEC_CHANNEL_BINDINGS,
    SECBUFFER_DATA, SECBUFFER_EMPTY, SECBUFFER_EXTRA, SECBUFFER_STREAM_HEADER,
    SECBUFFER_STREAM_TRAILER, SECBUFFER_TOKEN, SECBUFFER_VERSION, SECPKG_ATTR_REMOTE_CERT_CONTEXT,
    SECPKG_ATTR_STREAM_SIZES, SECPKG_ATTR_UNIQUE_BINDINGS, SECPKG_CRED_INBOUND,
    SECPKG_CRED_OUTBOUND, SecBuffer, SecBufferDesc, SecPkgContext_Bindings,
    SecPkgContext_StreamSizes, TLS_PARAMETERS, UNISP_NAME,
};
use windows_sys::Win32::Security::Credentials::SecHandle;
use windows_sys::Win32::Security::Cryptography::{
    AUTHTYPE_SERVER, CERT_CHAIN_CONTEXT, CERT_CHAIN_ENGINE_CONFIG,
    CERT_CHAIN_EXCLUSIVE_ENABLE_CA_FLAG, CERT_CHAIN_PARA, CERT_CHAIN_POLICY_PARA,
    CERT_CHAIN_POLICY_SSL, CERT_CHAIN_POLICY_STATUS, CERT_CONTEXT, CERT_FIND_HAS_PRIVATE_KEY,
    CERT_STORE_ADD_USE_EXISTING, CERT_STORE_CREATE_NEW_FLAG, CERT_STORE_PROV_MEMORY,
    CRYPT_INTEGER_BLOB, CertAddCertificateContextToStore, CertCloseStore,
    CertCreateCertificateChainEngine, CertCreateCertificateContext, CertFindCertificateInStore,
    CertFreeCertificateChain, CertFreeCertificateChainEngine, CertFreeCertificateContext,
    CertGetCertificateChain, CertOpenStore, CertVerifyCertificateChainPolicy, HCERTCHAINENGINE,
    HCERTSTORE, HTTPSPolicyCallbackData, PFXImportCertStore, PKCS_7_ASN_ENCODING,
    X509_ASN_ENCODING,
};

use super::{
    RingBuffer, TLS_CLOSED_STATUS, TLS_ERROR_STATUS, TLS_PLAINTEXT_INPUT_LIMIT,
    TLS_RENEGOTIATION_STATUS, TLS_WOULD_BLOCK_STATUS, TlsConfig, TlsTrust,
};

pub(crate) struct TlsConnection {
    schannel: SchannelContext,
    plain_input: RingBuffer,
    custom_root_verifier_host: Option<String>,
    peer_verified: bool,
    phase: SchannelPhase,
    // TLS close_notify is a half-close: plaintext writes stop immediately,
    // while reads may still drain the peer's close_notify.
    local_shutdown_started: bool,
    local_shutdown_complete: bool,
    last_error: Option<String>,
    last_want_read: bool,
    last_want_write: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchannelContextState {
    Uninitialized,
    HandleInitialized,
    ContextInitialized,
}

enum SchannelMode {
    Client { host: Option<Vec<u16>> },
    Server,
}

// Native SChannel keeps these fields in `struct Context`; keep the same
// boundary here so state transitions and byte accounting stay reviewable
// against third_party/moonbitlang_async/src/tls/schannel.c.
struct SchannelContext {
    state: SchannelContextState,
    handle: SecHandle,
    context: SecHandle,
    context_attrs: u32,
    bytes_read: usize,
    bytes_to_write: usize,
    stream_sizes: Option<SecPkgContext_StreamSizes>,
    custom_root_store: Option<CertStoreHandle>,
    custom_root_chain_engine: Option<CertChainEngine>,
    mode: SchannelMode,
}

impl Drop for SchannelContext {
    fn drop(&mut self) {
        // Match the native SChannel shim: custom trust state belongs to the
        // context and is released before the security handles.
        drop(self.custom_root_chain_engine.take());
        drop(self.custom_root_store.take());
        unsafe {
            if self.state == SchannelContextState::ContextInitialized {
                DeleteSecurityContext(&self.context);
            }
            if self.state != SchannelContextState::Uninitialized {
                FreeCredentialsHandle(&self.handle);
            }
        }
    }
}

enum SchannelStep {
    Complete,
    NeedMoreInput,
    Continue,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchannelPhase {
    Handshaking,
    Open,
    Closed,
}

impl TlsConnection {
    pub(crate) fn client(host: &str, _sni: bool, config: TlsConfig) -> Result<Self, String> {
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
        // Native Windows currently ignores `sni=false`; the target name is
        // passed to SChannel whenever a host is available.
        let target_host = (!host.is_empty()).then_some(host);
        let mut schannel = SchannelContext::client(target_host, trust == TlsTrust::SystemRoot)?;
        if trust == TlsTrust::CustomRoot {
            for root in &root_certificates {
                schannel
                    .add_root_certificate(root)
                    .map_err(|error| format!("failed to add TLS root certificate: {error}"))?;
            }
        }
        let custom_root_verifier_host = (trust == TlsTrust::CustomRoot).then(|| host.to_string());
        Ok(Self::new(schannel, custom_root_verifier_host))
    }

    pub(crate) fn server(config: TlsConfig) -> Result<Self, String> {
        let pfx_content = match config {
            TlsConfig::ServerPfx { pfx_content } => pfx_content,
            TlsConfig::ServerFiles {
                private_key_file: _private_key_file,
                private_key_type: _private_key_type,
                certificate_file: _certificate_file,
                certificate_type: _certificate_type,
            } => {
                return Err(
                    "SChannel TLS servers require a PKCS#12/PFX certificate context on Windows"
                        .to_string(),
                );
            }
            TlsConfig::Client { .. } => {
                return Err("TLS server requires server configuration".to_string());
            }
        };
        let schannel = SchannelContext::server(&pfx_content)?;
        Ok(Self::new(schannel, None))
    }

    fn new(schannel: SchannelContext, custom_root_verifier_host: Option<String>) -> Self {
        Self {
            schannel,
            plain_input: RingBuffer::new(TLS_PLAINTEXT_INPUT_LIMIT),
            custom_root_verifier_host,
            peer_verified: false,
            phase: SchannelPhase::Handshaking,
            local_shutdown_started: false,
            local_shutdown_complete: false,
            last_error: None,
            last_want_read: false,
            last_want_write: false,
        }
    }

    pub(crate) fn read_plain(
        &mut self,
        input: &mut [u8],
        dst: &mut [u8],
        _output: &mut [u8],
    ) -> i32 {
        self.schannel.bytes_read = 0;
        self.schannel.bytes_to_write = 0;
        if dst.is_empty() {
            return 0;
        }
        if !self.plain_input.is_empty() {
            return self.pop_plain_input(dst);
        }
        if self.phase == SchannelPhase::Closed {
            return TLS_CLOSED_STATUS;
        }
        if self.is_handshaking() {
            self.last_want_read = !self.output_pending();
            self.last_want_write = self.output_pending();
            return TLS_RENEGOTIATION_STATUS;
        }
        match self.decrypt_plain(input, dst) {
            Ok(read) if read > 0 => read,
            Ok(status) if status == TLS_CLOSED_STATUS => TLS_CLOSED_STATUS,
            Ok(status) if status == TLS_WOULD_BLOCK_STATUS => TLS_WOULD_BLOCK_STATUS,
            Ok(status) if status == TLS_RENEGOTIATION_STATUS => TLS_RENEGOTIATION_STATUS,
            Ok(status) if status == TLS_ERROR_STATUS => {
                unreachable!("TLS errors are returned through Err")
            }
            Ok(status) => status,
            Err(error) => self.error(error),
        }
    }

    fn decrypt_plain(&mut self, input: &mut [u8], dst: &mut [u8]) -> Result<i32, String> {
        if input.is_empty() {
            self.last_want_read = true;
            self.last_want_write = self.output_pending();
            return Ok(TLS_WOULD_BLOCK_STATUS);
        }

        let input_len = input.len();
        let mut buffers = [
            sec_buffer(SECBUFFER_DATA, input),
            empty_sec_buffer(),
            empty_sec_buffer(),
            empty_sec_buffer(),
        ];
        let desc = sec_buffer_desc(&mut buffers);
        let status =
            unsafe { DecryptMessage(&self.schannel.context, &desc, 0, std::ptr::null_mut()) };
        match status {
            SEC_E_OK => {
                self.schannel.bytes_read = consumed_len(input_len, &buffers);
                let plain = sec_buffer_slice(&buffers, SECBUFFER_DATA, input).unwrap_or(&[]);
                let plain_is_empty = plain.is_empty();
                let read = plain.len().min(dst.len());
                dst[..read].copy_from_slice(&plain[..read]);
                self.append_plain_input(&plain[read..])?;
                // A zero-length TLS application record is legal. If no
                // plaintext was produced, MoonBit first drops consumed
                // transport bytes and then retries or reads more input.
                self.last_want_read = plain_is_empty
                    && (self.schannel.bytes_read == 0 || self.schannel.bytes_read == input_len);
                self.last_want_write = self.output_pending();
                if read > 0 {
                    i32::try_from(read).map_err(|_| "TLS plaintext read size overflow".to_string())
                } else {
                    Ok(TLS_WOULD_BLOCK_STATUS)
                }
            }
            SEC_E_INCOMPLETE_MESSAGE => {
                self.schannel.bytes_read = 0;
                self.last_want_read = true;
                self.last_want_write = self.output_pending();
                Ok(TLS_WOULD_BLOCK_STATUS)
            }
            SEC_I_CONTEXT_EXPIRED => {
                self.phase = SchannelPhase::Closed;
                self.last_want_read = false;
                self.last_want_write = self.output_pending();
                Ok(TLS_CLOSED_STATUS)
            }
            SEC_I_RENEGOTIATE => {
                self.schannel.bytes_read = 0;
                self.phase = SchannelPhase::Handshaking;
                self.last_want_read = true;
                self.last_want_write = self.output_pending();
                Ok(TLS_RENEGOTIATION_STATUS)
            }
            error => Err(schannel_status_error("TLS plaintext read failed", error)),
        }
    }

    pub(crate) fn write_plain(&mut self, _input: &mut [u8], src: &[u8], output: &mut [u8]) -> i32 {
        self.schannel.bytes_read = 0;
        self.schannel.bytes_to_write = 0;
        if src.is_empty() {
            return 0;
        }
        if self.is_handshaking() {
            self.last_want_read = false;
            self.last_want_write = self.output_pending();
            return TLS_RENEGOTIATION_STATUS;
        }
        if self.phase == SchannelPhase::Closed || self.local_shutdown_started {
            return TLS_CLOSED_STATUS;
        }
        match self.encrypt_plain(src, output) {
            Ok(written) => i32::try_from(written)
                .unwrap_or_else(|_| self.error("TLS plaintext write size overflow".to_string())),
            Err(SchannelIoError {
                kind: SchannelIoKind::WouldBlock,
                ..
            }) => {
                self.last_want_read = false;
                self.last_want_write = true;
                TLS_WOULD_BLOCK_STATUS
            }
            Err(error) => self.error(error.message),
        }
    }

    pub(crate) fn wants_read(&self) -> bool {
        self.last_want_read
    }

    pub(crate) fn wants_write(&self) -> bool {
        self.last_want_write
    }

    pub(crate) fn is_handshaking(&self) -> bool {
        self.phase == SchannelPhase::Handshaking
    }

    pub(crate) fn shutdown(&mut self) -> i32 {
        if self.local_shutdown_complete {
            return 0;
        }
        if !self.local_shutdown_started {
            if let Err(error) = self.apply_schannel_shutdown() {
                return self.error(error);
            }
            self.local_shutdown_started = true;
        }
        0
    }

    pub(crate) fn connect(&mut self, input: &mut [u8], output: &mut [u8]) -> i32 {
        if !self.schannel.is_client() {
            return self.error("TLS connect requires a client handle".to_string());
        }
        self.advance_state(input, output)
    }

    pub(crate) fn accept(&mut self, input: &mut [u8], output: &mut [u8]) -> i32 {
        if self.schannel.is_client() {
            return self.error("TLS accept requires a server handle".to_string());
        }
        self.advance_state(input, output)
    }

    pub(crate) fn bytes_read(&self) -> i32 {
        self.schannel.bytes_read as i32
    }

    pub(crate) fn bytes_to_write(&self) -> i32 {
        self.schannel.bytes_to_write as i32
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

    fn advance_state(&mut self, input: &mut [u8], output: &mut [u8]) -> i32 {
        self.schannel.bytes_read = 0;
        self.schannel.bytes_to_write = 0;
        if self.local_shutdown_started && !self.local_shutdown_complete {
            return match self.drive_shutdown(input, output) {
                Ok(true) => 0,
                Ok(false) => TLS_WOULD_BLOCK_STATUS,
                Err(error) => self.error(error),
            };
        }
        if self.phase == SchannelPhase::Open {
            self.last_want_read = false;
            self.last_want_write = self.generated_output_pending();
            return 0;
        }
        if self.phase == SchannelPhase::Closed {
            self.last_want_read = false;
            self.last_want_write = self.generated_output_pending();
            return if self.local_shutdown_complete {
                0
            } else {
                TLS_CLOSED_STATUS
            };
        }
        match self.step_handshake(input, output) {
            Ok(SchannelStep::Complete) => {
                self.last_want_read = false;
                self.last_want_write = self.generated_output_pending();
                if let Err(error) = self.verify_peer_if_needed() {
                    return self.error(error);
                }
                self.phase = SchannelPhase::Open;
                0
            }
            Ok(SchannelStep::NeedMoreInput) => {
                self.phase = SchannelPhase::Handshaking;
                self.last_want_read = true;
                self.last_want_write = self.generated_output_pending();
                TLS_WOULD_BLOCK_STATUS
            }
            Ok(SchannelStep::Continue) => {
                self.phase = SchannelPhase::Handshaking;
                self.last_want_write = self.generated_output_pending();
                self.last_want_read = !self.last_want_write;
                TLS_WOULD_BLOCK_STATUS
            }
            Ok(SchannelStep::Closed) => {
                self.phase = SchannelPhase::Closed;
                self.last_want_read = false;
                self.last_want_write = self.generated_output_pending();
                TLS_CLOSED_STATUS
            }
            Err(error) => self.error(error),
        }
    }

    fn peer_certificate_der(&mut self) -> Result<Option<Vec<u8>>, String> {
        self.peer_certificate_context()
            .map(|certificate| certificate.map(|certificate| certificate.to_der().to_vec()))
    }

    fn unique_channel_binding_bytes(&self) -> Result<Option<Vec<u8>>, String> {
        self.schannel.unique_channel_binding()
    }

    fn server_endpoint_channel_binding_bytes(&self) -> Result<Option<Vec<u8>>, String> {
        self.schannel.server_endpoint_channel_binding()
    }

    fn verify_peer_if_needed(&mut self) -> Result<(), String> {
        if self.peer_verified {
            return Ok(());
        }
        let Some(host) = self.custom_root_verifier_host.clone() else {
            self.peer_verified = true;
            return Ok(());
        };
        self.schannel.verify_peer_certificate(&host)?;
        self.peer_verified = true;
        Ok(())
    }

    fn step_handshake(
        &mut self,
        guest_input: &mut [u8],
        output: &mut [u8],
    ) -> Result<SchannelStep, String> {
        let input_len = guest_input.len();
        // Native SChannel receives a pointer into a buffer even when the
        // logical input length is zero.
        let mut empty_input = [0];
        let input = if guest_input.is_empty() {
            &mut empty_input[..]
        } else {
            guest_input
        };

        let mut output_buffers = [sec_buffer(SECBUFFER_TOKEN, output)];
        let mut output_desc = sec_buffer_desc(&mut output_buffers);

        let (status, bytes_read, bytes_to_write, is_client_step) = match &self.schannel.mode {
            SchannelMode::Client { host } => {
                let mut input_buffers = [
                    sec_buffer_with_len(SECBUFFER_TOKEN, input, input_len),
                    empty_sec_buffer(),
                ];
                let input_desc = sec_buffer_desc(&mut input_buffers);
                let context_ptr = if self.schannel.is_context_initialized() {
                    &self.schannel.context as *const _
                } else {
                    std::ptr::null()
                };
                let input_desc_ptr = if self.schannel.is_context_initialized() {
                    &input_desc as *const _
                } else {
                    std::ptr::null()
                };
                let status = unsafe {
                    InitializeSecurityContextW(
                        &self.schannel.handle,
                        context_ptr,
                        host.as_ref()
                            .map(|host| host.as_ptr())
                            .unwrap_or(std::ptr::null()),
                        ISC_REQ_CONFIDENTIALITY | ISC_REQ_INTEGRITY,
                        0,
                        0,
                        input_desc_ptr,
                        0,
                        &mut self.schannel.context,
                        &mut output_desc,
                        &mut self.schannel.context_attrs,
                        std::ptr::null_mut(),
                    )
                };
                (
                    status,
                    consumed_len(input_len, &input_buffers),
                    sec_buffer_slice(&output_buffers, SECBUFFER_TOKEN, output)
                        .map(|output| output.len())
                        .unwrap_or(0),
                    true,
                )
            }
            SchannelMode::Server => {
                let mut input_buffers = [
                    sec_buffer_with_len(SECBUFFER_TOKEN, input, input_len),
                    empty_sec_buffer(),
                ];
                let input_desc = sec_buffer_desc(&mut input_buffers);
                let context_ptr = if self.schannel.is_context_initialized() {
                    &self.schannel.context as *const _
                } else {
                    std::ptr::null()
                };
                let status = unsafe {
                    AcceptSecurityContext(
                        &self.schannel.handle,
                        context_ptr,
                        &input_desc,
                        ASC_REQ_CONFIDENTIALITY | ASC_REQ_INTEGRITY,
                        0,
                        &mut self.schannel.context,
                        &mut output_desc,
                        &mut self.schannel.context_attrs,
                        std::ptr::null_mut(),
                    )
                };
                (
                    status,
                    consumed_len(input_len, &input_buffers),
                    sec_buffer_slice(&output_buffers, SECBUFFER_TOKEN, output)
                        .map(|output| output.len())
                        .unwrap_or(0),
                    false,
                )
            }
        };
        self.finish_handshake_step(status, bytes_read, bytes_to_write, is_client_step)
    }

    fn finish_handshake_step(
        &mut self,
        status: i32,
        bytes_read: usize,
        bytes_to_write: usize,
        is_client_step: bool,
    ) -> Result<SchannelStep, String> {
        match status {
            SEC_E_OK | SEC_I_CONTINUE_NEEDED | SEC_I_CONTEXT_EXPIRED => {
                self.mark_context_initialized_after_handshake_step(is_client_step);
                self.schannel.bytes_read = bytes_read;
                self.schannel.bytes_to_write = if status == SEC_I_CONTEXT_EXPIRED && !is_client_step
                {
                    0
                } else {
                    bytes_to_write
                };
                if status == SEC_E_OK {
                    self.schannel.stream_sizes = Some(query_stream_sizes(&self.schannel.context)?);
                    Ok(SchannelStep::Complete)
                } else if status == SEC_I_CONTEXT_EXPIRED {
                    Ok(SchannelStep::Closed)
                } else {
                    Ok(SchannelStep::Continue)
                }
            }
            SEC_E_INCOMPLETE_MESSAGE => {
                self.mark_context_initialized_after_handshake_step(is_client_step);
                self.schannel.bytes_read = 0;
                self.schannel.bytes_to_write = 0;
                Ok(SchannelStep::NeedMoreInput)
            }
            error => Err(schannel_status_error(
                if is_client_step {
                    "TLS client handshake failed"
                } else {
                    "TLS server handshake failed"
                },
                error,
            )),
        }
    }

    fn mark_context_initialized_after_handshake_step(&mut self, is_client_step: bool) {
        // Match the native SChannel shim: clients transition after any
        // non-error step, while servers wait until AcceptSecurityContext
        // publishes a real context handle for an incomplete ClientHello.
        if self.schannel.state == SchannelContextState::HandleInitialized
            && (is_client_step
                || self.schannel.context.dwLower != 0
                || self.schannel.context.dwUpper != 0)
        {
            self.schannel.state = SchannelContextState::ContextInitialized;
        }
    }

    fn encrypt_plain(&mut self, src: &[u8], output: &mut [u8]) -> Result<usize, SchannelIoError> {
        if src.is_empty() {
            return Ok(0);
        }
        let sizes = self.schannel.stream_sizes.ok_or_else(|| {
            SchannelIoError::fatal("TLS plaintext write attempted before handshake completed")
        })?;
        let overhead = sizes.cbHeader as usize + sizes.cbTrailer as usize;
        let output_remaining = output.len();
        if output_remaining <= overhead {
            return Err(SchannelIoError::would_block());
        }
        let len = src
            .len()
            .min(sizes.cbMaximumMessage as usize)
            .min(output_remaining - overhead);
        output[sizes.cbHeader as usize..sizes.cbHeader as usize + len].copy_from_slice(&src[..len]);
        let header_len = sizes.cbHeader as usize;
        let trailer_start = header_len + len;
        let mut buffers = [
            SecBuffer {
                cbBuffer: sizes.cbHeader,
                BufferType: SECBUFFER_STREAM_HEADER,
                pvBuffer: output.as_mut_ptr().cast(),
            },
            SecBuffer {
                cbBuffer: u32::try_from(len)
                    .map_err(|_| SchannelIoError::fatal("TLS plaintext write size overflow"))?,
                BufferType: SECBUFFER_DATA,
                pvBuffer: unsafe { output.as_mut_ptr().add(header_len) }.cast(),
            },
            SecBuffer {
                cbBuffer: sizes.cbTrailer,
                BufferType: SECBUFFER_STREAM_TRAILER,
                pvBuffer: unsafe { output.as_mut_ptr().add(trailer_start) }.cast(),
            },
            empty_sec_buffer(),
        ];
        let desc = sec_buffer_desc(&mut buffers);
        let status = unsafe { EncryptMessage(&self.schannel.context, 0, &desc, 0) };
        if status != SEC_E_OK {
            return Err(SchannelIoError::fatal(schannel_status_error(
                "TLS plaintext write failed",
                status,
            )));
        }
        let encrypted_len = buffers[0].cbBuffer as usize
            + buffers[1].cbBuffer as usize
            + buffers[2].cbBuffer as usize;
        self.schannel.bytes_to_write = encrypted_len;
        self.last_want_read = false;
        self.last_want_write = true;
        Ok(len)
    }

    fn apply_schannel_shutdown(&mut self) -> Result<(), String> {
        if !self.schannel.is_context_initialized() {
            self.local_shutdown_complete = true;
            return Ok(());
        }
        let mut shutdown = SCHANNEL_SHUTDOWN;
        let mut buffers = [SecBuffer {
            cbBuffer: std::mem::size_of::<u32>() as u32,
            BufferType: SECBUFFER_TOKEN,
            pvBuffer: (&mut shutdown as *mut u32).cast(),
        }];
        let desc = sec_buffer_desc(&mut buffers);
        let status = unsafe { ApplyControlToken(&self.schannel.context, &desc) };
        if status != SEC_E_OK {
            return Err(schannel_status_error("TLS shutdown failed", status));
        }
        Ok(())
    }

    fn drive_shutdown(&mut self, input: &mut [u8], output: &mut [u8]) -> Result<bool, String> {
        match self.step_handshake(input, output)? {
            SchannelStep::Complete => {
                self.phase = SchannelPhase::Open;
                self.local_shutdown_complete = true;
                self.last_want_read = false;
                self.last_want_write = self.generated_output_pending();
                Ok(true)
            }
            SchannelStep::Closed => {
                self.phase = SchannelPhase::Closed;
                self.local_shutdown_complete = true;
                self.last_want_read = false;
                self.last_want_write = self.generated_output_pending();
                Ok(true)
            }
            SchannelStep::NeedMoreInput => {
                self.phase = SchannelPhase::Handshaking;
                self.last_want_read = true;
                self.last_want_write = self.generated_output_pending();
                Ok(false)
            }
            SchannelStep::Continue => {
                self.phase = SchannelPhase::Handshaking;
                self.last_want_write = self.generated_output_pending();
                self.last_want_read = !self.last_want_write;
                Ok(false)
            }
        }
    }

    fn peer_certificate_context(&self) -> Result<Option<CertContextHandle>, String> {
        self.schannel.peer_certificate_context()
    }

    fn append_plain_input(&mut self, plain: &[u8]) -> Result<(), String> {
        if plain.len() > self.plain_input.remaining() {
            return Err("TLS plaintext input buffer limit exceeded".to_string());
        }
        self.plain_input.push(plain);
        Ok(())
    }

    fn pop_plain_input(&mut self, dst: &mut [u8]) -> i32 {
        let read = self.plain_input.pop(dst);
        i32::try_from(read)
            .unwrap_or_else(|_| self.error("TLS plaintext read size overflow".to_string()))
    }

    fn output_pending(&self) -> bool {
        false
    }

    fn generated_output_pending(&self) -> bool {
        self.output_pending() || self.schannel.bytes_to_write > 0
    }

    fn error(&mut self, message: String) -> i32 {
        self.last_error = Some(message);
        TLS_ERROR_STATUS
    }
}

impl SchannelContext {
    fn new(mode: SchannelMode) -> Self {
        schannel_new(mode)
    }

    fn client(host: Option<&str>, verify: bool) -> Result<Self, String> {
        let mut context = Self::new(SchannelMode::Client {
            host: host.map(|host| host.encode_utf16().chain(Some(0)).collect()),
        });
        context.init_client(verify)?;
        Ok(context)
    }

    fn init_client(&mut self, verify: bool) -> Result<(), String> {
        schannel_init_client(self, verify)
    }

    fn server(pfx_content: &[u8]) -> Result<Self, String> {
        let mut context = Self::new(SchannelMode::Server);
        context.init_server(pfx_content)?;
        Ok(context)
    }

    fn init_server(&mut self, pfx_content: &[u8]) -> Result<(), String> {
        schannel_init_server(self, pfx_content)
    }

    fn is_context_initialized(&self) -> bool {
        self.state == SchannelContextState::ContextInitialized
    }

    fn is_client(&self) -> bool {
        matches!(self.mode, SchannelMode::Client { .. })
    }

    fn add_root_certificate(&mut self, der: &[u8]) -> io::Result<()> {
        schannel_add_root_certificate(self, der)
    }

    fn verify_peer_certificate(&mut self, host_name: &str) -> Result<(), String> {
        schannel_verify_peer_certificate(self, host_name)
    }

    fn peer_certificate_context(&self) -> Result<Option<CertContextHandle>, String> {
        if !self.is_context_initialized() {
            return Ok(None);
        }
        let mut certificate: *mut CERT_CONTEXT = std::ptr::null_mut();
        let status = unsafe {
            QueryContextAttributesW(
                &self.context,
                SECPKG_ATTR_REMOTE_CERT_CONTEXT,
                &mut certificate as *mut _ as _,
            )
        };
        match status {
            SEC_E_OK if certificate.is_null() => Ok(None),
            SEC_E_OK => Ok(Some(CertContextHandle(certificate))),
            SEC_E_NO_CREDENTIALS => Ok(None),
            error => Err(schannel_status_error(
                "failed to get TLS peer certificate",
                error,
            )),
        }
    }

    fn unique_channel_binding(&self) -> Result<Option<Vec<u8>>, String> {
        self.channel_binding(
            SECPKG_ATTR_UNIQUE_BINDINGS,
            "failed to get TLS unique channel binding",
        )
    }

    fn server_endpoint_channel_binding(&self) -> Result<Option<Vec<u8>>, String> {
        self.channel_binding(
            SECPKG_ATTR_ENDPOINT_BINDINGS,
            "failed to get TLS server endpoint channel binding",
        )
    }

    fn channel_binding(
        &self,
        attribute: u32,
        error_context: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        if !self.is_context_initialized() {
            return Ok(None);
        }
        let mut bindings = SecPkgContext_Bindings {
            BindingsLength: 0,
            Bindings: std::ptr::null_mut(),
        };
        let status = unsafe {
            QueryContextAttributesW(&self.context, attribute, &mut bindings as *mut _ as _)
        };
        if status != SEC_E_OK {
            return Err(schannel_status_error(error_context, status));
        }
        if bindings.Bindings.is_null() {
            return Ok(None);
        }

        let bindings = ChannelBindingsHandle {
            bindings: bindings.Bindings,
            len: bindings.BindingsLength as usize,
        };
        let binding = unsafe { bindings.application_data() }?;
        Ok(Some(binding.to_vec()))
    }
}

ported_fns! {
    #[ported(source = "src/tls/schannel.c", original = "moonbitlang_async_schannel_new")]
    fn schannel_new(mode: SchannelMode) -> SchannelContext {
        SchannelContext {
            state: SchannelContextState::Uninitialized,
            handle: zeroed_sec_handle(),
            context: zeroed_sec_handle(),
            context_attrs: 0,
            bytes_read: 0,
            bytes_to_write: 0,
            stream_sizes: None,
            custom_root_store: None,
            custom_root_chain_engine: None,
            mode,
        }
    }

    #[ported(source = "src/tls/schannel.c", original = "moonbitlang_async_schannel_init_client")]
    fn schannel_init_client(ctx: &mut SchannelContext, verify: bool) -> Result<(), String> {
        let mut tls_param: TLS_PARAMETERS = unsafe { std::mem::zeroed() };
        tls_param.grbitDisabledProtocols =
            windows_sys::Win32::Security::Authentication::Identity::SP_PROT_TLS1_CLIENT;

        let mut auth_data: SCH_CREDENTIALS = unsafe { std::mem::zeroed() };
        auth_data.dwVersion = SCH_CREDENTIALS_VERSION;
        auth_data.dwFlags = SCH_CRED_IGNORE_NO_REVOCATION_CHECK | SCH_CRED_NO_DEFAULT_CREDS;
        if !verify {
            auth_data.dwFlags |= SCH_CRED_MANUAL_CRED_VALIDATION;
        }
        auth_data.cTlsParameters = 1;
        auth_data.pTlsParameters = &mut tls_param;
        let mut handle = zeroed_sec_handle();
        let status = unsafe {
            AcquireCredentialsHandleW(
                std::ptr::null(),
                UNISP_NAME,
                SECPKG_CRED_OUTBOUND,
                std::ptr::null(),
                &mut auth_data as *mut _ as _,
                None,
                std::ptr::null(),
                &mut handle,
                std::ptr::null_mut(),
            )
        };
        if status == SEC_E_OK {
            ctx.handle = handle;
            ctx.state = SchannelContextState::HandleInitialized;
            Ok(())
        } else {
            Err(schannel_status_error(
                "failed to create TLS client credentials",
                status,
            ))
        }
    }

    #[ported(source = "src/tls/schannel.c", original = "moonbitlang_async_schannel_init_server")]
    fn schannel_init_server(ctx: &mut SchannelContext, pfx_content: &[u8]) -> Result<(), String> {
        const ENCODING_TYPE: u32 = PKCS_7_ASN_ENCODING | X509_ASN_ENCODING;

        let pfx_len = u32::try_from(pfx_content.len())
            .map_err(|_| "TLS PFX file is too large".to_string())?;
        let pfx_store = CRYPT_INTEGER_BLOB {
            cbData: pfx_len,
            pbData: pfx_content.as_ptr().cast_mut(),
        };
        let cert_store = unsafe { PFXImportCertStore(&pfx_store, std::ptr::null(), 0) };
        if cert_store.is_null() {
            return Err(format!(
                "failed to import TLS PFX certificate: {}",
                io::Error::last_os_error()
            ));
        }
        let cert_store = CertStoreHandle(cert_store);

        let cert = unsafe {
            CertFindCertificateInStore(
                cert_store.as_ptr(),
                ENCODING_TYPE,
                0,
                CERT_FIND_HAS_PRIVATE_KEY,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        if cert.is_null() {
            return Err(format!(
                "no certificate with private key found in TLS PFX file: {}",
                io::Error::last_os_error()
            ));
        }
        let certificate = CertContextHandle(cert);
        let mut certificate_ptr = certificate.as_ptr();

        let mut tls_param: TLS_PARAMETERS = unsafe { std::mem::zeroed() };
        tls_param.grbitDisabledProtocols =
            windows_sys::Win32::Security::Authentication::Identity::SP_PROT_TLS1_0_SERVER
                | windows_sys::Win32::Security::Authentication::Identity::SP_PROT_TLS1_1_SERVER;

        let mut auth_data: SCH_CREDENTIALS = unsafe { std::mem::zeroed() };
        auth_data.dwVersion = SCH_CREDENTIALS_VERSION;
        // Keep the same credential format as the native SChannel shim.
        auth_data.cCreds = 1;
        auth_data.dwCredFormat = SCH_CRED_FORMAT_CERT_HASH_STORE;
        auth_data.paCred = &mut certificate_ptr;
        auth_data.dwFlags = SCH_USE_STRONG_CRYPTO;
        auth_data.cTlsParameters = 1;
        auth_data.pTlsParameters = &mut tls_param;
        let mut handle = zeroed_sec_handle();
        let status = unsafe {
            AcquireCredentialsHandleW(
                std::ptr::null(),
                UNISP_NAME,
                SECPKG_CRED_INBOUND,
                std::ptr::null(),
                &mut auth_data as *mut _ as _,
                None,
                std::ptr::null(),
                &mut handle,
                std::ptr::null_mut(),
            )
        };
        drop(certificate);
        drop(cert_store);
        if status == SEC_E_OK {
            ctx.handle = handle;
            ctx.state = SchannelContextState::HandleInitialized;
            Ok(())
        } else {
            Err(schannel_status_error(
                "failed to create TLS server credentials",
                status,
            ))
        }
    }

    #[ported(source = "src/tls/schannel.c", original = "get_or_create_custom_root_store")]
    fn get_or_create_custom_root_store(
        ctx: &mut SchannelContext,
    ) -> io::Result<&mut CertStoreHandle> {
        if ctx.custom_root_store.is_none() {
            ctx.custom_root_store = Some(CertStoreHandle::memory()?);
        }
        Ok(ctx
            .custom_root_store
            .as_mut()
            .expect("custom root store was just initialized"))
    }

    #[ported(source = "src/tls/schannel.c", original = "get_or_create_custom_root_chain_engine")]
    fn get_or_create_custom_root_chain_engine(
        ctx: &mut SchannelContext,
    ) -> io::Result<HCERTCHAINENGINE> {
        if ctx.custom_root_chain_engine.is_none() {
            let mut chain_engine = 0;
            let mut chain_config: CERT_CHAIN_ENGINE_CONFIG = unsafe { std::mem::zeroed() };
            chain_config.cbSize = std::mem::size_of::<CERT_CHAIN_ENGINE_CONFIG>() as u32;
            chain_config.hExclusiveRoot = ctx
                .custom_root_store
                .as_ref()
                .map(CertStoreHandle::as_ptr)
                .unwrap_or(std::ptr::null_mut());
            chain_config.dwExclusiveFlags = CERT_CHAIN_EXCLUSIVE_ENABLE_CA_FLAG;
            let ok = unsafe { CertCreateCertificateChainEngine(&chain_config, &mut chain_engine) };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            ctx.custom_root_chain_engine = Some(CertChainEngine(chain_engine));
        }
        Ok(ctx
            .custom_root_chain_engine
            .as_ref()
            .expect("custom root chain engine was just initialized")
            .as_ptr())
    }

    #[ported(source = "src/tls/schannel.c", original = "moonbitlang_async_schannel_add_root_certificate")]
    fn schannel_add_root_certificate(ctx: &mut SchannelContext, der: &[u8]) -> io::Result<()> {
        let cert = CertContextHandle::from_der(der)?;
        get_or_create_custom_root_store(ctx)?;
        ctx.custom_root_chain_engine.take();
        ctx.custom_root_store
            .as_mut()
            .expect("custom root store was just initialized")
            .add_certificate_context(&cert)
    }

    #[ported(source = "src/tls/schannel.c", original = "moonbitlang_async_schannel_verify_peer_certificate")]
    fn schannel_verify_peer_certificate(
        ctx: &mut SchannelContext,
        host_name: &str,
    ) -> Result<(), String> {
        let mut certificate: *mut CERT_CONTEXT = std::ptr::null_mut();
        let status = unsafe {
            QueryContextAttributesW(
                &ctx.context,
                SECPKG_ATTR_REMOTE_CERT_CONTEXT,
                &mut certificate as *mut _ as _,
            )
        };
        if status != SEC_E_OK {
            return Err(schannel_status_error(
                "failed to get TLS peer certificate",
                status,
            ));
        }
        let certificate = CertContextHandle(certificate);
        let chain_engine = get_or_create_custom_root_chain_engine(ctx)
            .map_err(|error| format!("failed to create TLS custom root chain engine: {error}"))?;

        let mut chain: *mut CERT_CHAIN_CONTEXT = std::ptr::null_mut();
        let mut chain_para: CERT_CHAIN_PARA = unsafe { std::mem::zeroed() };
        chain_para.cbSize = std::mem::size_of::<CERT_CHAIN_PARA>() as u32;
        let ok = unsafe {
            CertGetCertificateChain(
                chain_engine,
                certificate.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                &chain_para,
                0,
                std::ptr::null(),
                &mut chain,
            )
        };
        if ok == 0 {
            return Err(format!(
                "failed to build TLS peer certificate chain: {}",
                io::Error::last_os_error()
            ));
        }
        let chain = CertChain(chain);

        let mut host = host_name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let mut ssl_policy: HTTPSPolicyCallbackData = unsafe { std::mem::zeroed() };
        ssl_policy.Anonymous.cbSize = std::mem::size_of::<HTTPSPolicyCallbackData>() as u32;
        ssl_policy.dwAuthType = AUTHTYPE_SERVER;
        ssl_policy.fdwChecks = 0;
        if !host_name.is_empty() {
            ssl_policy.pwszServerName = host.as_mut_ptr();
        }

        let mut policy_para: CERT_CHAIN_POLICY_PARA = unsafe { std::mem::zeroed() };
        policy_para.cbSize = std::mem::size_of::<CERT_CHAIN_POLICY_PARA>() as u32;
        policy_para.pvExtraPolicyPara = &mut ssl_policy as *mut _ as _;
        let mut policy_status: CERT_CHAIN_POLICY_STATUS = unsafe { std::mem::zeroed() };
        policy_status.cbSize = std::mem::size_of::<CERT_CHAIN_POLICY_STATUS>() as u32;
        let ok = unsafe {
            CertVerifyCertificateChainPolicy(
                CERT_CHAIN_POLICY_SSL,
                chain.0,
                &policy_para,
                &mut policy_status,
            )
        };
        if ok == 0 {
            return Err(format!(
                "failed to verify TLS peer certificate chain policy: {}",
                io::Error::last_os_error()
            ));
        }
        if policy_status.dwError != 0 {
            return Err(format!(
                "TLS peer certificate verification failed: {}",
                io::Error::from_raw_os_error(policy_status.dwError as i32)
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
const _: &[crate::async_sys::PortedSymbol] = PORTED_SYMBOLS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchannelIoKind {
    WouldBlock,
    Fatal,
}

struct SchannelIoError {
    kind: SchannelIoKind,
    message: String,
}

impl SchannelIoError {
    fn would_block() -> Self {
        Self {
            kind: SchannelIoKind::WouldBlock,
            message: String::new(),
        }
    }

    fn fatal(message: impl Into<String>) -> Self {
        Self {
            kind: SchannelIoKind::Fatal,
            message: message.into(),
        }
    }
}

fn zeroed_sec_handle() -> SecHandle {
    SecHandle {
        dwLower: 0,
        dwUpper: 0,
    }
}

fn sec_buffer(buffer_type: u32, buffer: &mut [u8]) -> SecBuffer {
    sec_buffer_with_len(buffer_type, buffer, buffer.len())
}

fn sec_buffer_with_len(buffer_type: u32, buffer: &mut [u8], len: usize) -> SecBuffer {
    SecBuffer {
        cbBuffer: len as u32,
        BufferType: buffer_type,
        pvBuffer: buffer.as_mut_ptr().cast(),
    }
}

fn empty_sec_buffer() -> SecBuffer {
    SecBuffer {
        cbBuffer: 0,
        BufferType: SECBUFFER_EMPTY,
        pvBuffer: std::ptr::null_mut(),
    }
}

fn sec_buffer_desc(buffers: &mut [SecBuffer]) -> SecBufferDesc {
    SecBufferDesc {
        ulVersion: SECBUFFER_VERSION,
        cBuffers: buffers.len() as u32,
        pBuffers: buffers.as_mut_ptr(),
    }
}

fn consumed_len(input_len: usize, buffers: &[SecBuffer]) -> usize {
    buffers
        .iter()
        .find(|buffer| buffer.BufferType == SECBUFFER_EXTRA)
        .map(|buffer| input_len.saturating_sub(buffer.cbBuffer as usize))
        .unwrap_or(input_len)
}

fn sec_buffer_slice<'a>(
    buffers: &[SecBuffer],
    buffer_type: u32,
    backing: &'a [u8],
) -> Option<&'a [u8]> {
    let base = backing.as_ptr() as usize;
    let end = base.checked_add(backing.len())?;
    buffers
        .iter()
        .find(|buffer| buffer.BufferType == buffer_type && !buffer.pvBuffer.is_null())
        .and_then(|buffer| {
            let start = buffer.pvBuffer as usize;
            let len = buffer.cbBuffer as usize;
            let slice_end = start.checked_add(len)?;
            if start >= base && slice_end <= end {
                let offset = start - base;
                Some(&backing[offset..offset + len])
            } else {
                None
            }
        })
}

fn query_stream_sizes(context: &SecHandle) -> Result<SecPkgContext_StreamSizes, String> {
    let mut sizes: SecPkgContext_StreamSizes = unsafe { std::mem::zeroed() };
    let status = unsafe {
        QueryContextAttributesW(context, SECPKG_ATTR_STREAM_SIZES, &mut sizes as *mut _ as _)
    };
    if status == SEC_E_OK {
        Ok(sizes)
    } else {
        Err(schannel_status_error(
            "failed to query TLS stream sizes",
            status,
        ))
    }
}

fn schannel_status_error(context: &str, status: i32) -> String {
    format!(
        "{context}: SChannel status 0x{:08x}: {}",
        status as u32,
        io::Error::from_raw_os_error(status)
    )
}

struct CertStoreHandle(HCERTSTORE);

// These are owned Windows CryptoAPI handles with no thread-affine callbacks.
// Moonrun stores TLS connections behind a mutex, so moving the owner across
// threads does not introduce concurrent access to the underlying handle.
unsafe impl Send for CertStoreHandle {}

impl CertStoreHandle {
    fn memory() -> io::Result<Self> {
        let store = unsafe {
            CertOpenStore(
                CERT_STORE_PROV_MEMORY,
                0,
                0,
                CERT_STORE_CREATE_NEW_FLAG,
                std::ptr::null(),
            )
        };
        if store.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(store))
        }
    }

    fn add_certificate_context(&mut self, cert: &CertContextHandle) -> io::Result<()> {
        let ok = unsafe {
            CertAddCertificateContextToStore(
                self.0,
                cert.as_ptr(),
                CERT_STORE_ADD_USE_EXISTING,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn as_ptr(&self) -> HCERTSTORE {
        self.0
    }
}

impl Drop for CertStoreHandle {
    fn drop(&mut self) {
        unsafe {
            CertCloseStore(self.0, 0);
        }
    }
}

struct CertContextHandle(*mut CERT_CONTEXT);

impl CertContextHandle {
    fn from_der(cert: &[u8]) -> io::Result<Self> {
        let cert_len = u32::try_from(cert.len()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "TLS certificate is too large")
        })?;
        let context = unsafe {
            CertCreateCertificateContext(
                X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
                cert.as_ptr(),
                cert_len,
            )
        };
        if context.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(context))
        }
    }

    fn as_ptr(&self) -> *mut CERT_CONTEXT {
        self.0
    }

    fn to_der(&self) -> &[u8] {
        let context = unsafe { &*self.0 };
        unsafe { std::slice::from_raw_parts(context.pbCertEncoded, context.cbCertEncoded as usize) }
    }
}

impl Drop for CertContextHandle {
    fn drop(&mut self) {
        unsafe {
            CertFreeCertificateContext(self.0);
        }
    }
}

struct ChannelBindingsHandle {
    bindings: *mut SEC_CHANNEL_BINDINGS,
    len: usize,
}

impl ChannelBindingsHandle {
    unsafe fn application_data(&self) -> Result<&[u8], String> {
        let bindings = unsafe { &*self.bindings };
        let offset = usize::try_from(bindings.dwApplicationDataOffset)
            .map_err(|_| "TLS channel binding offset overflow".to_string())?;
        let len = usize::try_from(bindings.cbApplicationDataLength)
            .map_err(|_| "TLS channel binding size overflow".to_string())?;
        let end = offset
            .checked_add(len)
            .ok_or_else(|| "TLS channel binding bounds overflow".to_string())?;
        if end > self.len {
            return Err("TLS channel binding application data is out of bounds".to_string());
        }
        Ok(unsafe { std::slice::from_raw_parts((self.bindings as *const u8).add(offset), len) })
    }
}

impl Drop for ChannelBindingsHandle {
    fn drop(&mut self) {
        unsafe {
            FreeContextBuffer(self.bindings.cast());
        }
    }
}

struct CertChainEngine(HCERTCHAINENGINE);

// The chain engine is owned by the SChannel context and only accessed through
// the locked TLS connection. It is safe to move that owner between threads.
unsafe impl Send for CertChainEngine {}

impl CertChainEngine {
    fn as_ptr(&self) -> HCERTCHAINENGINE {
        self.0
    }
}

impl Drop for CertChainEngine {
    fn drop(&mut self) {
        unsafe {
            CertFreeCertificateChainEngine(self.0);
        }
    }
}

struct CertChain(*mut CERT_CHAIN_CONTEXT);

impl Drop for CertChain {
    fn drop(&mut self) {
        unsafe {
            CertFreeCertificateChain(self.0);
        }
    }
}
