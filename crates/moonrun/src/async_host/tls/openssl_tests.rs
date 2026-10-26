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

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use foreign_types::{ForeignType, ForeignTypeRef};
use openssl::asn1::Asn1Time;
use openssl::bn::BigNum;
use openssl::ocsp::{OcspBasicResponse, OcspCertId, OcspResponse, OcspResponseStatus};
use openssl::pkey::{PKey, Private};
use openssl::rsa::Rsa;
use openssl::ssl::{ClientHelloResponse, SslOptions, SslVersion};
use openssl::x509::X509NameBuilder;
use openssl::x509::extension::{
    BasicConstraints, ExtendedKeyUsage, KeyUsage, SubjectAlternativeName,
};

use super::*;
use security_framework::certificate::SecCertificate;

struct Identity {
    root: X509,
    root_key: PKey<Private>,
    certificate: X509,
    key: PKey<Private>,
}

impl Identity {
    fn new(expired: bool, client_only: bool) -> Self {
        let root_key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();
        let key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();
        let root = certificate(&root_key, None, false, false);
        let certificate = certificate(&key, Some((&root, &root_key)), expired, client_only);
        Self {
            root,
            root_key,
            certificate,
            key,
        }
    }

    fn server_context(&self, version: SslVersion) -> SslContextBuilder {
        let mut ctx = SslContextBuilder::new(SslMethod::tls_server()).unwrap();
        ctx.set_min_proto_version(Some(version)).unwrap();
        ctx.set_max_proto_version(Some(version)).unwrap();
        ctx.set_num_tickets(0).unwrap();
        ctx.set_certificate(&self.certificate).unwrap();
        ctx.set_private_key(&self.key).unwrap();
        ctx.add_extra_chain_cert(self.root.clone()).unwrap();
        ctx
    }

    fn server(&self, version: SslVersion) -> TlsConnection {
        let ctx = self.server_context(version);
        let mut ssl = Ssl::new(&ctx.build()).unwrap();
        ssl.set_accept_state();
        TlsConnection::new(ssl, TlsMode::Server).unwrap()
    }
}

fn certificate(
    key: &PKey<Private>,
    issuer: Option<(&X509, &PKey<Private>)>,
    expired: bool,
    client_only: bool,
) -> X509 {
    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_text(
        "CN",
        if issuer.is_none() {
            "Moonrun test CA"
        } else {
            "localhost"
        },
    )
    .unwrap();
    let name = name.build();
    let mut cert = X509::builder().unwrap();
    cert.set_version(2).unwrap();
    let serial = BigNum::from_u32(if issuer.is_none() { 1 } else { 2 })
        .unwrap()
        .to_asn1_integer()
        .unwrap();
    cert.set_serial_number(&serial).unwrap();
    cert.set_subject_name(&name).unwrap();
    cert.set_issuer_name(issuer.map_or(&name, |(c, _)| c.subject_name()))
        .unwrap();
    cert.set_pubkey(key).unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    cert.set_not_before(&Asn1Time::from_unix(now - 172800).unwrap())
        .unwrap();
    cert.set_not_after(
        &Asn1Time::from_unix(if expired { now - 86400 } else { now + 2592000 }).unwrap(),
    )
    .unwrap();
    if issuer.is_none() {
        cert.append_extension(BasicConstraints::new().critical().ca().build().unwrap())
            .unwrap();
        cert.append_extension(
            KeyUsage::new()
                .critical()
                .key_cert_sign()
                .crl_sign()
                .build()
                .unwrap(),
        )
        .unwrap();
    } else {
        cert.append_extension(BasicConstraints::new().critical().build().unwrap())
            .unwrap();
        cert.append_extension(
            KeyUsage::new()
                .critical()
                .digital_signature()
                .key_encipherment()
                .build()
                .unwrap(),
        )
        .unwrap();
        cert.append_extension(if client_only {
            ExtendedKeyUsage::new().client_auth().build().unwrap()
        } else {
            ExtendedKeyUsage::new().server_auth().build().unwrap()
        })
        .unwrap();
        let san = SubjectAlternativeName::new()
            .dns("localhost")
            .ip("127.0.0.1")
            .build(&cert.x509v3_context(issuer.map(|(c, _)| c.as_ref()), None))
            .unwrap();
        cert.append_extension(san).unwrap();
    }
    cert.sign(issuer.map_or(key, |(_, k)| k), MessageDigest::sha256())
        .unwrap();
    cert.build()
}

