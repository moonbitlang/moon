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

use moonutil::{
    constants::{MOD_NAME_STDLIB, MOONBITLANG_CORE},
    resolution::{DEFAULT_VERSION, ModuleName, ModuleSource},
};
use semver::Version;

use super::Registry;

/// A parsed registry path, with its module and package already separated.
/// Version text is retained for syntax-only consumers; `resolve` selects and
/// checks an exact module version before source acquisition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryPath {
    pub module: ModuleName,
    pub version: Option<String>,
    pub package: String,
}

impl RegistryPath {
    /// Interpret a concrete version.
    /// An omitted version stays unresolved; symbolic selectors such as `latest`
    /// must be handled by the command's version-selection policy.
    pub fn exact_version(&self) -> anyhow::Result<Option<Version>> {
        Ok(self.version.as_deref().map(Version::parse).transpose()?)
    }

    /// Select an exact version using the current registry index, without refreshing it.
    /// An explicit version does not require a registry entry.
    pub fn resolve(&self, registry: &impl Registry) -> anyhow::Result<ModuleSource> {
        let version = match self.exact_version()? {
            Some(version) => version,
            None => registry
                .get_latest_version(&self.module)
                .ok_or_else(|| anyhow::anyhow!("Module `{}` not found in registry", self.module))?,
        };
        Ok(ModuleSource::from_version(self.module.clone(), version))
    }

    pub fn full_path_without_version(&self) -> String {
        let module = self.module.to_string();
        if self.package.is_empty() {
            module
        } else {
            format!("{module}/{}", self.package)
        }
    }

    fn with_version(mut self, version: Option<&str>) -> anyhow::Result<Self> {
        if let Some(version) = version
            && (version.is_empty() || version.contains('/'))
        {
            anyhow::bail!("version must not be empty or contain path separators");
        }
        self.version = version.map(str::to_owned);
        Ok(self)
    }
}

fn split_version(input: &str) -> anyhow::Result<(&str, Option<&str>)> {
    match input.split_once('@') {
        Some((path, version)) => {
            if version.contains('@') {
                anyhow::bail!("must contain a single version marker");
            }
            Ok((path, Some(version)))
        }
        None => Ok((input, None)),
    }
}

fn parse_path_components(path: &str) -> anyhow::Result<Vec<&str>> {
    let components = path.split('/').collect::<Vec<_>>();
    // FIXME: Replace this defensive denylist with validation against the
    // registry's allowed component grammar once that grammar is defined.
    if components.iter().any(|component| {
        component.is_empty()
            || *component == "."
            || *component == ".."
            || component.contains(':')
            || component.contains('\\')
            || component.contains(['#', '?', '%'])
    }) {
        anyhow::bail!("path contains an invalid component");
    }
    Ok(components)
}

/// Parse a module-only coordinate, `user/module[@version]`.
/// Unlike a package coordinate, the whole path names the module, including
/// legacy module names with more than two components.
pub fn parse_module_path(input: &str) -> anyhow::Result<RegistryPath> {
    let (module, version) = split_version(input)?;
    if parse_path_components(module)?.len() < 2 {
        anyhow::bail!(
            "registry module name must be in the form of <author>/<module_name>[@<version>]"
        );
    }
    let parsed = RegistryPath {
        module: module.into(),
        version: None,
        package: String::new(),
    }
    .with_version(version)?;
    parsed.exact_version()?;
    Ok(parsed)
}

/// Parse `user/module[/package]` using the install/runwasm interpretation.
///
/// The first two segments are the module, and later segments are the package
/// path. Use `RegistryPath::resolve` to select the module's version.
/// Components may not be empty, `.`, `..`, or contain `:` or `\\`.
pub fn parse_install_style_path(input: &str) -> anyhow::Result<RegistryPath> {
    let components = parse_path_components(input)?;
    if components.len() < 2 {
        anyhow::bail!("must be in format `user/module/package`");
    }

    let module = ModuleName {
        username: components[0].into(),
        unqual: components[1].into(),
    };
    let package = components[2..].join("/");

    Ok(RegistryPath {
        module,
        version: None,
        package,
    })
}

/// Parse a binary-install path with an optional version and wildcard suffix.
/// The wildcard is removed before identifying the module and package, so both
/// `user/module...` and `user/module/...` select the module root.
pub fn parse_install_package_path(input: &str) -> anyhow::Result<(RegistryPath, bool)> {
    let (path, version) = split_version(input)?;
    let prefix = path
        .strip_suffix("...")
        .map(|path| path.trim_end_matches('/'));
    let parsed = parse_install_style_path(prefix.unwrap_or(path))?.with_version(version)?;
    parsed.exact_version()?;
    Ok((parsed, prefix.is_some()))
}

