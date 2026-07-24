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

#[cfg(test)]
pub(crate) mod mock;
pub(crate) mod online;
pub mod path;

use std::{collections::BTreeMap, path::Path, sync::Arc};

use indexmap::IndexMap;
use moonutil::dependency::SourceDependencyInfo;
use moonutil::resolution::ModuleName;
pub use online::*;
use semver::Version;

#[derive(Debug, Clone, Default)]
pub struct RegistryVersionInfo {
    pub deps: IndexMap<String, SourceDependencyInfo>,
    pub checksum: Option<String>,
}

pub trait Registry {
    /// Get all versions of a module.
    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Arc<BTreeMap<Version, RegistryVersionInfo>>>;

    /// Resolve an unversioned import-style path into:
    /// - module path
    /// - the latest version of the resolved module
    /// - full package path
    ///
    /// Resolution rules:
    /// - `moonbitlang/core[/package]` is resolved directly and uses
    ///   [`DEFAULT_VERSION`] as its version.
    /// - Otherwise, resolve the first two path segments as the module name.
    ///
    /// Returns an error if the path is malformed, contains an explicit version,
    /// or no module can be resolved from registry metadata.
    fn resolve_unversioned_path(&self, path: &str) -> anyhow::Result<(ModuleName, String, String)> {
        path::resolve_unversioned_registry_path(path, |module| {
            self.all_versions_of(module).ok().and_then(|versions| {
                versions
                    .last_key_value()
                    .map(|(latest_version, _)| latest_version.to_string())
            })
        })
    }

    fn get_latest_version(&self, name: &ModuleName) -> Option<Version> {
        let all_versions = self.all_versions_of(name).ok()?;
        all_versions
            .last_key_value()
            .map(|(version, _)| version.clone())
    }

    fn install_to(
        &self,
        name: &ModuleName,
        version: &Version,
        to: &Path,
        quiet: bool,
    ) -> anyhow::Result<()>;

    fn extract_to_verified(
        &self,
        name: &ModuleName,
        version: &Version,
        checksum: &str,
        to: &Path,
        quiet: bool,
    ) -> anyhow::Result<()>;

    fn checksum_of(&self, name: &ModuleName, version: &Version) -> anyhow::Result<String> {
        self.all_versions_of(name)?
            .get(version)
            .and_then(|info| info.checksum.clone())
            .ok_or_else(|| anyhow::anyhow!("No checksum found for {name}@{version}"))
    }
}

impl<R> Registry for &mut R
where
    R: Registry,
{
    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Arc<BTreeMap<Version, RegistryVersionInfo>>> {
        (**self).all_versions_of(name)
    }

    fn install_to(
        &self,
        name: &ModuleName,
        version: &Version,
        to: &Path,
        quiet: bool,
    ) -> anyhow::Result<()> {
        (**self).install_to(name, version, to, quiet)
    }

    fn extract_to_verified(
        &self,
        name: &ModuleName,
        version: &Version,
        checksum: &str,
        to: &Path,
        quiet: bool,
    ) -> anyhow::Result<()> {
        (**self).extract_to_verified(name, version, checksum, to, quiet)
    }

    fn checksum_of(&self, name: &ModuleName, version: &Version) -> anyhow::Result<String> {
        (**self).checksum_of(name, version)
    }

    fn get_latest_version(&self, name: &ModuleName) -> Option<Version> {
        (**self).get_latest_version(name)
    }

    fn resolve_unversioned_path(&self, path: &str) -> anyhow::Result<(ModuleName, String, String)> {
        (**self).resolve_unversioned_path(path)
    }
}

impl<R> Registry for Box<R>
where
    R: Registry + ?Sized,
{
    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Arc<BTreeMap<Version, RegistryVersionInfo>>> {
        (**self).all_versions_of(name)
    }

    fn install_to(
        &self,
        name: &ModuleName,
        version: &Version,
        to: &Path,
        quiet: bool,
    ) -> anyhow::Result<()> {
        (**self).install_to(name, version, to, quiet)
    }

    fn extract_to_verified(
        &self,
        name: &ModuleName,
        version: &Version,
        checksum: &str,
        to: &Path,
        quiet: bool,
    ) -> anyhow::Result<()> {
        (**self).extract_to_verified(name, version, checksum, to, quiet)
    }

    fn checksum_of(&self, name: &ModuleName, version: &Version) -> anyhow::Result<String> {
        (**self).checksum_of(name, version)
    }

    fn get_latest_version(&self, name: &ModuleName) -> Option<Version> {
        (**self).get_latest_version(name)
    }

    fn resolve_unversioned_path(&self, path: &str) -> anyhow::Result<(ModuleName, String, String)> {
        (**self).resolve_unversioned_path(path)
    }
}