// Drive the production buffer API without sockets, including partial records.
fn handshake(client: &mut TlsConnection, server: &mut TlsConnection) -> Result<(), String> {
    let mut to_client = Vec::new();
    let mut to_server = Vec::new();
    for _ in 0..1000 {
        let mut output = [0; 127];
        let status = client.connect(&mut to_client, &mut output);
        to_client.drain(..client.bytes_read());
        to_server.extend_from_slice(&output[..client.bytes_to_write()]);
        if status == TLS_ERROR_STATUS {
            return Err(client.take_error().unwrap());
        }
        let status = server.accept(&mut to_server, &mut output);
        to_server.drain(..server.bytes_read());
        to_client.extend_from_slice(&output[..server.bytes_to_write()]);
        assert_ne!(status, TLS_ERROR_STATUS, "{:?}", server.take_error());
        if !client.is_handshaking() && !server.is_handshaking() {
            return Ok(());
        }
    }
    panic!("TLS handshake stopped making progress");
}

#[test]
fn system_trust_rejects_an_openssl_only_anchor() {
    let identity = Identity::new(false, false);
    for version in [SslVersion::TLS1_2, SslVersion::TLS1_3] {
        let mut client = TlsConnection::client(
            "localhost",
            false,
            TlsConfig::Client {
                trust: TlsTrust::SystemRoot,
                root_certificates: vec![],
            },
        )
        .unwrap();
        // A root accepted by OpenSSL must not override the platform's verdict.
        configure_custom_root_store(
            client.pending_ssl.as_mut().unwrap(),
            &[identity.root.to_der().unwrap()],
        )
        .unwrap();
        let result = handshake(&mut client, &mut identity.server(version));
        assert!(result.is_err(), "an OpenSSL-only root bypassed macOS trust");
    }
}

fn system_client(host: &str, sni: bool, anchor: Option<&X509>) -> TlsConnection {
    let mut client = TlsConnection::client(
        host,
        sni,
        TlsConfig::Client {
            trust: TlsTrust::SystemRoot,
            root_certificates: vec![],
        },
    )
    .unwrap();
    if let Some(anchor) = anchor {
        let index = client.server_trust.unwrap();
        client
            .pending_ssl
            .as_mut()
            .unwrap()
            .ex_data_mut(index)
            .unwrap()
            .test_anchors
            .push(SecCertificate::from_der(&anchor.to_der().unwrap()).unwrap());
    }
    client
}

#[test]
fn platform_trust_verifies_identity_before_application_io() {
    let good = Identity::new(false, false);
    let expired = Identity::new(true, false);
    let client_only = Identity::new(false, true);
    for version in [SslVersion::TLS1_2, SslVersion::TLS1_3] {
        for (identity, hostname, sni, trusted, accepted) in [
            (&good, "localhost", true, true, true),
            (&good, "localhost", false, true, true),
            (&good, "127.0.0.1", false, true, true),
            (&good, "", false, true, true),
            (&good, "wrong.invalid", false, true, false),
            (&good, "127.0.0.2", false, true, false),
            (&good, "localhost", false, false, false),
            (&expired, "localhost", false, true, false),
            (&client_only, "localhost", false, true, false),
        ] {
            let mut client = system_client(hostname, sni, trusted.then_some(&identity.root));
            let mut server = identity.server(version);
            let result = handshake(&mut client, &mut server);
            assert_eq!(
                result.is_ok(),
                accepted,
                "{version:?}, {hostname}, sni={sni}: {result:?}"
            );
            if accepted {
                let mut encrypted = [0; 4096];
                assert_eq!(client.write_plain(&mut [], b"ping", &mut encrypted), 4);
                let mut received = [0; 4];
                assert_eq!(
                    server.read_plain(
                        &mut encrypted[..client.bytes_to_write()],
                        &mut received,
                        &mut [0; 4096]
                    ),
                    4
                );
                assert_eq!(&received, b"ping");
            } else {
                // SecTrust rejection aborts OpenSSL's handshake itself.
                assert!(!client.stream.as_ref().unwrap().ssl().is_init_finished());
                assert_rejected(&mut client, &result.unwrap_err());
            }
        }
    }
}

