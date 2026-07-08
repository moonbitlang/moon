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
    collections::HashSet,
    ffi::OsStr,
    fs::File,
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use indexmap::IndexMap;
use serde::Serialize;

use crate::{
    constants::{DEP_PATH, MOON_MOD, MOON_MOD_JSON, MOON_PKG, MOON_PKG_JSON},
    moon_pkg,
    package::{
        convert_pkg_dsl_to_package_with_supported_targets_decl,
        convert_pkg_json_to_package_with_supported_targets_decl,
    },
};

pub use crate::module::{
    ModuleDBJSON, MoonMod, MoonModJSON, MoonModJSONRules, MoonModRule, convert_module_to_mod_json,
};
pub use crate::package::{MoonPkg, MoonPkgJSON, SupportedTargetsDeclKind};

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("`source` should not contain invalid chars `{0:?}`")]
    ContainInvalidChars(Vec<char>),
    #[error("`source` not a subdirectory of the parent directory")]
    NotSubdirectory,
}

fn is_valid_folder_name(folder_name: &str) -> Result<(), SourceError> {
    let invalid_chars = ['<', '>', ':', '"', '|', '?', '*'];
    let invalid: Vec<char> = folder_name
        .chars()
        .filter(|c| invalid_chars.contains(c))
        .collect();
    if !invalid.is_empty() {
        return Err(SourceError::ContainInvalidChars(invalid));
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum NameError {
    #[error("`name` should not be empty")]
    EmptyName,
}

#[derive(Debug, thiserror::Error)]
#[error("failed to load `{}`", path.display())]
pub struct MoonModJSONFormatError {
    path: Box<Path>,
    #[source]
    kind: MoonModJSONFormatErrorKind,
}

// moonfmt's mod_json input expects each DSL rule call as a repeated top-level
// `rule` key rather than a JSON array, so this wrapper only customizes the
// serialization used when writing moon.mod DSL.
struct MoonfmtModJsonInput<'a>(&'a MoonModJSON);

impl Serialize for MoonfmtModJsonInput<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::{Error, SerializeMap};

        let mut value = serde_json_lenient::to_value(self.0).map_err(Error::custom)?;
        let serde_json_lenient::Value::Object(map) = &mut value else {
            unreachable!("MoonModJSON should serialize to an object");
        };
        let rules = map.remove("rule");
        let mut output = serializer.serialize_map(None)?;
        for (key, value) in map.iter() {
            output.serialize_entry(key, value)?;
        }
        match rules {
            Some(serde_json_lenient::Value::Array(rules)) => {
                for rule in rules {
                    output.serialize_entry("rule", &rule)?;
                }
            }
            Some(rule) => output.serialize_entry("rule", &rule)?,
            None => {}
        }
        output.end()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MoonModJSONFormatErrorKind {
    #[error("I/O error")]
    IO(#[from] std::io::Error),
    #[error("Parse error")]
    Parse(#[from] serde_json_lenient::Error),
    #[error("`name` bad format")]
    Name(#[from] NameError),
    #[error("`source` bad format")]
    Source(#[from] SourceError),
    #[error("`version` bad format")]
    Version(#[from] semver::Error),
    #[error("`preferred-backend` is not a valid backend")]
    PreferredBackend(anyhow::Error),
    #[error("`supported_targets` bad format")]
    SupportedTargets(anyhow::Error),
}

pub fn validate_module_dsl_deps(
    deps: Option<&IndexMap<String, crate::dependency::SourceDependencyInfo>>,
) -> anyhow::Result<()> {
    use crate::dependency::SourceDependencyInfo;

    for (name, dep) in deps.into_iter().flatten() {
        match dep {
            SourceDependencyInfo::Simple(_) => {}
            SourceDependencyInfo::Registry(info) if info.version.is_some() => {}
            SourceDependencyInfo::Registry(_) => {
                bail!(
                    "moon.mod only supports versioned registry dependencies in `import`, found `{}`",
                    name
                );
            }
            SourceDependencyInfo::Local(_) => {
                bail!(
                    "moon.mod does not support local dependency `{}` in `import`; use workspace configuration in `moon.work` instead. See https://docs.moonbitlang.com/en/latest/toolchain/moon/module.html#dependency-management",
                    name
                );
            }
            SourceDependencyInfo::Git(_) => {
                bail!(
                    "moon.mod only supports registry dependencies in `import`, found structured dependency `{}`",
                    name
                );
            }
        }
    }

    Ok(())
}

pub fn read_module_from_dsl(path: &Path) -> anyhow::Result<MoonMod> {
    let file = File::open(path).with_context(|| format!("failed to load `{}`", path.display()))?;
    let contents = std::io::read_to_string(file)
        .with_context(|| format!("failed to load `{}`", path.display()))?;

    let dsl = moon_pkg::parse(&contents)
        .with_context(|| format!("failed to load `{}`", path.display()))?;
    // Top-level DSL keys accepted in `moon.mod`; the boolean says whether
    // repeated entries should be collected as a JSON array instead of rejected.
    let toplevel_keys = std::collections::HashMap::from([
        ("import", false),
        ("options", false),
        ("warnings", false),
        ("name", false),
        ("version", false),
        ("rule", true),
        // metadata for mooncakes.io
        ("readme", false),
        ("repository", false),
        ("license", false),
        ("keywords", false),
        ("description", false),
        ("source", false),
        ("supported_targets", false),
        ("preferred_target", false),
    ]);
    let mut map = serde_json_lenient::Map::new();
    for (key, value) in dsl.iter() {
        let Some(&allow_duplicate) = toplevel_keys.get(key) else {
            bail!("Unexpected key '{}' found in moon.mod.", key);
        };
        if allow_duplicate {
            match map
                .entry(key.to_string())
                .or_insert_with(|| serde_json_lenient::Value::Array(Vec::new()))
            {
                serde_json_lenient::Value::Array(values) => values.push(value.clone()),
                _ => unreachable!("duplicate key should be initialized as array"),
            }
            continue;
        }
        if map.insert(key.to_string(), value.clone()).is_some() {
            bail!("Duplicate key '{}' found in moon.mod.", key);
        }
    }

    if let serde_json_lenient::Value::Object(options) = map.remove("options").unwrap_or_default() {
        for (key, value) in options {
            if map.contains_key(&key) {
                bail!("Duplicate key '{}' found in moon.mod.", key);
            }
            map.insert(key, value);
        }
    }

    if let Some(warnings) = map.remove("warnings") {
        let warnings = match warnings {
            serde_json_lenient::Value::String(s) => s,
            _ => String::new(),
        };
        let legacy_warn_list = match map.remove("warn-list") {
            Some(serde_json_lenient::Value::String(s)) => s,
            _ => String::new(),
        };
        let merged = format!("{warnings}{legacy_warn_list}");
        if !merged.is_empty() {
            map.insert(
                String::from("warn-list"),
                serde_json_lenient::Value::String(merged),
            );
        }
    }
    let rule = if let Some(rules) = map.remove("rule") {
        let rules: Vec<MoonModRule> = serde_json_lenient::from_value(rules)?;
        let mut names = HashSet::new();
        for rule in &rules {
            if !names.insert(rule.name.as_str()) {
                bail!("Duplicate rule name `{}` found in moon.mod.", rule.name);
            }
        }
        Some(rules)
    } else {
        None
    };

    if let Some(preferred_target) = map.remove("preferred_target") {
        map.insert(String::from("preferred-target"), preferred_target);
    }

    let supported_targets = map.remove("supported_targets");
    let legacy_supported_targets = map.remove("supported-targets");
    match (supported_targets, legacy_supported_targets) {
        (Some(supported_targets), Some(_)) => {
            map.insert(String::from("supported-targets"), supported_targets);
        }
        (Some(supported_targets), None) => {
            map.insert(String::from("supported-targets"), supported_targets);
        }
        (None, Some(legacy_supported_targets)) => {
            map.insert(String::from("supported-targets"), legacy_supported_targets);
        }
        (None, None) => {}
    }

    let imports = map.remove("import");
    let mut deps = match map.remove("deps") {
        Some(serde_json_lenient::Value::Object(deps)) => deps,
        Some(_) => bail!("`deps` in `moon.mod` must be an object"),
        _ => serde_json_lenient::Map::new(),
    };

    // Invariant: The moon_pkg parser lowers `import {}` to a JSON array.
    if let Some(serde_json_lenient::Value::Array(imports)) = imports {
        for item in imports {
            let spec = match item {
                serde_json_lenient::Value::String(spec) => spec,
                serde_json_lenient::Value::Object(_) => {
                    bail!("\"xxx\"@pkg is not supported in moon.mod");
                }
                _ => {
                    // Invariant: `moon_pkg::parse` only produces string or object entries for `import`.
                    unreachable!("unexpected non-string import entry");
                }
            };
            if let Some((name, version)) = spec.rsplit_once('@')
                && !name.is_empty()
                && !version.is_empty()
            {
                deps.insert(
                    name.to_string(),
                    serde_json_lenient::Value::String(version.to_string()),
                );
            } else {
                bail!(
                    "moon.mod only supports versioned registry dependencies in `import`, found `{}`",
                    spec
                );
            }
        }
    };

    // Convert `import {}` in `moon.mod` to `deps`. This differs from `moon.pkg`'s import configuration.
    map.insert(
        String::from("deps"),
        serde_json_lenient::Value::Object(deps),
    );

    let json = serde_json_lenient::Value::Object(map);

    let j: MoonModJSON = serde_json_lenient::from_value(json)
        .with_context(|| format!("failed to load `{}`", path.display()))?;
    validate_module_dsl_deps(j.deps.as_ref())?;

    if let Some(src) = &j.source {
        is_valid_folder_name(src).map_err(anyhow::Error::new)?;
        if src.starts_with('/') || src.starts_with('\\') {
            return Err(anyhow::Error::new(SourceError::NotSubdirectory));
        }
    }
    let mut module: MoonMod = j.try_into().map_err(anyhow::Error::new)?;
    module.rule = rule;
    Ok(module)
}

pub fn read_module_from_json(path: &Path) -> Result<MoonMod, MoonModJSONFormatError> {
    let file = File::open(path).map_err(|e| MoonModJSONFormatError {
        path: path.into(),
        kind: MoonModJSONFormatErrorKind::IO(e),
    })?;
    let reader = BufReader::new(file);
    let j: MoonModJSON =
        serde_json_lenient::from_reader(reader).map_err(|e| MoonModJSONFormatError {
            path: path.into(),
            kind: MoonModJSONFormatErrorKind::Parse(e),
        })?;

    if let Some(src) = &j.source {
        is_valid_folder_name(src).map_err(|e| MoonModJSONFormatError {
            path: path.into(),
            kind: MoonModJSONFormatErrorKind::Source(e),
        })?;
        if src.starts_with('/') || src.starts_with('\\') {
            return Err(MoonModJSONFormatError {
                path: path.into(),
                kind: MoonModJSONFormatErrorKind::Source(SourceError::NotSubdirectory),
            });
        }
    }
    j.try_into().map_err(|e| MoonModJSONFormatError {
        path: path.into(),
        kind: e,
    })
}

fn read_package_from_json_with_supported_targets_decl(
    path: &Path,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let j = serde_json_lenient::from_reader(reader).context(format!("Failed to parse {path:?}"))?;
    let emit_warnings = should_warn_manifest(path);
    convert_pkg_json_to_package_with_supported_targets_decl(j, emit_warnings)
}

/// Reads a moon.pkg from the given path.
fn read_package_from_dsl_with_supported_targets_decl(
    path: &Path,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    let file = File::open(path)?;
    let str = std::io::read_to_string(file)?;
    let dsl = moon_pkg::parse(&str)?;
    let emit_warnings = should_warn_manifest(path);
    convert_pkg_dsl_to_package_with_supported_targets_decl(dsl, emit_warnings)
}

/// Avoid emitting manifest warnings for dependency cache files in .mooncakes.
fn should_warn_manifest(path: &Path) -> bool {
    !path
        .components()
        .any(|component| component.as_os_str() == DEP_PATH)
}

#[derive(Debug, Clone, Copy)]
pub enum ManifestFormat {
    New,
    Legacy,
}

pub fn preferred_manifest_in_dir(
    dir: &Path,
    new_manifest: &'static str,
    legacy_manifest: &'static str,
) -> Option<(PathBuf, ManifestFormat)> {
    let new_path = dir.join(new_manifest);
    let legacy_path = dir.join(legacy_manifest);
    match (new_path.exists(), legacy_path.exists()) {
        (true, _) => Some((new_path, ManifestFormat::New)),
        (false, true) => Some((legacy_path, ManifestFormat::Legacy)),
        (false, false) => None,
    }
}

pub fn warn_if_shadowed_manifest(
    dir: &Path,
    legacy_manifest: &'static str,
    new_manifest: &'static str,
    location: &str,
) {
    if dir.join(new_manifest).exists() && dir.join(legacy_manifest).exists() {
        warn_known_shadowed_manifest(dir, legacy_manifest, new_manifest, location);
    }
}

pub fn warn_known_shadowed_manifest(
    dir: &Path,
    legacy_manifest: &'static str,
    new_manifest: &'static str,
    location: &str,
) {
    if !should_warn_manifest(dir) {
        return;
    }

    eprintln!(
        "Warning: Both {legacy_manifest} and {new_manifest} exist {location}, using the new format {new_manifest}. Please remove the deprecated {legacy_manifest}."
    );
}

pub fn write_module_json_to_file(m: &MoonModJSON, source_dir: &Path) -> anyhow::Result<()> {
    let p = source_dir.join(MOON_MOD_JSON);
    let file = File::create(p)?;
    let mut writer = BufWriter::new(file);
    serde_json_lenient::to_writer_pretty(&mut writer, &m)?;
    Ok(())
}

pub fn write_module_dsl_to_file(m: &MoonModJSON, source_dir: &Path) -> anyhow::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    validate_module_dsl_deps(m.deps.as_ref())?;

    let input = if m.rule.is_some() {
        serde_json_lenient::to_string_pretty(&MoonfmtModJsonInput(m))?
    } else {
        serde_json_lenient::to_string_pretty(m)?
    };
    let mut child = Command::new(&*crate::binaries::BINARIES.moonfmt)
        .arg("-file-type")
        .arg("mod_json")
        .arg("-")
        .arg("-o")
        .arg(source_dir.join(MOON_MOD))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run moonfmt for moon.mod generation")?;

    let mut stdin = child
        .stdin
        .take()
        .context("failed to open moonfmt stdin for moon.mod generation")?;
    let write_result = stdin.write_all(input.as_bytes());
    drop(stdin);
    if let Err(err) = write_result {
        let _ = child.wait();
        return Err(err).context("failed to write moon.mod.json to moonfmt stdin");
    }

    let output = child
        .wait_with_output()
        .context("failed to run moonfmt for moon.mod generation")?;
    if !output.status.success() {
        bail!(
            "failed to write moon.mod: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

pub fn write_package_json_to_file(pkg: &MoonPkgJSON, path: &Path) -> anyhow::Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    serde_json_lenient::to_writer_pretty(&mut writer, &pkg)?;
    Ok(())
}

pub fn read_module_desc_file_in_dir(dir: &Path) -> anyhow::Result<MoonMod> {
    match preferred_manifest_in_dir(dir, MOON_MOD, MOON_MOD_JSON) {
        Some((path, ManifestFormat::New)) => read_module_from_dsl(&path),
        Some((path, ManifestFormat::Legacy)) => Ok(read_module_from_json(&path)?),
        None => bail!(
            "Failed to find `{}` or `{}` for module at path `{}`",
            MOON_MOD,
            MOON_MOD_JSON,
            dir.display()
        ),
    }
}

pub fn read_package_desc_file_in_dir(dir: &Path) -> anyhow::Result<MoonPkg> {
    Ok(read_package_desc_file_in_dir_with_supported_targets_decl(dir)?.0)
}

pub fn read_package_desc_file_in_dir_with_supported_targets_decl(
    dir: &Path,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    match preferred_manifest_in_dir(dir, MOON_PKG, MOON_PKG_JSON) {
        Some((path, _)) => read_package_desc_file_from_path_with_supported_targets_decl(&path),
        None => bail!(
            "Failed to find `{}` or `{}` for package at path `{}`",
            MOON_PKG,
            MOON_PKG_JSON,
            dir.display()
        ),
    }
}

pub fn read_package_desc_file_from_path_with_supported_targets_decl(
    path: &Path,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    match path.file_name() {
        Some(filename) if filename == OsStr::new(MOON_PKG) => {
            read_package_from_dsl_with_supported_targets_decl(path)
        }
        Some(filename) if filename == OsStr::new(MOON_PKG_JSON) => {
            read_package_from_json_with_supported_targets_decl(path)
                .context(format!("Failed to load {:?}", path))
        }
        _ => bail!(
            "Unsupported package manifest path `{}`; expected `{}` or `{}`",
            path.display(),
            MOON_PKG,
            MOON_PKG_JSON
        ),
    }
}