/// Parse `username/module@version[/package]`.
///
/// Module and package components may not be empty, `.`, `..`, or contain `:`
/// or `\\`.
pub fn parse_module_at_version_path(input: &str) -> anyhow::Result<RegistryPath> {
    let (module_part, tail) = split_version(input)?;
    let tail =
        tail.ok_or_else(|| anyhow::anyhow!("must be in format `user/module@version/package`"))?;
    let (version, package) = match tail.split_once('/') {
        Some((version, package)) => {
            parse_path_components(package)?;
            (version, package)
        }
        None => (tail, ""),
    };
    let mut parsed = parse_install_style_path(module_part)?;
    if !parsed.package.is_empty() {
        anyhow::bail!("module name must be in format `user/module`");
    }
    parsed.package = package.to_owned();
    parsed.with_version(Some(version))
}

/// Parse `username/module[/package]@version`.
///
/// Module and package components may not be empty, `.`, `..`, or contain `:`
/// or `\\`.
pub fn parse_package_at_version_path(input: &str) -> anyhow::Result<RegistryPath> {
    let (path, version) = split_version(input)?;
    let version = version
        .ok_or_else(|| anyhow::anyhow!("must be in format `user/module/package@version`"))?;
    parse_install_style_path(path)?.with_version(Some(version))
}

/// Parse `username/module[@version][/package]` using the moonbit.import grammar.
pub fn parse_front_matter_import_path(path: &str) -> anyhow::Result<RegistryPath> {
    if path.contains('@') {
        parse_module_at_version_path(path)
    } else {
        parse_install_style_path(path)
    }
}

/// Resolve unversioned import-style registry paths.
pub fn resolve_unversioned_registry_path(
    path: &str,
    mut latest_version_of: impl FnMut(&ModuleName) -> Option<String>,
) -> anyhow::Result<(ModuleName, String, String)> {
    if path.contains('@') {
        anyhow::bail!("explicit versions are not allowed in this registry path");
    }

    if path == MOONBITLANG_CORE || path.starts_with(&format!("{MOONBITLANG_CORE}/")) {
        return Ok((
            MOD_NAME_STDLIB.clone(),
            DEFAULT_VERSION.to_string(),
            path.to_string(),
        ));
    }

    let parsed = parse_install_style_path(path)?;
    let latest_version = latest_version_of(&parsed.module)
        .ok_or_else(|| anyhow::anyhow!("module `{}` not found", parsed.module))?;
    Ok((parsed.module, latest_version, path.to_string()))
}

#[cfg(test)]
mod tests {
    use crate::registry::mock::MockRegistry;

    use super::{
        parse_front_matter_import_path, parse_install_package_path, parse_install_style_path,
        parse_module_at_version_path, parse_module_path, parse_package_at_version_path,
        resolve_unversioned_registry_path,
    };

    #[test]
    fn registry_module_paths_resolve_checked_versions() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("a/b", "1.5.0", [])
            .add_module_full("a/b/tools", "1.0.0", []);
        for (input, name, version, pinned) in [
            ("a/b", "a/b", "1.5.0", false),
            ("a/b@1.0.0", "a/b", "1.0.0", true),
            ("a/b@2.0.0-rc.1", "a/b", "2.0.0-rc.1", true),
            ("a/b/tools", "a/b/tools", "1.0.0", false),
        ] {
            let selector = parse_module_path(input).unwrap();
            assert_eq!(selector.version.is_some(), pinned);
            let module = selector.resolve(&registry).unwrap();
            assert_eq!(module.name().to_string(), name);
            assert_eq!(module.version().to_string(), version);
        }