fn assert_rejected(client: &mut TlsConnection, error: &str) {
    assert!(client.is_handshaking());
    // Neither changing operations nor consuming the guest diagnostic may clear
    // a rejection or emit application data after the failed handshake.
    client.take_error();
    assert_eq!(
        client.write_plain(&mut [], b"secret", &mut [0; 4096]),
        TLS_ERROR_STATUS
    );
    assert_eq!(client.bytes_to_write(), 0);
    assert_eq!(client.take_error().as_deref(), Some(error));
    let mut received = [0; 4];
    assert_eq!(
        client.read_plain(&mut [], &mut received, &mut [0; 4096]),
        TLS_ERROR_STATUS
    );
    assert_eq!(received, [0; 4]);
    assert_eq!(client.bytes_to_write(), 0);
    client.take_error();
    assert_eq!(client.connect(&mut [], &mut [0; 4096]), TLS_ERROR_STATUS);
    assert_eq!(client.take_error().as_deref(), Some(error));
}

#[test]
fn custom_roots_and_no_verification_keep_their_explicit_trust_modes() {
    let identity = Identity::new(false, false);
    for version in [SslVersion::TLS1_2, SslVersion::TLS1_3] {
        for (trust, host, roots, accepted) in [
            (
                TlsTrust::CustomRoot,
                "localhost",
                vec![identity.root.to_der().unwrap()],
                true,
            ),
            (
                TlsTrust::CustomRoot,
                "wrong.invalid",
                vec![identity.root.to_der().unwrap()],
                false,
            ),
            (TlsTrust::CustomRoot, "localhost", vec![], false),
            (TlsTrust::NoVerification, "wrong.invalid", vec![], true),
        ] {
            let mut client = TlsConnection::client(
                host,
                false,
                TlsConfig::Client {
                    trust,
                    root_certificates: roots,
                },
            )
            .unwrap();
            assert_eq!(
                handshake(&mut client, &mut identity.server(version)).is_ok(),
                accepted
            );
        }
    }
}

#[test]
fn invalid_hostname_is_an_error_without_sni() {
    assert!(
        TlsConnection::client(
            "localhost\0.example",
            false,
            TlsConfig::Client {
                trust: TlsTrust::SystemRoot,
                root_certificates: vec![],
            }
        )
        .is_err()
    );
}

// These fixture-generation and renegotiation APIs are absent from the safe bindings.
unsafe extern "C" {
    fn SSL_renegotiate(ssl: *mut openssl_sys::SSL) -> std::ffi::c_int;
    fn OCSP_basic_add1_status(
        response: *mut openssl_sys::OCSP_BASICRESP,
        id: *mut openssl_sys::OCSP_CERTID,
        status: std::ffi::c_int,
        reason: std::ffi::c_int,
        revoked_at: *mut openssl_sys::ASN1_TIME,
        this_update: *mut openssl_sys::ASN1_TIME,
        next_update: *mut openssl_sys::ASN1_TIME,
    ) -> *mut std::ffi::c_void;
    fn OCSP_basic_sign(
        response: *mut openssl_sys::OCSP_BASICRESP,
        signer: *mut openssl_sys::X509,
        key: *mut openssl_sys::EVP_PKEY,
        digest: *const openssl_sys::EVP_MD,
        certificates: *mut openssl_sys::stack_st_X509,
        flags: std::ffi::c_ulong,
    ) -> std::ffi::c_int;
}

