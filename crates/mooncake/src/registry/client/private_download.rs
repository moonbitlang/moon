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

use std::{fs::File, path::Path, time::SystemTime};

use anyhow::{Context, bail};
use moonutil::{registry::registry_api_url, resolution::ModuleName};
use reqwest::{StatusCode, Url, redirect::Policy};
use semver::Version;
use serde::Deserialize;

use super::{
    RegistryClient, RegistryMetadataDenied, encode_path_segment, persist_verified_archive,
};
use crate::registry::download;

// Deliberately neither Debug nor Serialize: signed URLs are short-lived secrets.
#[derive(Deserialize)]
struct DownloadCapability {
    module: String,
    version: Version,
    url: String,
    checksum: String,
    issued_at: u64,
    expires_at: u64,
}

fn is_loopback(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    })
}

impl DownloadCapability {
    fn validate(
        &self,
        name: &ModuleName,
        version: &Version,
        checksum: &str,
        allow_loopback: bool,
    ) -> anyhow::Result<Url> {
        if self.module != name.to_string()
            || &self.version != version
            || !self.checksum.eq_ignore_ascii_case(checksum)
        {
            bail!("registry download capability does not match {name}@{version}");
        }
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();
        if self.expires_at <= now
            || self.issued_at > now.saturating_add(60)
            || self.expires_at <= self.issued_at
            || self.expires_at - self.issued_at > 900
        {
            bail!("registry download capability has an invalid lifetime for {name}@{version}");
        }
        let url = Url::parse(&self.url)
            .map_err(|_| anyhow::anyhow!("registry returned an invalid download URL"))?;
        if !(url.scheme() == "https"
            || (allow_loopback && url.scheme() == "http" && is_loopback(&url)))
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            bail!("registry returned an invalid download URL");
        }
        Ok(url)
    }
}

fn request_capability(
    registry: &RegistryClient,
    name: &ModuleName,
    version: &Version,
) -> anyhow::Result<DownloadCapability> {
    let path = name
        .segments()
        .map(encode_path_segment)
        .collect::<Vec<_>>()
        .join("/");
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("version", &version.to_string())
        .finish();
    let response = registry
        .api_http_client()?
        .post(format!(
            "{}/api/v0/registry/modules/{path}/download-capability?{query}",
            registry.endpoints.api
        ))
        .send()
        .map_err(reqwest::Error::without_url)
        .with_context(|| format!("failed to request a source download for {name}@{version}"))?;
    if matches!(
        response.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ) {
        return Err(RegistryMetadataDenied(name.clone()).into());
    }
    if response.status() != StatusCode::OK {
        bail!(
            "registry source download request failed for {name}@{version} (HTTP {})",
            response.status()
        );
    }
    // Do not retain parser diagnostics: they can contain the response's signed URL.
    response
        .json()
        .map_err(|_| anyhow::anyhow!("invalid registry download capability for {name}@{version}"))
}

