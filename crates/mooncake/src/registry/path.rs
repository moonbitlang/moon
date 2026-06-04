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

use moonutil::{
    common::{MOD_NAME_STDLIB, MOONBITLANG_CORE},
    mooncakes::{DEFAULT_VERSION, ModuleName},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallStylePath {
    pub module: ModuleName,
    pub package: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedPackagePath {
    pub module: ModuleName,
    pub version: String,
    pub package: String,
}

impl VersionedPackagePath {
    pub fn full_path_without_version(&self) -> String {
        let module = self.module.to_string();
        if self.package.is_empty() {
            module
        } else {
            format!("{module}/{}", self.package)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatterImportPath {
    pub module: String,
    pub version: Option<String>,
    pub package: Option<String>,
}

/// Parse `user/module[/package]` using the install/runwasm interpretation.
///
/// This intentionally preserves the current command behavior: the first two
/// segments are always the module, and later segments are the package path.
/// Callers remain responsible for extra validation such as rejecting empty,
/// `.`/`..`, or drive-letter-like path components.
pub fn parse_install_style_path(input: &str) -> anyhow::Result<InstallStylePath> {
    let components = input.split('/').collect::<Vec<_>>();
    if components.len() < 2 || components.iter().any(|component| component.is_empty()) {
        anyhow::bail!("must be in format `user/module/package`");
    }

    let module = ModuleName {
        username: components[0].into(),
        unqual: components[1].into(),
    };
    let package = if components.len() > 2 {
        components[2..].join("/")
    } else {
        String::new()
    };

    Ok(InstallStylePath { module, package })
}

/// Parse `username/module@version[/package]`.
pub fn parse_module_at_version_path(input: &str) -> anyhow::Result<VersionedPackagePath> {
    if input.matches('@').count() > 1 {
        anyhow::bail!("must contain a single version marker");
    }
    let (module_part, tail) = input
        .split_once('@')
        .ok_or_else(|| anyhow::anyhow!("must be in format `user/module@version/package`"))?;
    let (version, package) = match tail.split_once('/') {
        Some((version, package)) => (version, package),
        None => (tail, ""),
    };
    if version.is_empty() || version.contains('/') {
        anyhow::bail!("version must not be empty or contain path separators");
    }

    let module_components = module_part.split('/').collect::<Vec<_>>();
    if module_components.len() != 2
        || module_components
            .iter()
            .any(|component| component.is_empty())
    {
        anyhow::bail!("module name must be in format `user/module`");
    }
    if !package.is_empty() && package.split('/').any(|component| component.is_empty()) {
        anyhow::bail!("package path must not contain empty components");
    }

    Ok(VersionedPackagePath {
        module: ModuleName {
            username: module_components[0].into(),
            unqual: module_components[1].into(),
        },
        version: version.to_string(),
        package: package.to_string(),
    })
}

/// Parse `username/module[/package]@version`.
pub fn parse_package_at_version_path(input: &str) -> anyhow::Result<VersionedPackagePath> {
    if input.matches('@').count() > 1 {
        anyhow::bail!("must contain a single version marker");
    }
    let (path, version) = input
        .rsplit_once('@')
        .ok_or_else(|| anyhow::anyhow!("must be in format `user/module/package@version`"))?;
    if version.is_empty() || version.contains('/') {
        anyhow::bail!("version must not be empty or contain path separators");
    }

    let parsed = parse_install_style_path(path)?;
    Ok(VersionedPackagePath {
        module: parsed.module,
        version: version.to_string(),
        package: parsed.package,
    })
}

/// Parse `username/module[@version][/package]` using the moonbit.import grammar.
pub fn parse_front_matter_import_path(path: &str) -> anyhow::Result<FrontMatterImportPath> {
    if path.matches('@').count() > 1 {
        anyhow::bail!("import path '{path}' must contain at most one version marker");
    }
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() < 2 {
        anyhow::bail!(
            "import path '{path}' must be in the form 'username/module@version[/package]'"
        );
    }

    let username = parts[0];
    let module_and_version = parts[1];
    let mut module_parts = module_and_version.splitn(2, '@');
    let module = module_parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("import path '{path}' has an empty module name"))?;
    let version = module_parts.next();
    if module.is_empty() {
        anyhow::bail!("import path '{path}' has an empty module name");
    }
    let version = match version {
        Some("") => anyhow::bail!("import path '{path}' has an empty version"),
        Some(v) => Some(v.to_string()),
        None => None,
    };
    if version.is_none() && parts[2..].iter().any(|part| part.contains('@')) {
        anyhow::bail!("import path '{path}' has a version marker outside the module segment");
    }
    let package = if parts.len() > 2 {
        let package = parts[2..].join("/");
        if package.is_empty() {
            anyhow::bail!("import path '{path}' has an empty package path");
        }
        Some(package)
    } else {
        None
    };

    Ok(FrontMatterImportPath {
        module: format!("{username}/{module}"),
        version,
        package,
    })
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
    use super::{
        parse_module_at_version_path, parse_package_at_version_path,
        resolve_unversioned_registry_path,
    };

    #[test]
    fn parse_module_at_version_path_supports_package_suffix() {
        let parsed = parse_module_at_version_path("moonbitlang/x@0.4.39/fs/path").unwrap();
        assert_eq!(parsed.module.to_string(), "moonbitlang/x");
        assert_eq!(parsed.version, "0.4.39");
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
        assert_eq!(parsed.version, "0.4.39");
        assert_eq!(parsed.package, "fs/path");
    }

    #[test]
    fn parse_package_at_version_path_rejects_module_version_package_suffix() {
        assert!(parse_package_at_version_path("moonbitlang/x@0.4.39/fs/path").is_err());
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
