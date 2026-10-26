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
    MOON_HOME, MoonHomeLayout,
    dependency::SourceDependencyInfo,
    locks::{lock_directory, lock_file},
    registry::{RegistryConfig, RegistryCredentials, registry_api_url},
    resolution::{ModuleName, ModuleSource},
    user_log::UserLog,
};
use reqwest::{
    StatusCode, Url,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
    redirect::Policy,
};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::{
    registry::{RegistryVersionInfo, download},
    update::{RegistryIndexRecloneReason, RegistryIndexUpdate, UpdateOutcome},
    zip_util::extract_zip_to_dir,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
struct RegistryIndexEntry {
    #[serde(default)]
    version: Option<String>,
    deps: Option<IndexMap<String, SourceDependencyInfo>>,
    checksum: Option<String>,
    #[serde(default)]
    yanked: bool,
    #[serde(rename = "yanked_reason")]
    yanked_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
enum RegistryVisibility {
    #[serde(rename = "PUBLIC")]
    Public,
    #[serde(rename = "PRIVATE")]
    Private,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct RegistryMetadata {
    module: String,
    visibility: RegistryVisibility,
    #[serde(flatten)]
    release: RegistryIndexEntry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MetadataSelector {
    Latest,
    Exact(Version),
}

impl MetadataSelector {
    fn query(&self) -> String {
        let mut query = form_urlencoded::Serializer::new(String::new());
        match self {
            Self::Latest => {}
            Self::Exact(version) => {
                query.append_pair("version", &version.to_string());
            }
        }
        query.finish()
    }

    fn matches(&self, version: &Version) -> bool {
        match self {
            Self::Latest => true,
            Self::Exact(expected) => version == expected,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Module `{0}` not found or access denied; check your registry login")]
pub(crate) struct RegistryMetadataDenied(ModuleName);

struct RegistryEndpoints {
    api: String,
    download: String,
    legacy_asset_urls: bool,
}

fn registry_authorization(
    saved: &RegistryCredentials,
    api: &Url,
) -> anyhow::Result<Option<HeaderValue>> {
    if !saved.matches_registry(api.as_str())? || saved.credentials.token.trim().is_empty() {
        return Ok(None);
    }
    let loopback = api.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    });
    if api.scheme() != "https" && !loopback {
        bail!("registry API credentials require HTTPS except on loopback addresses");
    }
    let mut authorization = HeaderValue::from_str(&format!("Bearer {}", saved.credentials.token))
        .context("invalid registry API token header")?;
    authorization.set_sensitive(true);
    Ok(Some(authorization))
}

impl RegistryEndpoints {
    fn from_config(config: &RegistryConfig) -> Self {
        Self {
            api: config.api.trim_end_matches('/').to_owned(),
            download: config.download.trim_end_matches('/').to_owned(),
            legacy_asset_urls: config.legacy_asset_urls,
        }
    }

    fn search(&self, keyword: &str, limit: u32) -> String {
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("kw", keyword)
            .append_pair("limit", &limit.to_string())
            .finish();
        format!("{}/api/v0/search?{query}", self.api)
    }

    fn package_archive(&self, name: &ModuleName, version: &Version) -> String {
        if self.legacy_asset_urls {
            let path = form_urlencoded::Serializer::new(String::new())
                .append_key_only(&format!("{}/{}/{}", name.username, name.unqual, version))
                .finish();
            format!("{}/user/{path}.zip", self.download)
        } else {
            format!(
                "{}/user/{}/{}/{}.zip",
                self.download,
                encode_path_segment(&name.username),
                encode_path_segment(&name.unqual),
                encode_path_segment(&version.to_string())
            )
        }
    }

    fn wasm_asset(&self, name: &ModuleName, version: &Version, package_path: &str) -> String {
        let artifact_name = wasm_artifact_name(name, package_path);
        if self.legacy_asset_urls {
            if package_path.is_empty() {
                format!("{}/assets/{}@{version}/{artifact_name}", self.api, name)
            } else {
                format!(
                    "{}/assets/{}@{version}/{package_path}/{artifact_name}",
                    self.api, name
                )
            }
        } else if package_path.is_empty() {
            format!(
                "{}/prebuild/{}@{version}/{artifact_name}",
                self.download, name
            )
        } else {
            format!(
                "{}/prebuild/{}@{version}/{package_path}/{artifact_name}",
                self.download, name
            )
        }
    }

    fn wasm_checksum(&self, name: &ModuleName, version: &Version, package_path: &str) -> String {
        format!("{}.sha256", self.wasm_asset(name, version, package_path))
    }
}

/// Percent-encode one RFC 3986 path segment without treating `/` as data.
fn encode_path_segment(segment: &str) -> String {
    segment.bytes().fold(String::new(), |mut encoded, byte| {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
        encoded
    })
}

/// One module returned by a Mooncakes registry search.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RegistrySearchResult {
    pub name: String,
    pub version: Version,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_package_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub matched_packages: Vec<RegistryPackageMatch>,
}

/// A matching package summary, which may come from an older module version.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RegistryPackageMatch {
    pub package: String,
    pub name: String,
    pub summary: String,
    pub summary_version: String,
    pub is_summary_current: bool,
    #[serde(default)]
    pub summary_fragments: Vec<RegistrySummaryFragment>,
}

/// Plain text from the registry's search excerpt, never terminal or HTML markup.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RegistrySummaryFragment {
    pub text: String,
    pub matched: bool,
}

/// A published release and its current registry deprecation state.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegistryRelease {
    pub version: Version,
    #[serde(default)]
    pub yanked: bool,
    #[serde(default)]
    pub yanked_reason: Option<String>,
}

/// Live metadata for a selected module release, with its complete version list.
#[derive(Debug, Deserialize, Serialize)]
pub struct RegistryModuleManifest {
    pub module: String,
    #[serde(flatten)]
    pub release: RegistryRelease,
    pub latest_version: Version,
    // Uploaded metadata is extensible. Preserve it for machine consumers, but
    // use the outer registry fields for version and deprecation state.
    pub metadata: serde_json::Map<String, serde_json::Value>,
    pub versions: Vec<RegistryRelease>,
    pub downloads: Option<u64>,
}

/// A module listed on a registry user's public profile.
#[derive(Debug, Deserialize, Serialize)]
pub struct RegistryPublishedModule {
    pub name: String,
    #[serde(flatten)]
    pub release: RegistryRelease,
}

/// All published modules returned by a registry user's public profile.
#[derive(Debug, Deserialize, Serialize)]
pub struct RegistryUserModules {
    pub username: String,
    pub modules: Vec<RegistryPublishedModule>,
}

/// Access to a configured Mooncakes registry and its local state.
///
/// This client owns remote queries, synchronization of the Git index and symbols
/// archive, and verified package and prebuilt wasm downloads. Resolution code
/// can depend on the narrower [`super::Registry`] interface when it only needs
/// local registry metadata.
pub struct RegistryClient {
    config: RegistryConfig,
    home: MoonHomeLayout,
    endpoints: RegistryEndpoints,
    cache: RefCell<HashMap<ModuleName, Arc<BTreeMap<Version, RegistryVersionInfo>>>>,
    metadata: RefCell<HashMap<std::path::PathBuf, RegistryMetadata>>,
    selections: RefCell<HashMap<std::path::PathBuf, Version>>,
    offline: bool,
    api_metadata: bool,
    metadata_http: RefCell<Option<reqwest::blocking::Client>>,
}

impl RegistryClient {
    fn uses_index(&self, _name: &ModuleName) -> bool {
        !self.api_metadata
    }

    fn metadata_path(&self, name: &ModuleName) -> anyhow::Result<std::path::PathBuf> {
        Ok(self
            .home
            .registry_dir()
            .join("metadata")
            .join(sha256_hex(
                registry_api_url(&self.endpoints.api)?.as_str().as_bytes(),
            ))
            .join(format!("{}.json", sha256_hex(name.to_string().as_bytes()))))
    }

    fn metadata_query_path(
        &self,
        name: &ModuleName,
        _: &MetadataSelector,
    ) -> anyhow::Result<std::path::PathBuf> {
        Ok(self
            .metadata_path(name)?
            .with_extension("metadata-v4")
            .join("latest.json"))
    }

    fn metadata_release_path(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<std::path::PathBuf> {
        Ok(self
            .metadata_path(name)?
            .with_extension("metadata-v4")
            .join("releases")
            .join(format!(
                "{}.json",
                sha256_hex(version.to_string().as_bytes())
            )))
    }

    fn cached_release(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<Option<RegistryMetadata>> {
        let path = self.metadata_release_path(name, version)?;
        let memory = self.metadata.borrow().get(&path).cloned();
        let cached = match memory {
            Some(cached) => cached,
            None => match std::fs::read(&path) {
                Ok(bytes) => serde_json::from_slice::<RegistryMetadata>(&bytes)
                    .context("failed to parse cached registry release")?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error).context("failed to read cached registry release"),
            },
        };
        Self::validate_metadata(name, &MetadataSelector::Exact(version.clone()), &cached)?;
        self.metadata.borrow_mut().insert(path, cached.clone());
        Ok(Some(cached))
    }

    fn cached_selection(
        &self,
        name: &ModuleName,
        selector: &MetadataSelector,
    ) -> anyhow::Result<Option<Version>> {
        let path = self.metadata_query_path(name, selector)?;
        let memory = self.selections.borrow().get(&path).cloned();
        let cached = match memory {
            Some(cached) => cached,
            None => match std::fs::read(&path) {
                Ok(bytes) => serde_json::from_slice::<Version>(&bytes)
                    .context("failed to parse cached registry selection")?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => {
                    return Err(error).context("failed to read cached registry selection");
                }
            },
        };
        if !selector.matches(&cached) {
            bail!("cached registry selection is outside the requested selector for {name}");
        }
        self.selections.borrow_mut().insert(path, cached.clone());
        Ok(Some(cached))
    }

    fn write_metadata_cache<T: Serialize>(&self, path: &Path, value: &T) -> anyhow::Result<()> {
        let root = self.home.registry_dir().join("metadata");
        let parent = path.parent().expect("metadata file has a parent");
        let mut directories = Vec::new();
        let mut current = parent;
        while current.starts_with(&root) {
            directories.push(current);
            if current == root {
                break;
            }
            current = current.parent().expect("private metadata path is rooted");
        }
        for directory in directories.into_iter().rev() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                std::fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(directory)?;
            }
            #[cfg(not(unix))]
            std::fs::create_dir_all(directory)?;
        }
        let (mut staged, path) = stage_registry_cache_file(path)?;
        serde_json::to_writer(staged.as_file_mut(), value)?;
        staged.as_file_mut().sync_all()?;
        staged.persist(path).map_err(|error| error.error)?;
        Ok(())
    }

    fn validate_metadata(
        name: &ModuleName,
        selector: &MetadataSelector,
        metadata: &RegistryMetadata,
    ) -> anyhow::Result<()> {
        if metadata.module != name.to_string() || metadata.visibility != RegistryVisibility::Public
        {
            bail!("invalid registry metadata identity for {name}");
        }
        let entry = &metadata.release;
        let version = Version::parse(
            entry
                .version
                .as_deref()
                .context("registry metadata has no version")?,
        )?;
        ModuleSource::from_version(name.clone(), version.clone())?;
        if !selector.matches(&version) {
            bail!("registry returned a version outside the requested selector for {name}");
        }
        let checksum = entry
            .checksum
            .as_deref()
            .context("registry metadata has no checksum")?;
        if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("registry metadata has an invalid SHA-256 checksum for {name}");
        }
        for (dependency, requirement) in entry.deps.iter().flatten() {
            if super::path::parse_module_path(dependency)?
                .version
                .is_some()
            {
                bail!("registry dependency names must not include a version for {name}");
            }
            if requirement.path().is_some() || requirement.version().is_none() {
                bail!("registry metadata contains a non-registry dependency for {name}");
            }
        }
        Ok(())
    }

    fn resolver_metadata(
        &self,
        name: &ModuleName,
        selector: &MetadataSelector,
    ) -> anyhow::Result<RegistryMetadata> {
        super::path::parse_module_path(&name.to_string())?;
        if let MetadataSelector::Exact(version) = selector {
            if let Some(metadata) = self.cached_release(name, version)? {
                return Ok(metadata);
            }
        } else {
            let version = if self.offline {
                self.cached_selection(name, selector)?
            } else {
                self.selections
                    .borrow()
                    .get(&self.metadata_query_path(name, selector)?)
                    .cloned()
            };
            if let Some(version) = version
                && let Some(metadata) = self.cached_release(name, &version)?
            {
                return Ok(metadata);
            }
        }
        if self.offline {
            bail!("missing cached registry metadata for {name}; frozen mode cannot fetch it");
        }
        let module_path = name
            .segments()
            .map(encode_path_segment)
            .collect::<Vec<_>>()
            .join("/");
        let mut url = format!(
            "{}/api/v0/registry/modules/{module_path}",
            self.endpoints.api
        );
        let query = selector.query();
        if !query.is_empty() {
            url.push('?');
            url.push_str(&query);
        }
        let response = self
            .anonymous_metadata_client()?
            .get(&url)
            .send()
            .with_context(|| format!("failed to fetch registry metadata for {name}"))?;
        if matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ) {
            return Err(RegistryMetadataDenied(name.clone()).into());
        }
        let metadata: RegistryMetadata = response
            .error_for_status()
            .with_context(|| format!("registry metadata request failed for {name}"))?
            .json()
            .with_context(|| format!("invalid registry metadata response for {name}"))?;
        Self::validate_metadata(name, selector, &metadata)?;
        let version = Version::parse(
            metadata
                .release
                .version
                .as_deref()
                .context("missing registry version")?,
        )?;
        let path = self.metadata_release_path(name, &version)?;
        self.write_metadata_cache(&path, &metadata)?;
        self.metadata.borrow_mut().insert(path, metadata.clone());
        if matches!(selector, MetadataSelector::Latest) {
            let path = self.metadata_query_path(name, selector)?;
            self.write_metadata_cache(&path, &version)?;
            self.selections.borrow_mut().insert(path, version);
        }
        Ok(metadata)
    }

    fn release_info(metadata: &RegistryMetadata) -> RegistryVersionInfo {
        RegistryVersionInfo {
            deps: metadata.release.deps.clone().unwrap_or_default(),
            yanked: metadata.release.yanked,
            yanked_reason: metadata.release.yanked_reason.clone(),
        }
    }

    pub(super) fn cached_latest_version(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Option<Version>> {
        use super::Registry;

        if self.uses_index(name) {
            let versions = match self.all_versions_of(name) {
                Ok(versions) => versions,
                Err(error) if error.is::<super::RegistryModuleNotFound>() => return Ok(None),
                Err(error) => return Err(error),
            };
            return Ok(versions
                .last_key_value()
                .map(|(version, _)| version.clone()));
        }
        if let Some(version) = self.cached_selection(name, &MetadataSelector::Latest)?
            && self.cached_release(name, &version)?.is_some()
        {
            return Ok(Some(version));
        }
        Ok(None)
    }

    fn anonymous_metadata_client(&self) -> anyhow::Result<reqwest::blocking::Client> {
        if let Some(client) = self.metadata_http.borrow().as_ref() {
            return Ok(client.clone());
        }
        let client = reqwest::blocking::Client::builder()
            .user_agent(format!("mooncake/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .context("failed to create metadata HTTP client")?;
        *self.metadata_http.borrow_mut() = Some(client.clone());
        Ok(client)
    }

    fn api_http_client(&self) -> anyhow::Result<reqwest::blocking::Client> {
        let mut builder = reqwest::blocking::Client::builder()
            .user_agent(format!("mooncake/{}", env!("CARGO_PKG_VERSION")));
        if let Some(saved) = RegistryCredentials::load(&self.home.credentials_path())? {
            let api = registry_api_url(&self.endpoints.api)?;
            if let Some(authorization) = registry_authorization(&saved, &api)? {
                let mut headers = HeaderMap::new();
                headers.insert(AUTHORIZATION, authorization);
                let api_path = format!("{}/api/", api.path().trim_end_matches('/'));
                builder =
                    builder
                        .default_headers(headers)
                        .redirect(Policy::custom(move |attempt| {
                            // Never forward credentials outside this registry's API,
                            // including same-host CDN paths and HTTPS downgrades.
                            if attempt.url().origin() != api.origin()
                                || !attempt.url().path().starts_with(&api_path)
                            {
                                attempt.stop()
                            } else if attempt.previous().len() >= 10 {
                                attempt.error("too many registry API redirects")
                            } else {
                                attempt.follow()
                            }
                        }));
            }
        }
        builder
            .build()
            .context("failed to create registry HTTP client")
    }

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
            cache: RefCell::new(HashMap::new()),
            metadata: RefCell::new(HashMap::new()),
            selections: RefCell::new(HashMap::new()),
            offline: false,
            api_metadata: std::env::var("MOON_REGISTRY_API").is_ok_and(|value| value == "1"),
            metadata_http: RefCell::new(None),
        }
    }

    /// Return whether a local registry index is available for offline fallback.
    pub fn has_cached_index(&self) -> bool {
        self.home.registry_index_dir().exists()
    }

    /// Use only existing metadata while resolving frozen dependencies.
    pub fn offline(mut self) -> Self {
        self.offline = true;
        self
    }

    /// Synchronize the Git index and symbols archive with the configured registry.
    pub fn sync(&self, user_log: &UserLog) -> anyhow::Result<()> {
        if self.api_metadata {
            self.selections.borrow_mut().clear();
            return Ok(());
        }
        let outcome = crate::update::sync(&self.home, &self.config, user_log)?;
        self.cache.borrow_mut().clear();
        self.metadata.borrow_mut().clear();
        self.selections.borrow_mut().clear();
        log_sync_outcome(outcome, user_log);
        Ok(())
    }

    /// Search the configured registry for published modules.
    #[tracing::instrument(skip(self), fields(api = %self.config.api))]
    pub fn search(&self, keyword: &str, limit: u32) -> anyhow::Result<Vec<RegistrySearchResult>> {
        let url = self.endpoints.search(keyword, limit);
        let response = self
            .api_http_client()?
            .get(&url)
            .send()
            .with_context(|| format!("failed to search registry at {url}"))?
            .error_for_status()
            .with_context(|| format!("registry search request failed at {url}"))?;

        response
            .json()
            .with_context(|| format!("failed to parse registry search response from {url}"))
    }

    /// Inspect a module release using live registry metadata.
    #[tracing::instrument(skip(self), fields(api = %self.config.api))]
    pub fn module_manifest(
        &self,
        name: &ModuleName,
        version: Option<&Version>,
    ) -> anyhow::Result<RegistryModuleManifest> {
        if name
            .segments()
            .any(|part| part.is_empty() || part == "." || part == "..")
        {
            bail!("module name must be in the form `username/module`");
        }
        let path = name
            .segments()
            .map(encode_path_segment)
            .collect::<Vec<_>>()
            .join("/");
        let suffix = version
            .map(|version| format!("@{version}"))
            .unwrap_or_default();
        let url = format!("{}/api/v0/manifest/{path}{suffix}", self.endpoints.api);
        let mut manifest: RegistryModuleManifest = self
            .api_http_client()?
            .get(&url)
            .send()
            .with_context(|| format!("failed to fetch module metadata from {url}"))?
            .error_for_status()
            .with_context(|| format!("registry module request failed at {url}"))?
            .json()
            .with_context(|| format!("failed to parse registry module response from {url}"))?;
        if manifest.module != name.to_string()
            || version.is_some_and(|version| manifest.release.version != *version)
        {
            bail!("registry returned metadata for a different module or version");
        }
        manifest.versions.sort_by(|a, b| b.version.cmp(&a.version));
        Ok(manifest)
    }

    /// List a user's published modules, including deprecated modules.
    #[tracing::instrument(skip(self), fields(api = %self.config.api))]
    pub fn user_modules(&self, username: &str) -> anyhow::Result<RegistryUserModules> {
        moonutil::registry::validate_username(username).map_err(anyhow::Error::msg)?;
        let url = format!(
            "{}/api/v0/user/{}",
            self.endpoints.api,
            encode_path_segment(username)
        );
        // The public profile endpoint returns the complete collection without
        // search limits or pagination. Credentials do not change its visibility.
        let mut profile: RegistryUserModules = self
            .api_http_client()?
            .get(&url)
            .send()
            .with_context(|| format!("failed to fetch published modules from {url}"))?
            .error_for_status()
            .with_context(|| format!("registry user request failed at {url}"))?
            .json()
            .with_context(|| format!("failed to parse registry user response from {url}"))?;
        if profile.username != username
            || profile.modules.iter().any(|module| {
                module
                    .name
                    .split_once('/')
                    .is_none_or(|(owner, _)| owner != username)
            })
        {
            bail!("registry returned published modules for a different user");
        }
        profile.modules.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(profile)
    }

    /// Return a verified, locally cached prebuilt wasm asset.
    pub fn acquire_wasm_asset(
        &self,
        module: &ModuleSource,
        package_path: &str,
        user_log: &UserLog,
    ) -> anyhow::Result<std::path::PathBuf> {
        let mut http = None;
        self.acquire_wasm_asset_with(module, package_path, user_log, |url| {
            // Already materialized artifacts remain usable offline. Only a
            // cache miss needs to choose an authenticated or public transport.
            if http.is_none() {
                http = Some(download::client_with_retry()?);
            }
            download_registry_asset(
                http.as_ref().expect("download client initialized"),
                url,
                user_log,
            )
        })
    }

    fn acquire_wasm_asset_with(
        &self,
        module: &ModuleSource,
        package_path: &str,
        user_log: &UserLog,
        mut download: impl FnMut(&str) -> anyhow::Result<Vec<u8>>,
    ) -> anyhow::Result<std::path::PathBuf> {
        let name = module.name();
        let version = module.version();
        validate_registry_module_name(name)?;
        validate_asset_package_path(name, package_path)?;
        let url = self.endpoints.wasm_asset(name, version, package_path);
        let checksum_url = self.endpoints.wasm_checksum(name, version, package_path);
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
                RegistryIndexRecloneReason::UpdateFailed => "Failed to update registry index",
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
    // These boundaries are already known: a package named v2 may belong to
    // an unsuffixed module when the coordinate supplies an explicit version.
    let parsed = super::path::parse_install_style_path(&name.to_string())
        .context("invalid registry asset package path")?;
    if parsed.module != *name || !parsed.package.is_empty() {
        bail!("invalid registry asset package path");
    }
    if !package_path.is_empty() {
        super::path::parse_path_components(package_path)
            .context("invalid registry asset package path")?;
    }
    Ok(())
}

fn validate_registry_module_name(name: &ModuleName) -> anyhow::Result<()> {
    if name
        .segments()
        .any(|component| component.contains(['#', '?', '%']))
    {
        bail!("registry module name cannot contain `#`, `?`, or `%`");
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
    let data = response
        .error_for_status()
        .with_context(|| format!("registry asset download returned error status for {url}"))?
        .bytes()
        .with_context(|| format!("failed to read registry asset response from {url}"))?;
    Ok(data.to_vec())
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
        let file = std::fs::File::open(index_file).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                anyhow::Error::new(super::RegistryModuleNotFound(name.clone()))
            } else {
                error.into()
            }
        })?;
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
                        yanked: entry.yanked,
                        yanked_reason: entry.yanked_reason,
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

    fn version_info(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<Option<RegistryVersionInfo>> {
        if self.uses_index(name) {
            return Ok(self.all_versions_of(name)?.get(version).cloned());
        }
        let metadata = self.resolver_metadata(name, &MetadataSelector::Exact(version.clone()))?;
        Ok(Some(Self::release_info(&metadata)))
    }

    fn latest_version(&self, name: &ModuleName) -> anyhow::Result<Option<Version>> {
        if self.uses_index(name) {
            return Ok(self
                .all_versions_of(name)?
                .last_key_value()
                .map(|(version, _)| version.clone()));
        }
        let metadata = self.resolver_metadata(name, &MetadataSelector::Latest)?;
        Ok(Some(Version::parse(
            metadata
                .release
                .version
                .as_deref()
                .context("missing version")?,
        )?))
    }
}

