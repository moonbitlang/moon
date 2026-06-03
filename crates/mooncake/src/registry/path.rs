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
    if components.len() < 2 {
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

/// Parse `username/module[@version][/package]` using the moonbit.import grammar.
pub fn parse_front_matter_import_path(path: &str) -> anyhow::Result<FrontMatterImportPath> {
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

/// Resolve import-style registry paths using the current registry resolver
/// behavior.
///
/// This preserves the existing permissive module-name behavior for explicit
/// version paths and the existing longest-prefix lookup for unversioned paths.
pub fn resolve_registry_path(
    path: &str,
    allow_explicit_version: bool,
    mut latest_version_of: impl FnMut(&ModuleName) -> Option<String>,
) -> Option<(ModuleName, String, String)> {
    let contains_at = path.contains('@');

    if path.starts_with(&format!("{MOONBITLANG_CORE}@"))
        || contains_at && path.starts_with(&format!("{MOONBITLANG_CORE}/"))
    {
        return None;
    }

    if path == MOONBITLANG_CORE || !contains_at && path.starts_with(&format!("{MOONBITLANG_CORE}/"))
    {
        return Some((
            MOD_NAME_STDLIB.clone(),
            DEFAULT_VERSION.to_string(),
            path.to_string(),
        ));
    }

    match (allow_explicit_version, contains_at) {
        (true, true) => {
            let (module_name, tail) = path.rsplit_once('@')?;
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
                package => format!("{module_name_str}/{package}"),
            };
            Some((module_name, version.to_string(), full_path_without_version))
        }
        (false, true) => None,
        (_, false) => {
            let segments = path.split('/').collect::<Vec<_>>();
            if segments.len() < 2 || segments.iter().any(|segment| segment.is_empty()) {
                return None;
            }

            for segment_count in (2..=segments.len()).rev() {
                let candidate_str = segments[..segment_count].join("/");
                let candidate = candidate_str.parse::<ModuleName>().ok()?;
                if candidate.username.is_empty() {
                    return None;
                }
                if let Some(latest_version) = latest_version_of(&candidate) {
                    return Some((candidate, latest_version, path.to_string()));
                }
            }
            None
        }
    }
}
