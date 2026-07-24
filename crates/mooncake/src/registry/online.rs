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
    io::BufRead,
    path::Path,
    sync::Arc,
};

use anyhow::bail;
use indexmap::map::IndexMap;
use moonutil::{
    dependency::SourceDependencyInfo, registry::RegistryConfig, resolution::ModuleName,
    scripts::execute_postadd_script,
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
    cache: RefCell<HashMap<ModuleName, Arc<BTreeMap<Version, RegistryVersionInfo>>>>,
}

impl OnlineRegistry {
    pub fn mooncakes_io() -> Self {
        let registry = RegistryConfig::load().registry;
        OnlineRegistry {
            index: moonutil::registry::index(),
            url_base: registry_download_base(&registry),
            cache: RefCell::new(HashMap::new()),
        }
    }

    fn index_file_of(&self, name: &ModuleName) -> std::path::PathBuf {
        self.index
            .join("user")
            .join(name.username.as_str())
            .join(format!("{}.index", name.unqual))
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
                        checksum: entry.checksum,
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

    fn install_to(
        &self,
        name: &ModuleName,
        version: &Version,
        to: &Path,
        quiet: bool,
    ) -> anyhow::Result<()> {
        self.install_to_impl(name, version, to, quiet)
    }

    fn extract_to_verified(
        &self,
        name: &ModuleName,
        version: &Version,
        checksum: &str,
        to: &Path,
        quiet: bool,
    ) -> anyhow::Result<()> {
        self.prepare_install_dir(to)?;
        let data = self.download_or_using_cache_verified(name, version, checksum, quiet)?;
        extract_zip_to_dir(to, data)?;
        Ok(())
    }
}

fn calc_sha2(p: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    use std::fs::File;
    use std::io::prelude::*;

    let mut file = File::open(p)?;

    let mut hasher = Sha256::new();
    let mut buffer = [0; 1024];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    // read hash digest and consume hasher
    let result = hasher.finalize();
    Ok(format!("{result:x}"))
}

fn verify_archive_checksum(
    name: &ModuleName,
    version: &Version,
    expected: &str,
    data: &[u8],
) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};

    let actual = format!("{:x}", Sha256::digest(data));
    if actual != expected {
        bail!("checksum mismatch for {name}@{version}: expected {expected}, downloaded {actual}");
    }
    Ok(())
}

impl OnlineRegistry {
    fn download_or_using_cache(
        &self,
        name: &ModuleName,
        version: &Version,
        quiet: bool,
    ) -> anyhow::Result<bytes::Bytes> {
        let cache_file = cache_of(name, version);
        if cache_file.exists() {
            let checksum = super::Registry::checksum_of(self, name, version)?;
            if let Some(data) = self.cached_archive_matching(name, version, &checksum, quiet)? {
                return Ok(data);
            }
        }
        let data = self.download_archive(name, version, quiet)?;
        std::fs::create_dir_all(cache_file.parent().unwrap())?;
        std::fs::write(cache_file, &data)?;
        Ok(data)
    }

    fn download_or_using_cache_verified(
        &self,
        name: &ModuleName,
        version: &Version,
        checksum: &str,
        quiet: bool,
    ) -> anyhow::Result<bytes::Bytes> {
        if let Some(data) = self.cached_archive_matching(name, version, checksum, quiet)? {
            return Ok(data);
        }
        let data = self.download_archive(name, version, quiet)?;
        verify_archive_checksum(name, version, checksum, &data)?;
        let cache_file = cache_of(name, version);
        std::fs::create_dir_all(cache_file.parent().unwrap())?;
        std::fs::write(cache_file, &data)?;
        Ok(data)
    }

    fn cached_archive_matching(
        &self,
        name: &ModuleName,
        version: &Version,
        checksum: &str,
        quiet: bool,
    ) -> anyhow::Result<Option<bytes::Bytes>> {
        let cache_file = cache_of(name, version);
        if !cache_file.exists() || !calc_sha2(&cache_file).is_ok_and(|actual| actual == checksum) {
            return Ok(None);
        }
        if !quiet {
            eprintln!("Using cached {name}@{version}");
        }
        Ok(Some(bytes::Bytes::from(std::fs::read(cache_file)?)))
    }

    fn download_archive(
        &self,
        name: &ModuleName,
        version: &Version,
        quiet: bool,
    ) -> anyhow::Result<bytes::Bytes> {
        if !self.index_file_of(name).exists() {
            anyhow::bail!("Module {}@{} not found", name, version);
        }
        if !quiet {
            eprintln!("Downloading {name}@{version}");
        }
        let filepath = form_urlencoded::Serializer::new(String::new())
            .append_key_only(&format!("{}/{}/{}", name.username, name.unqual, version))
            .finish();
        let url = format!("{}/{}.zip", self.url_base, filepath);
        let client = reqwest::blocking::Client::new();
        let data = client
            .get(url)
            .header(
                USER_AGENT,
                format!("mooncake/{}", env!("CARGO_PKG_VERSION")),
            )
            .send()?
            .error_for_status()?
            .bytes()?;
        Ok(data)
    }

    pub fn install_to_impl(
        &self,
        name: &ModuleName,
        version: &Version,
        pkg_install_dir: &Path,
        quiet: bool,
    ) -> anyhow::Result<()> {
        self.extract_to(name, version, pkg_install_dir, quiet)?;
        execute_postadd_script(pkg_install_dir)?;
        Ok(())
    }

    /// Download and extract a registry package without running `scripts.postadd`.
    pub fn extract_to(
        &self,
        name: &ModuleName,
        version: &Version,
        pkg_install_dir: &Path,
        quiet: bool,
    ) -> anyhow::Result<()> {
        self.prepare_install_dir(pkg_install_dir)?;
        let data = self.download_or_using_cache(name, version, quiet)?;
        extract_zip_to_dir(pkg_install_dir, data)?;
        Ok(())
    }

    fn prepare_install_dir(&self, pkg_install_dir: &Path) -> anyhow::Result<()> {
        if pkg_install_dir.exists() {
            std::fs::remove_dir_all(pkg_install_dir)?;
        }
        std::fs::create_dir_all(pkg_install_dir)?;
        Ok(())
    }
}

fn cache_of(name: &ModuleName, version: &Version) -> std::path::PathBuf {
    let cache_dir = moonutil::registry::cache();

    cache_dir
        .join(name.username.as_str())
        .join(name.unqual.as_str())
        .join(format!("{version}.zip"))
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
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::registry::Registry;

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
            versions
                .get(&version)
                .and_then(|info| info.checksum.as_deref()),
            Some("abc123")
        );

        let _ = std::fs::remove_dir_all(index);
    }

    #[test]
    fn downloaded_archive_must_match_registry_checksum() {
        let name = ModuleName {
            username: "example".into(),
            unqual: "package".into(),
        };
        let version = Version::parse("1.2.3").unwrap();
        let error = verify_archive_checksum(&name, &version, &"0".repeat(64), b"archive")
            .expect_err("mismatched archive should be rejected");

        assert!(error.to_string().starts_with(
            "checksum mismatch for example/package@1.2.3: expected 0000000000000000000000000000000000000000000000000000000000000000, downloaded "
        ));
    }
}