impl super::RegistrySource for RegistryClient {
    fn acquire_source_to(
        &self,
        module: &ModuleSource,
        expected_checksum: &str,
        to: &Path,
        user_log: &UserLog,
    ) -> anyhow::Result<()> {
        RegistryClient::acquire_source_to(self, module, expected_checksum, to, user_log)
    }

    fn source_archive_checksum(&self, module: &ModuleSource) -> anyhow::Result<String> {
        RegistryClient::source_archive_checksum(self, module)
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

fn stage_registry_cache_file(
    path: &Path,
) -> anyhow::Result<(tempfile::NamedTempFile, std::path::PathBuf)> {
    let parent = path.parent().context("registry cache file has no parent")?;
    // Keep Windows' extended-length prefix for tempfile's direct Win32 calls.
    // Canonicalize the existing parent because the destination may not exist yet.
    let parent =
        std::fs::canonicalize(parent).context("failed to canonicalize registry cache directory")?;
    let path = parent.join(
        path.file_name()
            .context("registry cache file has no name")?,
    );
    let staged =
        tempfile::NamedTempFile::new_in(&parent).context("failed to stage registry cache file")?;
    Ok((staged, path))
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
    let (mut archive, cache_file) = stage_registry_cache_file(cache_file)?;
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
    ) -> anyhow::Result<Option<String>> {
        let p = self.home.registry_index_file(name);
        let file = match std::fs::File::open(&p) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let reader = std::io::BufReader::new(file);

        let lines = reader.lines().collect::<std::io::Result<Vec<String>>>()?;
        let version_str = version.to_string();
        for line in lines.iter().rev() {
            let entry = serde_json_lenient::from_str::<RegistryIndexEntry>(line)?;
            if entry.version.as_deref() == Some(version_str.as_str()) {
                if let Some(checksum) = entry.checksum {
                    return Ok(Some(checksum));
                } else {
                    bail!(
                        "No checksum found for version {} in index file {:?}",
                        version,
                        p
                    );
                }
            }
        }
        Ok(None)
    }

    fn download_or_use_cache(
        &self,
        name: &ModuleName,
        version: &Version,
        expected_checksum: &str,
        user_log: &UserLog,
    ) -> anyhow::Result<File> {
        validate_registry_module_name(name)?;
        if self.uses_index(name) {
            if !self.home.registry_index_file(name).exists() {
                bail!("Module {name}@{version} not found");
            }
        } else {
            self.resolver_metadata(name, &MetadataSelector::Exact(version.clone()))?;
        }
        let cache_file = self.home.registry_source_archive_path(name, version);
        if let Some(archive) = open_cached_archive(&cache_file, expected_checksum)? {
            user_log.status(format!("Using cached {name}@{version}"));
            return Ok(archive);
        }
        if self.offline {
            bail!("Source for {name}@{version} is not cached in frozen mode");
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
        let mut response = download::client_with_retry()?
            .get(url)
            .send()?
            .error_for_status()?;

        persist_verified_archive(&mut response, &cache_file, name, version, expected_checksum)
    }

    /// Return the registry index's SHA-256 checksum for a published source archive.
    pub fn source_archive_checksum(&self, module: &ModuleSource) -> anyhow::Result<String> {
        if self.uses_index(module.name()) {
            if let Some(checksum) =
                self.read_checksum_from_index_file(module.name(), module.version())?
            {
                return Ok(checksum);
            }
            bail!(
                "Version {} of {} is not in the public index; run `moon update`",
                module.version(),
                module.name()
            );
        }
        let metadata = self.resolver_metadata(
            module.name(),
            &MetadataSelector::Exact(module.version().clone()),
        )?;
        metadata
            .release
            .checksum
            .map(|checksum| checksum.to_ascii_lowercase())
            .context("registry metadata does not contain the requested version or checksum")
    }

    /// Materialize verified published source without executing package hooks.
    pub fn materialize_source_to(
        &self,
        module: &ModuleSource,
        to: &Path,
        user_log: &UserLog,
    ) -> anyhow::Result<()> {
        let checksum = self.source_archive_checksum(module)?;
        self.acquire_source_to(module, &checksum, to, user_log)
    }

    /// Reuse or download, verify, and extract a registry package without
    /// running `scripts.postadd`.
    pub fn acquire_source_to(
        &self,
        module: &ModuleSource,
        expected_checksum: &str,
        pkg_install_dir: &Path,
        user_log: &UserLog,
    ) -> anyhow::Result<()> {
        let name = module.name();
        let version = module.version();
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

    pub(super) fn api_test_registry(home: &Path, api: &str) -> RegistryClient {
        RegistryClient::with_home(
            RegistryConfig {
                api: api.to_owned(),
                index: String::new(),
                download: api.to_owned(),
                symbols: None,
                legacy_asset_urls: false,
            },
            MoonHomeLayout::new(home.to_owned()),
        )
    }

    pub(super) fn api_test_server<T: AsRef<[u8]> + Send + 'static>(
        listener: TcpListener,
        responses: Vec<(u16, String, T)>,
    ) -> std::thread::JoinHandle<Vec<String>> {
        listener.set_nonblocking(true).unwrap();
        std::thread::spawn(move || {
            use std::io::BufRead;
            let mut requests = Vec::new();
            for (status, headers, body) in responses {
                let deadline = std::time::Instant::now() + Duration::from_secs(10);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(std::time::Instant::now() < deadline, "missing API request");
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("API test server failed: {error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = String::new();
                let mut reader = std::io::BufReader::new(&stream);
                loop {
                    let mut line = String::new();
                    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                    if line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                requests.push(request);
                let body = body.as_ref();
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n", body.len()).unwrap();
                stream.write_all(body).unwrap();
            }
            requests
        })
    }

    pub(super) fn resolver_response(name: &str, visibility: &str, version: &str) -> String {
        serde_json::json!({
            "module": name, "visibility": visibility,
            "version": version, "deps": {"alice/public": "0.1.0"},
            "checksum": "b".repeat(64), "yanked": false, "yanked_reason": null
        })
        .to_string()
    }

    pub(super) fn save_test_login(home: &Path, api: &str, username: &str, token: &str) {
        RegistryCredentials::save(
            &home.join("credentials.json"),
            moonutil::registry::Credentials {
                username: Some(username.to_owned()),
                token: token.to_owned(),
            },
            api,
        )
        .unwrap();
    }

    #[test]
    fn registry_metadata_cache_supports_long_paths() {
        let sandbox = tempfile::tempdir().unwrap();
        let home = sandbox.path().join("private-cache-home/".repeat(18));
        assert!(home.to_string_lossy().len() > 260);
        std::fs::create_dir_all(&home).unwrap();
        let api = "https://example.com";
        let registry = api_test_registry(&home, api);
        let name: ModuleName = "alice/secret".into();
        let version = Version::new(0, 1, 0);
        let selector = MetadataSelector::Latest;
        let release_path = registry.metadata_release_path(&name, &version).unwrap();
        let query_path = registry.metadata_query_path(&name, &selector).unwrap();
        let metadata: RegistryMetadata =
            serde_json::from_str(&resolver_response("alice/secret", "PUBLIC", "0.1.0")).unwrap();
        // Repeat each write to cover replacing an existing cache entry as well.
        for _ in 0..2 {
            registry
                .write_metadata_cache(&release_path, &metadata)
                .unwrap();
            registry
                .write_metadata_cache(&query_path, &version)
                .unwrap();
            let fresh = api_test_registry(&home, api).offline();
            assert!(fresh.cached_release(&name, &version).unwrap().is_some());
            assert_eq!(
                fresh.cached_selection(&name, &selector).unwrap(),
                Some(version.clone())
            );
        }
        registry.metadata.borrow_mut().clear();
        registry.selections.borrow_mut().clear();

        let archive_path = registry
            .metadata_path(&name)
            .unwrap()
            .with_extension("archives")
            .join("0.1.0.zip");
        let bytes = test_archive();
        let checksum = sha256_hex(&bytes);
        for _ in 0..2 {
            let mut archive = persist_verified_archive(
                &mut Cursor::new(&bytes),
                &archive_path,
                &name,
                &version,
                &checksum,
            )
            .unwrap();
            let mut contents = Vec::new();
            archive.read_to_end(&mut contents).unwrap();
            assert_eq!(contents, bytes);
            assert!(
                open_cached_archive(&archive_path, &checksum)
                    .unwrap()
                    .is_some()
            );
        }
    }

    #[test]
    fn exact_metadata_is_shared_by_resolution_and_checksum_without_selectors() {
        let home = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let server = api_test_server(
            listener,
            vec![(
                200,
                String::new(),
                resolver_response("alice/secret", "PUBLIC", "0.1.0"),
            )],
        );

        let mut registry = api_test_registry(home.path(), &api);
        registry.api_metadata = true;
        let name: ModuleName = "alice/secret".into();
        let version = Version::new(0, 1, 0);
        assert!(registry.version_info(&name, &version).unwrap().is_some());
        let source = ModuleSource::from_version(name.clone(), version.clone()).unwrap();
        assert_eq!(
            registry.source_archive_checksum(&source).unwrap(),
            "b".repeat(64)
        );
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].starts_with("GET /api/v0/registry/modules/alice/secret?version=0.1.0 ")
        );
        let mut cached = api_test_registry(home.path(), &api).offline();
        cached.api_metadata = true;
        assert!(cached.version_info(&name, &version).unwrap().is_some());
        assert!(cached.version_info(&name, &Version::new(0, 1, 1)).is_err());
        assert!(cached.latest_version(&name).is_err());
        assert!(cached.all_versions_of(&name).is_err());
        let path = cached.metadata_release_path(&name, &version).unwrap();
        let release: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(release["version"], "0.1.0");
        assert!(release.get("generation").is_none());
    }

    #[test]
    fn resolver_exact_requests_encode_build_metadata_and_validate_the_selector() {
        let home = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let server = api_test_server(
            listener,
            vec![
                (
                    200,
                    String::new(),
                    resolver_response("alice/secret", "PUBLIC", "0.1.0+01"),
                ),
                (
                    200,
                    String::new(),
                    resolver_response("alice/secret", "PUBLIC", "0.2.0"),
                ),
                (
                    200,
                    String::new(),
                    resolver_response("alice/secret", "PUBLIC", "2.0.0-alpha"),
                ),
            ],
        );
        let name: ModuleName = "alice/secret".into();
        let mut registry = api_test_registry(home.path(), &api);
        registry.api_metadata = true;
        assert!(
            registry
                .version_info(&name, &"0.1.0+01".parse().unwrap())
                .unwrap()
                .is_some()
        );
        assert!(
            registry
                .version_info(&name, &Version::new(0, 1, 0))
                .unwrap_err()
                .to_string()
                .contains("outside the requested selector")
        );
        assert!(
            registry
                .version_info(&name, &Version::new(0, 1, 0))
                .unwrap_err()
                .to_string()
                .contains("outside the requested selector")
        );
        let requests = server.join().unwrap();
        assert!(
            requests[0]
                .starts_with("GET /api/v0/registry/modules/alice/secret?version=0.1.0%2B01 ")
        );
        assert!(
            registry
                .offline()
                .version_info(&name, &Version::new(0, 1, 0))
                .is_err()
        );
    }

    #[test]
    fn public_index_queries_do_not_use_the_private_metadata_api() {
        let home = tempfile::tempdir().unwrap();
        let registry = api_test_registry(home.path(), "http://127.0.0.1:1");
        let name: ModuleName = "alice/public".into();
        let path = registry.home.registry_index_file(&name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "{\"version\":\"0.1.0\",\"checksum\":\"".to_owned() + &"a".repeat(64) + "\"}\n",
        )
        .unwrap();
        assert_eq!(
            registry.latest_version(&name).unwrap(),
            Some(Version::new(0, 1, 0))
        );
        assert!(
            registry
                .version_info(&name, &Version::new(0, 1, 0))
                .unwrap()
                .is_some()
        );
        assert!(
            registry
                .version_info(&name, &Version::new(0, 2, 0))
                .unwrap()
                .is_none()
        );
        assert!(
            registry
                .version_info(&name, &Version::new(0, 2, 0))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            registry
                .source_archive_checksum(
                    &ModuleSource::from_version(name, Version::new(0, 1, 0)).unwrap()
                )
                .unwrap(),
            "a".repeat(64)
        );
        assert!(!home.path().join("registry/private").exists());
    }

    #[test]
    fn api_mode_resolves_public_releases_without_git_or_download_credentials() {
        let home = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        save_test_login(home.path(), &api, "alice", "not-sent-by-public-metadata");
        let mut registry = api_test_registry(home.path(), &api);
        registry.api_metadata = true;
        let name: ModuleName = "alice/public".into();
        let version = Version::new(0, 1, 0);
        let index = registry.home.registry_index_file(&name);
        std::fs::create_dir_all(index.parent().unwrap()).unwrap();
        std::fs::write(&index, "not valid registry metadata").unwrap();
        let archive = test_archive();
        let checksum = sha256_hex(&archive);
        let metadata = serde_json::json!({
            "module": "alice/public", "visibility": "PUBLIC", "version": "0.1.0",
            "deps": {}, "checksum": checksum,
        });
        let server = api_test_server(
            listener,
            vec![
                (200, String::new(), metadata.to_string().into_bytes()),
                (200, String::new(), archive),
            ],
        );
        registry.sync(&quiet_user_log()).unwrap();
        assert!(registry.version_info(&name, &version).unwrap().is_some());
        let module = ModuleSource::from_version(name.clone(), version.clone()).unwrap();
        assert_eq!(registry.source_archive_checksum(&module).unwrap(), checksum);
        registry
            .materialize_source_to(&module, &home.path().join("source"), &quiet_user_log())
            .unwrap();
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[0].starts_with("GET /api/v0/registry/modules/alice/public?version=0.1.0 ")
        );
        assert!(requests[1].starts_with("GET /user/alice/public/0.1.0.zip "));
        assert!(
            requests
                .iter()
                .all(|request| !request.to_lowercase().contains("authorization:"))
        );
        assert!(
            registry
                .home
                .registry_source_archive_path(&name, &version)
                .is_file()
        );
        assert_eq!(
            std::fs::read_to_string(index).unwrap(),
            "not valid registry metadata"
        );
        assert!(!registry.home.registry_dir().join("symbols").exists());
    }

    #[test]
    fn latest_refresh_does_not_invalidate_exact_release_metadata() {
        let home = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let server = api_test_server(
            listener,
            vec![
                (
                    200,
                    String::new(),
                    resolver_response("alice/public", "PUBLIC", "0.1.0"),
                ),
                (
                    200,
                    String::new(),
                    resolver_response("alice/public", "PUBLIC", "0.1.1"),
                ),
            ],
        );
        let name: ModuleName = "alice/public".into();
        let mut registry = api_test_registry(home.path(), &api);
        registry.api_metadata = true;
        assert_eq!(
            registry.latest_version(&name).unwrap(),
            Some(Version::new(0, 1, 0))
        );
        assert_eq!(
            registry.latest_version(&name).unwrap(),
            Some(Version::new(0, 1, 0))
        );
        registry.sync(&quiet_user_log()).unwrap();
        assert_eq!(
            registry.latest_version(&name).unwrap(),
            Some(Version::new(0, 1, 1))
        );
        assert!(
            registry
                .version_info(&name, &Version::new(0, 1, 0))
                .unwrap()
                .is_some()
        );
        assert_eq!(server.join().unwrap().len(), 2);
        let mut cached = api_test_registry(home.path(), &api).offline();
        cached.api_metadata = true;
        assert_eq!(
            cached.latest_version(&name).unwrap(),
            Some(Version::new(0, 1, 1))
        );
        assert!(
            cached
                .version_info(&name, &Version::new(0, 1, 0))
                .unwrap()
                .is_some()
        );
        assert!(cached.version_info(&name, &Version::new(0, 1, 2)).is_err());
    }

    #[test]
    fn resolver_refresh_denial_does_not_reuse_a_previous_success() {
        let home = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let server = api_test_server(
            listener,
            vec![
                (
                    200,
                    String::new(),
                    resolver_response("alice/secret", "PUBLIC", "0.1.0"),
                ),
                (404, String::new(), "{}".to_owned()),
            ],
        );

        let name: ModuleName = "alice/secret".into();
        let mut registry = api_test_registry(home.path(), &api);
        registry.api_metadata = true;
        registry.latest_version(&name).unwrap();
        let mut current = api_test_registry(home.path(), &api);
        current.api_metadata = true;
        let error = current.latest_version(&name).unwrap_err();
        assert!(error.to_string().contains("not found or access denied"));
        assert!(!error.to_string().contains("PUBLIC"));
        assert_eq!(server.join().unwrap().len(), 2);
        let mut offline = api_test_registry(home.path(), &api).offline();
        offline.api_metadata = true;
        assert!(offline.latest_version(&name).is_ok());
    }

    #[test]
    fn resolver_rejects_invalid_authoritative_metadata_without_caching_it() {
        for body in [
            resolver_response("alice/wrong", "PUBLIC", "0.1.0"),
            resolver_response("alice/secret", "UNKNOWN", "0.1.0"),
            resolver_response("alice/secret", "PRIVATE", "0.1.0"),
            resolver_response("alice/secret", "PUBLIC", "invalid"),
            resolver_response("alice/secret", "PUBLIC", "0.1.0").replace(&"b".repeat(64), "bad"),
        ] {
            let home = tempfile::tempdir().unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let api = format!("http://{}", listener.local_addr().unwrap());
            let server = api_test_server(listener, vec![(200, String::new(), body)]);
            let mut registry = api_test_registry(home.path(), &api);
            registry.api_metadata = true;
            let name: ModuleName = "alice/secret".into();
            assert!(registry.latest_version(&name).is_err());
            assert!(
                !registry
                    .metadata_query_path(&name, &MetadataSelector::Latest)
                    .unwrap()
                    .exists()
            );
            server.join().unwrap();
        }
    }

    #[test]
    fn legacy_credentials_only_authorize_the_official_api() {
        let saved: RegistryCredentials =
            serde_json::from_str(r#"{"token":"test-secret","username":"alice"}"#).unwrap();
        let official = registry_authorization(
            &saved,
            &registry_api_url("https://mooncakes.io:443/").unwrap(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(official, "Bearer test-secret");
        assert!(official.is_sensitive());
        assert!(
            registry_authorization(
                &saved,
                &registry_api_url("https://registry.example").unwrap()
            )
            .unwrap()
            .is_none()
        );
        assert!(
            registry_authorization(&saved, &registry_api_url("http://mooncakes.io").unwrap())
                .unwrap()
                .is_none()
        );
        assert!(
            registry_authorization(
                &saved,
                &registry_api_url("https://mooncakes.io/other").unwrap()
            )
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn bound_credentials_require_the_exact_api_base_and_secure_transport() {
        let saved: RegistryCredentials = serde_json::from_value(serde_json::json!({
            "token": "test-secret", "registry_api": "https://registry.example/base/"
        }))
        .unwrap();
        assert!(
            registry_authorization(
                &saved,
                &registry_api_url("https://registry.example/base").unwrap()
            )
            .unwrap()
            .is_some()
        );
        assert!(
            registry_authorization(
                &saved,
                &registry_api_url("https://registry.example/other").unwrap()
            )
            .unwrap()
            .is_none()
        );
        assert!(
            registry_authorization(
                &saved,
                &registry_api_url("https://registry.example:8443/base").unwrap()
            )
            .unwrap()
            .is_none()
        );
        let insecure: RegistryCredentials = serde_json::from_value(serde_json::json!({
            "token": "test-secret", "registry_api": "http://registry.example"
        }))
        .unwrap();
        assert!(
            registry_authorization(
                &insecure,
                &registry_api_url("http://registry.example").unwrap()
            )
            .is_err()
        );
        for api in [
            "http://localhost:8001",
            "http://127.0.0.1:8001",
            "http://[::1]:8001",
        ] {
            let local: RegistryCredentials = serde_json::from_value(serde_json::json!({
                "token": "test-secret", "registry_api": api
            }))
            .unwrap();
            assert!(
                registry_authorization(&local, &registry_api_url(api).unwrap())
                    .unwrap()
                    .is_some()
            );
        }
    }

    #[test]
    fn api_reads_send_the_matching_saved_token_without_classifying_modules() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}/registry", listener.local_addr().unwrap());
        std::fs::write(
            sandbox.path().join("credentials.json"),
            serde_json::json!({
                "token": "test-secret", "username": "alice", "registry_api": api
            })
            .to_string(),
        )
        .unwrap();
        let registry = api_test_registry(sandbox.path(), &api);
        let server = api_test_server(
            listener,
            vec![
                (200, String::new(), "[]".to_owned()),
                (
                    200,
                    String::new(),
                    serde_json::json!({
                        "module":"alice/tools", "version":"0.1.0", "latest_version":"0.1.0",
                        "metadata":{}, "versions":[], "downloads":null
                    })
                    .to_string(),
                ),
                (
                    200,
                    String::new(),
                    r#"{"username":"alice","modules":[]}"#.to_owned(),
                ),
            ],
        );
        assert!(registry.search("tools", 5).unwrap().is_empty());
        registry
            .module_manifest(&"alice/tools".into(), None)
            .unwrap();
        assert!(registry.user_modules("alice").unwrap().modules.is_empty());
        let requests = server.join().unwrap();
        assert!(requests[0].starts_with("GET /registry/api/v0/search?"));
        assert!(requests[1].starts_with("GET /registry/api/v0/manifest/alice/tools "));
        assert!(requests[2].starts_with("GET /registry/api/v0/user/alice "));
        for request in requests {
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer test-secret\r\n")
            );
        }
    }

    #[test]
    fn missing_empty_and_unmatched_credentials_keep_api_reads_anonymous() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let registry = api_test_registry(sandbox.path(), &api);
        let server = api_test_server(listener, vec![(200, String::new(), "[]".to_owned()); 3]);
        registry.search("tools", 5).unwrap();
        std::fs::write(
            sandbox.path().join("credentials.json"),
            serde_json::json!({
                "token": "", "registry_api": api
            })
            .to_string(),
        )
        .unwrap();
        registry.search("tools", 5).unwrap();
        std::fs::write(
            sandbox.path().join("credentials.json"),
            r#"{"token":"official-secret"}"#,
        )
        .unwrap();
        registry.search("tools", 5).unwrap();
        for request in server.join().unwrap() {
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
        }
    }

    #[test]
    fn credentials_are_not_attached_to_archive_or_asset_download_clients() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let registry = api_test_registry(sandbox.path(), &api);
        std::fs::write(
            sandbox.path().join("credentials.json"),
            serde_json::json!({
                "token":"test-secret", "registry_api":api
            })
            .to_string(),
        )
        .unwrap();
        let server = api_test_server(listener, vec![(200, String::new(), "artifact".to_owned())]);
        download_registry_asset(
            &download::client_with_retry().unwrap(),
            &format!(
                "{}/prebuild/alice/tools@0.1.0/main.wasm",
                registry.endpoints.download
            ),
            &quiet_user_log(),
        )
        .unwrap();
        assert!(
            !server.join().unwrap()[0]
                .to_ascii_lowercase()
                .contains("authorization:")
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let (mut registry, name, version) = test_registry(&sandbox);
        registry.endpoints.download = api.clone();
        std::fs::write(
            sandbox.path().join("credentials.json"),
            serde_json::json!({
                "token": "test-secret", "registry_api": api
            })
            .to_string(),
        )
        .unwrap();
        let archive = test_archive();
        let checksum = sha256_hex(&archive);
        let server = api_test_server(listener, vec![(200, String::new(), archive)]);
        registry
            .acquire_source_to(
                &ModuleSource::from_version(name, version).unwrap(),
                &checksum,
                &sandbox.path().join("downloaded-source"),
                &quiet_user_log(),
            )
            .unwrap();
        assert!(
            !server.join().unwrap()[0]
                .to_ascii_lowercase()
                .contains("authorization:")
        );
    }

    #[test]
    fn authenticated_api_redirects_stay_within_the_bound_api() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let registry = api_test_registry(sandbox.path(), &api);
        std::fs::write(
            sandbox.path().join("credentials.json"),
            serde_json::json!({
                "token":"test-secret", "registry_api":api
            })
            .to_string(),
        )
        .unwrap();
        let server = api_test_server(
            listener,
            vec![
                (302, "Location: /api/v0/next\r\n".to_owned(), String::new()),
                (200, String::new(), "ok".to_owned()),
                (
                    302,
                    format!("Location: {api}/prebuild/file.wasm\r\n"),
                    String::new(),
                ),
                (
                    302,
                    "Location: http://127.0.0.1:1/api/v0/other\r\n".to_owned(),
                    String::new(),
                ),
            ],
        );
        let http = registry.api_http_client().unwrap();
        assert_eq!(
            http.get(format!("{api}/api/v0/start"))
                .send()
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            http.get(format!("{api}/api/v0/start"))
                .send()
                .unwrap()
                .status(),
            302
        );
        assert_eq!(
            http.get(format!("{api}/api/v0/start"))
                .send()
                .unwrap()
                .status(),
            302
        );
        for request in server.join().unwrap() {
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer test-secret\r\n")
            );
        }
    }