// Like the guest, drive a renegotiation encountered during application reads
// through connect/accept when the host requests it. Small buffers split records.
fn renegotiate(client: &mut TlsConnection, server: &mut TlsConnection) -> Result<(), String> {
    let previous_client = client.unique_channel_binding_bytes().unwrap();
    let previous_server = server.unique_channel_binding_bytes().unwrap();
    // The server owns this live SSL, and scheduling a handshake does no I/O.
    assert_eq!(
        unsafe { SSL_renegotiate(server.stream.as_ref().unwrap().ssl().as_ptr()) },
        1
    );
    let mut to_client = Vec::new();
    let mut to_server = Vec::new();
    for _ in 0..1000 {
        let mut output = [0; 127];
        let status = if server.is_handshaking() {
            server.accept(&mut to_server, &mut output)
        } else {
            server.read_plain(&mut to_server, &mut [0; 4], &mut output)
        };
        to_server.drain(..server.bytes_read());
        to_client.extend_from_slice(&output[..server.bytes_to_write()]);
        if status == TLS_ERROR_STATUS {
            return Err(server.take_error().unwrap());
        }
        let status = if client.is_handshaking() {
            client.connect(&mut to_client, &mut output)
        } else {
            client.read_plain(&mut to_client, &mut [0; 4], &mut output)
        };
        to_client.drain(..client.bytes_read());
        to_server.extend_from_slice(&output[..client.bytes_to_write()]);
        if status == TLS_ERROR_STATUS {
            return Err(client.take_error().unwrap());
        }
        if !client.is_handshaking()
            && !server.is_handshaking()
            && client.unique_channel_binding_bytes().unwrap() != previous_client
            && server.unique_channel_binding_bytes().unwrap() != previous_server
            && to_client.is_empty()
            && to_server.is_empty()
        {
            return Ok(());
        }
    }
    panic!("TLS renegotiation stopped making progress");
}

#[test]
fn tls12_full_and_abbreviated_renegotiation_preserve_application_io() {
    let identity = Identity::new(false, false);
    for resumed in [false, true] {
        let mut ctx = identity.server_context(SslVersion::TLS1_2);
        if !resumed {
            ctx.set_options(SslOptions::NO_SESSION_RESUMPTION_ON_RENEGOTIATION);
        }
        let mut ssl = Ssl::new(&ctx.build()).unwrap();
        ssl.set_accept_state();
        let mut server = TlsConnection::new(ssl, TlsMode::Server).unwrap();
        let mut client = system_client("localhost", false, Some(&identity.root));
        handshake(&mut client, &mut server).unwrap();
        renegotiate(&mut client, &mut server).unwrap();
        assert_eq!(
            client.stream.as_ref().unwrap().ssl().session_reused(),
            resumed
        );
        let mut encrypted = [0; 4096];
        assert_eq!(client.write_plain(&mut [], b"ping", &mut encrypted), 4);
        let mut received = [0; 4];
        assert_eq!(
            server.read_plain(
                &mut encrypted[..client.bytes_to_write()],
                &mut received,
                &mut [0; 4096]
            ),
            4
        );
        assert_eq!(&received, b"ping");
    }
}

#[test]
fn tls12_renegotiation_rejects_a_changed_untrusted_certificate() {
    let identity = Identity::new(false, false);
    let expired = certificate(
        &identity.key,
        Some((&identity.root, &identity.root_key)),
        true,
        false,
    );
    let expired_der = expired.to_der().unwrap();
    let mut ctx = identity.server_context(SslVersion::TLS1_2);
    ctx.set_options(SslOptions::NO_SESSION_RESUMPTION_ON_RENEGOTIATION);
    let hellos = AtomicUsize::new(0);
    ctx.set_client_hello_callback(move |ssl, _| {
        if hellos.fetch_add(1, Ordering::Relaxed) > 0 {
            ssl.set_certificate(&expired)?;
        }
        Ok(ClientHelloResponse::SUCCESS)
    });
    let mut ssl = Ssl::new(&ctx.build()).unwrap();
    ssl.set_accept_state();
    let mut server = TlsConnection::new(ssl, TlsMode::Server).unwrap();
    let mut client = system_client("localhost", false, Some(&identity.root));
    handshake(&mut client, &mut server).unwrap();
    let error = renegotiate(&mut client, &mut server).unwrap_err();
    assert!(
        error.contains("macOS certificate verification failed"),
        "{error}"
    );
    assert_eq!(client.peer_certificate_der().unwrap().unwrap(), expired_der);
    assert_rejected(&mut client, &error);
}

