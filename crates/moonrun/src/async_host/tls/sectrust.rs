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
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::cell::OnceCell;

use openssl::ssl::SslRef;
use security_framework::{
    certificate::SecCertificate, policy::SecPolicy, secure_transport::SslProtocolSide,
    trust::SecTrust,
};

pub(super) struct ServerTrust {
    hostname: Option<String>,
    allow_network_fetch: bool,
    result: OnceCell<Result<(), String>>,
    // Test anchors belong only to this evaluation; never modify system trust.
    #[cfg(test)]
    pub(super) test_anchors: Vec<SecCertificate>,
}

impl ServerTrust {
    pub(super) fn new(host: &str, allow_network_fetch: bool) -> Self {
        Self {
            hostname: (!host.is_empty()).then(|| host.to_owned()),
            allow_network_fetch,
            result: OnceCell::new(),
            #[cfg(test)]
            test_anchors: Vec::new(),
        }
    }

    pub(super) fn is_verified(&self) -> bool {
        matches!(self.result.get(), Some(Ok(())))
    }

    pub(super) fn verify(&self, ssl: &SslRef) -> Result<(), String> {
        // Cache only this connection's verdict, including rejection. Each new
        // connection evaluates current system trust; session reuse and TLS 1.2
        // renegotiation are disabled so they cannot reuse this decision.
        self.result
            .get_or_init(|| {
                let trust = self.create_trust(ssl)?;
                // Synchronous by design, like the existing OpenSSL verifier. Leave
                // Apple's revocation/network policy intact unless Moonrun's network
                // permissions prohibit its independent issuer/OCSP fetches.
                trust.evaluate_with_error().map_err(|error| {
                    format!(
                        "macOS certificate verification failed ({}): {error}",
                        error.code()
                    )
                })
            })
            .clone()
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
        if !self.allow_network_fetch {
            trust
                .set_network_fetch_allowed(false)
                .map_err(|error| error.to_string())?;
        }
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
