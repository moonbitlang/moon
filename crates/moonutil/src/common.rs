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

use crate::cond_expr::{CompileCondition, OptLevel};
use crate::module::{MoonMod, MoonModJSON, MoonModRule};
use crate::moon_pkg;
use crate::mooncakes::ModuleName;
use crate::package::{
    MoonPkg, MoonPkgJSON, SupportedTargetsDeclKind,
    convert_pkg_dsl_to_package_with_supported_targets_decl,
    convert_pkg_json_to_package_with_supported_targets_decl,
};
use anyhow::{Context, bail};
use clap::ValueEnum;
use fs4::fs_std::FileExt;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::hash::Hash;
use std::io::ErrorKind;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub const MOON_MOD: &str = "moon.mod";
pub const MOON_MOD_JSON: &str = "moon.mod.json";
pub const MOON_PKG_JSON: &str = "moon.pkg.json";
pub const MOON_WORK: &str = "moon.work";
pub const MOON_WORK_ENV: &str = "MOON_WORK";
pub const MOON_NO_WORKSPACE: &str = "MOON_NO_WORKSPACE";
pub const MOON_PKG: &str = "moon.pkg";
pub const MBTI_GENERATED: &str = "pkg.generated.mbti";
pub const MBTI_USER_WRITTEN: &str = "pkg.mbti";
pub const MOONBITLANG_CORE: &str = "moonbitlang/core";
pub const MOONBITLANG_CORE_BUILTIN: &str = "moonbitlang/core/builtin";
pub const MOONBITLANG_CORE_PRELUDE: &str = "moonbitlang/core/prelude";
pub const MOONBITLANG_COVERAGE: &str = "moonbitlang/core/coverage";
pub const MOONBITLANG_ABORT: &str = "moonbitlang/core/abort";

pub static MOD_NAME_STDLIB: ModuleName = ModuleName {
    username: arcstr::literal!("moonbitlang"),
    unqual: arcstr::literal!("core"),
};

pub const MOON_TEST_DELIMITER_BEGIN: &str = "----- BEGIN MOON TEST RESULT -----";
pub const MOON_TEST_DELIMITER_END: &str = "----- END MOON TEST RESULT -----";

pub const MOON_COVERAGE_DELIMITER_BEGIN: &str = "----- BEGIN MOONBIT COVERAGE -----";
pub const MOON_COVERAGE_DELIMITER_END: &str = "----- END MOONBIT COVERAGE -----";

pub const MOON_LOCK: &str = ".moon-lock";

pub const WATCH_MODE_DIR: &str = "watch";

pub const TEST_INFO_FILE: &str = "test_info.json";

pub const WHITEBOX_TEST_PATCH: &str = "_wbtest.json";
pub const BLACKBOX_TEST_PATCH: &str = "_test.json";

pub const DOT_MBT_DOT_MD: &str = ".mbt.md";
pub const DOT_MBTP: &str = ".mbtp";
pub const DOT_MBL: &str = ".mbl";
pub const DOT_MBY: &str = ".mby";

pub const MOON_BIN_DIR: &str = "__moonbin__";

pub const MOONCAKE_BIN: &str = "$mooncake_bin";
pub const MOD_DIR: &str = "$mod_dir";
pub const PKG_DIR: &str = "$pkg_dir";

pub const SINGLE_FILE_TEST_PACKAGE: &str = "moon/test/single";
pub const SINGLE_FILE_TEST_MODULE: &str = "moon/test";

pub const SUB_PKG_POSTFIX: &str = "_sub";

pub const PRELUDE_PROOF_DIR: &str = "prelude_proof";

pub const O_EXT: &str = if cfg!(windows) { "obj" } else { "o" };
#[allow(unused)]
pub const DYN_EXT: &str = if cfg!(windows) {
    "dll"
} else if cfg!(target_os = "macos") {
    "dylib"
} else {
    "so"
};

pub const A_EXT: &str = if cfg!(windows) { "lib" } else { "a" };

pub fn is_moon_pkg_exist(dir: &Path) -> bool {
    dir.join(MOON_PKG).exists() || dir.join(MOON_PKG_JSON).exists()
}

pub fn is_moon_pkg(filename: &str) -> bool {
    filename == MOON_PKG || filename == MOON_PKG_JSON
}

pub fn is_moon_mod_exist(dir: &Path) -> bool {
    dir.join(MOON_MOD).exists() || dir.join(MOON_MOD_JSON).exists()
}

pub fn is_moon_mod(filename: &str) -> bool {
    filename == MOON_MOD || filename == MOON_MOD_JSON
}

