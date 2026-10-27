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

//! On-demand HTTP release metadata and the cache policy for exact/latest queries.

use super::{RegistryUnavailable, RegistryVersionInfo, encode_path_segment};

pub(super) mod cache;
use anyhow::{Context, bail};
use cache::MetadataCache;
use indexmap::IndexMap;
use moonutil::{
    MoonHomeLayout,
    dependency::SourceDependencyInfo,
    registry::registry_api_url,
    resolution::{ModuleName, ModuleSource},
};
use reqwest::StatusCode;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::cell::OnceCell;

/// Module visibility within one registry; it does not select a registry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
enum ModuleVisibility {
    #[serde(rename = "PUBLIC")]
    Public,
    #[serde(rename = "PRIVATE")]
    Private,
}

/// HTTP responses and persisted releases require an exact version and checksum.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct ModuleMetadata {
    module: String,
    visibility: ModuleVisibility,
    version: Version,
    #[serde(default, deserialize_with = "deserialize_dependencies")]
    deps: IndexMap<String, SourceDependencyInfo>,
    checksum: String,
    #[serde(default)]
    yanked: bool,
    yanked_reason: Option<String>,
}

// Normalize optional dependency maps at the HTTP wire boundary. Internally,
// every release has one dependency map, including an empty one.
fn deserialize_dependencies<'de, D>(
    deserializer: D,
) -> Result<IndexMap<String, SourceDependencyInfo>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(
        Option::<IndexMap<String, SourceDependencyInfo>>::deserialize(deserializer)?
            .unwrap_or_default(),
    )
}

pub(super) struct HttpMetadata {
    api: String,
    cache: MetadataCache,
    client: OnceCell<reqwest::blocking::Client>,
}

impl HttpMetadata {
    pub(super) fn new(api: String, home: MoonHomeLayout) -> Self {
        Self {
            cache: MetadataCache::new(home, api.clone()),
            api,
            client: OnceCell::new(),
        }
    }

    pub(super) fn version_info(
        &self,
        name: &ModuleName,
        version: &Version,
        frozen: bool,
    ) -> anyhow::Result<RegistryVersionInfo> {
        let metadata = self.exact_release(name, version, frozen)?;
        Ok(metadata.into())
    }

    pub(super) fn cached_version_info(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<Option<RegistryVersionInfo>> {
        Ok(self.cache.release(name, version)?.map(Into::into))
    }

    pub(super) fn refresh_version_info(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<RegistryVersionInfo> {
        super::path::parse_module_path(&name.to_string())?;
        Ok(self.fetch_release(name, Some(version))?.into())
    }

    pub(super) fn source_archive_checksum(
        &self,
        name: &ModuleName,
        version: &Version,
        frozen: bool,
    ) -> anyhow::Result<String> {
        Ok(self
            .exact_release(name, version, frozen)?
            .checksum
            .to_ascii_lowercase())
    }

    fn exact_release(
        &self,
        name: &ModuleName,
        version: &Version,
        frozen: bool,
    ) -> anyhow::Result<ModuleMetadata> {
        super::path::parse_module_path(&name.to_string())?;
        if let Some(metadata) = self.cache.release(name, version)? {
            return Ok(metadata);
        }
        if frozen {
            bail!("missing cached registry metadata for {name}; frozen mode cannot fetch it");
        }
        self.fetch_release(name, Some(version))
    }

    pub(super) fn query_latest_version(&self, name: &ModuleName) -> anyhow::Result<Version> {
        super::path::parse_module_path(&name.to_string())?;
        let metadata = self.fetch_release(name, None)?;
        self.cache.store_latest(name, &metadata.version)?;
        Ok(metadata.version)
    }

    pub(super) fn cached_latest_version(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Option<Version>> {
        self.cache.latest(name)
    }

    /// This endpoint returns latest when the version query parameter is absent.
    /// Keep that wire representation here; callers use distinct exact/latest APIs.
    fn fetch_release(
        &self,
        name: &ModuleName,
        version: Option<&Version>,
    ) -> anyhow::Result<ModuleMetadata> {
        let module_path = name
            .segments()
            .map(encode_path_segment)
            .collect::<Vec<_>>()
            .join("/");
        let api = registry_api_url(&self.api)?;
        let mut url = format!(
            "{}/api/v0/registry/modules/{module_path}",
            api.as_str().trim_end_matches('/')
        );
        if let Some(version) = version {
            let query = form_urlencoded::Serializer::new(String::new())
                .append_pair("version", &version.to_string())
                .finish();
            url.push('?');
            url.push_str(&query);
        }
        let response = self
            .http_client()?
            .get(&url)
            .send()
            .map_err(|error| {
                if error.is_connect() || error.is_timeout() || error.is_request() {
                    anyhow::Error::new(RegistryUnavailable(error.into()))
                } else {
                    error.into()
                }
            })
            .with_context(|| format!("failed to fetch registry metadata for {name}"))?;
        if matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
        ) {
            // The endpoint deliberately does not distinguish absence from denial.
            bail!("Module `{name}` not found or access denied");
        }
        let status = response.status();
        let response = response
            .error_for_status()
            .map_err(|error| {
                if status.is_server_error()
                    || status == StatusCode::REQUEST_TIMEOUT
                    || status == StatusCode::TOO_MANY_REQUESTS
                {
                    anyhow::Error::new(RegistryUnavailable(error.into()))
                } else {
                    error.into()
                }
            })
            .with_context(|| format!("registry metadata request failed for {name}"))?;
        let bytes = response
            .bytes()
            .map_err(|error| RegistryUnavailable(error.into()))
            .with_context(|| format!("failed to read registry metadata response for {name}"))?;
        let metadata: ModuleMetadata = serde_json::from_slice(&bytes)
            .with_context(|| format!("invalid registry metadata response for {name}"))?;
        metadata.validate(name, version)?;
        self.cache.store_release(name, metadata)
    }

    fn http_client(&self) -> anyhow::Result<&reqwest::blocking::Client> {
        if let Some(client) = self.client.get() {
            return Ok(client);
        }
        // Resolver requests are anonymous; authenticated command APIs have their
        // own client and credential policy. Cache-only reads need no client;
        // retain successful initialization while allowing failures to be retried.
        let client = reqwest::blocking::Client::builder()
            .user_agent(format!("mooncake/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .context("failed to create metadata HTTP client")?;
        Ok(self.client.get_or_init(|| client))
    }
}

impl ModuleMetadata {
    fn validate(&self, name: &ModuleName, expected: Option<&Version>) -> anyhow::Result<()> {
        if self.module != name.to_string() || self.visibility != ModuleVisibility::Public {
            bail!("invalid registry metadata identity for {name}");
        }
        ModuleSource::from_version(name.clone(), self.version.clone())?;
        if expected.is_some_and(|version| version != &self.version) {
            bail!("registry returned a different version than requested for {name}");
        }
        let checksum = &self.checksum;
        if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("registry metadata has an invalid SHA-256 checksum for {name}");
        }
        for (dependency, requirement) in &self.deps {
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
}

impl From<ModuleMetadata> for RegistryVersionInfo {
    fn from(metadata: ModuleMetadata) -> Self {
        Self {
            deps: metadata.deps,
            yanked: metadata.yanked,
            yanked_reason: metadata.yanked_reason,
        }
    }
}