    #[test]
    fn malformed_credentials_errors_do_not_include_secrets() {
        let sandbox = tempfile::TempDir::new().unwrap();
        std::fs::write(
            sandbox.path().join("credentials.json"),
            r#"{"token":"test-secret","username":"alice","registry_api":"secret-invalid-address"}"#,
        )
        .unwrap();
        let registry = api_test_registry(sandbox.path(), "https://mooncakes.io");
        let error = format!("{:#}", registry.api_http_client().unwrap_err());
        assert!(!error.contains("test-secret"));
        std::fs::write(
            sandbox.path().join("credentials.json"),
            serde_json::json!({
                "token": "test-secret\r\nX-Injected: value", "registry_api": "https://mooncakes.io"
            })
            .to_string(),
        )
        .unwrap();
        let error = format!("{:#}", registry.api_http_client().unwrap_err());
        assert!(!error.contains("test-secret"));
        assert!(!error.contains("X-Injected"));
        assert!(!error.contains("secret-invalid-address"));
        std::fs::write(sandbox.path().join("credentials.json"), r#"{"token":"test-secret","username":"alice","registry_api":"https://mooncakes.io","unexpected":"#).unwrap();
        let error = format!("{:#}", registry.api_http_client().unwrap_err());
        assert!(!error.contains("test-secret"));
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
    fn split_registry_uses_configured_download_service() {
        let endpoints = RegistryEndpoints::from_config(&RegistryConfig {
            api: "https://mooncakes.io/".to_owned(),
            index: String::new(),
            download: "https://download.mooncakes.io/".to_owned(),
            symbols: None,
            legacy_asset_urls: false,
        });
        assert_eq!(
            endpoints.package_archive(&"test/pkg".into(), &Version::new(1, 2, 3)),
            "https://download.mooncakes.io/user/test/pkg/1.2.3.zip"
        );
    }

    #[test]
    fn split_registry_keeps_api_and_download_urls_independent() {
        let endpoints = RegistryEndpoints::from_config(&RegistryConfig {
            api: "https://api.example.com/".to_owned(),
            index: String::new(),
            download: "https://download.example.com/".to_owned(),
            symbols: None,
            legacy_asset_urls: false,
        });
        assert_eq!(
            endpoints.package_archive(&"test/pkg".into(), &Version::new(1, 2, 3)),
            "https://download.example.com/user/test/pkg/1.2.3.zip"
        );
        assert_eq!(
            endpoints.search("json query", 7),
            "https://api.example.com/api/v0/search?kw=json+query&limit=7"
        );
    }

    #[test]
    fn split_registry_rejects_reserved_archive_path_components() {
        let endpoints = RegistryEndpoints::from_config(&RegistryConfig {
            api: "https://api.example.com".to_owned(),
            index: String::new(),
            download: "https://download.example.com".to_owned(),
            symbols: None,
            legacy_asset_urls: false,
        });
        let name: ModuleName = "test#/pkg?".into();
        assert!(validate_registry_module_name(&name).is_err());
        assert_eq!(
            endpoints.package_archive(&"test/pkg".into(), &Version::parse("1.2.3+build").unwrap()),
            "https://download.example.com/user/test/pkg/1.2.3%2Bbuild.zip"
        );
    }

    #[test]
    fn registry_search_url_uses_configured_registry_and_encodes_query() {
        let endpoints = RegistryEndpoints::from_config(&RegistryConfig {
            api: "https://registry.example.com/".to_owned(),
            index: String::new(),
            download: String::new(),
            symbols: None,
            legacy_asset_urls: false,
        });
        assert_eq!(
            endpoints.search("json query", 7),
            "https://registry.example.com/api/v0/search?kw=json+query&limit=7"
        );
    }

    #[test]
    fn legacy_registry_config_keeps_existing_download_urls() {
        let endpoints = RegistryEndpoints::from_config(&RegistryConfig {
            api: "https://registry.example.com".to_owned(),
            index: String::new(),
            download: "https://registry.example.com".to_owned(),
            symbols: None,
            legacy_asset_urls: true,
        });
        let name: ModuleName = "test/pkg".into();
        let version = Version::new(1, 2, 3);

        assert_eq!(
            endpoints.package_archive(&name, &version),
            "https://registry.example.com/user/test%2Fpkg%2F1.2.3.zip"
        );
        assert_eq!(
            endpoints.wasm_asset(&name, &version, "cmd/tool"),
            "https://registry.example.com/assets/test/pkg@1.2.3/cmd/tool/tool.wasm"
        );
        assert_eq!(
            endpoints.wasm_checksum(&name, &version, "cmd/tool"),
            "https://registry.example.com/assets/test/pkg@1.2.3/cmd/tool/tool.wasm.sha256"
        );
    }

    #[test]
    fn registry_search_preserves_package_matches() {
        let response = serde_json::json!({
            "name": "alice/tools",
            "version": "2.0.0",
            "description": "Tools",
            "repository": "https://github.com/alice/tools",
            "downloads": 12345678901_u64,
            "matched_package_count": 7,
            "matched_packages": [{
                "package": "fs",
                "name": "alice/tools/fs",
                "summary": "Read files",
                "summary_version": "1.0.0",
                "is_summary_current": false,
                "summary_fragments": [
                    {"text": "Read ", "matched": false},
                    {"text": "files", "matched": true}
                ]
            }]
        });
        let result: RegistrySearchResult = serde_json::from_value(response.clone()).unwrap();
        assert_eq!(serde_json::to_value(result).unwrap(), response);
    }

    #[test]
    fn registry_search_result_ignores_unneeded_response_fields() {
        let results: Vec<RegistrySearchResult> = serde_json::from_str(
            r#"[{"name":"mizchi/jq","version":"0.2.2","license":"Apache-2.0","description":"A jq clone","is_new":false}]"#,
        )
        .unwrap();

        assert_eq!(
            results,
            [RegistrySearchResult {
                name: "mizchi/jq".to_owned(),
                version: Version::new(0, 2, 2),
                description: Some("A jq clone".to_owned()),
                repository: None,
                downloads: None,
                matched_package_count: None,
                matched_packages: Vec::new(),
            }]
        );
    }

    fn asset_test_registry(sandbox: &tempfile::TempDir) -> RegistryClient {
        RegistryClient::with_home(
            RegistryConfig {
                api: "https://mooncakes.io".to_owned(),
                index: String::new(),
                download: "https://download.mooncakes.io".to_owned(),
                symbols: None,
                legacy_asset_urls: false,
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
            "https://download.mooncakes.io/prebuild/moonbitlang/parser@0.3.3/cmd/moonfmt/moonfmt.wasm"
        );
        assert_eq!(
            registry
                .endpoints
                .wasm_asset(&parser_module(), &version, ""),
            "https://download.mooncakes.io/prebuild/moonbitlang/parser@0.3.3/parser.wasm"
        );
    }

    #[test]
    fn wasm_assets_preserve_major_version_module_boundaries() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let registry = asset_test_registry(&sandbox);
        let wasm = b"\0asmtest";
        let checksum = sha256_hex(wasm);

        for (name, package, url, cache_suffix) in [
            (
                "a/b/v2",
                "",
                "https://download.mooncakes.io/prebuild/a/b/v2@2.0.0/v2.wasm",
                "a/b/v2/2.0.0/v2.wasm",
            ),
            (
                "a/b/v2",
                "tool",
                "https://download.mooncakes.io/prebuild/a/b/v2@2.0.0/tool/tool.wasm",
                "a/b/v2/2.0.0/tool/tool.wasm",
            ),
            (
                "a/b",
                "v2/tool",
                "https://download.mooncakes.io/prebuild/a/b@2.0.0/v2/tool/tool.wasm",
                "a/b/2.0.0/v2/tool/tool.wasm",
            ),
        ] {
            let mut urls = Vec::new();
            let path = registry
                .acquire_wasm_asset_with(
                    &ModuleSource::from_version(name.into(), Version::new(2, 0, 0)).unwrap(),
                    package,
                    &quiet_user_log(),
                    |url| {
                        urls.push(url.to_owned());
                        if url.ends_with(".sha256") {
                            Ok(checksum.as_bytes().to_vec())
                        } else {
                            Ok(wasm.to_vec())
                        }
                    },
                )
                .unwrap();
            assert_eq!(urls, [format!("{url}.sha256"), url.to_owned()]);
            assert_eq!(path, registry.home.registry_assets_dir().join(cache_suffix));
            assert_eq!(std::fs::read(path).unwrap(), wasm);
        }
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
                &ModuleSource::from_version(parser_module(), Version::new(0, 3, 3)).unwrap(),
                "cmd/moonfmt",
                &quiet_user_log(),
                |url| {
                    urls.push(url.to_owned());
                    if url.ends_with(".wasm.sha256") {
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
                "https://download.mooncakes.io/prebuild/moonbitlang/parser@0.3.3/cmd/moonfmt/moonfmt.wasm.sha256",
                "https://download.mooncakes.io/prebuild/moonbitlang/parser@0.3.3/cmd/moonfmt/moonfmt.wasm",
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
            .acquire_wasm_asset_with(
                &ModuleSource::from_version(name.clone(), version.clone()).unwrap(),
                "cmd/moonfmt",
                &quiet_user_log(),
                |_| bail!("cache hit should not download"),
            )
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
                        api: "https://mooncakes.io".to_owned(),
                        index: String::new(),
                        download: "https://download.mooncakes.io".to_owned(),
                        symbols: None,
                        legacy_asset_urls: false,
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
                        &ModuleSource::from_version(parser_module(), Version::new(0, 3, 3))
                            .unwrap(),
                        "cmd/moonfmt",
                        &quiet_user_log(),
                        |url| {
                            if url.ends_with(".wasm.sha256") {
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
            .acquire_wasm_asset_with(
                &ModuleSource::from_version(name.clone(), version.clone()).unwrap(),
                "cmd/moonfmt",
                &quiet_user_log(),
                |url| {
                    if url.ends_with(".wasm.sha256") {
                        Ok(format!("{expected_checksum}\n").into_bytes())
                    } else {
                        Ok(b"different wasm".to_vec())
                    }
                },
            )
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
                &ModuleSource::from_version(parser_module(), Version::new(0, 3, 3)).unwrap(),
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
            registry.endpoints.download = format!("http://{unavailable_address}");

            registry
                .acquire_source_to(
                    &ModuleSource::from_version(name.clone(), version.clone()).unwrap(),
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
                    api: String::new(),
                    index: String::new(),
                    download: String::new(),
                    symbols: None,
                    legacy_asset_urls: false,
                },
                home,
            ),
            name,
            version,
        )
    }

    pub(super) fn test_archive() -> Vec<u8> {
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
            .acquire_source_to(
                &ModuleSource::from_version(name.clone(), version.clone()).unwrap(),
                &checksum,
                &destination,
                &quiet_user_log(),
            )
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
                    &ModuleSource::from_version(name.clone(), version.clone()).unwrap(),
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
                            api: url_base.clone(),
                            index: String::new(),
                            download: url_base,
                            symbols: None,
                            legacy_asset_urls: false,
                        },
                        home,
                    );
                    barrier.wait();
                    registry.acquire_source_to(
                        &ModuleSource::from_version(name.clone(), version.clone()).unwrap(),
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
        let archive_path = registry.home.registry_source_archive_path(&name, &version);
        std::fs::create_dir_all(&archive_path).unwrap();
        let destination = sandbox.path().join("source");

        let error = registry
            .acquire_source_to(
                &ModuleSource::from_version(name.clone(), version.clone()).unwrap(),
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
    fn all_versions_distinguishes_missing_modules_from_invalid_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let registry = api_test_registry(dir.path(), "https://registry.example.test");
        let module: ModuleName = "test/module".into();
        let error = registry.all_versions_of(&module).unwrap_err();
        assert!(error.is::<crate::registry::RegistryModuleNotFound>());

        let index_file = registry.home.registry_index_file(&module);
        std::fs::create_dir_all(index_file.parent().unwrap()).unwrap();
        std::fs::write(index_file, r#"{"version":"invalid"}"#).unwrap();
        let error = registry.all_versions_of(&module).unwrap_err();
        assert!(!error.is::<crate::registry::RegistryModuleNotFound>());
        assert!(error.is::<semver::Error>());

        let error = registry.cached_latest_version(&module).unwrap_err();
        assert!(error.is::<semver::Error>());
    }

    #[test]
    fn executable_latest_refreshes_after_a_missing_local_module_index() {
        let run_git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        let base = tempfile::tempdir().unwrap();
        let source = base.path().join("source");
        run_git(&["init", "--quiet", source.to_str().unwrap()]);
        run_git(&[
            "-C",
            source.to_str().unwrap(),
            "checkout",
            "--quiet",
            "-b",
            "main",
        ]);
        run_git(&[
            "-C",
            source.to_str().unwrap(),
            "config",
            "user.name",
            "Moon Test",
        ]);
        run_git(&[
            "-C",
            source.to_str().unwrap(),
            "config",
            "user.email",
            "moon-test@example.invalid",
        ]);
        let index_file = source.join("user/alice/tool.index");
        std::fs::create_dir_all(index_file.parent().unwrap()).unwrap();
        std::fs::write(
            &index_file,
            r#"{"version":"1.2.3","deps":{}}
"#,
        )
        .unwrap();
        run_git(&["-C", source.to_str().unwrap(), "add", "."]);
        run_git(&[
            "-C",
            source.to_str().unwrap(),
            "commit",
            "--quiet",
            "-m",
            "add module",
        ]);

        let bare = base.path().join("index.git");
        run_git(&[
            "clone",
            "--bare",
            "--quiet",
            source.to_str().unwrap(),
            bare.to_str().unwrap(),
        ]);
        let bare = dunce::canonicalize(bare)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let index = if cfg!(windows) {
            format!("file:///{bare}")
        } else {
            format!("file://{bare}")
        };
        for has_local_index in [false, true] {
            let home = MoonHomeLayout::new(base.path().join(format!("home-{has_local_index}")));
            if has_local_index {
                std::fs::create_dir_all(home.registry_index_dir()).unwrap();
            }
            let mut registry = RegistryClient::with_home(
                RegistryConfig {
                    api: index.clone(),
                    index: index.clone(),
                    download: String::new(),
                    symbols: None,
                    legacy_asset_urls: false,
                },
                home,
            );
            registry.api_metadata = false;

            let package = registry
                .resolve_executable_package("alice/tool", &quiet_user_log())
                .unwrap();
            assert_eq!(package.module.name().to_string(), "alice/tool");
            assert_eq!(package.module.version().to_string(), "1.2.3");
        }
    }

    #[test]
    fn cached_latest_version_preserves_index_io_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut registry = api_test_registry(dir.path(), "https://registry.example.test");
        registry.api_metadata = false;
        let module: ModuleName = "test/module".into();
        std::fs::create_dir_all(registry.home.registry_index_file(&module)).unwrap();

        let error = registry.cached_latest_version(&module).unwrap_err();
        assert!(error.is::<std::io::Error>());
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
                api: String::new(),
                index: String::new(),
                download: String::new(),
                symbols: None,
                legacy_asset_urls: false,
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
            Some("abc123".to_owned())
        );

        let _ = std::fs::remove_dir_all(home.root());
    }

    #[test]
    fn deprecated_dependencies_warn_after_mvs_selection() {
        use crate::registry::mock::create_mock_module;
        use crate::resolver::{ModuleResolutionConfig, resolve_modules};
        use moonutil::resolution::{ModuleSource, ResolvedModule, ResolvedRootModules};
        use serde_json::json;
        let dir = tempfile::tempdir().unwrap();
        let home = MoonHomeLayout::new(dir.path().to_owned());
        for (name, releases) in [
            (
                "dep/direct",
                vec![json!({"version": "1.0.0", "yanked": true,
                    "yanked_reason": "Use\n  \u{1b}[31mdep/replacement\u{1b}[0m.  See its migration guide.\u{1b}]52;c;aWdub3JlZA==\u{7}"})],
            ),
            (
                "dep/left",
                vec![json!({"version": "1.0.0", "deps": {"dep/transitive": "1.0.0"}})],
            ),
            (
                "dep/right",
                vec![json!({"version": "1.0.0", "deps": {
                    "dep/transitive": "1.0.0", "dep/upgraded": "1.1.0"
                }})],
            ),
            (
                "dep/transitive",
                vec![
                    json!({"version": "1.0.0", "yanked": true}),
                    json!({"version": "1.1.0", "yanked": false}),
                ],
            ),
            (
                "dep/upgraded",
                vec![
                    json!({"version": "1.0.0", "yanked": true,
                        "deps": {"dep/unused": "1.0.0"}}),
                    json!({"version": "1.1.0", "yanked": false,
                        "yanked_reason": "A stale reason is not a deprecation."}),
                ],
            ),
            (
                "dep/unused",
                vec![json!({"version": "1.0.0", "yanked": true})],
            ),
            (
                "dep/local",
                vec![json!({"version": "1.0.0", "yanked": true})],
            ),
        ] {
            let index = home.registry_index_file(&name.into());
            std::fs::create_dir_all(index.parent().unwrap()).unwrap();
            std::fs::write(
                index,
                releases
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
            .unwrap();
        }
        let registry = RegistryClient::with_home(
            RegistryConfig {
                api: String::new(),
                index: String::new(),
                download: String::new(),
                symbols: None,
                legacy_asset_urls: false,
            },
            home,
        );
        let mut roots = ResolvedRootModules::with_key();
        for (name, deps) in [
            (
                "app/a",
                vec![
                    ("dep/direct", "1.0.0"),
                    ("dep/left", "1.0.0"),
                    ("dep/upgraded", "1.0.0"),
                    ("dep/local", "1.0.0"),
                ],
            ),
            ("app/b", vec![("dep/right", "1.0.0")]),
            ("dep/local", vec![]),
        ] {
            let module = Arc::new(create_mock_module(name, "1.0.0", deps));
            let source = ModuleSource::from_local_module(&module, &dir.path().join(name)).unwrap();
            roots.insert(ResolvedModule::new(source, module));
        }
        let config = ModuleResolutionConfig {
            registry: &registry,
            inject_std: false,
        };
        for level in [log::LevelFilter::Warn, log::LevelFilter::Error] {
            let (user_log, capture) = UserLog::captured(level);
            let resolved = resolve_modules(&config, roots.clone(), &user_log).unwrap();
            let selected = resolved
                .all_modules()
                .map(|source| (source.name().to_string(), source.version().to_string()))
                .collect::<HashMap<_, _>>();
            assert_eq!(selected["dep/transitive"], "1.0.0");
            assert_eq!(selected["dep/upgraded"], "1.1.0");
            assert!(!selected.contains_key("dep/unused"));
            let warnings = capture
                .take()
                .into_iter()
                .map(|entry| format!("{}\n", entry.message))
                .collect::<String>();
            if level == log::LevelFilter::Warn {
                expect_test::expect![[r#"
                    Dependency `dep/direct@1.0.0` is deprecated: Use
                      dep/replacement.  See its migration guide.
                    Dependency `dep/transitive@1.0.0` is deprecated.
                      required through app/a@1.0.0 -> dep/left@1.0.0 -> dep/transitive@1.0.0
                "#]]
                .assert_eq(&warnings);
            } else {
                assert_eq!(warnings, "");
            }
        }
    }
}
