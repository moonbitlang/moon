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

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    fs::File,
    io::{BufRead, Read, Seek, Write},
    path::Path,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, bail};
use indexmap::map::IndexMap;
use moonutil::{
    MOON_HOME, MoonHomeLayout,
    dependency::SourceDependencyInfo,
    locks::{lock_directory, lock_file},
    registry::RegistryConfig,
    resolution::ModuleName,
    user_log::UserLog,
};
use reqwest::{StatusCode, header::USER_AGENT};
use semver::Version;
use serde::Deserialize;

use crate::{
    registry::RegistryVersionInfo,
    update::{RegistryIndexRecloneReason, RegistryIndexUpdate, UpdateOutcome},
    zip_util::extract_zip_to_dir,
};

// Bound connection setup separately from stalled reads. Blocking response
// bodies are streamed so the read timeout resets whenever data arrives.
const REGISTRY_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const REGISTRY_READ_TIMEOUT: Duration = Duration::from_secs(2 * 60);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RegistryIndexEntry {
    version: Option<String>,
    deps: Option<IndexMap<String, SourceDependencyInfo>>,
    checksum: Option<String>,
}

struct RegistryEndpoints {
    packages: String,
    assets: String,
}

impl RegistryEndpoints {
    fn from_config(config: &RegistryConfig) -> Self {
        let registry = config.registry.trim_end_matches('/');
        let packages = if registry == "https://mooncakes.io" {
            "https://download.mooncakes.io/user".to_owned()
        } else {
            format!("{registry}/user")
        };
        Self {
            packages,
            assets: format!("{registry}/assets"),
        }
    }

    fn package_archive(&self, name: &ModuleName, version: &Version) -> String {
        let path = form_urlencoded::Serializer::new(String::new())
            .append_key_only(&format!("{}/{}/{}", name.username, name.unqual, version))
            .finish();
        format!("{}/{}.zip", self.packages, path)
    }

    fn wasm_asset(&self, name: &ModuleName, version: &Version, package_path: &str) -> String {
        let artifact_name = wasm_artifact_name(name, package_path);
        if package_path.is_empty() {
            format!("{}/{}@{version}/{artifact_name}", self.assets, name)
        } else {
            format!(
                "{}/{}@{version}/{package_path}/{artifact_name}",
                self.assets, name
            )
        }
    }
}

/// Access to a configured Mooncakes registry and its local state.
///
/// This client owns synchronization of the Git index and symbols archive as
/// well as verified package and prebuilt wasm downloads. Resolution code can
/// depend on the narrower [`super::Registry`] interface when it does not need
/// to synchronize the registry.
pub struct RegistryClient {
    config: RegistryConfig,
    home: MoonHomeLayout,
    endpoints: RegistryEndpoints,
    http: reqwest::blocking::Client,
    cache: RefCell<HashMap<ModuleName, Arc<BTreeMap<Version, RegistryVersionInfo>>>>,
}

impl RegistryClient {
    /// Load registry configuration and use the standard local index and cache.
    pub fn configured() -> Self {
        Self::from_config(RegistryConfig::load())
    }

    fn from_config(config: RegistryConfig) -> Self {
        Self::with_home(config, MOON_HOME.clone())
    }