pub fn is_moon_work(filename: &str) -> bool {
    filename == MOON_WORK
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageSourceFileKind {
    Mbt,
    MbtMd,
    Mbtp,
    Mbl,
    Mby,
}

pub fn package_source_file_kind(filename: &str) -> Option<PackageSourceFileKind> {
    if filename.ends_with(".mbt") {
        Some(PackageSourceFileKind::Mbt)
    } else if filename.ends_with(DOT_MBT_DOT_MD) {
        Some(PackageSourceFileKind::MbtMd)
    } else if filename.ends_with(DOT_MBTP) {
        Some(PackageSourceFileKind::Mbtp)
    } else if filename.ends_with(DOT_MBL) {
        Some(PackageSourceFileKind::Mbl)
    } else if filename.ends_with(DOT_MBY) {
        Some(PackageSourceFileKind::Mby)
    } else {
        None
    }
}

pub fn is_watch_relevant_project_file(filename: &str) -> bool {
    package_source_file_kind(filename).is_some()
        || is_moon_pkg(filename)
        || is_moon_mod(filename)
        || is_moon_work(filename)
}

#[test]
fn package_source_file_kind_detects_supported_package_inputs() {
    assert_eq!(
        package_source_file_kind("main.mbt"),
        Some(PackageSourceFileKind::Mbt)
    );
    assert_eq!(
        package_source_file_kind("guide.mbt.md"),
        Some(PackageSourceFileKind::MbtMd)
    );
    assert_eq!(
        package_source_file_kind("proof.mbtp"),
        Some(PackageSourceFileKind::Mbtp)
    );
    assert_eq!(
        package_source_file_kind("lexer.mbl"),
        Some(PackageSourceFileKind::Mbl)
    );
    assert_eq!(
        package_source_file_kind("parser.mby"),
        Some(PackageSourceFileKind::Mby)
    );
    assert_eq!(package_source_file_kind("moon.pkg"), None);
}

#[test]
fn watch_relevant_project_file_covers_sources_and_manifests() {
    assert!(is_watch_relevant_project_file("moon.mod"));
    assert!(is_watch_relevant_project_file("moon.mod.json"));
    assert!(is_watch_relevant_project_file("moon.work"));
    assert!(is_watch_relevant_project_file("moon.pkg"));
    assert!(is_watch_relevant_project_file("moon.pkg.json"));
    assert!(is_watch_relevant_project_file("lexer.mbl"));
    assert!(!is_watch_relevant_project_file("README.md"));
}

#[derive(Debug, Deserialize)]
pub struct PatchJSON {
    pub drops: Vec<String>,
    pub patches: Vec<PatchItem>,
}

#[derive(Debug, Deserialize)]
pub struct PatchItem {
    pub name: String,
    pub content: String,
}

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
        if let Some(serde_json_lenient::Value::Array(rules)) = rules {
            for rule in rules {
                output.serialize_entry("rule", &rule)?;
            }
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

fn validate_module_dsl_deps(
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
                    "moon.mod does not support local dependency `{}` in `import`; use workspace configuration in `moon.work` instead",
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
        ("supported_targets", false),
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
    let emit_warnings = should_warn_pkg(path);
    convert_pkg_json_to_package_with_supported_targets_decl(j, emit_warnings)
}

/// Reads a moon.pkg from the given path.
fn read_package_from_dsl_with_supported_targets_decl(
    path: &Path,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    let file = File::open(path)?;
    let str = std::io::read_to_string(file)?;
    let dsl = moon_pkg::parse(&str)?;
    let emit_warnings = should_warn_pkg(path);
    convert_pkg_dsl_to_package_with_supported_targets_decl(dsl, emit_warnings)
}

/// avoid emit warnings for moon.pkg in .mooncakes
fn should_warn_pkg(path: &Path) -> bool {
    !path
        .components()
        .any(|component| component.as_os_str() == DEP_PATH)
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
    let mut child = Command::new(&*crate::BINARIES.moonfmt)
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
    let mod_path = dir.join(MOON_MOD);
    if mod_path.exists() {
        return read_module_from_dsl(&mod_path);
    }

    let mod_json_path = dir.join(MOON_MOD_JSON);
    if !mod_json_path.exists() {
        bail!(
            "Failed to find `{}` or `{}` for module at path `{}`",
            MOON_MOD,
            MOON_MOD_JSON,
            dir.display()
        );
    }
    Ok(read_module_from_json(&mod_json_path)?)
}

pub fn read_package_desc_file_in_dir(dir: &Path) -> anyhow::Result<MoonPkg> {
    Ok(read_package_desc_file_in_dir_with_supported_targets_decl(dir)?.0)
}

pub fn read_package_desc_file_in_dir_with_supported_targets_decl(
    dir: &Path,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    if dir.join(MOON_PKG).exists() {
        read_package_from_dsl_with_supported_targets_decl(&dir.join(MOON_PKG))
    } else if dir.join(MOON_PKG_JSON).exists() {
        read_package_from_json_with_supported_targets_decl(&dir.join(MOON_PKG_JSON))
            .context(format!("Failed to load {:?}", dir.join(MOON_PKG_JSON)))
    } else {
        bail!(
            "Failed to find `{}` or `{}` for package at path `{}`",
            MOON_PKG,
            MOON_PKG_JSON,
            dir.display()
        );
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
#[repr(u8)]
pub enum OutputFormat {
    #[default]
    Wat,
    Wasm,
    Js,
    Native,
    LLVM,
}

impl OutputFormat {
    pub fn to_str(&self) -> &str {
        match self {
            OutputFormat::Wat => "wat",
            OutputFormat::Wasm => "wasm",
            OutputFormat::Js => "js",
            OutputFormat::Native => "c",
            OutputFormat::LLVM => O_EXT,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, ValueEnum)]
pub enum SurfaceTarget {
    Wasm,
    WasmGC,
    Js,
    Native,
    LLVM,
    All,
}

pub fn lower_surface_targets(st: &[SurfaceTarget]) -> Vec<TargetBackend> {
    let mut result = std::collections::HashSet::new();
    for item in st {
        match item {
            SurfaceTarget::Wasm => {
                result.insert(TargetBackend::Wasm);
            }
            SurfaceTarget::WasmGC => {
                result.insert(TargetBackend::WasmGC);
            }
            SurfaceTarget::Js => {
                result.insert(TargetBackend::Js);
            }
            SurfaceTarget::Native => {
                result.insert(TargetBackend::Native);
            }
            SurfaceTarget::LLVM => {
                result.insert(TargetBackend::LLVM);
            }
            SurfaceTarget::All => {
                result.insert(TargetBackend::Wasm);
                result.insert(TargetBackend::WasmGC);
                result.insert(TargetBackend::Js);
                result.insert(TargetBackend::Native);
            }
        }
    }
    let mut result: Vec<TargetBackend> = result.into_iter().collect();
    result.sort();
    result
}

#[rustfmt::skip]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Serialize, Deserialize, Default, Hash)]
#[repr(u8)]
pub enum TargetBackend {
    Wasm,
    #[default]
    WasmGC,
    Js,
    Native,
    LLVM
}

impl std::fmt::Display for TargetBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_flag())
    }
}

impl TargetBackend {
    pub fn to_flag(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm-gc",
            Self::Js => "js",
            Self::Native => "native",
            Self::LLVM => "llvm",
        }
    }

    pub fn to_extension(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm",
            Self::Js => "js",
            Self::Native => "exe",
            Self::LLVM => "exe",
        }
    }

    pub fn to_artifact(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm",
            Self::Js => "js",
            Self::Native => "c",
            Self::LLVM => O_EXT,
        }
    }

    pub fn to_dir_name(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm-gc",
            Self::Js => "js",
            Self::Native => "native",
            Self::LLVM => "llvm",
        }
    }

    pub fn to_backend_ext(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm-gc",
            Self::Js => "js",
            Self::Native => "native",
            Self::LLVM => "llvm",
        }
    }

    pub fn str_to_backend(s: &str) -> anyhow::Result<Self> {
        match s {
            "wasm" => Ok(Self::Wasm),
            "wasm-gc" => Ok(Self::WasmGC),
            "js" => Ok(Self::Js),
            "native" => Ok(Self::Native),
            "llvm" => Ok(Self::LLVM),
            _ => bail!(
                "invalid backend: {}, only support wasm, wasm-gc, js, native, llvm",
                s
            ),
        }
    }

    pub fn indexset_to_string(backends: &indexmap::IndexSet<TargetBackend>) -> String {
        let mut backends = backends
            .iter()
            .map(|b| b.to_flag().to_string())
            .collect::<Vec<_>>();
        backends.sort();
        format!("[{}]", backends.join(", "))
    }

    pub fn is_native(self) -> bool {
        match self {
            Self::Native | Self::LLVM => true,
            Self::Wasm | Self::WasmGC | Self::Js => false,
        }
    }

    pub fn is_wasm(self) -> bool {
        match self {
            Self::Wasm | Self::WasmGC => true,
            Self::Js | Self::Native | Self::LLVM => false,
        }
    }

    pub fn allowed_as_project_target(self) -> bool {
        match self {
            Self::Wasm | Self::WasmGC | Self::Js | Self::Native => true,
            Self::LLVM => false,
        }
    }

    pub fn supports_source_map(self) -> bool {
        match self {
            Self::WasmGC | Self::Js => true,
            Self::Wasm | Self::Native | Self::LLVM => false,
        }
    }

    pub fn all() -> &'static [Self] {
        Self::value_variants()
    }
}

