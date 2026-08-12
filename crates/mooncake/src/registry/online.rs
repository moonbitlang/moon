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
};

use anyhow::{Context, bail};
use indexmap::map::IndexMap;
use moonutil::{
    dependency::SourceDependencyInfo, locks::FileLock, registry::RegistryConfig,
    resolution::ModuleName, user_log::UserLog,
};
use reqwest::header::USER_AGENT;
use semver::Version;
use serde::Deserialize;

use crate::{registry::RegistryVersionInfo, zip_util::extract_zip_to_dir};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RegistryIndexEntry {
    version: Option<String>,
    deps: Option<IndexMap<String, SourceDependencyInfo>>,
    checksum: Option<String>,
}

pub struct OnlineRegistry {
    index: std::path::PathBuf,
    url_base: String, // TODO: add download feature to registry interface
    archive_cache: std::path::PathBuf,
    cache: RefCell<HashMap<ModuleName, Arc<BTreeMap<Version, RegistryVersionInfo>>>>,
}

impl OnlineRegistry {
    pub fn mooncakes_io() -> Self {
        let registry = RegistryConfig::load().registry;
        OnlineRegistry {
            index: moonutil::registry::index(),
            url_base: registry_download_base(&registry),
            archive_cache: moonutil::registry::cache(),
            cache: RefCell::new(HashMap::new()),
        }
    }

    fn index_file_of(&self, name: &ModuleName) -> std::path::PathBuf {
        self.index
            .join("user")
            .join(name.username.as_str())
            .join(format!("{}.index", name.unqual))
    }

    fn archive_cache_file_of(&self, name: &ModuleName, version: &Version) -> std::path::PathBuf {
        self.archive_cache
            .join(name.username.as_str())
            .join(name.unqual.as_str())
            .join(format!("{version}.zip"))
    }
}

fn registry_download_base(registry: &str) -> String {
    let registry = registry.trim_end_matches('/');
    if registry == "https://mooncakes.io" {
        "https://download.mooncakes.io/user".to_string()
    } else {
        format!("{registry}/user")
    }
}

impl super::Registry for OnlineRegistry {
    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Arc<BTreeMap<Version, RegistryVersionInfo>>> {
        // check cache
        if let Some(v) = self.cache.borrow().get(name) {
            return Ok(Arc::clone(v));
        }

        let index_file = self.index_file_of(name);
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

    fn acquire_source_to(
        &self,
        name: &ModuleName,
        version: &Version,
        expected_checksum: &str,
        to: &Path,
        user_log: &UserLog,
    ) -> anyhow::Result<()> {
        OnlineRegistry::acquire_source_to(self, name, version, expected_checksum, to, user_log)
    }

    fn source_archive_checksum(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<String> {
        self.read_checksum_from_index_file(name, version)
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

impl OnlineRegistry {
    fn read_checksum_from_index_file(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<String> {
        let p = self.index_file_of(name);
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
        let pkg_index = self.index_file_of(name);
        if !pkg_index.exists() {
            anyhow::bail!("Module {}@{} not found", name, version);
        }
        let cache_file = self.archive_cache_file_of(name, version);
        let cache_dir = cache_file
            .parent()
            .expect("registry cache file has a parent");
        std::fs::create_dir_all(cache_dir)?;
        let _cache_lock = FileLock::lock_with_user_log(cache_dir, user_log).with_context(|| {
            format!(
                "Unable to lock registry archive cache directory `{}`",
                cache_dir.display()
            )
        })?;
        match open_verified_archive(&cache_file, expected_checksum) {
            Ok(Some(archive)) => {
                user_log.status(format!("Using cached {name}@{version}"));
                return Ok(archive);
            }
            Ok(None) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "Unable to read cached registry archive `{}`",
                        cache_file.display()
                    )
                });
            }
        }
        user_log.status(format!("Downloading {name}@{version}"));
        let filepath = form_urlencoded::Serializer::new(String::new())
            .append_key_only(&format!("{}/{}/{}", name.username, name.unqual, version))
            .finish();
        let url = format!("{}/{}.zip", self.url_base, filepath);
        let client = reqwest::blocking::Client::new();
        let mut response = client
            .get(url)
            .header(
                USER_AGENT,
                format!("mooncake/{}", env!("CARGO_PKG_VERSION")),
            )
            .send()?
            .error_for_status()?;

        persist_verified_archive(&mut response, &cache_file, name, version, expected_checksum)
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

    use super::*;
    use crate::registry::Registry;

    fn quiet_user_log() -> UserLog {
        UserLog::new(log::LevelFilter::Error)
    }

    #[test]
    fn official_registry_uses_download_service() {
        assert_eq!(
            registry_download_base("https://mooncakes.io/"),
            "https://download.mooncakes.io/user"
        );
    }

    #[test]
    fn configured_registry_serves_package_downloads() {
        assert_eq!(
            registry_download_base("https://registry.example.com/"),
            "https://registry.example.com/user"
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
            registry.url_base = format!("http://{unavailable_address}");

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

    fn test_registry(sandbox: &tempfile::TempDir) -> (OnlineRegistry, ModuleName, Version) {
        let name: ModuleName = "test/module".into();
        let version = Version::new(1, 2, 3);
        let index = sandbox.path().join("index");
        let index_file = index.join("user/test/module.index");
        std::fs::create_dir_all(index_file.parent().unwrap()).unwrap();
        std::fs::write(&index_file, "registry entry exists").unwrap();
        (
            OnlineRegistry {
                index,
                url_base: String::new(),
                archive_cache: sandbox.path().join("archive-cache"),
                cache: RefCell::new(HashMap::new()),
            },
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
        let archive_path = registry.archive_cache_file_of(&name, &version);
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
                let index = registry.index.clone();
                let archive_cache = registry.archive_cache.clone();
                let url_base = url_base.clone();
                let name = name.clone();
                let version = version.clone();
                let checksum = checksum.clone();
                let destination = sandbox.path().join(format!("source-{client}"));
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let registry = OnlineRegistry {
                        index,
                        url_base,
                        archive_cache,
                        cache: RefCell::new(HashMap::new()),
                    };
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
        let archive_path = registry.archive_cache_file_of(&name, &version);
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
        let index = temp_index_dir();
        let index_file = index.join("user").join("bobzhang").join("openseek.index");
        std::fs::create_dir_all(index_file.parent().unwrap()).unwrap();
        std::fs::write(
            &index_file,
            r#"{"name":"bobzhang/openseek","version":"0.2.1","deps":{"bobzhang/jsonl":"0.2.0"},"preferred_target":"native","checksum":"abc123","rule":{"name":"md_to_mbt_string","command":"moon run --quiet --target native scripts/md_to_mbt_string -- \"$input\" \"$output\""}}
"#,
        )
        .unwrap();

        let registry = OnlineRegistry {
            index: index.clone(),
            url_base: String::new(),
            archive_cache: index.join("archive-cache"),
            cache: RefCell::new(HashMap::new()),
        };
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

        let _ = std::fs::remove_dir_all(index);
    }
}