    fn with_home(config: RegistryConfig, home: MoonHomeLayout) -> Self {
        Self {
            endpoints: RegistryEndpoints::from_config(&config),
            config,
            home,
            http: reqwest::blocking::Client::builder()
                .user_agent(format!("mooncake/{}", env!("CARGO_PKG_VERSION")))
                .timeout(REGISTRY_READ_TIMEOUT)
                .connect_timeout(REGISTRY_CONNECT_TIMEOUT)
                .build()
                .expect("failed to create registry HTTP client"),
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Return whether a local registry index is available for offline fallback.
    pub fn has_cached_index(&self) -> bool {
        self.home.registry_index_dir().exists()
    }

    /// Synchronize the Git index and symbols archive with the configured registry.
    pub fn sync(&self, user_log: &UserLog) -> anyhow::Result<()> {
        let outcome = crate::update::sync(&self.home, &self.config, &self.http, user_log)?;
        self.cache.borrow_mut().clear();
        log_sync_outcome(outcome, user_log);
        Ok(())
    }

    /// Return a verified, locally cached prebuilt wasm asset.
    pub fn acquire_wasm_asset(
        &self,
        name: &ModuleName,
        version: &Version,
        package_path: &str,
        user_log: &UserLog,
    ) -> anyhow::Result<std::path::PathBuf> {
        self.acquire_wasm_asset_with(name, version, package_path, user_log, |url| {
            download_registry_asset(&self.http, url, user_log)
        })
    }

    fn acquire_wasm_asset_with(
        &self,
        name: &ModuleName,
        version: &Version,
        package_path: &str,
        user_log: &UserLog,
        mut download: impl FnMut(&str) -> anyhow::Result<Vec<u8>>,
    ) -> anyhow::Result<std::path::PathBuf> {
        validate_asset_package_path(name, package_path)?;
        let url = self.endpoints.wasm_asset(name, version, package_path);
        let checksum_url = format!("{url}.sha256");
        let cache_path = self.home.registry_executable_artifact_path(
            name,
            version,
            package_path,
            &wasm_artifact_name(name, package_path),
        );

        ensure_cached_wasm(&cache_path, user_log, |staged| {
            let checksum_bytes = download(&checksum_url)?;
            let expected_checksum = parse_sha256_checksum(&checksum_bytes)
                .with_context(|| format!("invalid SHA-256 checksum from {checksum_url}"))?;
            let bytes = download(&url)?;
            let actual_checksum = sha256_hex(&bytes);
            if actual_checksum != expected_checksum {
                bail!(
                    "prebuilt wasm checksum mismatch for {url}: expected {expected_checksum}, got {actual_checksum}"
                );
            }
            std::fs::write(staged, bytes)
                .with_context(|| format!("failed to write cache file {}", staged.display()))?;
            Ok(())
        })
    }
}

fn log_sync_outcome(outcome: UpdateOutcome, user_log: &UserLog) {
    match outcome.registry_index {
        RegistryIndexUpdate::Cloned => user_log.status("Registry index cloned successfully"),
        RegistryIndexUpdate::Updated => user_log.status("Registry index updated successfully"),
        RegistryIndexUpdate::Recloned(reason) => {
            let reason = match reason {
                RegistryIndexRecloneReason::PullFailed => "Failed to update registry index",
                RegistryIndexRecloneReason::RemoteMismatch => {
                    "Registry index remote does not match the configured URL"
                }
                RegistryIndexRecloneReason::NotGitRepository => {
                    "Registry index is not a Git repository"
                }
                RegistryIndexRecloneReason::MissingOrigin => "Registry index has no origin remote",
            };
            user_log.status(format!("{reason}, re-cloning"));
            user_log.status("Registry index re-cloned successfully");
        }
        RegistryIndexUpdate::ConcurrentUpdateReused => {
            user_log.status("Registry update already completed by another process");
            return;
        }
    }
    if outcome.symbols_updated {
        user_log.status("Symbols updated successfully");
    }
}

fn wasm_artifact_name(name: &ModuleName, package_path: &str) -> String {
    let stem = if package_path.is_empty() {
        name.last_segment()
    } else {
        package_path
            .rsplit('/')
            .next()
            .expect("non-empty package path has a last segment")
    };
    format!("{stem}.wasm")
}

fn validate_asset_package_path(name: &ModuleName, package_path: &str) -> anyhow::Result<()> {
    let coordinate = if package_path.is_empty() {
        name.to_string()
    } else {
        format!("{name}/{package_path}")
    };
    let parsed = super::path::parse_install_style_path(&coordinate)
        .context("invalid registry asset package path")?;
    if parsed.module != *name || parsed.package != package_path {
        bail!("invalid registry asset package path");
    }
    Ok(())
}

fn download_registry_asset(
    http: &reqwest::blocking::Client,
    url: &str,
    user_log: &UserLog,
) -> anyhow::Result<Vec<u8>> {
    user_log.info(format!("Downloading {url}"));
    let response = http
        .get(url)
        .send()
        .with_context(|| format!("failed to download registry asset from {url}"))?;
    if response.status() == StatusCode::NOT_FOUND {
        bail!("Prebuilt wasm asset does not exist");
    }
    let mut response = response
        .error_for_status()
        .with_context(|| format!("registry asset download returned error status for {url}"))?;
    let mut data = Vec::new();
    response
        .read_to_end(&mut data)
        .with_context(|| format!("failed to read registry asset response from {url}"))?;
    Ok(data)
}

fn ensure_cached_wasm(
    cache_path: &Path,
    user_log: &UserLog,
    produce: impl FnOnce(&Path) -> anyhow::Result<()>,
) -> anyhow::Result<std::path::PathBuf> {
    if cache_path.exists() {
        user_log.info(format!("Using cached {}", cache_path.to_string_lossy()));
        return Ok(cache_path.to_path_buf());
    }

    let parent = cache_path
        .parent()
        .context("registry cache path has no parent")?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create registry cache directory {}",
            parent.display()
        )
    })?;
    let _lock = lock_directory(parent, user_log)
        .with_context(|| format!("failed to lock cache directory {}", parent.display()))?;

    if cache_path.exists() {
        user_log.info(format!("Using cached {}", cache_path.to_string_lossy()));
        return Ok(cache_path.to_path_buf());
    }

    let staged = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create cache file in {}", parent.display()))?;
    produce(staged.path())?;
    staged
        .as_file()
        .sync_all()
        .with_context(|| format!("failed to sync cache file {}", staged.path().display()))?;
    staged
        .persist(cache_path)
        .with_context(|| format!("failed to publish cached file to {}", cache_path.display()))?;
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(cache_path.to_path_buf())
}

fn parse_sha256_checksum(bytes: &[u8]) -> anyhow::Result<String> {
    let text = std::str::from_utf8(bytes).context("SHA-256 checksum is not valid UTF-8")?;
    let checksum = text
        .split_whitespace()
        .next()
        .context("SHA-256 checksum is empty")?;
    if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("SHA-256 checksum must be a 64-character hex digest");
    }
    Ok(checksum.to_ascii_lowercase())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    format!("{:x}", Sha256::digest(bytes))
}

