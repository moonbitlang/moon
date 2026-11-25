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

use super::*;
use crate::policy;
#[cfg(not(windows))]
use crate::policy::Policy;
use crate::runtime::{Env, WorkingDirectory};

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

#[test]
fn tls_handles_reject_stale_and_other_family_handles() {
    let keys = Rc::new(RefCell::new(HostKeys::default()));
    let host = HostTls::new(
        Arc::new(HostFs::new(
            None,
            Arc::new(Env::ambient()),
            Arc::new(WorkingDirectory::Ambient),
        )),
        keys.clone(),
    );
    let connection = host.new_connection();
    let mut buffers = Handles::new(keys, HostResourceKind::CBuffer);
    let buffer_key = buffers.insert(b"retained".to_vec());
    let buffer = buffer_key.data().as_ffi();
    // Zero is deliberately invalid; nullable cleanup uses the Runtime's sentinel.
    for handle in [0, buffer] {
        assert_eq!(host.free(handle), Err(AsyncHostError::Badf));
        assert_eq!(host.take_error(handle), Err(AsyncHostError::Badf));
    }
    host.free(null_handle()).unwrap();
    assert_eq!(host.take_error(null_handle()), Err(AsyncHostError::Badf));
    assert_eq!(buffers.get(buffer_key).unwrap(), b"retained");

    host.free(connection).unwrap();
    let replacement = host.new_connection();
    assert_ne!(connection, replacement);
    assert_eq!(host.free(connection), Err(AsyncHostError::Badf));
    assert_eq!(host.take_error(connection), Err(AsyncHostError::Badf));
    assert_eq!(
        host.add_root_certificate(replacement, b"pending certificate"),
        Ok(TLS_SUCCESS_STATUS)
    );

    host.free(replacement).unwrap();
    buffers.remove(buffer_key).unwrap();
    assert!(host.leak_summary().is_none());
}

#[cfg(not(windows))]
#[test]
fn tls_set_server_files_resolves_relative_paths_against_fixed_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(tmp.path()).unwrap();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../third_party/moonbitlang_async/test_keys");
    std::fs::copy(fixtures.join("key.pem"), root.join("key.pem")).unwrap();
    std::fs::copy(fixtures.join("cert.pem"), root.join("cert.pem")).unwrap();
    for (policy_text, denied_file) in [
        (None, None),
        (Some("[fs]\nread = [\".\"]\n"), None),
        (Some("[fs]\nread = []\n"), Some("private key")),
        (Some("[fs]\nread = [\"key.pem\"]\n"), Some("certificate")),
    ] {
        let (policy, environment) = if let Some(text) = policy_text {
            let policy_file = root.join("policy.toml");
            std::fs::write(&policy_file, text).unwrap();
            let (policy, environment) = policy::load_file(&policy_file).unwrap();
            (Some(policy), environment.realize().unwrap())
        } else {
            (None, Env::ambient())
        };
        let host = HostTls::new(
            Arc::new(HostFs::new(
                policy.as_ref().map(Policy::filesystem),
                Arc::new(environment),
                Arc::new(WorkingDirectory::Fixed(root.clone())),
            )),
            Rc::new(RefCell::new(HostKeys::default())),
        );
        let handle = host.new_connection();
        let status = host
            .set_server_files(
                handle,
                "key.pem".into(),
                TlsFileType::Pem,
                "cert.pem".into(),
                TlsFileType::Pem,
            )
            .unwrap();
        if let Some(label) = denied_file {
            assert_eq!(status, TLS_ERROR_STATUS, "policy: {policy_text:?}");
            assert_eq!(
                host.take_error(handle).unwrap(),
                Some(format!(
                    "failed to access TLS {label} file: PermissionDenied"
                )),
            );
        } else {
            assert_eq!(status, TLS_SUCCESS_STATUS, "policy: {policy_text:?}");
        }
        host.free(handle).unwrap();
    }
}

#[test]
fn tls_set_server_files_checks_file_policy_before_backend_load() {
    let tmp = tempfile::tempdir().unwrap();
    let allowed = tmp.path().join("allowed");
    let denied = tmp.path().join("denied");
    std::fs::create_dir(&allowed).unwrap();
    std::fs::create_dir(&denied).unwrap();
    let key_file = denied.join("key.pem");
    let cert_file = allowed.join("cert.pem");
    std::fs::write(&key_file, "key").unwrap();
    std::fs::write(&cert_file, "cert").unwrap();
    let policy_file = tmp.path().join("policy.toml");
    std::fs::write(&policy_file, "[fs]\nread = [\"allowed\"]\n").unwrap();
    let (policy, environment) = policy::load_file(&policy_file).unwrap();
    let host = HostTls::new(
        Arc::new(HostFs::new(
            Some(policy.filesystem()),
            Arc::new(environment.realize().unwrap()),
            Arc::new(WorkingDirectory::Ambient),
        )),
        Rc::new(RefCell::new(HostKeys::default())),
    );

    let handle = host.new_connection();
    let status = host
        .set_server_files(
            handle,
            key_file,
            TlsFileType::Pem,
            cert_file,
            TlsFileType::Pem,
        )
        .unwrap();

    assert_eq!(status, TLS_ERROR_STATUS);
    assert_eq!(
        host.take_error(handle).unwrap().as_deref(),
        Some("failed to access TLS private key file: PermissionDenied"),
    );
    host.free(handle).unwrap();
}