#[derive(Debug, Clone)]
pub struct BuildPackageFlags {
    pub debug_flag: bool,
    pub strip_flag: bool,
    pub source_map: bool,
    pub enable_coverage: bool,
    // treat all warnings as errors
    pub deny_warn: bool,
    pub target_backend: TargetBackend,
    pub warn_list: Option<String>,
    pub enable_value_tracing: bool,
}

impl BuildPackageFlags {
    pub fn new() -> Self {
        Self {
            debug_flag: false,
            strip_flag: true,
            source_map: false,
            enable_coverage: false,
            deny_warn: false,
            target_backend: TargetBackend::default(),
            warn_list: None,
            enable_value_tracing: false,
        }
    }
}

impl Default for BuildPackageFlags {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LinkCoreFlags {
    pub debug_flag: bool,
    pub source_map: bool,
    pub output_format: OutputFormat,
    pub target_backend: TargetBackend,
}

impl LinkCoreFlags {
    pub fn new() -> Self {
        Self {
            debug_flag: false,
            source_map: false,
            output_format: OutputFormat::Wasm,
            target_backend: TargetBackend::default(),
        }
    }
}

impl Default for LinkCoreFlags {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct MoonbuildOpt {
    pub source_dir: PathBuf,
    pub raw_target_dir: PathBuf,
    pub target_dir: PathBuf,
    pub test_opt: Option<TestOpt>,
    pub check_opt: Option<CheckOpt>,
    pub build_opt: Option<BuildOpt>,
    pub sort_input: bool,
    pub run_mode: RunMode,
    pub fmt_opt: Option<FmtOpt>,
    pub args: Vec<String>,
    pub verbose: bool,
    pub quiet: bool,
    pub no_render_output: bool,
    pub no_parallelize: bool,
    pub build_graph: bool,
    /// Max parallel tasks to run in n2; `None` to use default
    pub parallelism: Option<usize>,
    pub use_tcc_run: bool,
    pub dynamic_stub_libs: Option<Vec<String>>,
    pub render_no_loc: DiagnosticLevel,
}

#[derive(Debug, Clone)]
pub struct BuildOpt {
    pub install_path: Option<PathBuf>,