impl super::Registry for RegistryClient {
    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Arc<BTreeMap<Version, RegistryVersionInfo>>> {
        // check cache
        if let Some(v) = self.cache.borrow().get(name) {
            return Ok(Arc::clone(v));
        }

        let index_file = self.home.registry_index_file(name);
        log::debug!("Reading versions of {} from {}", name, index_file.display());
        let file = std::fs::File::open(index_file)?;
        let reader = std::io::BufReader::new(file);

        let lines = reader.lines();
        let mut res = BTreeMap::new();
        for line in lines {
            let line = line?;
            let entry = match serde_json_lenient::from_str::<RegistryIndexEntry>(&line) {
                Ok(m) => m,
                Err(e) => {
                    log::warn!("Error when reading index file of {}: {}", name, e);
                    continue;
                }
            };
            if let Some(v) = entry.version.as_deref() {
                res.insert(
                    Version::parse(v)?,
                    RegistryVersionInfo {
                        deps: entry.deps.unwrap_or_default(),
                    },
                );
            }
        }

        // put in cache
        let res = Arc::new(res);
        self.cache
            .borrow_mut()
            .insert(name.clone(), Arc::clone(&res));

        Ok(res)
    }
}

impl super::RegistrySource for RegistryClient {
    fn acquire_source_to(
        &self,
        name: &ModuleName,
        version: &Version,
        expected_checksum: &str,
        to: &Path,
        user_log: &UserLog,
    ) -> anyhow::Result<()> {
        RegistryClient::acquire_source_to(self, name, version, expected_checksum, to, user_log)
    }

    fn source_archive_checksum(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<String> {
        RegistryClient::source_archive_checksum(self, name, version)
    }
}

fn calc_sha2(reader: &mut impl Read) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    let mut buffer = [0; 1024];
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    // read hash digest and consume hasher
    let result = hasher.finalize();
    Ok(format!("{result:x}"))
}

/// Keep the file that was hashed open so a concurrent cache-path replacement
/// cannot change the bytes later consumed by extraction.
fn open_verified_archive(path: &Path, expected_checksum: &str) -> std::io::Result<Option<File>> {
    let mut archive = File::open(path)?;
    if calc_sha2(&mut archive)? != expected_checksum {
        return Ok(None);
    }
    archive.rewind()?;
    Ok(Some(archive))
}

fn open_cached_archive(path: &Path, expected_checksum: &str) -> anyhow::Result<Option<File>> {
    match open_verified_archive(path, expected_checksum) {
        Ok(archive) => Ok(archive),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| {
            format!(
                "Unable to read cached registry archive `{}`",
                path.display()
            )
        }),
    }
}

fn copy_archive_and_verify_checksum(
    source: &mut impl Read,
    destination: &mut impl Write,
    name: &ModuleName,
    version: &Version,
    expected_checksum: &str,
) -> anyhow::Result<()> {
    use sha2::Digest;

    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let bytes_read = source.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        destination.write_all(&buffer[..bytes_read])?;
    }

    let actual_checksum = format!("{:x}", hasher.finalize());
    if actual_checksum != expected_checksum {
        bail!(
            "Checksum mismatch for {name}@{version}: expected {expected_checksum}, got {actual_checksum}"
        );
    }
    Ok(())
}

fn persist_verified_archive(
    source: &mut impl Read,
    cache_file: &Path,
    name: &ModuleName,
    version: &Version,
    expected_checksum: &str,
) -> anyhow::Result<File> {
    let parent = cache_file
        .parent()
        .expect("registry cache file has a parent");
    std::fs::create_dir_all(parent)?;
    let mut archive = tempfile::NamedTempFile::new_in(parent)?;
    copy_archive_and_verify_checksum(source, &mut archive, name, version, expected_checksum)?;
    archive.flush()?;
    archive.as_file_mut().sync_all()?;
    let mut archive = archive.persist(cache_file).map_err(|error| error.error)?;
    archive.rewind()?;
    Ok(archive)
}

