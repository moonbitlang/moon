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

mod authentication;
mod client;
pub(crate) mod download;
mod executable;
mod external;
mod git;
mod http;
#[cfg(test)]
pub(crate) mod mock;
pub mod path;

use std::path::Path;

use anyhow::Context;

pub use client::{
    RegistryClient, RegistryModuleManifest, RegistryPackageMatch, RegistryPublishedModule,
    RegistryRelease, RegistrySearchResult, RegistrySummaryFragment, RegistryUserModules,
};
pub use executable::ResolvedExecutablePackage;
use indexmap::IndexMap;
use moonutil::dependency::SourceDependencyInfo;
use moonutil::resolution::{ModuleName, ModuleSource};
use moonutil::user_log::UserLog;
use semver::Version;

#[derive(Debug, Clone, Default)]
pub struct RegistryVersionInfo {
    pub deps: IndexMap<String, SourceDependencyInfo>,
    /// Advisory only: deprecated releases remain eligible for version selection.
    pub yanked: bool,
    pub yanked_reason: Option<String>,
}

/// A registry has no entry for the requested module.
#[derive(Debug, thiserror::Error)]
#[error("module `{0}` was not found in the registry")]
pub struct RegistryModuleNotFound(pub ModuleName);

/// A metadata refresh could not complete. Callers may explicitly use cached data.
/// Git synchronization failures, including local Git and filesystem failures,
/// belong here, as do HTTP transport errors, 5xx responses, and request throttling.
/// Authoritative absence and failures to read or validate module
/// metadata records do not: a cached answer cannot override those results.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct RegistryUnavailable(pub anyhow::Error);

pub trait Registry {
    /// Read one exact published version. Never substitute another version.
    ///
    /// `None` means the version is absent from a known module. Return
    /// [`RegistryModuleNotFound`] if the module has no registry entry, and
    /// preserve read errors so callers can distinguish failures from absence.
    /// Backends that cannot distinguish absence from denied access return an
    /// error rather than guessing which part of the coordinate is missing.
    fn version_info(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<Option<RegistryVersionInfo>>;

    /// Read the locally known latest version without contacting the registry.
    /// `None` means there is no cached latest answer, not that the remote module
    /// does not exist. Preserve local read errors instead of treating them as misses.
    /// A partial cache of exact releases cannot be used to infer latest.
    fn cached_latest_version(&self, name: &ModuleName) -> anyhow::Result<Option<Version>>;

    /// Query the registry for its current latest version and retain the answer.
    /// Every call must contact or revalidate against the registry, even when a
    /// cached answer exists. Failure must not silently return a stale version.
    /// Git synchronizes its index; an HTTP backend queries its latest endpoint.
    /// Return [`RegistryUnavailable`] when acquiring or refreshing metadata fails,
    /// distinguishing that failure from reading or validating a module's answer.
    fn query_latest_version(
        &self,
        name: &ModuleName,
        user_log: &UserLog,
    ) -> anyhow::Result<Version>;

    /// Resolve an unversioned import-style path into:
    /// - module path
    /// - the cached latest version of the resolved module
    /// - full package path
    ///
    /// Resolution rules:
    /// - `moonbitlang/core[/package]` is resolved directly and uses
    ///   [`DEFAULT_VERSION`] as its version.
    /// - Otherwise, resolve `user/module[/vN]` as the module name.
    ///
    /// Does not refresh registry metadata. Returns an error if the path is
    /// malformed, contains an explicit version, or has no cached latest answer.
    fn resolve_cached_unversioned_path(
        &self,
        path: &str,
    ) -> anyhow::Result<(ModuleName, String, String)> {
        path::resolve_unversioned_registry_path(path, |module| {
            Ok(self
                .cached_latest_version(module)?
                .map(|version| version.to_string()))
        })
    }
}

pub(crate) trait RegistrySource {
    /// Materialize verified published source without executing package hooks.
    fn materialize_source_to(
        &self,
        module: &ModuleSource,
        to: &Path,
        user_log: &UserLog,
    ) -> anyhow::Result<()> {
        let checksum = self.source_archive_checksum(module)?;
        self.acquire_source_to(module, &checksum, to, user_log)
    }

    /// Ensure the published source archive is available, verify it against the
    /// checksum selected by the caller, and extract it without executing
    /// package-controlled hooks such as `scripts.postadd`.
    ///
    /// The registry adapter decides whether to reuse a downloaded archive or
    /// acquire it remotely; callers do not need to prepare an archive first.
    fn acquire_source_to(
        &self,
        module: &ModuleSource,
        expected_checksum: &str,
        to: &Path,
        user_log: &UserLog,
    ) -> anyhow::Result<()>;

