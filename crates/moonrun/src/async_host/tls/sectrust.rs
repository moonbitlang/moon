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

use std::sync::LazyLock;

use openssl::{
    error::ErrorStack,
    ex_data::Index,
    ssl::{Ssl, SslContextBuilder, SslRef},
};
use security_framework::{
    certificate::SecCertificate, policy::SecPolicy, secure_transport::SslProtocolSide,
    trust::SecTrust,
};

pub(super) struct ServerTrust {
    hostname: Option<String>,
    pub(super) error: Option<String>,
    // Test anchors belong only to this evaluation; never modify system trust.
    #[cfg(test)]
    pub(super) test_anchors: Vec<SecCertificate>,
}

// Allocate one typed slot, not a new OpenSSL ex-data index for every connection.
static TRUST_INDEX: LazyLock<Result<Index<Ssl, ServerTrust>, ErrorStack>> =
    LazyLock::new(Ssl::new_ex_index);

pub(super) fn configure(ctx: &mut SslContextBuilder) -> Result<Index<Ssl, ServerTrust>, String> {
    let index = TRUST_INDEX
        .as_ref()
        .copied()
        .map_err(|error| error.to_string())?;
    // Unlike the certificate callback, this runs after TLS 1.2's optional OCSP
    // message. With status requested it runs even without a staple, on every
    // full handshake, including renegotiation advanced inside SSL_read/write.
    // Abbreviated handshakes reuse the already authenticated session instead.
    ctx.set_status_callback(move |ssl| {
        let trust = ssl.ex_data(index).ok_or_else(ErrorStack::get)?;
        let error = trust.verify(ssl).err();
        let accepted = error.is_none();
        ssl.ex_data_mut(index).ok_or_else(ErrorStack::get)?.error = error;
        Ok(accepted)
    })
    .map_err(|error| format!("failed to configure macOS certificate verification: {error}"))?;
    Ok(index)
}

impl ServerTrust {
    pub(super) fn new(host: &str) -> Self {
        Self {
            hostname: (!host.is_empty()).then(|| host.to_owned()),
            error: None,
            #[cfg(test)]
            test_anchors: Vec::new(),
        }
    }

    fn verify(&self, ssl: &SslRef) -> Result<(), String> {
        let trust = self.create_trust(ssl)?;
        // Synchronous by design, like the existing OpenSSL verifier. System
        // trust services retain Apple's network policy; their issuer/revocation
        // fetches are outside guest socket permissions.
        trust.evaluate_with_error().map_err(|error| {
            format!(
                "macOS certificate verification failed ({}): {error}",
                error.code()
            )
        })
    }

    fn create_trust(&self, ssl: &SslRef) -> Result<SecTrust, String> {
        let leaf = ssl
            .peer_certificate()
            .ok_or("TLS peer did not provide a certificate")?;
        let leaf = leaf.to_der().map_err(|error| error.to_string())?;
        let mut certificates =
            vec![SecCertificate::from_der(&leaf).map_err(|error| error.to_string())?];
        if let Some(chain) = ssl.peer_cert_chain() {
            for certificate in chain {
                let der = certificate.to_der().map_err(|error| error.to_string())?;
                if der != leaf {
                    certificates
                        .push(SecCertificate::from_der(&der).map_err(|error| error.to_string())?);
                }
            }
        }
        let policy = SecPolicy::create_ssl(SslProtocolSide::SERVER, self.hostname.as_deref());
        // The peer's chain supplies candidates, never trust anchors.
        let mut trust = SecTrust::create_with_certificates(&certificates, &[policy])
            .map_err(|error| error.to_string())?;
        if let Some(response) = ssl.ocsp_status() {
            trust
                .set_trust_ocsp_response(std::iter::once(response))
                .map_err(|error| error.to_string())?;
        }
        #[cfg(test)]
        if !self.test_anchors.is_empty() {
            trust
                .set_anchor_certificates(&self.test_anchors)
                .map_err(|error| error.to_string())?;
        }
        Ok(trust)
    }
}