impl RegistryClient {
    fn read_checksum_from_index_file(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<String> {
        let p = self.home.registry_index_file(name);
        let file = std::fs::File::open(&p)?;
        let reader = std::io::BufReader::new(file);

        let lines = reader.lines().collect::<std::io::Result<Vec<String>>>()?;
        let version_str = version.to_string();
        for line in lines.iter().rev() {
            let entry = serde_json_lenient::from_str::<RegistryIndexEntry>(line)?;
            if entry.version.as_deref() == Some(version_str.as_str()) {
                if let Some(checksum) = entry.checksum {
                    return Ok(checksum);
                } else {
                    bail!(
                        "No checksum found for version {} in index file {:?}",
                        version,
                        p
                    );
                }
            }
        }
        bail!(
            "No description found for version {} in index file {:?}",
            version,
            p,
        );
    }

    fn download_or_use_cache(
        &self,
        name: &ModuleName,
        version: &Version,
        expected_checksum: &str,
        user_log: &UserLog,
    ) -> anyhow::Result<File> {
        let pkg_index = self.home.registry_index_file(name);
        if !pkg_index.exists() {
            anyhow::bail!("Module {}@{} not found", name, version);
        }
        let cache_file = self.home.registry_source_archive_path(name, version);
        if let Some(archive) = open_cached_archive(&cache_file, expected_checksum)? {
            user_log.status(format!("Using cached {name}@{version}"));
            return Ok(archive);
        }

        let cache_dir = cache_file
            .parent()
            .expect("registry cache file has a parent");
        std::fs::create_dir_all(cache_dir)?;
        let _cache_lock = lock_file(&cache_file, user_log).with_context(|| {
            format!(
                "Unable to lock registry archive cache file `{}`",
                cache_file.display()
            )
        })?;
        if let Some(archive) = open_cached_archive(&cache_file, expected_checksum)? {
            user_log.status(format!("Using cached {name}@{version}"));
            return Ok(archive);
        }
        user_log.status(format!("Downloading {name}@{version}"));
        let url = self.endpoints.package_archive(name, version);
        let mut response = self
            .http
            .get(url)
            .header(
                USER_AGENT,
                format!("mooncake/{}", env!("CARGO_PKG_VERSION")),
            )
            .send()?
            .error_for_status()?;

        persist_verified_archive(&mut response, &cache_file, name, version, expected_checksum)
    }

    /// Return the registry index's SHA-256 checksum for a published source archive.
    pub fn source_archive_checksum(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<String> {
        self.read_checksum_from_index_file(name, version)
    }

    /// Materialize verified published source without executing package hooks.
    pub fn materialize_source_to(
        &self,
        name: &ModuleName,
        version: &Version,
        to: &Path,
        user_log: &UserLog,
    ) -> anyhow::Result<()> {
        let checksum = self.source_archive_checksum(name, version)?;
        self.acquire_source_to(name, version, &checksum, to, user_log)
    }

    /// Reuse or download, verify, and extract a registry package without
    /// running `scripts.postadd`.
    pub fn acquire_source_to(
        &self,
        name: &ModuleName,
        version: &Version,
        expected_checksum: &str,
        pkg_install_dir: &Path,
        user_log: &UserLog,
    ) -> anyhow::Result<()> {
        // ensure dir exists and is empty
        if !pkg_install_dir.exists() {
            std::fs::create_dir_all(pkg_install_dir).unwrap();
        } else {
            std::fs::remove_dir_all(pkg_install_dir).unwrap();
            std::fs::create_dir_all(pkg_install_dir).unwrap();
        }

        let archive = self.download_or_use_cache(name, version, expected_checksum, user_log)?;
        extract_zip_to_dir(pkg_install_dir, archive)?;
        Ok(())
    }
}

#[test]
fn test_urlencode() {
    let s = form_urlencoded::Serializer::new(String::new())
        .append_key_only("0.1.2+3")
        .finish();
    assert_eq!(s, "0.1.2%2B3");
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Cursor, Read, Write},
        net::TcpListener,
        sync::{
            Arc, Barrier,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use log::LevelFilter;
    use moonutil::{constants::MOON_LOCK, locks::lock_file, user_log::UserLogEntryLevel};

    use super::*;
    use crate::registry::Registry;

    fn quiet_user_log() -> UserLog {
        UserLog::new(log::LevelFilter::Error)
    }

    #[test]
    fn registry_update_status_respects_quiet_user_log() {
        let (user_log, capture) = UserLog::captured(LevelFilter::Error);

        log_sync_outcome(
            UpdateOutcome {
                registry_index: RegistryIndexUpdate::Cloned,
                symbols_updated: true,
            },
            &user_log,
        );

        assert!(capture.take().is_empty());
    }

    #[test]
    fn registry_update_status_describes_reclone() {
        let (user_log, capture) = UserLog::captured(LevelFilter::Warn);

        log_sync_outcome(
            UpdateOutcome {
                registry_index: RegistryIndexUpdate::Recloned(
                    RegistryIndexRecloneReason::NotGitRepository,
                ),
                symbols_updated: true,
            },
            &user_log,
        );

        let entries = capture.take();
        assert_eq!(entries.len(), 3);
        assert!(
            entries
                .iter()
                .all(|entry| matches!(entry.level, UserLogEntryLevel::Info))
        );
        assert_eq!(
            entries
                .into_iter()
                .map(|entry| entry.message)
                .collect::<Vec<_>>(),
            [
                "Registry index is not a Git repository, re-cloning",
                "Registry index re-cloned successfully",
                "Symbols updated successfully",
            ]
        );
    }

    #[test]
    fn registry_update_status_describes_concurrent_reuse() {
        let (user_log, capture) = UserLog::captured(LevelFilter::Warn);

        log_sync_outcome(
            UpdateOutcome {
                registry_index: RegistryIndexUpdate::ConcurrentUpdateReused,
                symbols_updated: true,
            },
            &user_log,
        );

        assert_eq!(
            capture
                .take()
                .into_iter()
                .map(|entry| entry.message)
                .collect::<Vec<_>>(),
            ["Registry update already completed by another process"]
        );
    }

    #[test]
    fn official_registry_uses_download_service() {
        let endpoints = RegistryEndpoints::from_config(&RegistryConfig {
            registry: "https://mooncakes.io/".to_owned(),
            index: String::new(),
            symbols: None,
        });
        assert_eq!(
            endpoints.package_archive(&"test/pkg".into(), &Version::new(1, 2, 3)),
            "https://download.mooncakes.io/user/test%2Fpkg%2F1.2.3.zip"
        );
    }

    #[test]
    fn configured_registry_serves_package_downloads() {
        let endpoints = RegistryEndpoints::from_config(&RegistryConfig {
            registry: "https://registry.example.com/".to_owned(),
            index: String::new(),
            symbols: None,
        });
        assert_eq!(
            endpoints.package_archive(&"test/pkg".into(), &Version::new(1, 2, 3)),
            "https://registry.example.com/user/test%2Fpkg%2F1.2.3.zip"
        );
    }

    fn asset_test_registry(sandbox: &tempfile::TempDir) -> RegistryClient {
        RegistryClient::with_home(
            RegistryConfig {
                registry: "https://mooncakes.io".to_owned(),
                index: String::new(),
                symbols: None,
            },
            MoonHomeLayout::new(sandbox.path().to_path_buf()),
        )
    }

    fn parser_module() -> ModuleName {
        "moonbitlang/parser".into()
    }

    #[test]
    fn wasm_asset_urls_are_internal_to_registry_client() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let registry = asset_test_registry(&sandbox);
        let version = Version::new(0, 3, 3);

        assert_eq!(
            registry
                .endpoints
                .wasm_asset(&parser_module(), &version, "cmd/moonfmt"),
            "https://mooncakes.io/assets/moonbitlang/parser@0.3.3/cmd/moonfmt/moonfmt.wasm"
        );
        assert_eq!(
            registry
                .endpoints
                .wasm_asset(&parser_module(), &version, ""),
            "https://mooncakes.io/assets/moonbitlang/parser@0.3.3/parser.wasm"
        );
    }

    #[test]
    fn parse_wasm_asset_sha256sum_output() {
        let checksum = "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789";
        assert_eq!(
            parse_sha256_checksum(format!("{checksum}  moonfmt.wasm\n").as_bytes()).unwrap(),
            checksum.to_ascii_lowercase()
        );
        assert!(parse_sha256_checksum(b"not-a-checksum").is_err());
        assert!(parse_sha256_checksum(b"").is_err());
    }

    #[test]
    fn wasm_asset_cache_miss_downloads_checksum_and_wasm() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let registry = asset_test_registry(&sandbox);
        let wasm = b"\0asmtest".to_vec();
        let checksum = sha256_hex(&wasm);
        let mut urls = Vec::new();
        let path = registry
            .acquire_wasm_asset_with(
                &parser_module(),
                &Version::new(0, 3, 3),
                "cmd/moonfmt",
                &quiet_user_log(),
                |url| {
                    urls.push(url.to_owned());
                    if url.ends_with(".sha256") {
                        Ok(format!("{checksum}  moonfmt.wasm\n").into_bytes())
                    } else {
                        Ok(wasm.clone())
                    }
                },
            )
            .unwrap();

        assert_eq!(std::fs::read(path).unwrap(), b"\0asmtest");
        assert_eq!(
            urls,
            [
                "https://mooncakes.io/assets/moonbitlang/parser@0.3.3/cmd/moonfmt/moonfmt.wasm.sha256",
                "https://mooncakes.io/assets/moonbitlang/parser@0.3.3/cmd/moonfmt/moonfmt.wasm",
            ]
        );
    }

