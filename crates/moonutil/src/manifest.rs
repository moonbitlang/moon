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
    ffi::OsStr,
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json_lenient::{Map, Value};

use crate::{
    constants::{MOON_MOD, MOON_MOD_JSON, MOON_PKG, MOON_PKG_JSON},
    dependency::SourceDependencyInfo,
    module::ModuleValidationError,
    moon_pkg,
    package::{
        convert_pkg_dsl_to_package_with_supported_targets_decl,
        convert_pkg_json_to_package_with_supported_targets_decl,
    },
    target::TargetBackend,
    user_log::UserLog,
};

pub use crate::module::{MoonMod, MoonModJSON, MoonModRule, convert_module_to_mod_json};
pub use crate::package::{MoonPkg, MoonPkgJSON, SupportedTargetsDeclKind};

/// Unknown fields are captured by Serde's `flatten` on each configuration object.
/// Format only their keys: the value of an unknown option is opaque to Moon.
pub(crate) fn unknown_field_paths<'a>(
    fields: &'a serde_json_lenient::Map<String, serde_json_lenient::Value>,
    prefix: &'a str,
) -> impl Iterator<Item = String> + 'a {
    fields.keys().map(move |key| {
        if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        }
    })
}

/// Consumes recognized options; the remaining entries are unknown fields.
pub(crate) struct ManifestOptions {
    pub(crate) fields: Map<String, Value>,
    pub(crate) manifest: &'static str,
}

impl ManifestOptions {
    pub(crate) fn read<T: DeserializeOwned>(
        &mut self,
        name: &str,
        alias: Option<&str>,
    ) -> anyhow::Result<Option<T>> {
        let value = self.fields.remove(name);
        let alias_value = alias.and_then(|alias| self.fields.remove(alias));
        if value.is_some() && alias_value.is_some() {
            bail!("duplicate field `{name}`");
        }
        let key = if value.is_some() {
            name
        } else {
            alias.unwrap_or(name)
        };
        match value.or(alias_value) {
            Some(value) => Option::<T>::deserialize(value)
                .with_context(|| format!("invalid `{key}` in {}", self.manifest)),
            None => Ok(None),
        }
    }
}

pub(crate) fn warn_unknown_fields(unknown: &[String], manifest: &Path, user_log: &UserLog) {
    for field in unknown {
        user_log.warn(format!(
            "Unknown configuration key `{field}` in `{}`.",
            manifest.display()
        ));
    }
}

#[derive(Debug, thiserror::Error)]
#[error("failed to load `{}`", path.display())]
pub struct MoonModJSONFormatError {
    path: Box<Path>,
    #[source]
    kind: MoonModJSONFormatErrorKind,
}

// User-authored JSON and existing tools use the historical field names.
// Keep this conversion at output boundaries; MoonModJSON serialization is internal.
fn legacy_module_json(m: &MoonModJSON) -> serde_json_lenient::Result<Map<String, Value>> {
    let Value::Object(fields) = serde_json_lenient::to_value(m)? else {
        unreachable!("MoonModJSON should serialize to an object");
    };
    let mut fields: Map<String, Value> = fields
        .into_iter()
        .map(|(key, value)| {
            let key = match key.as_str() {
                "import" => "deps".into(),
                "warnings" => "warn-list".into(),
                "preferred_target" => "preferred-target".into(),
                "supported_targets" => "supported-targets".into(),
                _ => key,
            };
            (key, value)
        })
        .collect();
    if let Some(Value::Array(mut rules)) = fields.remove("rule") {
        match rules.len() {
            0 => {}
            1 => {
                fields.insert("rule".into(), rules.pop().unwrap());
            }
            _ => {
                fields.insert("rule".into(), Value::Array(rules));
            }
        }
    }
    Ok(fields)
}

// moonfmt's mod_json protocol represents multiple rules as repeated keys.
// TODO: Remove this wrapper if the supported moonfmt versions accept rule arrays.
struct MoonfmtModJsonInput<'a>(&'a MoonModJSON);