    pub filter_package: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CheckOpt {
    pub package_name_filter: Option<String>,
    pub patch_file: Option<PathBuf>,
    pub no_mi: bool,
    pub explain: bool,
}

#[derive(Debug, Clone, Copy, Hash)]
pub struct TestIndexRange {
    pub start: u32,
    pub end: u32,
}

impl TestIndexRange {
    pub fn from_single(index: u32) -> Result<Self, TestIndexRangeParseError> {
        let end = index
            .checked_add(1)
            .ok_or(TestIndexRangeParseError::EndOverflow)?;
        Ok(Self { start: index, end })
    }

    pub fn contains(self, index: u32) -> bool {
        self.start <= index && index < self.end
    }

    pub fn as_range(self) -> std::ops::Range<u32> {
        self.start..self.end
    }
}

#[derive(Debug, thiserror::Error, Clone)]
pub enum TestIndexRangeParseError {
    #[error("index is empty")]
    Empty,
    #[error("missing range start")]
    MissingStart,
    #[error("missing range end")]
    MissingEnd,
    #[error("invalid number `{0}`")]
    InvalidNumber(String),
    #[error("range end must be greater than start")]
    InvalidRange,
    #[error("range end overflows u32")]
    EndOverflow,
}

impl FromStr for TestIndexRange {
    type Err = TestIndexRangeParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let s = input.trim();
        if s.is_empty() {
            return Err(TestIndexRangeParseError::Empty);
        }

        if s.contains("..") {
            return Err(TestIndexRangeParseError::InvalidRange);
        }

        if let Some((start, end)) = s.split_once('-') {
            if end.contains('-') {
                return Err(TestIndexRangeParseError::InvalidRange);
            }
            let start = parse_index_bound(start, TestIndexRangeParseError::MissingStart)?;
            let end = parse_index_bound(end, TestIndexRangeParseError::MissingEnd)?;
            if start >= end {
                return Err(TestIndexRangeParseError::InvalidRange);
            }
            return Ok(Self { start, end });
        }

