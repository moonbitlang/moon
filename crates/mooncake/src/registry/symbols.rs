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

//! Download and materialize symbols independently of registry metadata.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use moonutil::{
    MoonHomeLayout, locks::lock_directory, registry::RegistryConfig, user_log::UserLog,
};
use reqwest::{
    StatusCode,
    header::{ETAG, HeaderValue, IF_NONE_MATCH},
};
use serde::{Deserialize, Serialize};

use super::{download, stage_registry_cache_file, unique_sibling_dir};
use crate::zip_util::extract_zip_to_dir;

const SYMBOLS_URL: &str = "https://download.mooncakes.io/symbols.zip";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SymbolsSyncState {
    url: String,
    generation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    etag: Option<String>,
}

impl SymbolsSyncState {
    fn read(path: &Path) -> anyhow::Result<Option<Self>> {
        match std::fs::read(path) {
            Ok(data) => Ok(Some(serde_json::from_slice(&data)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write(&self, path: &Path) -> anyhow::Result<()> {
        let (mut staged, path) = stage_registry_cache_file(path)?;
        serde_json::to_writer(staged.as_file_mut(), self)?;
        staged.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub(super) fn sync(
    home: &MoonHomeLayout,
    config: &RegistryConfig,
    user_log: &UserLog,
) -> anyhow::Result<()> {
    let state_path = home.registry_symbols_update_state_path();
    let observed = SymbolsSyncState::read(&state_path);
    let registry_dir = home.registry_dir();
    std::fs::create_dir_all(&registry_dir)
        .with_context(|| format!("failed to create `{}`", registry_dir.display()))?;
    let _lock = lock_directory(&registry_dir, user_log)
        .context("failed to lock registry update directory")?;
    let current = SymbolsSyncState::read(&state_path);
    let url = symbols_url(config);

    // Only a successful symbols synchronization completed while waiting can
    // satisfy this request. Index synchronization never changes this record.
    if let (Ok(observed), Ok(Some(current))) = (&observed, &current)
        && observed.as_ref() != Some(current)
        && current.url == url
    {
        user_log.status("Symbols update already completed by another process");
        return Ok(());
    }

    let previous_etag = match &current {
        Ok(Some(state)) if state.url == url => state.etag.as_deref(),
        _ => None,
    };
    let etag = update_symbols_from_url(
        &registry_dir,
        &home.registry_symbols_dir(),
        &url,
        previous_etag,
    )?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let state = SymbolsSyncState {
        url,
        generation: format!("{}-{nanos}", std::process::id()),
        etag,
    };
    if let Err(error) = state.write(&state_path) {
        user_log.warn(format!(
            "failed to record completed symbols update: {error:#}"
        ));
    }
    user_log.status("Symbols updated successfully");
    Ok(())
}

#[derive(Debug)]
enum SymbolsDownload {
    Modified {
        data: bytes::Bytes,
        etag: Option<String>,
    },
    NotModified {
        etag: String,
    },
}

#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(has_previous_etag = previous_etag.is_some())
)]
fn download_symbols_zip(
    symbols_url: &str,
    previous_etag: Option<&str>,
) -> anyhow::Result<SymbolsDownload> {
    let client = download::client_with_retry()?;
    let conditional_etag = previous_etag.and_then(|etag| {
        HeaderValue::from_bytes(etag.as_bytes())
            .ok()
            .map(|value| (etag, value))
    });
    if previous_etag.is_some() && conditional_etag.is_none() {
        tracing::debug!("Ignoring invalid symbols.zip ETag");
    }

    let mut request = client.get(symbols_url);
    if let Some((_, value)) = &conditional_etag {
        request = request.header(IF_NONE_MATCH, value.clone());
    }
    let response = request.send().context("failed to fetch symbols.zip")?;
    let response_etag = response
        .headers()
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    tracing::debug!(
        status = %response.status(),
        has_etag = response_etag.is_some(),
        "Received symbols.zip response"
    );
    if response.status() == StatusCode::NOT_MODIFIED {
        let etag = response_etag
            .or_else(|| conditional_etag.map(|(etag, _)| etag.to_owned()))
            .context("symbols.zip returned 304 without a usable ETag")?;
        return Ok(SymbolsDownload::NotModified { etag });
    }

    let data = response
        .error_for_status()
        .context("symbols.zip download returned error status")?
        .bytes()
        .context("failed to read symbols.zip response body")?;
    Ok(SymbolsDownload::Modified {
        data,
        etag: response_etag,
    })
}

fn update_symbols_from_url(
    registry_dir: &Path,
    target_dir: &Path,
    symbols_url: &str,
    previous_etag: Option<&str>,
) -> anyhow::Result<Option<String>> {
    std::fs::create_dir_all(registry_dir)
        .with_context(|| format!("failed to create `{}`", registry_dir.display()))?;

    let previous_etag = target_dir.is_dir().then_some(previous_etag).flatten();
    let download = download_symbols_zip(symbols_url, previous_etag)?;
    let (data, etag) = match download {
        SymbolsDownload::Modified { data, etag } => (data, etag),
        SymbolsDownload::NotModified { etag } if target_dir.is_dir() => return Ok(Some(etag)),
        SymbolsDownload::NotModified { .. } => match download_symbols_zip(symbols_url, None)? {
            SymbolsDownload::Modified { data, etag } => (data, etag),
            SymbolsDownload::NotModified { .. } => {
                anyhow::bail!("symbols.zip returned 304 but local symbols are missing")
            }
        },
    };

    let tmp_dir = unique_sibling_dir(registry_dir, ".symbols.tmp")
        .context("failed to create temp directory for symbols")?;
    std::fs::create_dir_all(&tmp_dir)?;

    if let Err(e) = extract_zip_to_dir(&tmp_dir, std::io::Cursor::new(data)) {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(e);
    }

    if target_dir.exists() {
        let backup_dir = unique_sibling_dir(registry_dir, ".symbols.old")
            .context("failed to create backup dir")?;
        std::fs::rename(target_dir, &backup_dir)
            .context("failed to move existing symbols dir to backup")?;

        if let Err(e) = std::fs::rename(&tmp_dir, target_dir) {
            let _ = std::fs::rename(&backup_dir, target_dir);
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(anyhow::Error::from(e).context("failed to replace symbols directory"));
        }

        std::fs::remove_dir_all(&backup_dir).with_context(|| {
            format!(
                "failed to remove old symbols directory at `{}`",
                backup_dir.display()
            )
        })?;
    } else {
        std::fs::rename(&tmp_dir, target_dir)
            .context("failed to move symbols directory into place")?;
    }

    Ok(etag)
}

fn symbols_url(registry_config: &RegistryConfig) -> String {
    if let Some(symbols) = &registry_config.symbols {
        symbols.clone()
    } else if registry_config.legacy_asset_urls {
        SYMBOLS_URL.to_owned()
    } else {
        format!(
            "{}/symbols.zip",
            registry_config.download.trim_end_matches('/')
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Cursor, Read, Write},
        net::TcpListener,
        thread::JoinHandle,
    };

    fn quiet_user_log() -> UserLog {
        UserLog::new(log::LevelFilter::Off)
    }

    fn test_moon_home(root: &tempfile::TempDir) -> MoonHomeLayout {
        let home = MoonHomeLayout::new(root.path().to_path_buf());
        std::fs::create_dir_all(home.registry_dir()).unwrap();
        home
    }

    #[test]
    fn symbols_url_uses_the_split_download_endpoint() {
        let config = RegistryConfig {
            api: "https://api.example".to_owned(),
            index: "https://index.example".to_owned(),
            download: "https://download.example/".to_owned(),
            symbols: None,
            legacy_asset_urls: false,
        };

        assert_eq!(symbols_url(&config), "https://download.example/symbols.zip");
    }

    #[test]
    fn symbols_url_preserves_legacy_overrides() {
        let config = RegistryConfig {
            api: "https://registry.example".to_owned(),
            index: "https://index.example".to_owned(),
            download: "https://registry.example".to_owned(),
            symbols: Some("https://mirror.example/symbols.zip".to_owned()),
            legacy_asset_urls: true,
        };

        assert_eq!(symbols_url(&config), "https://mirror.example/symbols.zip");
    }

    struct TestHttpResponse {
        status: &'static str,
        headers: Vec<(&'static str, &'static str)>,
        body: Vec<u8>,
    }

    fn serve_http(responses: Vec<TestHttpResponse>) -> (String, JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            responses
                .into_iter()
                .map(|response| {
                    let (mut stream, _) = listener.accept().unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0; 1024];
                    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                        let read = stream.read(&mut buffer).unwrap();
                        assert_ne!(read, 0, "request ended before its headers");
                        request.extend_from_slice(&buffer[..read]);
                    }

                    write!(
                        stream,
                        "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                        response.status,
                        response.body.len()
                    )
                    .unwrap();
                    for (name, value) in response.headers {
                        write!(stream, "{name}: {value}\r\n").unwrap();
                    }
                    stream.write_all(b"\r\n").unwrap();
                    stream.write_all(&response.body).unwrap();
                    String::from_utf8(request).unwrap()
                })
                .collect()
        });
        (format!("http://{address}/symbols.zip"), server)
    }