#[test]
fn tls12_renegotiation_evaluates_the_current_ocsp_staple() {
    let identity = Identity::new(false, false);
    for status in [
        None,
        Some(openssl_sys::V_OCSP_CERTSTATUS_GOOD),
        Some(openssl_sys::V_OCSP_CERTSTATUS_REVOKED),
    ] {
        let good = ocsp_response(&identity, openssl_sys::V_OCSP_CERTSTATUS_GOOD);
        let next = status.map(|status| ocsp_response(&identity, status));
        let expected = next.clone();
        let mut ctx = identity.server_context(SslVersion::TLS1_2);
        ctx.set_options(SslOptions::NO_SESSION_RESUMPTION_ON_RENEGOTIATION);
        let requests = AtomicUsize::new(0);
        ctx.set_status_callback(move |ssl| {
            let response = if requests.fetch_add(1, Ordering::Relaxed) == 0 {
                Some(good.as_slice())
            } else {
                next.as_deref()
            };
            if let Some(response) = response {
                ssl.set_ocsp_status(response)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
        .unwrap();
        let mut ssl = Ssl::new(&ctx.build()).unwrap();
        ssl.set_accept_state();
        let mut server = TlsConnection::new(ssl, TlsMode::Server).unwrap();
        let mut client = system_client("localhost", false, Some(&identity.root));
        handshake(&mut client, &mut server).unwrap();
        let result = renegotiate(&mut client, &mut server);
        assert_eq!(
            client.stream.as_ref().unwrap().ssl().ocsp_status(),
            expected.as_deref()
        );
        if status == Some(openssl_sys::V_OCSP_CERTSTATUS_REVOKED) {
            let error = result.unwrap_err();
            assert!(
                error.contains("macOS certificate verification failed"),
                "{error}"
            );
            assert_rejected(&mut client, &error);
        } else {
            result.unwrap();
        }
    }
}

fn ocsp_response(identity: &Identity, status: i32) -> Vec<u8> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let revoked_at = Asn1Time::from_unix(now - 120).unwrap();
    let this_update = Asn1Time::from_unix(now - 60).unwrap();
    let next_update = Asn1Time::from_unix(now + 86400).unwrap();
    let id = OcspCertId::from_cert(MessageDigest::sha1(), &identity.certificate, &identity.root)
        .unwrap();
    // All borrowed inputs outlive these calls. add1 copies its inputs, and the
    // newly allocated response is immediately placed under the owning wrapper.
    let response = unsafe {
        let raw = openssl_sys::OCSP_BASICRESP_new();
        assert!(!raw.is_null());
        let response = OcspBasicResponse::from_ptr(raw);
        assert!(
            !OCSP_basic_add1_status(
                response.as_ptr(),
                id.as_ptr(),
                status,
                openssl_sys::OCSP_REVOKED_STATUS_KEYCOMPROMISE,
                revoked_at.as_ptr(),
                this_update.as_ptr(),
                next_update.as_ptr()
            )
            .is_null()
        );
        assert_eq!(
            OCSP_basic_sign(
                response.as_ptr(),
                identity.root.as_ptr(),
                identity.root_key.as_ptr(),
                MessageDigest::sha256().as_ptr(),
                std::ptr::null_mut(),
                0
            ),
            1
        );
        response
    };
    OcspResponse::create(OcspResponseStatus::SUCCESSFUL, Some(&response))
        .unwrap()
        .to_der()
        .unwrap()
}

#[test]
fn stapled_ocsp_is_evaluated_for_tls12_and_tls13() {
    let identity = Identity::new(false, false);
    for version in [SslVersion::TLS1_2, SslVersion::TLS1_3] {
        for (status, accepted) in [
            (openssl_sys::V_OCSP_CERTSTATUS_GOOD, true),
            (openssl_sys::V_OCSP_CERTSTATUS_REVOKED, false),
        ] {
            let response = ocsp_response(&identity, status);
            let mut ctx = identity.server_context(version);
            ctx.set_status_callback(move |ssl| {
                ssl.set_ocsp_status(&response)?;
                Ok(true)
            })
            .unwrap();
            let mut ssl = Ssl::new(&ctx.build()).unwrap();
            ssl.set_accept_state();
            let mut server = TlsConnection::new(ssl, TlsMode::Server).unwrap();
            let mut client = system_client("localhost", false, Some(&identity.root));
            let result = handshake(&mut client, &mut server);
            assert!(
                client
                    .stream
                    .as_ref()
                    .unwrap()
                    .ssl()
                    .ocsp_status()
                    .is_some()
            );
            assert_eq!(
                result.is_ok(),
                accepted,
                "{version:?}, status={status}: {result:?}"
            );
        }
    }
}
