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

use std::{collections::BTreeMap, path::Path, sync::Arc};

use moonutil::common::{MOD_NAME_STDLIB, MOONBITLANG_CORE};
use moonutil::module::MoonMod;
use moonutil::mooncakes::{DEFAULT_VERSION, ModuleName};
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
    ///   the part before the rightmost `@` is treated as the module name and
    ///   the part right after `@` (up to next `/`) is treated as version.
    /// - `moonbitlang/core[/package]` is resolved directly and uses
    ///   [`DEFAULT_VERSION`] as its version.
    /// - Otherwise, resolve by longest-prefix match against existing modules in
    ///   the registry (for example, `a/b/c/d` prefers module `a/b/c` over `a/b`)
    ///   and fill the module version with that module's latest version.
    ///
    /// Returns `None` if the path is malformed, explicit versions are disallowed,
    /// or no module can be resolved from registry metadata.
    fn resolve_path(
        &self,
        path: &str,
        allow_explicit_version: bool,
    ) -> Option<(ModuleName, String, String)> {
        let contains_at = path.contains('@');

        // reject paths like `moonbitlang/core@version` and `moonbitlang/core/path@version`
        if path.starts_with(&format!("{MOONBITLANG_CORE}@"))
            || contains_at && path.starts_with(&format!("{MOONBITLANG_CORE}/"))
        {
            return None;
        }

        // handle `moonbitlang/core` and `moonbitlang/core/path` (no @ in path) special case
        if path == MOONBITLANG_CORE
            || !contains_at && path.starts_with(&format!("{MOONBITLANG_CORE}/"))
        {
            return Some((
                MOD_NAME_STDLIB.clone(),
                DEFAULT_VERSION.to_string(),
                path.to_string(),
            ));
        }

        match (allow_explicit_version, contains_at) {
            // handle explicit version case
            // "path/to/module@version/package/path"
            // "path/to/module@version"
            (true, true) => {
                if let Some((module_name, tail)) = path.rsplit_once('@') {
                    let module_name = module_name.parse::<ModuleName>().ok()?;
                    if module_name.username.is_empty() {
                        return None;
                    }
                    let (version, package) = match tail.split_once('/') {
                        Some((version, package)) => (version, package),
                        None => (tail, ""),
                    };
                    if version.is_empty() {
                        return None;
                    }
                    let module_name_str = module_name.to_string();
                    let full_path_without_version = match package {
                        "" => module_name_str,
                        pkg => format!("{module_name_str}/{pkg}"),
                    };
                    Some((module_name, version.to_string(), full_path_without_version))
                } else {
                    panic!("unreachable: contains_at is true but no '@' found");
                }
            }
            // reject explicit version case when disallowed
            (false, true) => None,
            // handle unversioned path case, try longest-prefix match to find the module and its latest version
            // "a/b/c/d" -> prefer "a/b/c" over "a/b"
            (_, false) => {
                let segments: Vec<&str> = path.split('/').collect();
                if segments.len() < 2 || segments.iter().any(|s| s.is_empty()) {
                    return None;
                }

                for segment_count in (2..=segments.len()).rev() {
                    let candidate_str = segments[..segment_count].join("/");
                    let candidate = candidate_str.parse::<ModuleName>().ok()?;
                    if candidate.username.is_empty() {
                        return None;
                    }
                    let latest_version =
                        self.all_versions_of(&candidate).ok().and_then(|versions| {
                            versions
                                .last_key_value()
                                .map(|(latest_version, _)| latest_version.to_string())
                        });
                    if let Some(latest_version) = latest_version {
                        return Some((candidate, latest_version, path.to_string()));
                    }
                }
                None
            }
        }
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
    ) -> Option<(ModuleName, String, String)> {
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
    ) -> Option<(ModuleName, String, String)> {
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
            .add_module_full("path/to/module", "0.2.0", [])
            .add_module_full("path/to/module", "0.1.0", []);

        let (name, version, full_path) = registry
            .resolve_path("path/to/module/a/b", true)
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "path/to/module");
        assert_eq!(version, "0.2.0");
        assert_eq!(full_path, "path/to/module/a/b");
    }

    #[test]
    fn resolve_path_longest_prefix() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("a/b/c", "0.2.0", [])
            .add_module_full("a/b/c", "0.1.0", [])
            .add_module_full("a/b/c/d/e", "0.3.0", [])
            .add_module_full("a/b/c/d/e", "0.1.0", []);

        let (name, version, full_path) = registry
            .resolve_path("a/b/c/d/e/f/g", true)
            .expect("module path should resolve");
        assert_eq!(name.to_string(), "a/b/c/d/e");
        assert_eq!(version, "0.3.0");
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
    fn resolve_path_uses_explicit_version_boundary() {
        let mut registry = MockRegistry::new();
        registry.add_module_full("moonbitlang/x/fs", "0.4.39", []);

        let (name, version, full_path) = registry
            .resolve_path("moonbitlang/x/fs@0.4.39/path", true)
            .expect("explicit version path should resolve");
        assert_eq!(name.to_string(), "moonbitlang/x/fs");
        assert_eq!(version, "0.4.39");
        assert_eq!(full_path, "moonbitlang/x/fs/path");
    }

    #[test]
    fn resolve_path_rejects_explicit_version_when_disallowed() {
        let mut registry = MockRegistry::new();
        registry.add_module_full("moonbitlang/x/fs", "0.4.39", []);
        assert!(
            registry
                .resolve_path("moonbitlang/x/fs@0.4.39/path", false)
                .is_none()
        );
    }
}