    fn symbols_zip(contents: &str) -> Vec<u8> {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("symbol.txt", zip::write::FileOptions::default())
            .unwrap();
        archive.write_all(contents.as_bytes()).unwrap();
        archive.finish().unwrap().into_inner()
    }

    #[test]
    fn symbols_update_reuses_an_etag_on_not_modified() {
        let root = tempfile::tempdir().unwrap();
        let home = test_moon_home(&root);
        let (url, server) = serve_http(vec![
            TestHttpResponse {
                status: "200 OK",
                headers: vec![("ETag", "\"symbols-v1\"")],
                body: symbols_zip("downloaded"),
            },
            TestHttpResponse {
                status: "304 Not Modified",
                headers: Vec::new(),
                body: Vec::new(),
            },
        ]);

        let config = RegistryConfig {
            index: root
                .path()
                .join("missing.git")
                .to_string_lossy()
                .into_owned(),
            symbols: Some(url),
            ..RegistryConfig::default()
        };
        std::fs::write(home.registry_update_state_path(), "index state").unwrap();
        sync(&home, &config, &quiet_user_log()).unwrap();
        let state = SymbolsSyncState::read(&home.registry_symbols_update_state_path())
            .unwrap()
            .unwrap();
        assert_eq!(state.etag.as_deref(), Some("\"symbols-v1\""));
        std::fs::write(home.registry_symbols_dir().join("symbol.txt"), "local").unwrap();

        sync(&home, &config, &quiet_user_log()).unwrap();
        let refreshed = SymbolsSyncState::read(&home.registry_symbols_update_state_path())
            .unwrap()
            .unwrap();
        assert_eq!(refreshed.etag, state.etag);
        assert_ne!(refreshed.generation, state.generation);
        assert_eq!(
            std::fs::read_to_string(home.registry_update_state_path()).unwrap(),
            "index state"
        );
        assert_eq!(
            std::fs::read_to_string(home.registry_symbols_dir().join("symbol.txt")).unwrap(),
            "local"
        );
        let requests = server.join().unwrap();
        assert!(!requests[0].to_ascii_lowercase().contains("if-none-match"));
        assert!(
            requests[1]
                .to_ascii_lowercase()
                .contains("if-none-match: \"symbols-v1\"")
        );
    }