        // An explicit version needs no registry entry or latest-version lookup.
        let module = parse_module_path("a/b@2.0.0")
            .unwrap()
            .resolve(&MockRegistry::new())
            .unwrap();
        assert_eq!(module.version().to_string(), "2.0.0");
    }

    #[test]
    fn registry_install_paths_resolve_checked_versions() {
        let mut registry = MockRegistry::new();
        registry.add_module_full("a/b", "1.5.0", []);
        for (input, name, version, package) in [
            ("a/b", "a/b", "1.5.0", ""),
            ("a/b@1.0.0", "a/b", "1.0.0", ""),
            ("a/b/cmd/tool", "a/b", "1.5.0", "cmd/tool"),
            ("a/b/cmd/tool@1.0.0", "a/b", "1.0.0", "cmd/tool"),
        ] {
            let (path, wildcard) = parse_install_package_path(input).unwrap();
            let module = path.resolve(&registry).unwrap();
            assert_eq!(module.name().to_string(), name);
            assert_eq!(module.version().to_string(), version);
            assert_eq!(path.package, package);
            assert!(!wildcard);
        }
    }

    #[test]
    fn registry_paths_reject_invalid_names_and_versions() {
        for input in [
            "a",
            "a//b",
            "a/b/../c",
            "a/b#x",
            "a/b@",
            "a/b@invalid",
            "a/b@latest",
            "a/b@1.0.0@2.0.0",
        ] {
            assert!(parse_module_path(input).is_err(), "accepted {input}");
            assert!(
                parse_install_package_path(input).is_err(),
                "accepted {input}"
            );
        }
        let selector = parse_module_path("a/b").unwrap();
        assert_eq!(
            selector
                .resolve(&MockRegistry::new())
                .unwrap_err()
                .to_string(),
            "Module `a/b` not found in registry"
        );
    }

    #[test]
    fn install_wildcards_are_separated_before_identifying_the_module() {
        let mut registry = MockRegistry::new();
        registry.add_module_full("a/b", "1.5.0", []);
        for (input, name, package, version) in [
            ("a/b/...", "a/b", "", "1.5.0"),
            ("a/b...", "a/b", "", "1.5.0"),
            ("a/b/cmd/...@1.0.0", "a/b", "cmd", "1.0.0"),
        ] {
            let (path, wildcard) = parse_install_package_path(input).unwrap();
            assert!(wildcard, "{input}");
            assert_eq!(path.package, package);
            let module = path.resolve(&registry).unwrap();
            assert_eq!(module.name().to_string(), name);
            assert_eq!(module.version().to_string(), version);
        }
        for input in [
            "a/b/...@invalid",
            "a/b/...@1.0.0@2.0.0",
            "a/b@1.0.0/cmd/...",
        ] {
            assert!(
                parse_install_package_path(input).is_err(),
                "accepted {input}"
            );
        }
    }

    #[test]
    fn version_placement_preserves_the_module_boundary() {
        let registry = MockRegistry::new();
        for (input, name, package) in [
            ("a/b@1.0.0", "a/b", ""),
            ("a/b@1.0.0/cmd", "a/b", "cmd"),
            ("a/b@1.0.0/v2/cmd", "a/b", "v2/cmd"),
        ] {
            let path = parse_module_at_version_path(input).unwrap();
            assert_eq!(parse_front_matter_import_path(input).unwrap(), path);
            assert_eq!(path.resolve(&registry).unwrap().name().to_string(), name);
            assert_eq!(path.package, package);
        }
        let path = parse_module_at_version_path("a/b@version/cmd").unwrap();
        assert_eq!(path.version.as_deref(), Some("version"));
        assert!(path.resolve(&registry).is_err());
    }

    #[test]
    fn parse_module_at_version_path_supports_package_suffix() {
        let parsed = parse_module_at_version_path("moonbitlang/x@0.4.39/fs/path").unwrap();
        assert_eq!(parsed.module.to_string(), "moonbitlang/x");
        assert_eq!(parsed.version.as_deref(), Some("0.4.39"));
        assert_eq!(parsed.package, "fs/path");
        assert_eq!(parsed.full_path_without_version(), "moonbitlang/x/fs/path");
    }

    #[test]
    fn parse_module_at_version_path_rejects_three_segment_module() {
        assert!(parse_module_at_version_path("moonbitlang/x/fs@0.4.39/path").is_err());
    }

    #[test]
    fn parse_package_at_version_path_supports_package_version_suffix() {
        let parsed = parse_package_at_version_path("moonbitlang/x/fs/path@0.4.39").unwrap();
        assert_eq!(parsed.module.to_string(), "moonbitlang/x");
        assert_eq!(parsed.version.as_deref(), Some("0.4.39"));
        assert_eq!(parsed.package, "fs/path");
    }

    #[test]
    fn parse_package_at_version_path_rejects_module_version_package_suffix() {
        assert!(parse_package_at_version_path("moonbitlang/x@0.4.39/fs/path").is_err());
    }

    #[test]
    fn registry_path_parsers_reject_invalid_components() {
        for path in [
            "user/module/.",
            "user/module/..",
            "user#/module/package",
            "user/module?/package",
            "user/module/package%",
            "C:/module/package",
            r"user/module/a\..\..\evil",
        ] {
            assert!(parse_install_style_path(path).is_err(), "accepted {path}");
        }
        assert!(parse_module_at_version_path(r"user/module@1.2.3/a\..\..\evil").is_err());
        assert!(parse_package_at_version_path(r"user/module/a\..\..\evil@1.2.3").is_err());
    }

    #[test]
    fn resolve_unversioned_registry_path_uses_first_two_segments() {
        let resolved = resolve_unversioned_registry_path("a/b/c/d", |module| {
            (module.to_string() == "a/b").then(|| "1.0.0".to_string())
        })
        .unwrap();
        assert_eq!(resolved.0.to_string(), "a/b");
        assert_eq!(resolved.1, "1.0.0");
        assert_eq!(resolved.2, "a/b/c/d");
    }
}
