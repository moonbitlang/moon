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

//! Package manifest decoding, compatibility rules, and author diagnostics.
//! The package module owns the data; this module owns how declarations become it.

use std::{collections::HashSet, fs::File, path::Path};

use anyhow::{Context, bail};
use serde::Deserialize;
use serde_json_lenient::{Map, Value};

use super::{ManifestOptions, unknown_field_paths, warn_unknown_fields};
use crate::{
    module::MoonModRule,
    moon_pkg,
    package::{
        BoolOrLink, Import, Link, MoonPkg, MoonPkgFormatterJSON, MoonPkgGenerate, MoonPkgJSON,
        PackageKind, PkgJSONImport, PkgJSONImportItem, PkgType, SupportedTargetsDeclKind,
        pkg_json_imports_to_imports,
    },
    supported_targets::resolve_supported_targets,
    target::TargetBackend,
    user_log::UserLog,
};

#[cfg(test)]
mod tests;

/// Parse a `moon.pkg` declaration without accessing the filesystem.
///
/// `path` identifies the source in diagnostics; it need not exist. The caller
/// decides whether author-facing warnings apply, while `user_log` controls their
/// visibility and capture, as for file loading.
pub fn parse_package(
    source: &str,
    path: &Path,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<MoonPkg> {
    Ok(convert_pkg_dsl_to_package_with_supported_targets_decl(
        moon_pkg::parse(source)?,
        path,
        emit_warnings,
        user_log,
    )?
    .0)
}

pub(super) fn read_package_from_json_with_supported_targets_decl(
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
pub(super) fn read_package_from_dsl_with_supported_targets_decl(
    path: &Path,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    let file = File::open(path)?;
    let str = std::io::read_to_string(file)?;
    let dsl = moon_pkg::parse(&str)?;
    convert_pkg_dsl_to_package_with_supported_targets_decl(dsl, path, emit_warnings, user_log)
}

impl BoolOrLink {
    fn into_link(self) -> Option<Link> {
        match self {
            Self::Bool(_) => None,
            Self::Link(link) => Some(*link),
        }
    }
}

impl MoonPkgJSON {
    /// Diagnostics for fields that the JSON-to-package conversion consumes.
    fn consumed_unknown_fields(&self) -> Vec<String> {
        let mut unknown: Vec<_> = unknown_field_paths(&self.extra, "").collect();
        if let Some(formatter) = &self.formatter {
            unknown.extend(unknown_field_paths(&formatter.extra, "formatter"));
        }
        if let Some(pkgtype) = &self.pkgtype {
            unknown.extend(unknown_field_paths(&pkgtype.extra, "pkgtype"));
        }
        for (name, imports) in [
            ("import", &self.import),
            ("wbtest-import", &self.wbtest_import),
            ("test-import", &self.test_import),
        ] {
            if let Some(imports) = imports {
                unknown.extend(imports.unknown_fields(name));
            }
        }
        unknown
    }
}

impl PkgJSONImport {
    fn unknown_fields(&self, prefix: &str) -> Vec<String> {
        let mut unknown = Vec::new();
        if let Self::List(imports) = self {
            for (index, import) in imports.iter().enumerate() {
                if let PkgJSONImportItem::Object { extra, .. } = import {
                    unknown.extend(unknown_field_paths(extra, &format!("{prefix}[{index}]")));
                }
            }
        }
        unknown
    }
}

impl Link {
    // TODO(1.0): Remove backend exports fields and their lowering after the
    // deprecation window; exports then belong to source-level #export_name.
    fn warn_deprecated_exports(&self, user_log: &UserLog) {
        for (backend, exports) in [
            (
                "wasm",
                self.wasm.as_ref().and_then(|cfg| cfg.exports.as_ref()),
            ),
            (
                "wasm-gc",
                self.wasm_gc.as_ref().and_then(|cfg| cfg.exports.as_ref()),
            ),
            ("js", self.js.as_ref().and_then(|cfg| cfg.exports.as_ref())),
            (
                "native",
                self.native.as_ref().and_then(|cfg| cfg.exports.as_ref()),
            ),
        ] {
            if exports.is_some() {
                user_log.warn(format!(
                    "`link.{backend}.exports` is deprecated; use `#export_name` attributes in MoonBit source instead."
                ));
            }
        }
    }
}

impl MoonPkg {
    /// Nested configuration is shared by the JSON and DSL package decoders.
    fn unknown_fields(&self) -> Vec<String> {
        let mut unknown = Vec::new();
        if let Some(link) = &self.link {
            unknown.extend(unknown_field_paths(&link.extra, "link"));
            if let Some(native) = &link.native {
                unknown.extend(unknown_field_paths(&native.extra, "link.native"));
            }
            if let Some(js) = &link.js {
                unknown.extend(unknown_field_paths(&js.extra, "link.js"));
            }
            if let Some(wasm) = &link.wasm {
                unknown.extend(unknown_field_paths(&wasm.extra, "link.wasm"));
                if let Some(memory) = &wasm.import_memory {
                    unknown.extend(unknown_field_paths(
                        &memory.extra,
                        "link.wasm.import-memory",
                    ));
                }
                if let Some(limits) = &wasm.memory_limits {
                    unknown.extend(unknown_field_paths(
                        &limits.extra,
                        "link.wasm.memory-limits",
                    ));
                }
            }
            if let Some(wasm_gc) = &link.wasm_gc {
                unknown.extend(unknown_field_paths(&wasm_gc.extra, "link.wasm-gc"));
                if let Some(memory) = &wasm_gc.import_memory {
                    unknown.extend(unknown_field_paths(
                        &memory.extra,
                        "link.wasm-gc.import-memory",
                    ));
                }
                if let Some(limits) = &wasm_gc.memory_limits {
                    unknown.extend(unknown_field_paths(
                        &limits.extra,
                        "link.wasm-gc.memory-limits",
                    ));
                }
            }
        }
        if let Some(virtual_pkg) = &self.virtual_pkg {
            unknown.extend(unknown_field_paths(&virtual_pkg.extra, "virtual"));
        }
        for (index, rule) in self.local_rules.iter().flatten().enumerate() {
            unknown.extend(unknown_field_paths(&rule.extra, &format!("rule[{index}]")));
        }
        for (index, generate) in self.pre_build.iter().flatten().enumerate() {
            let (MoonPkgGenerate::Direct { extra, .. } | MoonPkgGenerate::Rule { extra, .. }) =
                generate;
            unknown.extend(unknown_field_paths(extra, &format!("pre-build[{index}]")));
        }
        unknown
    }
}

/// Imports collected in source order before duplicate-item warnings are emitted.
#[derive(Default)]
struct PackageImports {
    regular: Vec<Import>,
    whitebox: Vec<Import>,
    blackbox: Vec<Import>,
}

impl PackageImports {
    fn warn_duplicates(&self, user_log: &UserLog) {
        warn_duplicate_imports(&self.regular, "import", user_log);
        warn_duplicate_imports(&self.whitebox, "wbtest-import", user_log);
        warn_duplicate_imports(&self.blackbox, "test-import", user_log);
    }

    fn get_mut(&mut self, key: &str) -> Option<(&'static str, &mut Vec<Import>)> {
        match key {
            "import" => Some(("import", &mut self.regular)),
            "wbtest-import" | "wbtest_import" => Some(("wbtest-import", &mut self.whitebox)),
            "test-import" | "test_import" => Some(("test-import", &mut self.blackbox)),
            _ => None,
        }
    }
}

fn warn_duplicate_imports(imports: &[Import], kind: &str, user_log: &UserLog) {
    let mut seen = HashSet::new();
    let mut warned = HashSet::new();
    for import in imports {
        let path = import.get_path();
        for &backend in TargetBackend::all() {
            if import.supports_backend(backend)
                && !seen.insert((path, backend))
                && warned.insert(path)
            {
                user_log.warn(format!("Duplicate import of package `{path}` in `{kind}`."));
            }
        }
    }
}

/// Collect declarations and reject duplicate configuration before decoding fields.
fn normalize_pkg_dsl(
    dsl: moon_pkg::Dsl,
    path: &Path,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<(Map<String, Value>, PackageImports)> {
    // Top-level DSL keys accepted in `moon.pkg`; the boolean says whether
    // repeated entries should be collected as a JSON array instead of rejected.
    let toplevel_keys = std::collections::HashMap::from([
        ("import", true),
        ("wbtest-import", true),
        ("test-import", true),
        ("options", false),
        ("warnings", false),
        ("dev_build", true),
        ("formatter", false),
        ("rule", true),
        ("supported_targets", false),
        ("pkgtype", false),
        ("experimental", false),
        ("proof_enabled", false),
        ("max_concurrent_tests", false),
        ("regex_backend", false),
        ("virtual", false),
    ]);
    let mut map = serde_json_lenient::Map::new();
    let mut imports = PackageImports::default();
    // Empty import blocks still declare a setting. Repeated blocks append, but
    // a legacy option cannot supply the same import kind as a block.
    let mut declared_imports = HashSet::new();
    for (key, value) in dsl.iter() {
        let Some(&allow_duplicate) = toplevel_keys.get(key) else {
            bail!("Unexpected key '{}' found in moon.pkg.", key);
        };
        if let Some((key, imports)) = imports.get_mut(key) {
            declared_imports.insert(key);
            imports.extend(serde_json_lenient::from_value::<Vec<Import>>(
                value.clone(),
            )?);
            continue;
        }
        if allow_duplicate {
            match map
                .entry(key.to_string())
                .or_insert_with(|| Value::Array(Vec::new()))
            {
                Value::Array(values) => values.push(value.clone()),
                _ => unreachable!("duplicate key should be initialized as array"),
            }
            continue;
        }
        if map.insert(key.to_string(), value.clone()).is_some() {
            bail!("Duplicate key '{}' found in moon.pkg.", key);
        }
    }
    // `virtual(implement: ...)` names the virtual package this package
    // implements; any other `virtual(...)` declares this package as virtual.
    if let Some(Value::Object(args)) = map.get_mut("virtual")
        && let Some(target) = args.remove("implement")
    {
        if !args.is_empty() {
            bail!("`virtual(implement: ...)` cannot take other arguments in moon.pkg.");
        }
        map.remove("virtual");
        map.insert(String::from("implement"), target);
    }
    if let Some(options) = map.remove("options") {
        let options = Map::<String, Value>::deserialize(options)
            .context("`options` in moon.pkg must be an object")?;
        for (k, v) in options {
            if let Some((key, imports)) = imports.get_mut(&k) {
                if !declared_imports.insert(key) {
                    bail!("duplicate field `{key}`");
                }
                let value = Option::<PkgJSONImport>::deserialize(v)?;
                if emit_warnings && let Some(value) = &value {
                    warn_unknown_fields(&value.unknown_fields(&k), path, user_log);
                }
                *imports = pkg_json_imports_to_imports(value);
            } else if map.insert(k.clone(), v).is_some() {
                bail!("duplicate field `{k}`");
            }
        }
    }
    if let Some(warnings) = map.remove("warnings") {
        if map.contains_key("warn_list") || map.contains_key("warn-list") {
            bail!("duplicate field `warn_list`");
        }
        let warnings =
            String::deserialize(warnings).context("`warnings` in moon.pkg must be a string")?;
        if !warnings.is_empty() {
            map.insert(String::from("warn-list"), Value::String(warnings));
        }
    }
    if emit_warnings
        && map.contains_key("supported-targets")
        && !map.contains_key("supported_targets")
    {
        user_log.warn("`options(\"supported-targets\": ...)` in `moon.pkg` is deprecated. Please use `supported_targets = ...` instead.");
    }
    if let Some(v) = map.remove("dev_build")
        && map.insert(String::from("pre_build"), v).is_some()
    {
        bail!("duplicate field `pre_build`");
    }
    Ok((map, imports))
}

fn convert_pkg_dsl_to_package_with_supported_targets_decl(
    dsl: moon_pkg::Dsl,
    path: &Path,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    let (map, imports) = normalize_pkg_dsl(dsl, path, emit_warnings, user_log)?;
    let mut options = ManifestOptions {
        fields: map,
        manifest: "moon.pkg",
    };
    // Keep explicit declarations until their conflicts have been checked.
    let experimental: Option<bool> = options.read("experimental", None)?;
    let allow_experimental_packages = options.read(
        "allow_experimental_packages",
        Some("allow-experimental-packages"),
    )?;
    let is_main = options.read("is_main", Some("is-main"))?;
    let pkgtype: Option<PkgType> = options.read("pkgtype", None)?;
    let link = options.read("link", None)?;
    let formatter: Option<MoonPkgFormatterJSON> = options.read("formatter", None)?;
    let warn_list = options.read("warn_list", Some("warn-list"))?;
    let alert_list = options.read("alert-list", None)?;
    // TODO: Remove recognition when moonc stops reading this deprecated option.
    // This does not restore Moon's removed test-import-all behavior.
    let legacy_test_import_all: Option<bool> = options.read("test-import-all", None)?;
    let proof_enabled: Option<bool> = options.read("proof_enabled", Some("proof-enabled"))?;
    let targets = options.read("targets", None)?;
    let pre_build = options.read("pre_build", Some("pre-build"))?;
    let local_rules: Option<Vec<MoonModRule>> = options.read("rule", None)?;
    let supported_targets = options.read("supported_targets", Some("supported-targets"))?;
    let native_stub = options.read("native_stub", Some("native-stub"))?;
    let virtual_pkg = options.read("virtual_pkg", Some("virtual"))?;
    let implement = options.read("implement", None)?;
    let overrides = options.read("overrides", None)?;
    let max_concurrent_tests =
        options.read("max_concurrent_tests", Some("max-concurrent-tests"))?;
    let regex_backend = options.read("regex_backend", Some("regex-backend"))?;
    if emit_warnings {
        let mut unknown: Vec<_> = unknown_field_paths(&options.fields, "").collect();
        if let Some(formatter) = &formatter {
            unknown.extend(unknown_field_paths(&formatter.extra, "formatter"));
        }
        if let Some(pkgtype) = &pkgtype {
            unknown.extend(unknown_field_paths(&pkgtype.extra, "pkgtype"));
        }
        warn_unknown_fields(&unknown, path, user_log);
        if legacy_test_import_all.is_some() {
            user_log
                .warn("`test-import-all` in moon.pkg is deprecated; use explicit imports instead.");
        }
        imports.warn_duplicates(user_log);
    }
    let (is_main, force_link) = resolve_package_kind(
        pkgtype.as_ref(),
        is_main,
        link.as_ref(),
        emit_warnings,
        user_log,
    )?;
    let (supported_targets, supported_targets_decl_kind) =
        resolve_supported_targets(supported_targets.as_ref())?;
    validate_local_rules(local_rules.as_deref().unwrap_or_default())?;

    let package = MoonPkg {
        experimental: experimental.unwrap_or(false),
        allow_experimental_packages,
        is_main,
        force_link,
        sub_package: None,
        imports: imports.regular,
        wbtest_imports: imports.whitebox,
        test_imports: imports.blackbox,
        formatter: formatter.unwrap_or_default().into(),
        link: link.and_then(BoolOrLink::into_link),
        warn_list,
        alert_list,
        proof_enabled: proof_enabled.unwrap_or(false),
        targets,
        pre_build,
        supported_targets,
        native_stub,
        virtual_pkg,
        implement,
        overrides,
        max_concurrent_tests,
        regex_backend,
        local_rules,
    };
    if emit_warnings {
        warn_unknown_fields(&package.unknown_fields(), path, user_log);
        if let Some(link) = &package.link {
            link.warn_deprecated_exports(user_log);
        }
    }
    Ok((package, supported_targets_decl_kind))
}

fn convert_pkg_json_to_package_with_supported_targets_decl(
    j: MoonPkgJSON,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    let imports = PackageImports {
        regular: pkg_json_imports_to_imports(j.import),
        whitebox: pkg_json_imports_to_imports(j.wbtest_import),
        blackbox: pkg_json_imports_to_imports(j.test_import),
    };
    if emit_warnings {
        if j.legacy_test_import_all.is_some() {
            user_log.warn(
                "`test-import-all` in moon.pkg.json is deprecated; use explicit imports instead.",
            );
        }
        imports.warn_duplicates(user_log);
    }

    let (is_main, force_link) = resolve_package_kind(
        j.pkgtype.as_ref(),
        j.is_main,
        j.link.as_ref(),
        emit_warnings,
        user_log,
    )?;
    let (supported_backends, supported_targets_decl_kind) =
        resolve_supported_targets(j.supported_targets.as_ref())?;
    validate_local_rules(j.rule.as_deref().unwrap_or_default())?;

    let result = MoonPkg {
        experimental: j.experimental.unwrap_or(false),
        allow_experimental_packages: j.allow_experimental_packages,
        is_main,
        force_link,
        sub_package: None,
        imports: imports.regular,
        wbtest_imports: imports.whitebox,
        test_imports: imports.blackbox,
        formatter: j.formatter.unwrap_or_default().into(),
        link: j.link.and_then(BoolOrLink::into_link),
        warn_list: j.warn_list,
        alert_list: j.alert_list,
        proof_enabled: j.proof_enabled.unwrap_or(false),
        targets: j.targets,
        pre_build: j.pre_build,
        supported_targets: supported_backends,
        native_stub: j.native_stub,
        virtual_pkg: j.virtual_pkg,
        implement: j.implement,
        overrides: j.overrides,
        max_concurrent_tests: j.max_concurrent_tests,
        regex_backend: j.regex_backend,
        local_rules: j.rule,
    };
    if emit_warnings && let Some(link) = &result.link {
        link.warn_deprecated_exports(user_log);
    }
    Ok((result, supported_targets_decl_kind))
}

// TODO(1.0): Remove the is-main and boolean link compatibility paths after the
// deprecation window. Keep structured link configuration and pkgtype resolution.
fn resolve_package_kind(
    pkgtype: Option<&PkgType>,
    is_main: Option<bool>,
    link: Option<&BoolOrLink>,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<(bool, bool)> {
    // Legacy `force_link` from the boolean `link: true` (a structured
    // `link: { ... }` config is not a force-link signal).
    let legacy_force_link = match link {
        None => false,
        Some(BoolOrLink::Bool(b)) => *b,
        Some(BoolOrLink::Link(_)) => false,
    };

    // `pkgtype` is the source of truth when present. The legacy `is-main` /
    // `link: true` flags are honored only as a fallback during migration; an
    // explicitly-set legacy flag that contradicts `pkgtype` is a hard error,
    // while a redundant-but-consistent one only warns.
    let flags = match pkgtype.map(|p| p.kind) {
        Some(kind) => {
            let (want_main, want_force_link) = match kind {
                PackageKind::Library => (false, false),
                PackageKind::Executable => (true, false),
                PackageKind::ForeignLibrary => (false, true),
            };
            if let Some(is_main_flag) = is_main {
                if is_main_flag != want_main {
                    bail!(
                        "`pkgtype(kind: \"{}\")` conflicts with `is-main: {}` in moon.pkg.",
                        kind.as_str(),
                        is_main_flag
                    );
                } else if emit_warnings {
                    let warning = format!(
                        "`is-main` is redundant with `pkgtype(kind: \"{}\")` in `moon.pkg`. `is-main` is deprecated; please remove it.",
                        kind.as_str()
                    );
                    user_log.warn(warning);
                }
            }
            if let Some(BoolOrLink::Bool(link_flag)) = link {
                if *link_flag != want_force_link {
                    bail!(
                        "`pkgtype(kind: \"{}\")` conflicts with `link: {}` in moon.pkg.",
                        kind.as_str(),
                        link_flag
                    );
                } else if emit_warnings {
                    let warning = format!(
                        "`link: {}` is redundant with `pkgtype(kind: \"{}\")` in `moon.pkg`. The boolean `link` is deprecated; please remove it.",
                        link_flag,
                        kind.as_str()
                    );
                    user_log.warn(warning);
                }
            }
            (want_main, want_force_link)
        }
        None => {
            if emit_warnings {
                if let Some(is_main) = is_main {
                    let migration = if is_main {
                        "use `pkgtype(kind: \"executable\")` instead"
                    } else {
                        "remove it"
                    };
                    user_log.warn(format!("`is-main: {is_main}` is deprecated; {migration}."));
                }
                if let Some(BoolOrLink::Bool(link)) = link {
                    let migration = if !link {
                        "remove it"
                    } else if is_main == Some(true) {
                        "remove it from executable packages"
                    } else {
                        "use `pkgtype(kind: \"foreign_library\")` instead"
                    };
                    user_log.warn(format!("`link: {link}` is deprecated; {migration}."));
                }
            }
            (is_main.unwrap_or(false), legacy_force_link)
        }
    };
    Ok(flags)
}

fn validate_local_rules(rules: &[MoonModRule]) -> anyhow::Result<()> {
    let mut names = HashSet::new();
    for rule in rules {
        if !names.insert(rule.name.as_str()) {
            bail!("Duplicate rule name `{}` found in moon.pkg.", rule.name);
        }
    }
    Ok(())
}