pub(crate) fn default_registry() -> Box<dyn Registry> {
    Box::new(OnlineRegistry::mooncakes_io())
}

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod tests {
    use super::Registry;
    use crate::registry::mock::MockRegistry;
    use moonutil::{constants::MOONBITLANG_CORE, resolution::DEFAULT_VERSION};

    #[test]
    fn resolve_unversioned_path_uses_latest_version() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("path/to", "0.2.0", [])
            .add_module_full("path/to", "0.1.0", []);

        let (name, version, full_path) = registry
            .resolve_unversioned_path("path/to/module/a/b")
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "path/to");
        assert_eq!(version, "0.2.0");
        assert_eq!(full_path, "path/to/module/a/b");
    }

    #[test]
    fn resolve_unversioned_path_latest_prefers_release_over_same_base_prerelease() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("path/to", "1.2.0-rc.1", [])
            .add_module_full("path/to", "1.2.0", []);

        let (name, version, full_path) = registry
            .resolve_unversioned_path("path/to/module/a/b")
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "path/to");
        assert_eq!(version, "1.2.0");
        assert_eq!(full_path, "path/to/module/a/b");
    }

    #[test]
    fn resolve_unversioned_path_latest_uses_prerelease_when_it_is_semver_max() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("path/to", "1.2.9", [])
            .add_module_full("path/to", "1.3.0-rc.1", []);

        let (name, version, full_path) = registry
            .resolve_unversioned_path("path/to/module/a/b")
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "path/to");
        assert_eq!(version, "1.3.0-rc.1");
        assert_eq!(full_path, "path/to/module/a/b");
    }

    #[test]
    fn resolve_unversioned_path_uses_first_two_segments_as_module() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("a/b", "0.2.0", [])
            .add_module_full("a/b", "0.1.0", []);

        let (name, version, full_path) = registry
            .resolve_unversioned_path("a/b/c/d/e/f/g")
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "a/b");
        assert_eq!(version, "0.2.0");
        assert_eq!(full_path, "a/b/c/d/e/f/g");
    }

    #[test]
    fn resolve_unversioned_path_returns_default_version_for_core() {
        let registry = MockRegistry::new();
        let (root_name, root_version, root_full_path) = registry
            .resolve_unversioned_path("moonbitlang/core")
            .expect("core root path should resolve");
        assert_eq!(root_name.to_string(), MOONBITLANG_CORE);
        assert_eq!(root_version, DEFAULT_VERSION.to_string());
        assert_eq!(root_full_path, "moonbitlang/core");

        let (name, version, full_path) = registry
            .resolve_unversioned_path("moonbitlang/core/list")
            .expect("core path should resolve");
        assert_eq!(name.to_string(), MOONBITLANG_CORE);
        assert_eq!(version, DEFAULT_VERSION.to_string());
        assert_eq!(full_path, "moonbitlang/core/list");
    }

    #[test]
    fn resolve_unversioned_path_does_not_treat_corexx_as_core() {
        let mut registry = MockRegistry::new();
        registry.add_module_full("moonbitlang/corexx", "0.1.0", []);
        let (name, version, full_path) = registry
            .resolve_unversioned_path("moonbitlang/corexx/list")
            .expect("corexx path should resolve as a normal module");
        assert_eq!(name.to_string(), "moonbitlang/corexx");
        assert_eq!(version, "0.1.0");
        assert_eq!(full_path, "moonbitlang/corexx/list");
    }

    #[test]
    fn resolve_unversioned_path_rejects_explicit_version() {
        let registry = MockRegistry::new();
        assert!(
            registry
                .resolve_unversioned_path("moonbitlang/x@0.4.39/fs")
                .is_err()
        );
    }
}