    #[test]
    fn wasm_asset_cache_hit_does_not_download() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let registry = asset_test_registry(&sandbox);
        let name = parser_module();
        let version = Version::new(0, 3, 3);
        let cache_path = registry.home.registry_executable_artifact_path(
            &name,
            &version,
            "cmd/moonfmt",
            "moonfmt.wasm",
        );
        std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
        std::fs::write(&cache_path, b"\0asmtest").unwrap();

        let path = registry
            .acquire_wasm_asset_with(&name, &version, "cmd/moonfmt", &quiet_user_log(), |_| {
                bail!("cache hit should not download")
            })
            .unwrap();

        assert_eq!(path, cache_path);
        assert!(!cache_path.parent().unwrap().join(MOON_LOCK).exists());
    }

    #[test]
    fn concurrent_wasm_asset_cache_misses_download_once() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let home = MoonHomeLayout::new(sandbox.path().to_path_buf());
        let start = Arc::new(Barrier::new(3));
        let download_count = Arc::new(AtomicUsize::new(0));
        let wasm = b"\0asmtest".to_vec();
        let checksum = sha256_hex(&wasm);

        let threads = (0..2)
            .map(|_| {
                let registry = RegistryClient::with_home(
                    RegistryConfig {
                        registry: "https://mooncakes.io".to_owned(),
                        index: String::new(),
                        symbols: None,
                    },
                    home.clone(),
                );
                let start = Arc::clone(&start);
                let download_count = Arc::clone(&download_count);
                let wasm = wasm.clone();
                let checksum = checksum.clone();
                std::thread::spawn(move || {
                    start.wait();
                    registry.acquire_wasm_asset_with(
                        &parser_module(),
                        &Version::new(0, 3, 3),
                        "cmd/moonfmt",
                        &quiet_user_log(),
                        |url| {
                            if url.ends_with(".sha256") {
                                download_count.fetch_add(1, Ordering::SeqCst);
                                std::thread::sleep(Duration::from_millis(50));
                                Ok(format!("{checksum}  moonfmt.wasm\n").into_bytes())
                            } else {
                                Ok(wasm.clone())
                            }
                        },
                    )
                })
            })
            .collect::<Vec<_>>();
        start.wait();

        for thread in threads {
            assert_eq!(
                std::fs::read(thread.join().unwrap().unwrap()).unwrap(),
                b"\0asmtest"
            );
        }
        assert_eq!(download_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn wasm_asset_checksum_mismatch_is_not_cached() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let registry = asset_test_registry(&sandbox);
        let name = parser_module();
        let version = Version::new(0, 3, 3);
        let expected_checksum = sha256_hex(b"expected wasm");

        let error = registry
            .acquire_wasm_asset_with(&name, &version, "cmd/moonfmt", &quiet_user_log(), |url| {
                if url.ends_with(".sha256") {
                    Ok(format!("{expected_checksum}\n").into_bytes())
                } else {
                    Ok(b"different wasm".to_vec())
                }
            })
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("prebuilt wasm checksum mismatch")
        );
        assert!(
            !registry
                .home
                .registry_executable_artifact_path(&name, &version, "cmd/moonfmt", "moonfmt.wasm")
                .exists()
        );
    }

    #[test]
    fn wasm_asset_rejects_unvalidated_package_paths() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let registry = asset_test_registry(&sandbox);
        let error = registry
            .acquire_wasm_asset_with(
                &parser_module(),
                &Version::new(0, 3, 3),
                "../escape",
                &quiet_user_log(),
                |_| bail!("invalid paths must fail before downloading"),
            )
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("invalid registry asset package path")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn registry_download_returns_error_when_system_proxy_service_is_unavailable() {
        const CHILD_PROCESS: &str = "MOON_TEST_SYSTEM_PROXY_UNAVAILABLE_CHILD";

        if std::env::var_os(CHILD_PROCESS).is_some() {
            let sandbox = tempfile::TempDir::new().unwrap();
            let (mut registry, name, version) = test_registry(&sandbox);
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let unavailable_address = listener.local_addr().unwrap();
            drop(listener);
            registry.endpoints.packages = format!("http://{unavailable_address}");

            registry
                .acquire_source_to(
                    &name,
                    &version,
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    &sandbox.path().join("source"),
                    &quiet_user_log(),
                )
                .unwrap_err();
            return;
        }

        // Apply the macOS profile to a child process, then run only this test there.
        let mut child = std::process::Command::new("/usr/bin/sandbox-exec");
        child
            .args([
                "-p",
                r#"(version 1)
(allow default)
(deny mach-lookup (global-name "com.apple.SystemConfiguration.configd"))"#,
            ])
            .arg(std::env::current_exe().unwrap())
            .args([
                "registry_download_returns_error_when_system_proxy_service_is_unavailable",
                "--nocapture",
            ])
            .env(CHILD_PROCESS, "1");
        for name in [
            "ALL_PROXY",
            "all_proxy",
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "NO_PROXY",
            "no_proxy",
        ] {
            child.env_remove(name);
        }

        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "sandboxed registry download failed with {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    #[cfg(unix)]
    #[test]
    fn verified_archive_handle_survives_cache_path_replacement() {
        let cache = tempfile::TempDir::new().unwrap();
        let archive_path = cache.path().join("archive.zip");
        std::fs::write(&archive_path, "verified archive").unwrap();

        let mut archive = open_verified_archive(
            &archive_path,
            "040a1170825ade3ff37b189dd280153ecfafb99ee929d1cbebb40fe135afdf26",
        )
        .unwrap()
        .unwrap();

        let mut replacement = tempfile::NamedTempFile::new_in(cache.path()).unwrap();
        replacement.write_all(b"replacement archive").unwrap();
        replacement.persist(&archive_path).unwrap();

        let mut contents = String::new();
        archive.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "verified archive");
    }

    #[test]
    fn cached_archive_checksum_mismatch_is_rejected() {
        let cache = tempfile::TempDir::new().unwrap();
        let archive_path = cache.path().join("archive.zip");
        std::fs::write(&archive_path, "cached archive").unwrap();

        let archive = open_verified_archive(
            &archive_path,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .unwrap();

        assert!(archive.is_none());
        assert_eq!(
            std::fs::read_to_string(archive_path).unwrap(),
            "cached archive"
        );
    }

    fn test_registry(sandbox: &tempfile::TempDir) -> (RegistryClient, ModuleName, Version) {
        let name: ModuleName = "test/module".into();
        let version = Version::new(1, 2, 3);
        let home = MoonHomeLayout::new(sandbox.path().to_path_buf());
        let index_file = home.registry_index_file(&name);
        std::fs::create_dir_all(index_file.parent().unwrap()).unwrap();
        std::fs::write(&index_file, "registry entry exists").unwrap();
        (
            RegistryClient::with_home(
                RegistryConfig {
                    registry: String::new(),
                    index: String::new(),
                    symbols: None,
                },
                home,
            ),
            name,
            version,
        )
    }

    fn test_archive() -> Vec<u8> {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("moon.mod", zip::write::FileOptions::default())
            .unwrap();
        archive
            .write_all(b"name = \"test/module\"\nversion = \"1.2.3\"\n")
            .unwrap();
        archive.finish().unwrap().into_inner()
    }

    #[test]
    fn acquire_source_to_reuses_and_extracts_a_verified_cached_zip() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let (registry, name, version) = test_registry(&sandbox);
        let archive = test_archive();
        let checksum = calc_sha2(&mut Cursor::new(&archive)).unwrap();
        let archive_path = registry.home.registry_source_archive_path(&name, &version);
        std::fs::create_dir_all(archive_path.parent().unwrap()).unwrap();
        std::fs::write(archive_path, archive).unwrap();
        let destination = sandbox.path().join("source");

        registry
            .acquire_source_to(&name, &version, &checksum, &destination, &quiet_user_log())
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(destination.join("moon.mod")).unwrap(),
            "name = \"test/module\"\nversion = \"1.2.3\"\n"
        );
    }

    #[test]
    fn verified_cached_archive_does_not_wait_for_download_lock() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let (registry, name, version) = test_registry(&sandbox);
        let archive = test_archive();
        let checksum = calc_sha2(&mut Cursor::new(&archive)).unwrap();
        let archive_path = registry.home.registry_source_archive_path(&name, &version);
        std::fs::create_dir_all(archive_path.parent().unwrap()).unwrap();
        std::fs::write(&archive_path, archive).unwrap();

        let module_lock =
            lock_directory(archive_path.parent().unwrap(), &quiet_user_log()).unwrap();
        let version_lock = lock_file(&archive_path, &quiet_user_log()).unwrap();

        let destination = sandbox.path().join("source");
        let (result_sender, result_receiver) = std::sync::mpsc::channel();
        let client = std::thread::spawn(move || {
            result_sender
                .send(registry.acquire_source_to(
                    &name,
                    &version,
                    &checksum,
                    &destination,
                    &quiet_user_log(),
                ))
                .unwrap();
        });

        let result = result_receiver.recv_timeout(Duration::from_secs(5));
        drop(version_lock);
        drop(module_lock);
        client.join().unwrap();
        result
            .expect("verified cache hits should not wait for the download lock")
            .unwrap();
    }

    #[test]
    fn concurrent_archive_acquisitions_are_coalesced() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let (registry, name, version) = test_registry(&sandbox);
        let archive = test_archive();
        let checksum = calc_sha2(&mut Cursor::new(&archive)).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url_base = format!("http://{}", listener.local_addr().unwrap());
        let request_count = Arc::new(AtomicUsize::new(0));
        let stop_server = Arc::new(AtomicBool::new(false));
        let server = {
            let request_count = Arc::clone(&request_count);
            let stop_server = Arc::clone(&stop_server);
            std::thread::spawn(move || {
                loop {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            // Accepted sockets inherit the listener's non-blocking mode on
                            // Windows, but request handling below is intentionally blocking.
                            stream.set_nonblocking(false).unwrap();
                            let mut request = [0; 2048];
                            assert_ne!(stream.read(&mut request).unwrap(), 0);
                            if request_count.fetch_add(1, Ordering::Relaxed) == 0 {
                                // Keep the first request in flight long enough for both
                                // callers to observe the initially empty cache.
                                std::thread::sleep(Duration::from_millis(200));
                            }
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                archive.len()
                            );
                            stream.write_all(response.as_bytes()).unwrap();
                            stream.write_all(&archive).unwrap();
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if stop_server.load(Ordering::Relaxed) {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("registry test server failed: {error}"),
                    }
                }
            })
        };

        let barrier = Arc::new(Barrier::new(2));
        let clients = (0..2)
            .map(|client| {
                let home = registry.home.clone();
                let url_base = url_base.clone();
                let name = name.clone();
                let version = version.clone();
                let checksum = checksum.clone();
                let destination = sandbox.path().join(format!("source-{client}"));
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let registry = RegistryClient::with_home(
                        RegistryConfig {
                            registry: url_base,
                            index: String::new(),
                            symbols: None,
                        },
                        home,
                    );
                    barrier.wait();
                    registry.acquire_source_to(
                        &name,
                        &version,
                        &checksum,
                        &destination,
                        &quiet_user_log(),
                    )?;
                    anyhow::ensure!(
                        std::fs::read_to_string(destination.join("moon.mod"))?
                            == "name = \"test/module\"\nversion = \"1.2.3\"\n"
                    );
                    Ok::<_, anyhow::Error>(())
                })
            })
            .collect::<Vec<_>>();

        let client_results = clients
            .into_iter()
            .map(|client| client.join().unwrap())
            .collect::<Vec<_>>();
        stop_server.store(true, Ordering::Relaxed);
        server.join().unwrap();
        for result in client_results {
            result.unwrap();
        }
        assert_eq!(request_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn sequential_archive_downloads_reuse_http_connection() {
        fn serve_requests(mut stream: std::net::TcpStream, archive: &[u8], count: usize) {
            for _ in 0..count {
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                loop {
                    let read = stream.read(&mut buffer).unwrap();
                    if read == 0 {
                        return;
                    }
                    request.extend_from_slice(&buffer[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }

                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
                    archive.len()
                )
                .unwrap();
                stream.write_all(archive).unwrap();
            }
        }

        let sandbox = tempfile::TempDir::new().unwrap();
        let (mut registry, name, first_version) = test_registry(&sandbox);
        let second_version = Version::new(1, 2, 4);
        let archive = test_archive();
        let checksum = calc_sha2(&mut Cursor::new(&archive)).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        registry.endpoints.packages = format!("http://{}", listener.local_addr().unwrap());

        let server = std::thread::spawn(move || {
            let (first_stream, _) = listener.accept().unwrap();
            let first_control = first_stream.try_clone().unwrap();
            let archive = Arc::new(archive);
            let (first_done_tx, first_done_rx) = std::sync::mpsc::channel();
            let first_connection = {
                let archive = Arc::clone(&archive);
                std::thread::spawn(move || {
                    serve_requests(first_stream, &archive, 2);
                    let _ = first_done_tx.send(());
                })
            };

            listener.set_nonblocking(true).unwrap();
            let connections = loop {
                match first_done_rx.try_recv() {
                    Ok(()) => break 1,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        panic!("first registry test connection failed")
                    }
                }
                match listener.accept() {
                    Ok((second_stream, _)) => {
                        serve_requests(second_stream, &archive, 1);
                        break 2;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) => panic!("registry test server failed: {error}"),
                }
                std::thread::sleep(Duration::from_millis(5));
            };

            if connections != 1 {
                let _ = first_control.shutdown(std::net::Shutdown::Both);
            }
            first_connection.join().unwrap();
            connections
        });

        registry
            .acquire_source_to(
                &name,
                &first_version,
                &checksum,
                &sandbox.path().join("source-first"),
                &quiet_user_log(),
            )
            .unwrap();
        registry
            .acquire_source_to(
                &name,
                &second_version,
                &checksum,
                &sandbox.path().join("source-second"),
                &quiet_user_log(),
            )
            .unwrap();

        assert_eq!(
            server.join().unwrap(),
            1,
            "sequential registry downloads should share one HTTP connection"
        );
    }

    #[test]
    fn downloaded_archive_checksum_mismatch_is_rejected() {
        let name: ModuleName = "test/module".into();
        let version = Version::new(1, 2, 3);
        let archive = test_archive();
        let mut downloaded = Vec::new();
        let expected_checksum = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        let error = copy_archive_and_verify_checksum(
            &mut Cursor::new(&archive),
            &mut downloaded,
            &name,
            &version,
            expected_checksum,
        )
        .unwrap_err();

        assert!(
            format!("{error:#}").contains("Checksum mismatch for test/module@1.2.3"),
            "{error:#}"
        );
        assert_eq!(downloaded, archive);
    }

    #[test]
    fn checksum_mismatch_does_not_publish_archive_cache_file() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let cache_file = sandbox.path().join("archive.zip");
        let archive = test_archive();
        let name: ModuleName = "test/module".into();
        let version = Version::new(1, 2, 3);

        let error = persist_verified_archive(
            &mut Cursor::new(&archive),
            &cache_file,
            &name,
            &version,
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("Checksum mismatch for test/module@1.2.3"));
        assert!(!cache_file.exists());
    }

    #[test]
    fn acquire_source_to_reports_cached_archive_io_errors() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let (registry, name, version) = test_registry(&sandbox);
        let archive_path = registry.home.registry_source_archive_path(&name, &version);
        std::fs::create_dir_all(&archive_path).unwrap();
        let destination = sandbox.path().join("source");

        let error = registry
            .acquire_source_to(
                &name,
                &version,
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                &destination,
                &quiet_user_log(),
            )
            .unwrap_err();

        assert!(
            format!("{error:#}").contains("Unable to read cached registry archive"),
            "{error:#}"
        );
        assert!(!destination.join("moon.mod").exists());
    }

    fn temp_index_dir() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "mooncake-registry-index-test-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn all_versions_accepts_single_rule_object_from_index_jsonl() {
        let home = MoonHomeLayout::new(temp_index_dir());
        let module: ModuleName = "bobzhang/openseek".into();
        let index_file = home.registry_index_file(&module);
        std::fs::create_dir_all(index_file.parent().unwrap()).unwrap();
        std::fs::write(
            &index_file,
            r#"{"name":"bobzhang/openseek","version":"0.2.1","deps":{"bobzhang/jsonl":"0.2.0"},"preferred_target":"native","checksum":"abc123","rule":{"name":"md_to_mbt_string","command":"moon run --quiet --target native scripts/md_to_mbt_string -- \"$input\" \"$output\""}}
"#,
        )
        .unwrap();

        let registry = RegistryClient::with_home(
            RegistryConfig {
                registry: String::new(),
                index: String::new(),
                symbols: None,
            },
            home.clone(),
        );
        let versions = registry
            .all_versions_of(&ModuleName {
                username: "bobzhang".into(),
                unqual: "openseek".into(),
            })
            .unwrap();

        let version = Version::parse("0.2.1").unwrap();
        assert!(versions.contains_key(&version));
        assert_eq!(
            registry
                .read_checksum_from_index_file(
                    &ModuleName {
                        username: "bobzhang".into(),
                        unqual: "openseek".into(),
                    },
                    &version,
                )
                .unwrap(),
            "abc123"
        );

        let _ = std::fs::remove_dir_all(home.root());
    }
}