impl Serialize for MoonfmtModJsonInput<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::{Error, SerializeMap};

        let mut map = legacy_module_json(self.0).map_err(Error::custom)?;
        let rules = map.remove("rule");
        let mut output = serializer.serialize_map(None)?;
        for (key, value) in map.iter() {
            output.serialize_entry(key, value)?;
        }
        match rules {
            Some(Value::Array(rules)) => {
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
    #[error(transparent)]
    Validation(#[from] ModuleValidationError),
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
        ("private", false),
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

    if let Some(options) = map.remove("options") {
        let options =
            serde_json_lenient::Map::<String, serde_json_lenient::Value>::deserialize(options)
                .context("`options` in moon.mod must be an object")?;
        for (key, value) in options {
            if map.contains_key(&key) {
                bail!("Duplicate key '{}' found in moon.mod.", key);
            }
            map.insert(key, value);
        }
    }

    // Preserve the module DSL's warning-list concatenation at its input boundary.
    if let Some(warnings) = map.remove("warnings") {
        let warnings =
            String::deserialize(warnings).context("`warnings` in moon.mod must be a string")?;
        let legacy = map
            .remove("warn-list")
            .map(Option::<String>::deserialize)
            .transpose()
            .context("`warn-list` in moon.mod must be a string")?
            .flatten()
            .unwrap_or_default();
        let merged = format!("{warnings}{legacy}");
        if !merged.is_empty() {
            map.insert("warnings".into(), Value::String(merged));
        }
    } else if let Some(legacy) = map.remove("warn-list") {
        map.insert("warnings".into(), legacy);
    }
    for (canonical, legacy) in [
        ("preferred_target", "preferred-target"),
        ("supported_targets", "supported-targets"),
    ] {
        if let Some(value) = map.remove(legacy)
            && map.insert(canonical.to_owned(), value).is_some()
        {
            bail!("Duplicate key '{canonical}' found in moon.mod.");
        }
    }

    // Legacy options(deps: ...) is decoded only here. Both spellings contribute
    // semantic module dependencies; DSL imports keep their existing precedence.
    let mut deps = map
        .remove("deps")
        .map(IndexMap::<String, SourceDependencyInfo>::deserialize)
        .transpose()?
        .unwrap_or_default();
    if let Some(imports) = map.remove("import") {
        for item in
            Vec::<Value>::deserialize(imports).context("`import` in moon.mod must be an array")?
        {
            let spec = match item {
                Value::String(spec) => spec,
                Value::Object(_) => bail!("\"xxx\"@pkg is not supported in moon.mod"),
                _ => bail!("module imports must be versioned dependency strings"),
            };
            let (name, version) = spec.rsplit_once('@')
                .filter(|(name, version)| !name.is_empty() && !version.is_empty())
                .with_context(|| format!(
                    "moon.mod only supports versioned registry dependencies in `import`, found `{spec}`"
                ))?;
            deps.insert(
                name.to_owned(),
                SourceDependencyInfo::Simple(
                    version.parse().map_err(ModuleValidationError::Version)?,
                ),
            );
        }
    }
    validate_module_dsl_deps(Some(&deps))?;

    let mut options = ManifestOptions {
        fields: map,
        manifest: "moon.mod",
    };
    let version: Option<String> = options.read("version", None)?;
    let preferred_target: Option<String> = options.read("preferred_target", None)?;
    let module = MoonMod {
        name: options
            .read("name", None)?
            .context("missing field `name` in moon.mod")?,
        version: version
            .as_deref()
            .map(semver::Version::parse)
            .transpose()
            .map_err(ModuleValidationError::Version)?,
        // Absence is allowed, but an explicit null is not a visibility choice.
        private: options
            .fields
            .remove("private")
            .map(bool::deserialize)
            .transpose()?,
        deps,
        readme: options.read("readme", None)?,
        repository: options.read("repository", None)?,
        license: options.read("license", None)?,
        keywords: options.read("keywords", None)?,
        description: options.read("description", None)?,
        compile_flags: options.read("compile-flags", None)?,
        link_flags: options.read("link-flags", None)?,
        source: options.read("source", Some("root-dir"))?,
        rule: options.read("rule", None)?,
        warn_list: options.read("warnings", None)?,
        scripts: options.read("scripts", None)?,
        preferred_target: preferred_target
            .as_deref()
            .map(TargetBackend::str_to_backend)
            .transpose()
            .map_err(ModuleValidationError::PreferredTarget)?,
        supported_targets: options.read("supported_targets", None)?,
        __moonbit_unstable_prebuild: options.read("--moonbit-unstable-prebuild", None)?,
        ext: Value::Object(options.fields),
    };
    module.validated().map_err(anyhow::Error::new)
}

pub fn read_module_from_json(path: &Path) -> Result<MoonMod, MoonModJSONFormatError> {
    let contents = std::fs::read_to_string(path).map_err(|e| MoonModJSONFormatError {
        path: path.into(),
        kind: MoonModJSONFormatErrorKind::IO(e),
    })?;
    let j: MoonModJSON =
        serde_json_lenient::from_str(&contents).map_err(|e| MoonModJSONFormatError {
            path: path.into(),
            kind: MoonModJSONFormatErrorKind::Parse(e),
        })?;

    j.try_into().map_err(|e| MoonModJSONFormatError {
        path: path.into(),
        kind: MoonModJSONFormatErrorKind::Validation(e),
    })
}

fn read_package_from_json_with_supported_targets_decl(
    path: &Path,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    let contents = std::fs::read_to_string(path)?;
    let j: MoonPkgJSON = serde_json_lenient::from_str(&contents)
        .with_context(|| format!("Failed to parse {path:?}"))?;
    let mut unknown = if emit_warnings {
        j.consumed_unknown_fields()
    } else {
        Vec::new()
    };
    let package =
        convert_pkg_json_to_package_with_supported_targets_decl(j, emit_warnings, user_log)?;
    if emit_warnings {
        unknown.extend(package.0.unknown_fields());
        warn_unknown_fields(&unknown, path, user_log);
    }
    Ok(package)
}

/// Reads a moon.pkg from the given path.
fn read_package_from_dsl_with_supported_targets_decl(
    path: &Path,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    let file = File::open(path)?;
    let str = std::io::read_to_string(file)?;
    let dsl = moon_pkg::parse(&str)?;
    convert_pkg_dsl_to_package_with_supported_targets_decl(dsl, path, emit_warnings, user_log)
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

/// Warn about the selected manifest without reading or parsing it again.
pub fn warn_module_manifest(
    dir: &Path,
    format: ManifestFormat,
    module: &MoonMod,
    user_log: &UserLog,
) {
    let manifest = match format {
        ManifestFormat::New => MOON_MOD,
        ManifestFormat::Legacy => MOON_MOD_JSON,
    };
    let mut unknown: Vec<_> = module
        .ext
        .as_object()
        .into_iter()
        .flat_map(|fields| fields.keys())
        // The resolver still diagnoses removed binary dependencies.
        .filter(|key| key.as_str() != "bin-deps")
        .cloned()
        .collect();
    unknown.extend(
        module
            .scripts
            .iter()
            .flat_map(|scripts| scripts.keys())
            .filter(|name| name.as_str() != "postadd")
            .map(|name| format!("scripts.{name}")),
    );
    for (index, rule) in module.rule.iter().flatten().enumerate() {
        unknown.extend(unknown_field_paths(&rule.extra, &format!("rule[{index}]")));
    }
    warn_unknown_fields(&unknown, &dir.join(manifest), user_log);
    if matches!(format, ManifestFormat::Legacy) {
        return;
    }
    if dir.join(MOON_MOD_JSON).exists() {
        warn_known_shadowed_manifest(
            MOON_MOD_JSON,
            MOON_MOD,
            &format!("at module root '{}'", dir.display()),
            user_log,
        );
    }
}

pub fn warn_known_shadowed_manifest(
    legacy_manifest: &'static str,
    new_manifest: &'static str,
    location: &str,
    user_log: &UserLog,
) {
    user_log.warn(format!(
            "Both {legacy_manifest} and {new_manifest} exist {location}, using the new format {new_manifest}. Please remove the deprecated {legacy_manifest}."
        ));
}

/// Write a module manifest in the user-facing legacy JSON format.
pub fn write_module_json_to_file(m: &MoonModJSON, source_dir: &Path) -> anyhow::Result<()> {
    let json = legacy_module_json(m)?;
    let p = source_dir.join(MOON_MOD_JSON);
    let file = File::create(p)?;
    let mut writer = BufWriter::new(file);
    serde_json_lenient::to_writer_pretty(&mut writer, &json)?;
    Ok(())
}

pub fn write_module_dsl_to_file(m: &MoonModJSON, path: &Path) -> anyhow::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    validate_module_dsl_deps(m.import.as_ref())?;

    let input = serde_json_lenient::to_string_pretty(&MoonfmtModJsonInput(m))?;
    let mut child = Command::new(&*crate::binaries::BINARIES.moonfmt)
        .arg("-file-type")
        .arg("mod_json")
        .arg("-")
        .arg("-o")
        .arg(path)
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
    read_module_desc_file_in_dir_with_legacy(dir, false)
}

pub fn read_module_desc_file_in_dir_with_legacy(
    dir: &Path,
    allow_legacy: bool,
) -> anyhow::Result<MoonMod> {
    Ok(read_module_manifest(dir, allow_legacy)?.1)
}

/// Read a project manifest, retaining its format for warnings and migration.
pub fn read_module_manifest(
    dir: &Path,
    allow_legacy: bool,
) -> anyhow::Result<(ManifestFormat, MoonMod)> {
    let (format, module) = load_module_manifest(dir, allow_legacy)?;
    // Only authors can migrate packaging rules. Dependency filters do not affect
    // this project's archive; rejecting them would break existing consumers.
    for field in ["include", "exclude"] {
        if module.ext.get(field).is_some() {
            let filename = match format {
                ManifestFormat::New => MOON_MOD,
                ManifestFormat::Legacy => MOON_MOD_JSON,
            };
            return Err(ModuleValidationError::RemovedPackagingField(field))
                .with_context(|| format!("failed to load `{}`", dir.join(filename).display()));
        }
    }
    Ok((format, module))
}

/// Read a dependency, allowing obsolete packaging filters from published modules.
/// All ordinary parsing and semantic validation still applies.
pub fn read_dependency_module_manifest(dir: &Path) -> anyhow::Result<MoonMod> {
    Ok(load_module_manifest(dir, false)?.1)
}

fn load_module_manifest(
    dir: &Path,
    allow_legacy: bool,
) -> anyhow::Result<(ManifestFormat, MoonMod)> {
    match preferred_manifest_in_dir(dir, MOON_MOD, MOON_MOD_JSON) {
        Some((path, ManifestFormat::New)) => {
            let module = read_module_from_dsl(&path)?;
            Ok((ManifestFormat::New, module))
        }
        Some((path, ManifestFormat::Legacy)) if allow_legacy => {
            let module = read_module_from_json(&path)?;
            Ok((ManifestFormat::Legacy, module))
        }
        Some((_, ManifestFormat::Legacy)) => bail!(
            "`{MOON_MOD_JSON}` at '{}' is no longer supported. Run `moon fmt` to migrate to `{MOON_MOD}`.",
            dir.display()
        ),
        None => bail!(
            "Failed to find `{}` or `{}` for module at path `{}`",
            MOON_MOD,
            MOON_MOD_JSON,
            dir.display()
        ),
    }
}

pub fn read_package_desc_file_in_dir(
    dir: &Path,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<MoonPkg> {
    Ok(read_package_desc_file_in_dir_with_supported_targets_decl(dir, emit_warnings, user_log)?.0)
}

pub fn read_package_desc_file_in_dir_with_supported_targets_decl(
    dir: &Path,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    match preferred_manifest_in_dir(dir, MOON_PKG, MOON_PKG_JSON) {
        Some((path, _)) => read_package_desc_file_from_path_with_supported_targets_decl(
            &path,
            user_log,
            false,
            emit_warnings,
        ),
        None => bail!(
            "Failed to find `{}` or `{}` for package at path `{}`",
            MOON_PKG,
            MOON_PKG_JSON,
            dir.display()
        ),
    }
}

/// Read the selected package manifest. The caller decides whether author-facing
/// warnings apply; `UserLog` independently controls their visibility.
pub fn read_package_desc_file_from_path_with_supported_targets_decl(
    path: &Path,
    user_log: &UserLog,
    allow_legacy: bool,
    emit_warnings: bool,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    match path.file_name() {
        Some(filename) if filename == OsStr::new(MOON_PKG) => {
            read_package_from_dsl_with_supported_targets_decl(path, emit_warnings, user_log)
        }
        Some(filename) if filename == OsStr::new(MOON_PKG_JSON) => {
            if !allow_legacy {
                bail!(
                    "`{MOON_PKG_JSON}` at '{}' is no longer supported. Run `moon fmt` to migrate to `{MOON_PKG}`.",
                    path.parent().unwrap_or(path).display()
                );
            }
            read_package_from_json_with_supported_targets_decl(path, emit_warnings, user_log)
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