    #[test]
    fn concurrent_symbols_updates_are_coalesced() {
        let root = tempfile::tempdir().unwrap();
        let home = test_moon_home(&root);
        let (url, server) = serve_http(vec![TestHttpResponse {
            status: "200 OK",
            headers: Vec::new(),
            body: symbols_zip("downloaded"),
        }]);
        let config = RegistryConfig {
            symbols: Some(url),
            ..RegistryConfig::default()
        };
        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
        let lock = lock_directory(&home.registry_dir(), &quiet_user_log()).unwrap();
        std::thread::scope(|scope| {
            let first = scope.spawn(|| sync(&home, &config, &user_log));
            let second = scope.spawn(|| sync(&home, &config, &user_log));
            // Release the lock only after both callers observed the old state.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let mut waiting = 0;
            while waiting < 2 && std::time::Instant::now() < deadline {
                waiting += capture
                    .take()
                    .iter()
                    .filter(|entry| entry.message.starts_with("Blocking waiting for file lock"))
                    .count();
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            drop(lock);
            first.join().unwrap().unwrap();
            second.join().unwrap().unwrap();
            assert_eq!(waiting, 2);
        });
        assert_eq!(server.join().unwrap().len(), 1);
        assert!(capture.take().iter().any(|entry| entry.message == "Symbols update already completed by another process"));
    }

    #[test]
    fn symbols_sync_does_not_reuse_another_urls_etag() {
        let root = tempfile::tempdir().unwrap();
        let home = test_moon_home(&root);
        std::fs::create_dir(home.registry_symbols_dir()).unwrap();
        SymbolsSyncState {
            url: "https://old.example/symbols.zip".to_owned(),
            generation: "previous".to_owned(),
            etag: Some("\"old\"".to_owned()),
        }
        .write(&home.registry_symbols_update_state_path())
        .unwrap();
        let (url, server) = serve_http(vec![TestHttpResponse {
            status: "200 OK",
            headers: Vec::new(),
            body: symbols_zip("new registry"),
        }]);
        let config = RegistryConfig {
            symbols: Some(url),
            ..RegistryConfig::default()
        };
        sync(&home, &config, &quiet_user_log()).unwrap();
        let requests = server.join().unwrap();
        assert!(!requests[0].to_ascii_lowercase().contains("if-none-match"));
        assert_eq!(
            SymbolsSyncState::read(&home.registry_symbols_update_state_path())
                .unwrap()
                .unwrap()
                .etag,
            None
        );
    }

    #[test]
    fn failed_symbols_sync_is_retried_without_publishing_its_etag() {
        let root = tempfile::tempdir().unwrap();
        let home = test_moon_home(&root);
        let (url, server) = serve_http(vec![
            TestHttpResponse {
                status: "200 OK",
                headers: vec![("ETag", "\"broken\"")],
                body: b"not a zip archive".to_vec(),
            },
            TestHttpResponse {
                status: "200 OK",
                headers: vec![("ETag", "\"valid\"")],
                body: symbols_zip("downloaded"),
            },
        ]);
        let config = RegistryConfig {
            symbols: Some(url),
            ..RegistryConfig::default()
        };
        assert!(sync(&home, &config, &quiet_user_log()).is_err());
        assert!(!home.registry_symbols_update_state_path().exists());
        assert!(!home.registry_symbols_dir().exists());
        sync(&home, &config, &quiet_user_log()).unwrap();
        let requests = server.join().unwrap();
        assert!(!requests[1].to_ascii_lowercase().contains("if-none-match"));
        assert_eq!(
            SymbolsSyncState::read(&home.registry_symbols_update_state_path())
                .unwrap()
                .unwrap()
                .etag
                .as_deref(),
            Some("\"valid\"")
        );
    }

    #[test]
    fn symbols_update_ignores_an_etag_when_local_symbols_are_missing() {
        let root = tempfile::tempdir().unwrap();
        let home = test_moon_home(&root);
        let (url, server) = serve_http(vec![TestHttpResponse {
            status: "200 OK",
            headers: vec![("ETag", "\"symbols-v2\"")],
            body: symbols_zip("restored"),
        }]);
        let etag = "\"symbols-v1\"".to_owned();

        let updated = update_symbols_from_url(
            &home.registry_dir(),
            &home.registry_symbols_dir(),
            &url,
            Some(&etag),
        )
        .unwrap()
        .unwrap();

        assert_eq!(updated, "\"symbols-v2\"");
        assert_eq!(
            std::fs::read_to_string(home.registry_symbols_dir().join("symbol.txt")).unwrap(),
            "restored"
        );
        let requests = server.join().unwrap();
        assert!(!requests[0].to_ascii_lowercase().contains("if-none-match"));
    }

    #[test]
    fn symbols_update_ignores_an_invalid_etag() {
        let root = tempfile::tempdir().unwrap();
        let home = test_moon_home(&root);
        std::fs::create_dir(home.registry_symbols_dir()).unwrap();
        let (url, server) = serve_http(vec![TestHttpResponse {
            status: "200 OK",
            headers: Vec::new(),
            body: symbols_zip("updated"),
        }]);
        let etag = "invalid\netag".to_owned();

        let updated = update_symbols_from_url(
            &home.registry_dir(),
            &home.registry_symbols_dir(),
            &url,
            Some(&etag),
        )
        .unwrap();

        assert_eq!(updated, None);
        let requests = server.join().unwrap();
        assert!(!requests[0].to_ascii_lowercase().contains("if-none-match"));
    }
}