pub(super) fn download_archive(
    registry: &RegistryClient,
    name: &ModuleName,
    version: &Version,
    checksum: &str,
    cache_file: &Path,
) -> anyhow::Result<File> {
    let allow_loopback = is_loopback(&registry_api_url(&registry.endpoints.api)?);
    // Separate from the authenticated API client, including its default headers.
    let client = download::client_builder_with_retry()
        .https_only(!allow_loopback)
        .redirect(Policy::none())
        .build()
        .context("failed to create private source download client")?;
    for attempt in 0..2 {
        let capability = request_capability(registry, name, version)?;
        let url = capability.validate(name, version, checksum, allow_loopback)?;
        let mut response = client
            .get(url)
            .send()
            .map_err(reqwest::Error::without_url)
            .with_context(|| format!("failed to download source for {name}@{version}"))?;
        if response.status() == StatusCode::FORBIDDEN && attempt == 0 {
            // Expired capabilities require fresh authorization; never retry anonymously.
            continue;
        }
        if response.status() != StatusCode::OK {
            bail!(
                "source download failed for {name}@{version} (HTTP {})",
                response.status()
            );
        }
        return persist_verified_archive(&mut response, cache_file, name, version, checksum);
    }
    unreachable!("the second attempt always returns")
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use moonutil::resolution::ModuleSource;

    use super::super::{
        sha256_hex,
        tests::{
            api_test_registry, api_test_server, resolver_response, save_test_login, test_archive,
        },
    };
    use super::*;

    struct Fixture {
        home: tempfile::TempDir,
        api: String,
        api_listener: TcpListener,
        cdn: String,
        cdn_listener: TcpListener,
        module: ModuleSource,
        archive: Vec<u8>,
        checksum: String,
    }

    impl Fixture {
        fn new() -> Self {
            let home = tempfile::tempdir().unwrap();
            let api_listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let api = format!("http://{}", api_listener.local_addr().unwrap());
            let cdn_listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let cdn = format!("http://{}", cdn_listener.local_addr().unwrap());
            let archive = test_archive();
            let checksum = sha256_hex(&archive);
            save_test_login(home.path(), &api, "alice", "test-private-token");
            Self {
                home,
                api,
                api_listener,
                cdn,
                cdn_listener,
                archive,
                checksum,
                module: ModuleSource::from_version(
                    "alice/secret/nested".into(),
                    "0.1.0+01".parse().unwrap(),
                )
                .unwrap(),
            }
        }

        fn metadata(&self) -> Vec<u8> {
            resolver_response(
                &self.module.name().to_string(),
                "PRIVATE",
                &self.module.version().to_string(),
            )
            .replace(&"b".repeat(64), &self.checksum)
            .into_bytes()
        }

        fn capability(&self, signature: &str) -> serde_json::Value {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            serde_json::json!({
                "module": self.module.name().to_string(), "version": self.module.version(),
                "checksum": self.checksum, "issued_at": now, "expires_at": now+900,
                "url": format!("{}/sources/archive.zip?Signature={signature}", self.cdn),
            })
        }

        fn registry(&self) -> RegistryClient {
            api_test_registry(self.home.path(), &self.api)
        }

        fn materialize(&self, registry: &RegistryClient, directory: &str) -> anyhow::Result<()> {
            registry.materialize_source_to(
                &self.module,
                &self.home.path().join(directory),
                &moonutil::user_log::UserLog::new(log::LevelFilter::Error),
            )
        }

        fn archive_path(&self) -> std::path::PathBuf {
            self.registry()
                .metadata_cache(self.module.name())
                .unwrap()
                .path
                .with_extension("archives")
                .join(format!("{}.zip", self.module.version()))
        }

        fn assert_not_cached(&self) {
            assert!(!self.archive_path().exists());
            assert!(
                !self
                    .registry()
                    .home
                    .registry_source_archive_path(self.module.name(), self.module.version())
                    .exists()
            );
        }
    }

    #[test]
    fn private_download_authenticates_only_the_api_and_reuses_verified_offline_archives() {
        let fixture = Fixture::new();
        let server = api_test_server(
            fixture.api_listener.try_clone().unwrap(),
            vec![
                (200, String::new(), fixture.metadata()),
                (
                    200,
                    String::new(),
                    fixture
                        .capability("sensitive-signature")
                        .to_string()
                        .into_bytes(),
                ),
            ],
        );
        let cdn = api_test_server(
            fixture.cdn_listener.try_clone().unwrap(),
            vec![(200, String::new(), fixture.archive.clone())],
        );
        fixture.materialize(&fixture.registry(), "source").unwrap();
        fixture
            .materialize(&fixture.registry().offline(), "offline")
            .unwrap();
        assert!(fixture.home.path().join("offline/moon.mod").is_file());
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].starts_with("POST /api/v0/registry/modules/alice/secret/nested/download-capability?version=0.1.0%2B01 "));
        for request in requests {
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer test-private-token")
            );
        }
        let requests = cdn.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0].starts_with("GET /sources/archive.zip?Signature=sensitive-signature "));
        assert!(!requests[0].to_ascii_lowercase().contains("authorization:"));
        assert!(
            !fixture
                .registry()
                .home
                .registry_source_archive_path(fixture.module.name(), fixture.module.version())
                .exists()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(fixture.archive_path())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o077,
                0
            );
        }
        save_test_login(fixture.home.path(), &fixture.api, "bob", "another-token");
        assert!(
            fixture
                .materialize(&fixture.registry().offline(), "bob")
                .is_err()
        );
    }

    #[test]
    fn private_download_renews_once_after_cdn_forbidden() {
        let fixture = Fixture::new();
        let server = api_test_server(
            fixture.api_listener.try_clone().unwrap(),
            vec![
                (200, String::new(), fixture.metadata()),
                (
                    200,
                    String::new(),
                    fixture.capability("first").to_string().into_bytes(),
                ),
                (
                    200,
                    String::new(),
                    fixture.capability("second").to_string().into_bytes(),
                ),
            ],
        );
        let cdn = api_test_server(
            fixture.cdn_listener.try_clone().unwrap(),
            vec![
                (403, String::new(), Vec::new()),
                (200, String::new(), fixture.archive.clone()),
            ],
        );
        fixture.materialize(&fixture.registry(), "source").unwrap();
        assert_eq!(server.join().unwrap().len(), 3);
        let requests = cdn.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].contains("Signature=first"));
        assert!(requests[1].contains("Signature=second"));
        assert!(
            requests
                .iter()
                .all(|r| !r.to_ascii_lowercase().contains("authorization:"))
        );
    }

    #[test]
    fn private_download_stops_after_two_cdn_denials_without_logging_capabilities() {
        let fixture = Fixture::new();
        let server = api_test_server(
            fixture.api_listener.try_clone().unwrap(),
            vec![
                (200, String::new(), fixture.metadata()),
                (
                    200,
                    String::new(),
                    fixture.capability("do-not-log").to_string().into_bytes(),
                ),
                (
                    200,
                    String::new(),
                    fixture.capability("do-not-log").to_string().into_bytes(),
                ),
            ],
        );
        let cdn = api_test_server(
            fixture.cdn_listener.try_clone().unwrap(),
            vec![
                (403, String::new(), Vec::new()),
                (403, String::new(), Vec::new()),
            ],
        );
        let error = fixture
            .materialize(&fixture.registry(), "source")
            .unwrap_err();
        let diagnostic = format!("{error:#}");
        assert!(diagnostic.contains("403"));
        assert!(!diagnostic.contains("do-not-log") && !diagnostic.contains("Signature="));
        assert_eq!(server.join().unwrap().len(), 3);
        assert_eq!(cdn.join().unwrap().len(), 2);
        fixture.assert_not_cached();
    }

    #[test]
    fn private_download_renewal_denial_is_authoritative() {
        let fixture = Fixture::new();
        let server = api_test_server(
            fixture.api_listener.try_clone().unwrap(),
            vec![
                (200, String::new(), fixture.metadata()),
                (
                    200,
                    String::new(),
                    fixture.capability("do-not-log").to_string().into_bytes(),
                ),
                (404, String::new(), b"{}".to_vec()),
            ],
        );
        let cdn = api_test_server(
            fixture.cdn_listener.try_clone().unwrap(),
            vec![(403, String::new(), Vec::new())],
        );
        let error = fixture
            .materialize(&fixture.registry(), "source")
            .unwrap_err();
        assert!(error.downcast_ref::<RegistryMetadataDenied>().is_some());
        assert_eq!(server.join().unwrap().len(), 3);
        assert_eq!(cdn.join().unwrap().len(), 1);
        fixture.assert_not_cached();
    }

    #[test]
    fn private_download_rejects_corrupt_bytes_without_publishing_an_archive() {
        let fixture = Fixture::new();
        let server = api_test_server(
            fixture.api_listener.try_clone().unwrap(),
            vec![
                (200, String::new(), fixture.metadata()),
                (
                    200,
                    String::new(),
                    fixture.capability("do-not-log").to_string().into_bytes(),
                ),
            ],
        );
        let cdn = api_test_server(
            fixture.cdn_listener.try_clone().unwrap(),
            vec![(200, String::new(), b"corrupt archive".to_vec())],
        );
        let error = fixture
            .materialize(&fixture.registry(), "source")
            .unwrap_err();
        assert!(format!("{error:#}").contains("Checksum mismatch"));
        assert_eq!(server.join().unwrap().len(), 2);
        assert_eq!(cdn.join().unwrap().len(), 1);
        fixture.assert_not_cached();
    }

    #[test]
    fn private_download_validates_identity_lifetime_and_url_without_caching_the_capability() {
        for (field, value) in [
            ("module", serde_json::json!("bob/other")),
            ("version", serde_json::json!("0.1.0+02")),
            ("checksum", serde_json::json!("a".repeat(64))),
            ("expires_at", serde_json::json!(1)),
            ("issued_at", serde_json::json!(u64::MAX)),
            (
                "url",
                serde_json::json!("http://example.com/archive?Signature=do-not-log"),
            ),
            (
                "url",
                serde_json::json!("https://user:password@example.com/archive?Signature=do-not-log"),
            ),
            (
                "url",
                serde_json::json!("https://example.com/archive#Signature=do-not-log"),
            ),
            ("version", serde_json::json!("invalid-do-not-log")),
        ] {
            let fixture = Fixture::new();
            let mut capability = fixture.capability("do-not-log");
            capability[field] = value;
            let server = api_test_server(
                fixture.api_listener.try_clone().unwrap(),
                vec![
                    (200, String::new(), fixture.metadata()),
                    (200, String::new(), capability.to_string().into_bytes()),
                ],
            );
            let error = fixture
                .materialize(&fixture.registry(), "source")
                .unwrap_err();
            assert!(!format!("{error:#}").contains("do-not-log"));
            assert_eq!(server.join().unwrap().len(), 2);
            fixture.assert_not_cached();
        }
    }

    #[test]
    fn private_download_transport_errors_do_not_expose_signed_urls() {
        let fixture = Fixture::new();
        let mut capability = fixture.capability("do-not-log");
        // A closed loopback port gives a real reqwest error containing a URL.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        capability["url"] = serde_json::json!(format!(
            "http://{}/source?Signature=do-not-log",
            listener.local_addr().unwrap()
        ));
        drop(listener);
        let server = api_test_server(
            fixture.api_listener.try_clone().unwrap(),
            vec![
                (200, String::new(), fixture.metadata()),
                (200, String::new(), capability.to_string().into_bytes()),
            ],
        );
        let error = fixture
            .materialize(&fixture.registry(), "source")
            .unwrap_err();
        assert!(!format!("{error:#}").contains("do-not-log"));
        assert!(!format!("{error:?}").contains("Signature="));
        assert_eq!(server.join().unwrap().len(), 2);
        fixture.assert_not_cached();
    }

    #[test]
    fn private_source_cache_namespace_tracks_registry_and_account() {
        use crate::registry::RegistrySource;

        let fixture = Fixture::new();
        let server = api_test_server(
            fixture.api_listener.try_clone().unwrap(),
            vec![
                (200, String::new(), fixture.metadata()),
                (200, String::new(), fixture.metadata()),
            ],
        );
        let alice = fixture
            .registry()
            .source_cache_namespace(&fixture.module)
            .unwrap();
        save_test_login(fixture.home.path(), &fixture.api, "bob", "bob-token");
        let bob = fixture
            .registry()
            .source_cache_namespace(&fixture.module)
            .unwrap();
        assert_ne!(alice, bob);
        server.join().unwrap();
        let other = api_test_registry(fixture.home.path(), "https://example.com")
            .private_cache_namespace()
            .unwrap();
        assert!(other.is_none());
        let index = fixture
            .registry()
            .home
            .registry_index_file(fixture.module.name());
        std::fs::create_dir_all(index.parent().unwrap()).unwrap();
        std::fs::write(index, "").unwrap();
        assert_eq!(
            fixture
                .registry()
                .source_cache_namespace(&fixture.module)
                .unwrap(),
            None
        );
    }
}
