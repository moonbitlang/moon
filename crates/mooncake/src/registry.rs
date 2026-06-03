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

use moonutil::module::MoonMod;
use moonutil::mooncakes::ModuleName;
pub use online::*;
use semver::Version;

pub trait Registry {
    /// Get all versions of a module.
    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Arc<BTreeMap<Version, Arc<MoonMod>>>>;

    fn get_module_version(&self, name: &ModuleName, version: &Version) -> Option<Arc<MoonMod>> {
        let all_versions = self.all_versions_of(name).ok()?;
        all_versions.get(version).cloned()
    }

    /// Resolve an import-style path into:
    /// - module path
    /// - resolved version string, if the path contains an explicit version,
    ///   or the latest version of the resolved module if the path is unversioned
    /// - full package path with the version marker removed
    ///
    /// Resolution rules:
    /// - If `allow_explicit_version` is `true` and `path` contains `@`,
    ///   resolve `username/module@version[/package]`.
    /// - `moonbitlang/core[/package]` is resolved directly and uses
    ///   [`DEFAULT_VERSION`] as its version.
    /// - Otherwise, resolve the first two path segments as the module name and
    ///   fill the module version with that module's latest version.
    ///
    /// Returns an error if the path is malformed, explicit versions are disallowed,
    /// or no module can be resolved from registry metadata.
    fn resolve_path(
        &self,
        path: &str,
        allow_explicit_version: bool,
    ) -> anyhow::Result<(ModuleName, String, String)> {
        path::resolve_registry_path(path, allow_explicit_version, |module| {
            self.all_versions_of(module).ok().and_then(|versions| {
                versions
                    .last_key_value()
                    .map(|(latest_version, _)| latest_version.to_string())
            })
        })
    }

    fn get_latest_version(&self, name: &ModuleName) -> Option<Arc<MoonMod>> {
        let all_versions = self.all_versions_of(name).ok()?;
        all_versions.values().last().cloned()
    }

    fn install_to(
        &self,
        name: &ModuleName,
        version: &Version,
        to: &Path,
        quiet: bool,
    ) -> anyhow::Result<()>;
}