    /// Return the registry's SHA-256 checksum for the published source
    /// ZIP archive.
    fn source_archive_checksum(&self, module: &ModuleSource) -> anyhow::Result<String>;
}

impl<R> Registry for &mut R
where
    R: Registry,
{
    fn version_info(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<Option<RegistryVersionInfo>> {
        (**self).version_info(name, version)
    }

    fn cached_latest_version(&self, name: &ModuleName) -> anyhow::Result<Option<Version>> {
        (**self).cached_latest_version(name)
    }

    fn query_latest_version(
        &self,
        name: &ModuleName,
        user_log: &UserLog,
    ) -> anyhow::Result<Version> {
        (**self).query_latest_version(name, user_log)
    }

    fn resolve_cached_unversioned_path(
        &self,
        path: &str,
    ) -> anyhow::Result<(ModuleName, String, String)> {
        (**self).resolve_cached_unversioned_path(path)
    }
}

impl<R> Registry for Box<R>
where
    R: Registry + ?Sized,
{
    fn version_info(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> anyhow::Result<Option<RegistryVersionInfo>> {
        (**self).version_info(name, version)
    }

    fn cached_latest_version(&self, name: &ModuleName) -> anyhow::Result<Option<Version>> {
        (**self).cached_latest_version(name)
    }

    fn query_latest_version(
        &self,
        name: &ModuleName,
        user_log: &UserLog,
    ) -> anyhow::Result<Version> {
        (**self).query_latest_version(name, user_log)
    }

    fn resolve_cached_unversioned_path(
        &self,
        path: &str,
    ) -> anyhow::Result<(ModuleName, String, String)> {
        (**self).resolve_cached_unversioned_path(path)
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

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    format!("{:x}", Sha256::digest(bytes))
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

pub(crate) fn default_registry(user_log: &UserLog) -> RegistryClient {
    RegistryClient::configured(user_log)
}

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod tests {
    use super::Registry;
    use crate::registry::mock::MockRegistry;
    use moonutil::{constants::MOONBITLANG_CORE, resolution::DEFAULT_VERSION};

    #[test]
    fn resolve_cached_unversioned_path_uses_latest_version() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("path/to", "0.2.0", [])
            .add_module_full("path/to", "0.1.0", []);

        let (name, version, full_path) = registry
            .resolve_cached_unversioned_path("path/to/module/a/b")
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "path/to");
        assert_eq!(version, "0.2.0");
        assert_eq!(full_path, "path/to/module/a/b");
    }

    #[test]
    fn resolve_cached_unversioned_path_latest_prefers_release_over_same_base_prerelease() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("path/to", "1.2.0-rc.1", [])
            .add_module_full("path/to", "1.2.0", []);

        let (name, version, full_path) = registry
            .resolve_cached_unversioned_path("path/to/module/a/b")
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "path/to");
        assert_eq!(version, "1.2.0");
        assert_eq!(full_path, "path/to/module/a/b");
    }

    #[test]
    fn resolve_cached_unversioned_path_latest_uses_prerelease_when_it_is_semver_max() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("path/to", "1.2.9", [])
            .add_module_full("path/to", "1.3.0-rc.1", []);

        let (name, version, full_path) = registry
            .resolve_cached_unversioned_path("path/to/module/a/b")
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "path/to");
        assert_eq!(version, "1.3.0-rc.1");
        assert_eq!(full_path, "path/to/module/a/b");
    }

    #[test]
    fn resolve_cached_unversioned_path_uses_first_two_segments_as_module() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("a/b", "0.2.0", [])
            .add_module_full("a/b", "0.1.0", []);

        let (name, version, full_path) = registry
            .resolve_cached_unversioned_path("a/b/c/d/e/f/g")
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "a/b");
        assert_eq!(version, "0.2.0");
        assert_eq!(full_path, "a/b/c/d/e/f/g");
    }

    #[test]
    fn resolve_cached_unversioned_path_returns_default_version_for_core() {
        let registry = MockRegistry::new();
        let (root_name, root_version, root_full_path) = registry
            .resolve_cached_unversioned_path("moonbitlang/core")
            .expect("core root path should resolve");
        assert_eq!(root_name.to_string(), MOONBITLANG_CORE);
        assert_eq!(root_version, DEFAULT_VERSION.to_string());
        assert_eq!(root_full_path, "moonbitlang/core");

        let (name, version, full_path) = registry
            .resolve_cached_unversioned_path("moonbitlang/core/list")
            .expect("core path should resolve");
        assert_eq!(name.to_string(), MOONBITLANG_CORE);
        assert_eq!(version, DEFAULT_VERSION.to_string());
        assert_eq!(full_path, "moonbitlang/core/list");
    }

    #[test]
    fn resolve_cached_unversioned_path_does_not_treat_corexx_as_core() {
        let mut registry = MockRegistry::new();
        registry.add_module_full("moonbitlang/corexx", "0.1.0", []);
        let (name, version, full_path) = registry
            .resolve_cached_unversioned_path("moonbitlang/corexx/list")
            .expect("corexx path should resolve as a normal module");
        assert_eq!(name.to_string(), "moonbitlang/corexx");
        assert_eq!(version, "0.1.0");
        assert_eq!(full_path, "moonbitlang/corexx/list");
    }

    #[test]
    fn resolve_cached_unversioned_path_rejects_explicit_version() {
        let registry = MockRegistry::new();
        assert!(
            registry
                .resolve_cached_unversioned_path("moonbitlang/x@0.4.39/fs")
                .is_err()
        );
    }
}