        let start = parse_index_bound(s, TestIndexRangeParseError::Empty)?;
        TestIndexRange::from_single(start)
    }
}

fn parse_index_bound(
    s: &str,
    empty_error: TestIndexRangeParseError,
) -> Result<u32, TestIndexRangeParseError> {
    if s.is_empty() {
        return Err(empty_error);
    }
    s.parse::<u32>()
        .map_err(|_| TestIndexRangeParseError::InvalidNumber(s.to_string()))
}

#[derive(Debug, Clone)]
pub struct TestOpt {
    pub filter_package: Option<HashSet<String>>,
    pub filter_file: Option<String>,
    pub filter_index: Option<TestIndexRange>,
    pub filter_doc_index: Option<u32>,
    pub limit: u32,
    pub test_failure_json: bool,
    pub display_backend_hint: Option<()>, // use Option to avoid if else
    pub patch_file: Option<PathBuf>,
    /// Glob pattern to filter tests by name
    pub filter_name: Option<String>,
}

#[derive(serde::Serialize, Clone)]
pub struct TestArtifacts {
    pub artifacts_path: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test_filter_args: Vec<String>,
}

#[derive(Debug, Clone, Default, ValueEnum)]
pub enum BlockStyle {
    #[default]
    False,
    True,
}

impl BlockStyle {
    pub fn is_line(&self) -> bool {
        matches!(self, Self::True)
    }
}

#[derive(Debug, Clone)]
pub struct FmtOpt {
    pub check: bool,
    pub block_style: BlockStyle,
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MooncOpt {
    pub build_opt: BuildPackageFlags,
    pub link_opt: LinkCoreFlags,
    pub extra_build_opt: Vec<String>,
    pub extra_link_opt: Vec<String>,
    pub nostd: bool,
    pub json_diagnostics: bool,
    pub single_file: bool,
}

impl Default for MooncOpt {
    fn default() -> Self {
        Self::new()
    }
}

impl MooncOpt {
    pub fn new() -> Self {
        Self {
            build_opt: BuildPackageFlags::new(),
            link_opt: LinkCoreFlags::new(),
            extra_build_opt: vec![],
            extra_link_opt: vec![],
            nostd: false,
            json_diagnostics: true,
            single_file: false,
        }
    }
}

pub(crate) const DEP_PATH: &str = ".mooncakes";

pub const BUILD_DIR: &str = "_build";

pub const IGNORE_DIRS: &[&str] = &[BUILD_DIR, ".git", "node_modules", DEP_PATH];

pub fn dialoguer_ctrlc_handler() {
    // Fix cursor disappears after ctrc+c
    // https://github.com/console-rs/dialoguer/issues/77
    let term = dialoguer::console::Term::stdout();
    let _ = term.show_cursor();
    std::process::exit(1);
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VersionItems {
    pub items: Vec<VersionItem>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VersionItem {
    pub name: String,
    pub version: String,
    pub path: Option<String>,
}

// Copy from https://github.com/rust-lang/cargo/blob/e52e360/crates/cargo-test-support/src/paths.rs#L113
pub trait CargoPathExt {
    fn rm_rf(&self);
}

impl CargoPathExt for Path {
    fn rm_rf(&self) {
        let meta = match self.symlink_metadata() {
            Ok(meta) => meta,
            Err(e) => {
                if e.kind() == ErrorKind::NotFound {
                    return;
                }
                panic!("failed to remove {self:?}, could not read: {e:?}");
            }
        };
        // There is a race condition between fetching the metadata and
        // actually performing the removal, but we don't care all that much
        // for our tests.
        if meta.is_dir() {
            if let Err(e) = fs::remove_dir_all(self) {
                panic!("failed to remove {self:?}: {e:?}")
            }
        } else if let Err(e) = fs::remove_file(self) {
            panic!("failed to remove {self:?}: {e:?}")
        }
    }
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum RunMode {
    Bench,
    Build,
    Check,
    Prove,
    Run,
    Test,
    Bundle,
    Format,
}

impl RunMode {
    pub fn to_dir_name(self) -> &'static str {
        match self {
            Self::Bench => "bench",
            Self::Build | Self::Run => "build",
            Self::Check => "check",
            Self::Prove => "prove",
            Self::Test => "test",
            Self::Bundle => "bundle",
            Self::Format => "format",
        }
    }
}

pub fn backend_filter(
    files_and_con: &IndexMap<PathBuf, CompileCondition>,
    debug_flag: bool,
    target_backend: TargetBackend,
) -> Vec<std::path::PathBuf> {
    files_and_con
        .iter()
        .filter_map(|(file, cond)| {
            if cond.eval(OptLevel::from_debug_flag(debug_flag), target_backend) {
                Some(file)
            } else {
                None
            }
        })
        .cloned()
        .collect()
}

#[test]
fn test_backend_filter() {
    use expect_test::expect;

    let cond = CompileCondition {
        backend: vec![
            TargetBackend::Js,
            TargetBackend::Wasm,
            TargetBackend::WasmGC,
        ],
        optlevel: vec![OptLevel::Debug, OptLevel::Release],
    };

    let files: IndexMap<PathBuf, CompileCondition> = IndexMap::from([
        (PathBuf::from("a.mbt"), cond.clone()),
        (PathBuf::from("a_test.mbt"), cond.clone()),
        (PathBuf::from("b.mbt"), cond.clone()),
        (PathBuf::from("b_test.mbt"), cond.clone()),
        (
            PathBuf::from("x.js.mbt"),
            CompileCondition {
                backend: vec![TargetBackend::Js],
                optlevel: vec![OptLevel::Debug, OptLevel::Release],
            },
        ),
        (
            PathBuf::from("x_test.js.mbt"),
            CompileCondition {
                backend: vec![TargetBackend::Js],
                optlevel: vec![OptLevel::Debug, OptLevel::Release],
            },
        ),
        (
            PathBuf::from("x.wasm.mbt"),
            CompileCondition {
                backend: vec![TargetBackend::Wasm],
                optlevel: vec![OptLevel::Debug, OptLevel::Release],
            },
        ),
        (
            PathBuf::from("x_test.wasm.mbt"),
            CompileCondition {
                backend: vec![TargetBackend::Wasm],
                optlevel: vec![OptLevel::Debug, OptLevel::Release],
            },
        ),
        (
            PathBuf::from("x.wasm-gc.mbt"),
            CompileCondition {
                backend: vec![TargetBackend::WasmGC],
                optlevel: vec![OptLevel::Debug, OptLevel::Release],
            },
        ),
        (
            PathBuf::from("x_test.wasm-gc.mbt"),
            CompileCondition {
                backend: vec![TargetBackend::WasmGC],
                optlevel: vec![OptLevel::Debug, OptLevel::Release],
            },
        ),
    ]);

    expect![[r#"
        [
            "a.mbt",
            "a_test.mbt",
            "b.mbt",
            "b_test.mbt",
            "x.js.mbt",
            "x_test.js.mbt",
        ]
    "#]]
    .assert_debug_eq(&backend_filter(&files, false, TargetBackend::Js));
    expect![[r#"
        [
            "a.mbt",
            "a_test.mbt",
            "b.mbt",
            "b_test.mbt",
            "x.wasm.mbt",
            "x_test.wasm.mbt",
        ]
    "#]]
    .assert_debug_eq(&backend_filter(&files, false, TargetBackend::Wasm));
    expect![[r#"
        [
            "a.mbt",
            "a_test.mbt",
            "b.mbt",
            "b_test.mbt",
            "x.wasm-gc.mbt",
            "x_test.wasm-gc.mbt",
        ]
    "#]]
    .assert_debug_eq(&backend_filter(&files, false, TargetBackend::WasmGC));
}

pub fn get_cargo_pkg_version() -> String {
    env!("CARGO_PKG_VERSION").into()
}

pub fn get_moon_version() -> String {
    format!(
        "{} ({} {})",
        env!("CARGO_PKG_VERSION"),
        env!("VERGEN_GIT_SHA"),
        std::env!("VERGEN_BUILD_DATE")
    )
}

pub fn get_moonc_version() -> anyhow::Result<String> {
    get_program_version_ex(&*crate::BINARIES.moonc, "-v")
}

pub fn get_moonrun_version() -> anyhow::Result<String> {
    get_program_version(&*crate::BINARIES.moonrun)
}

pub fn get_program_version(program: impl AsRef<OsStr>) -> anyhow::Result<String> {
    get_program_version_ex(program, "--version")
}

fn get_program_version_ex(
    program: impl AsRef<OsStr>,
    option: impl AsRef<OsStr>,
) -> anyhow::Result<String> {
    let program = program.as_ref();
    let output = std::process::Command::new(program).arg(option).output();
    match output {
        Ok(output) => {
            if output.status.success() {
                Ok(std::str::from_utf8(&output.stdout)?.trim().to_string())
            } else {
                bail!(
                    "failed to get {program:?} version: {}",
                    std::str::from_utf8(&output.stderr)?
                );
            }
        }
        Err(e) => bail!("failed to get {program:?} version: {e}"),
    }
}

#[test]
fn test_get_version() {
    let v = get_moon_version();
    println!("moon_version: {v}");
    assert!(!v.is_empty());
    let v = get_moonc_version().unwrap();
    println!("moonc_version: {v}");
    assert!(!v.is_empty());
}

pub struct FileLock {
    _file: std::fs::File,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        fs4::fs_std::FileExt::unlock(&self._file).unwrap();
    }
}

impl FileLock {
    pub fn lock(path: &std::path::Path) -> std::io::Result<Self> {
        let file = std::fs::File::create(path.join(MOON_LOCK))?;
        match file.try_lock_exclusive() {
            Ok(_) => Ok(FileLock { _file: file }),
            Err(_) => {
                #[cfg(not(test))]
                eprintln!(
                    "Blocking waiting for file lock {} ...",
                    path.join(MOON_LOCK).display()
                );
                file.lock_exclusive()
                    .map_err(|e| std::io::Error::new(e.kind(), "failed to lock target dir"))?;
                Ok(FileLock { _file: file })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum GeneratedTestDriver {
    InternalTest(PathBuf),
    WhiteboxTest(PathBuf),
    BlackboxTest(PathBuf),
}

#[derive(Debug, ValueEnum, Clone, Copy)]
pub enum DriverKind {
    Internal,
    Whitebox,
    Blackbox,
}

impl std::fmt::Display for DriverKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::Internal => "internal",
            Self::Whitebox => "whitebox",
            Self::Blackbox => "blackbox",
        };
        write!(f, "{kind}")
    }
}

#[derive(Debug, Default, ValueEnum, Clone, PartialEq, Copy, PartialOrd)]
pub enum DiagnosticLevel {
    Info,
    #[value(alias = "warning")]
    Warn,
    #[default]
    Error,
}

impl std::fmt::Display for DiagnosticLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
        })
    }
}

pub const BLACKBOX_TEST_DRIVER: &str = "__generated_driver_for_blackbox_test.mbt";

pub type FileName = String;
pub type TestName = String;
pub type TestBlockIndex = u32;

#[derive(Deserialize, Debug, Clone)]
pub struct MbtTestInfo {
    /// The index of the test block in the file, starting from 0.
    pub index: TestBlockIndex,
    /// The function name of the test block
    pub func: String,
    /// The name of the test block, if any
    pub name: Option<TestName>,
    /// The line number of the definition of the test block, if any
    #[serde(default)]
    pub line_number: Option<usize>,
    /// The attributes of the test block (e.g., #cfg conditions)
    #[serde(default)]
    pub attrs: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MooncGenTestInfo {
    pub no_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    pub with_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    #[serde(default)] // for backward compatibility
    pub with_bench_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    #[serde(default)]
    pub async_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    #[serde(default)]
    pub async_tests_with_args: IndexMap<FileName, Vec<MbtTestInfo>>,
}

impl MbtTestInfo {
    pub fn has_skip(&self) -> bool {
        self.attrs.iter().any(|attr| attr.starts_with("#skip"))
    }
}

impl MooncGenTestInfo {
    /// Convert part of the driver metadata into MoonBit declaraction code for
    /// the test driver to use.
    pub fn section_to_mbt(section: &IndexMap<FileName, Vec<MbtTestInfo>>) -> String {
        use std::fmt::Write;

        let mut result = String::new();
        let default_name = "";

        // Writing to string cannot fail, so unwrap() is safe here.
        writeln!(result, "{{").unwrap();
        for (file, tests) in section {
            writeln!(result, "  \"{file}\": {{").unwrap();
            for test in tests {
                // tests with #skip attribute are also included in the driver, they will
                // eventually be skipped by using cli arguments to the driver executable
                writeln!(
                    result,
                    "    {}: ({}, [\"{}\"]),",
                    test.index,
                    test.func,
                    test.name.as_deref().unwrap_or(default_name)
                )
                .unwrap();
            }
            writeln!(result, "  }},").unwrap();
        }
        writeln!(result, "}}").unwrap();

        result
    }
}

pub fn line_col_to_byte_idx(
    line_index: &line_index::LineIndex,
    line: u32,
    col: u32,
) -> Option<usize> {
    let offset = line_index.offset(line_index.to_utf8(
        line_index::WideEncoding::Utf32,
        line_index::WideLineCol { line, col },
    )?)?;
    Some(usize::from(offset))
}

pub fn get_desc_name(package_name: &str, artifact: &str) -> String {
    if artifact.contains("internal_test") {
        format!("{}_{}", package_name, "internal_test")
    } else if artifact.contains("whitebox_test") {
        format!("{}_{}", package_name, "whitebox_test")
    } else {
        package_name.to_string()
    }
}

pub trait StringExt {
    fn replace_crlf_to_lf(&self) -> String;
}

impl StringExt for str {
    fn replace_crlf_to_lf(&self) -> String {
        self.replace("\r\n", "\n")
    }
}

pub enum PrePostBuild {
    PreBuild,
}

impl PrePostBuild {
    pub fn name(&self) -> String {
        match self {
            PrePostBuild::PreBuild => "pre-build".into(),
        }
    }

    pub fn dbname(&self) -> String {
        format!("{}.db", self.name())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum IgnoredMoonScript {
    Prebuild,
    Postadd,
}

impl IgnoredMoonScript {
    pub fn env_var(self) -> &'static str {
        match self {
            IgnoredMoonScript::Prebuild => "MOON_IGNORE_PREBUILD",
            IgnoredMoonScript::Postadd => "MOON_IGNORE_POSTADD",
        }
    }
}

pub fn is_moon_script_ignored(script: IgnoredMoonScript) -> bool {
    std::env::var_os(script.env_var()).is_some()
}

pub fn execute_postadd_script(dir: &Path) -> anyhow::Result<()> {
    if is_moon_script_ignored(IgnoredMoonScript::Postadd) {
        return Ok(());
    }
    let m = read_module_desc_file_in_dir(dir)?;
    if let Some(scripts) = &m.scripts
        && scripts.contains_key("postadd")
    {
        let postadd = scripts
            .get("postadd")
            .unwrap()
            .split(' ')
            .collect::<Vec<_>>();
        if !postadd.is_empty() {
            let command = postadd[0];
            let args = &postadd[1..];
            let output = std::process::Command::new(command)
                .args(args)
                .current_dir(dir)
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .output()?;
            if !output.status.success() {
                bail!(
                    "failed to execute postadd script in {},\ncommand: {},\n{}",
                    dir.display(),
                    command,
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
pub struct MbtMdHeader {
    pub moonbit: Option<MbtMdSection>,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct MbtMdSection {
    pub deps: Option<IndexMap<String, crate::dependency::SourceDependencyInfo>>,
    pub import: Option<crate::package::PkgJSONImport>,
    pub backend: Option<String>,
}

pub fn parse_front_matter_config(single_file_path: &Path) -> anyhow::Result<Option<MbtMdHeader>> {
    let single_file_string = single_file_path.display().to_string();
    let front_matter_config: Option<MbtMdHeader> = if single_file_string.ends_with(DOT_MBT_DOT_MD) {
        let content = std::fs::read_to_string(single_file_path)?;
        let pattern = regex::Regex::new(r"(?s)^---\s*\n((?:[^\n]+\n)*?)---\s*\n")?;
        if let Some(cap) = pattern.captures(&content) {
            let yaml_content = cap.get(1).unwrap().as_str();
            let config: MbtMdHeader = serde_yaml::from_str(yaml_content).map_err(|e| {
                anyhow::anyhow!("Failed to parse front matter in markdown file: {}", e)
            })?;

            Some(config)
        } else {
            None
        }
    } else {
        None
    };
    Ok(front_matter_config)
}

/// Glob pattern matching supporting `*` (any sequence), `?` (any single character),
/// and other glob patterns. Uses the `globset` crate for robust matching.
pub enum GlobPatternMatcher<'a> {
    Compiled(globset::GlobMatcher),
    Literal(&'a str),
}

impl<'a> GlobPatternMatcher<'a> {
    pub fn new(pattern: &'a str) -> Self {
        use globset::GlobBuilder;
        let glob = GlobBuilder::new(pattern)
            .case_insensitive(false)
            .literal_separator(false)
            .build();
        match glob {
            Ok(glob) => Self::Compiled(glob.compile_matcher()),
            // If pattern is invalid, fall back to literal comparison
            Err(_) => Self::Literal(pattern),
        }
    }

    pub fn is_match(&self, text: &str) -> bool {
        match self {
            Self::Compiled(matcher) => matcher.is_match(text),
            Self::Literal(pattern) => pattern == &text,
        }
    }
}

/// Returns true if the text matches the pattern.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    GlobPatternMatcher::new(pattern).is_match(text)
}