impl<R> Registry for &mut R
where
    R: Registry,
{
    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Arc<BTreeMap<Version, Arc<MoonMod>>>> {
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

    fn get_module_version(&self, name: &ModuleName, version: &Version) -> Option<Arc<MoonMod>> {
        (**self).get_module_version(name, version)
    }

    fn get_latest_version(&self, name: &ModuleName) -> Option<Arc<MoonMod>> {
        (**self).get_latest_version(name)
    }

    fn resolve_path(
        &self,
        path: &str,
        allow_explicit_version: bool,
    ) -> anyhow::Result<(ModuleName, String, String)> {
        (**self).resolve_path(path, allow_explicit_version)
    }
}

impl<R> Registry for Box<R>
where
    R: Registry + ?Sized,
{
    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Arc<BTreeMap<Version, Arc<MoonMod>>>> {
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

    fn get_module_version(&self, name: &ModuleName, version: &Version) -> Option<Arc<MoonMod>> {
        (**self).get_module_version(name, version)
    }

    fn get_latest_version(&self, name: &ModuleName) -> Option<Arc<MoonMod>> {
        (**self).get_latest_version(name)
    }

    fn resolve_path(
        &self,
        path: &str,
        allow_explicit_version: bool,
    ) -> anyhow::Result<(ModuleName, String, String)> {
        (**self).resolve_path(path, allow_explicit_version)
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
    use moonutil::{common::MOONBITLANG_CORE, mooncakes::DEFAULT_VERSION};

    #[test]
    fn resolve_path_uses_latest_version_for_unversioned_path() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("path/to", "0.2.0", [])
            .add_module_full("path/to", "0.1.0", []);

        let (name, version, full_path) = registry
            .resolve_path("path/to/module/a/b", true)
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "path/to");
        assert_eq!(version, "0.2.0");
        assert_eq!(full_path, "path/to/module/a/b");
    }

    #[test]
    fn resolve_path_latest_prefers_release_over_same_base_prerelease() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("path/to", "1.2.0-rc.1", [])
            .add_module_full("path/to", "1.2.0", []);

        let (name, version, full_path) = registry
            .resolve_path("path/to/module/a/b", true)
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "path/to");
        assert_eq!(version, "1.2.0");
        assert_eq!(full_path, "path/to/module/a/b");
    }

    #[test]
    fn resolve_path_latest_uses_prerelease_when_it_is_semver_max() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("path/to", "1.2.9", [])
            .add_module_full("path/to", "1.3.0-rc.1", []);

        let (name, version, full_path) = registry
            .resolve_path("path/to/module/a/b", true)
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "path/to");
        assert_eq!(version, "1.3.0-rc.1");
        assert_eq!(full_path, "path/to/module/a/b");
    }

    #[test]
    fn resolve_path_uses_first_two_segments_as_module() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("a/b", "0.2.0", [])
            .add_module_full("a/b", "0.1.0", []);

        let (name, version, full_path) = registry
            .resolve_path("a/b/c/d/e/f/g", true)
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "a/b");
        assert_eq!(version, "0.2.0");
        assert_eq!(full_path, "a/b/c/d/e/f/g");
    }

    #[test]
    fn resolve_path_returns_default_version_for_core() {
        let registry = MockRegistry::new();
        let (root_name, root_version, root_full_path) = registry
            .resolve_path("moonbitlang/core", true)
            .expect("core root path should resolve");
        assert_eq!(root_name.to_string(), MOONBITLANG_CORE);
        assert_eq!(root_version, DEFAULT_VERSION.to_string());
        assert_eq!(root_full_path, "moonbitlang/core");

        let (name, version, full_path) = registry
            .resolve_path("moonbitlang/core/list", true)
            .expect("core path should resolve");
        assert_eq!(name.to_string(), MOONBITLANG_CORE);
        assert_eq!(version, DEFAULT_VERSION.to_string());
        assert_eq!(full_path, "moonbitlang/core/list");
    }

    #[test]
    fn resolve_path_does_not_treat_corexx_as_core() {
        let mut registry = MockRegistry::new();
        registry.add_module_full("moonbitlang/corexx", "0.1.0", []);
        let (name, version, full_path) = registry
            .resolve_path("moonbitlang/corexx/list", true)
            .expect("corexx path should resolve as a normal module");
        assert_eq!(name.to_string(), "moonbitlang/corexx");
        assert_eq!(version, "0.1.0");
        assert_eq!(full_path, "moonbitlang/corexx/list");
    }

    #[test]
    fn resolve_path_uses_explicit_version_package_suffix_after_two_segment_module() {
        let mut registry = MockRegistry::new();
        registry.add_module_full("moonbitlang/x", "0.4.39", []);

        let (name, version, full_path) = registry
            .resolve_path("moonbitlang/x@0.4.39/fs/path", true)
            .expect("explicit version path should resolve");
        assert_eq!(name.to_string(), "moonbitlang/x");
        assert_eq!(version, "0.4.39");
        assert_eq!(full_path, "moonbitlang/x/fs/path");
    }

    #[test]
    fn resolve_path_rejects_three_segment_explicit_module_name() {
        let registry = MockRegistry::new();
        assert!(
            registry
                .resolve_path("moonbitlang/x/fs@0.4.39/path", true)
                .is_err()
        );
    }

    #[test]
    fn resolve_path_rejects_package_version_suffix() {
        let mut registry = MockRegistry::new();
        registry.add_module_full("moonbitlang/x", "0.4.39", []);
        assert!(
            registry
                .resolve_path("moonbitlang/x/fs@0.4.39", true)
                .is_err()
        );
    }

    #[test]
    fn resolve_path_rejects_versioned_core_package_suffix() {
        let registry = MockRegistry::new();
        assert!(
            registry
                .resolve_path("moonbitlang/core/list@0.4.38", true)
                .is_err()
        );
    }

    #[test]
    fn resolve_path_rejects_explicit_version_when_disallowed() {
        let mut registry = MockRegistry::new();
        registry.add_module_full("moonbitlang/x/fs", "0.4.39", []);
        assert!(
            registry
                .resolve_path("moonbitlang/x@0.4.39/fs", false)
                .is_err()
        );
    }
}
