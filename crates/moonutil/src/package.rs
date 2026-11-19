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

use std::{collections::HashSet, path::Path};

use anyhow::{Context, bail};
use indexmap::{IndexMap, IndexSet};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json_lenient::{Map, Value};

pub use crate::supported_targets::resolve_supported_targets;
use crate::{
    cond_expr::CondExprs,
    manifest::{ManifestOptions, unknown_field_paths},
    module::MoonModRule,
    moon_pkg,
    target::TargetBackend::{self, Js, LLVM, Native, Wasm, WasmGC},
    user_log::UserLog,
};

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
pub struct MoonPkgFormatterJSON {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<std::collections::HashSet<String>>")]
    pub ignore: Option<IndexSet<String>>,
    #[serde(flatten, skip_serializing)]
    #[schemars(skip)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MoonPkgFormatter {
    pub ignore: IndexSet<String>,
}

impl From<MoonPkgFormatterJSON> for MoonPkgFormatter {
    fn from(config: MoonPkgFormatterJSON) -> Self {
        Self {
            ignore: config.ignore.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(untagged)]
pub enum PkgJSONImport {
    /// Path and alias of an imported package
    #[schemars(with = "std::collections::HashMap<String, Option<String>>")]
    Map(IndexMap<String, Option<String>>),
    List(Vec<PkgJSONImportItem>),
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(untagged)]
pub enum PkgJSONImportItem {
    String(String),
    Object {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        alias: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(alias = "import-all")]
        #[serde(rename(serialize = "import-all"))]
        #[schemars(rename = "import-all")]
        import_all: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<Vec<String>>,
        #[serde(flatten, skip_serializing)]
        #[schemars(skip)]
        extra: Map<String, Value>,
    },
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BoolOrLink {
    Bool(bool),
    Link(Box<Link>),
}

impl BoolOrLink {
    fn into_link(self) -> Option<Link> {
        match self {
            Self::Bool(_) => None,
            Self::Link(link) => Some(*link),
        }
    }
}

/// The kind of a package, declared via `pkgtype(kind: "...")` in `moon.pkg`.
///
/// This supersedes the deprecated `is-main` and `link: true` flags:
/// - `library` (the default): a normal library, emits a `.core` consumed by
///   other MoonBit packages.
/// - `executable`: has a `main` and links to a runnable artifact
///   (equivalent to the deprecated `is-main: true`).
/// - `foreign_library`: a non-main library force-linked into a standalone,
///   foreign-consumable artifact (equivalent to the deprecated `link: true`).
///   The native/LLVM backends build it as a static library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    Library,
    Executable,
    ForeignLibrary,
}

impl PackageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PackageKind::Library => "library",
            PackageKind::Executable => "executable",
            PackageKind::ForeignLibrary => "foreign_library",
        }
    }

    /// The `(is_main, force_link)` this kind lowers to.
    fn to_flags(self) -> (bool, bool) {
        match self {
            PackageKind::Library => (false, false),
            PackageKind::Executable => (true, false),
            PackageKind::ForeignLibrary => (false, true),
        }
    }
}

/// The `pkgtype(kind: "...")` declaration in `moon.pkg`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PkgType {
    pub kind: PackageKind,
    #[serde(flatten, skip_serializing)]
    #[schemars(skip)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(untagged)]
pub enum SupportedTargetsConfig {
    Expr(String),
    LegacyArray(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupportedTargetsDeclKind {
    Omitted,
    Expr,
    LegacyArray,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(
    title = "JSON schema for MoonBit moon.pkg.json files",
    description = "A package of MoonBit language"
)]
pub struct MoonPkgJSON {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Mark this package as experimental, warning packages that depend on it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<bool>,

    /// Acknowledge all experimental dependencies, or only the listed full package names.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(
        rename = "allow-experimental-packages",
        alias = "allow_experimental_packages"
    )]
    pub allow_experimental_packages: Option<AllowExperimentalPackages>,

    /// Specify whether this package is a main package or not.
    ///
    /// **Deprecated:** use `pkgtype(kind: "executable")` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "is-main")]
    #[serde(alias = "is_main")]
    #[serde(rename(serialize = "is-main"))]
    #[schemars(rename = "is-main")]
    pub is_main: Option<bool>,

    /// The kind of this package: `library` (default), `executable`, or
    /// `foreign_library`. Declared via `pkgtype(kind: "...")` in `moon.pkg`;
    /// supersedes the deprecated `is-main` and `link: true` flags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pkgtype: Option<PkgType>,

    /// Imported packages of the package
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import: Option<PkgJSONImport>,

    /// White box test imported packages of the package
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "wbtest_import")]
    #[serde(alias = "wbtest-import")]
    #[schemars(rename = "wbtest-import")]
    pub wbtest_import: Option<PkgJSONImport>,

    /// Black box test imported packages of the package
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "test_import")]
    #[serde(alias = "test-import")]
    #[schemars(rename = "test-import")]
    pub test_import: Option<PkgJSONImport>,

    /// Link configuration for the package.
    ///
    /// The boolean form `link: true` (force-link a non-main library) is
    /// **deprecated**: use `pkgtype(kind: "foreign_library")` instead. The
    /// structured form `link: { ... }` remains supported as link configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<BoolOrLink>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub formatter: Option<MoonPkgFormatterJSON>,

    /// Warn list setting of the package
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "warn-list")]
    #[serde(alias = "warn_list")]
    #[schemars(rename = "warn-list")]
    pub warn_list: Option<String>,

    /// Alert options read from the manifest by moonc.
    #[serde(rename = "alert-list", skip_serializing_if = "Option::is_none")]
    pub alert_list: Option<String>,

    // TODO: Remove recognition when moonc stops reading this deprecated option.
    #[serde(rename = "test-import-all", skip_serializing)]
    #[schemars(skip)]
    pub legacy_test_import_all: Option<bool>,

    /// Whether this package participates in proof-oriented workflows.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "proof-enabled")]
    #[serde(rename(serialize = "proof-enabled"))]
    #[schemars(rename = "proof-enabled")]
    pub proof_enabled: Option<bool>,

    /// Conditional compilation targets
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "targets")]
    #[schemars(rename = "targets")]
    #[schemars(with = "Option<std::collections::HashMap<String, StringOrArray>>")]
    pub targets: Option<CondExprs>,

    /// Command for moon generate
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "pre-build")]
    #[schemars(rename = "pre-build")]
    pub pre_build: Option<Vec<MoonPkgGenerate>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub rule: Option<Vec<MoonModRule>>,

    /// Supported backend set for this package.
    ///
    /// This accepts either expression syntax (for example: `"js"` or
    /// `"all-js+wasm-gc"`) or legacy array syntax.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "supported-targets")]
    #[schemars(rename = "supported-targets")]
    pub supported_targets: Option<SupportedTargetsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "native-stub")]
    #[schemars(rename = "native-stub")]
    pub native_stub: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "virtual")]
    #[schemars(rename = "virtual")]
    pub virtual_pkg: Option<VirtualPkg>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub implement: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "max-concurrent-tests")]
    #[schemars(rename = "max-concurrent-tests")]
    pub max_concurrent_tests: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "regex-backend")]
    #[schemars(rename = "regex-backend")]
    pub regex_backend: Option<RegexBackend>,
    #[serde(flatten, skip_serializing)]
    #[schemars(skip)]
    pub extra: Map<String, Value>,
}

impl MoonPkgJSON {
    /// Diagnostics for fields that the JSON-to-package conversion consumes.
    pub(crate) fn consumed_unknown_fields(&self) -> Vec<String> {
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

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(untagged)]
pub enum AllowExperimentalPackages {
    Bool(bool),
    Packages(Vec<String>),
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct VirtualPkg {
    #[serde(alias = "has-default")]
    #[schemars(rename = "has-default")]
    pub has_default: bool,
    #[serde(flatten, skip_serializing)]
    #[schemars(skip)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[schemars(rename = "import-memory")]
pub struct ImportMemory {
    pub module: String,
    pub name: String,
    #[serde(flatten, skip_serializing)]
    #[schemars(skip)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[schemars(rename = "memory-limits")]
pub struct MemoryLimits {
    pub min: u32,
    pub max: u32,
    #[serde(flatten, skip_serializing)]
    #[schemars(skip)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct WasmLinkConfig {
    /// **Deprecated:** use the `#export_name` attribute in MoonBit source instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exports: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "heap-start-address")]
    pub heap_start_address: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "import-memory")]
    pub import_memory: Option<ImportMemory>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "memory-limits")]
    pub memory_limits: Option<MemoryLimits>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "shared-memory")]
    pub shared_memory: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "export-memory-name")]
    pub export_memory_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<Vec<String>>,
    #[serde(flatten, skip_serializing)]
    #[schemars(skip)]
    pub extra: Map<String, Value>,
}

/// Native C/C++ compilation and linking configuration for MoonBit packages.
///
/// Controls how C stub files and main executables are compiled and linked.
/// C stub object files are collected into static archives.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct NativeLinkConfig {
    // FIXME: We have no way to force link a native library when not `is_main`
    /// Function exports for the final native executable.
    ///
    /// **Deprecated:** use the `#export_name` attribute in MoonBit source instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exports: Option<Vec<String>>,

    /// Custom C compiler for main MoonBit-generated C code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<String>,

    /// Compiler flags for main MoonBit-generated C code (whitespace-separated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc_flags: Option<String>,

    /// Linker flags for the main executable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc_link_flags: Option<String>,

    /// Custom C compiler for C stub files
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub_cc: Option<String>,

    /// Compiler flags for C stub compilation (whitespace-separated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub_cc_flags: Option<String>,

    #[serde(flatten, skip_serializing)]
    #[schemars(skip)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct WasmGcLinkConfig {
    /// **Deprecated:** use the `#export_name` attribute in MoonBit source instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exports: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_memory: Option<ImportMemory>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_limits: Option<MemoryLimits>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_memory: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_memory_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_js_builtin_string: Option<bool>,

    /// Use the compiler's FFI byte representation for WasmGC.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_ffi_bytes: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_string_constants: Option<String>,
    #[serde(flatten, skip_serializing)]
    #[schemars(skip)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct JsLinkConfig {
    /// **Deprecated:** use the `#export_name` attribute in MoonBit source instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exports: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<JsFormat>,
    #[serde(flatten, skip_serializing)]
    #[schemars(skip)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Copy, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[repr(u8)]
pub enum JsFormat {
    #[default]
    #[serde(rename = "esm")]
    ESM,
    #[serde(rename = "cjs")]
    CJS,
    #[serde(rename = "iife")]
    IIFE,
}

impl JsFormat {
    pub fn to_flag(&self) -> &'static str {
        match self {
            JsFormat::ESM => "esm",
            JsFormat::CJS => "cjs",
            JsFormat::IIFE => "iife",
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RegexBackend {
    Auto,
    Block,
    Table,
    Runtime,
}

impl RegexBackend {
    pub fn to_flag(&self) -> &'static str {
        match self {
            RegexBackend::Auto => "auto",
            RegexBackend::Block => "block",
            RegexBackend::Table => "table",
            RegexBackend::Runtime => "runtime",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct Link {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasm: Option<WasmLinkConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "wasm-gc")]
    pub wasm_gc: Option<WasmGcLinkConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub js: Option<JsLinkConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub native: Option<NativeLinkConfig>,
    #[serde(flatten, skip_serializing)]
    #[schemars(skip)]
    pub extra: Map<String, Value>,
}

impl Link {
    pub fn need_link(&self, target: TargetBackend) -> bool {
        match target {
            Wasm | WasmGC | Js => true,
            Native | LLVM => self.native.as_ref().is_some_and(|n| {
                n.cc.is_some()
                    || n.cc_flags.is_some()
                    || n.cc_link_flags.is_some()
                    || n.exports.is_some()
            }),
        }
    }
}

#[derive(Debug, Serialize, Clone, JsonSchema)]
#[serde(untagged)]
pub enum MoonPkgGenerate {
    Direct {
        input: StringOrArray,
        output: StringOrArray,
        command: String,
        #[serde(flatten, skip_serializing)]
        #[schemars(skip)]
        extra: Map<String, Value>,
    },
    Rule {
        rule: String,
        input: StringOrArray,
        output: StringOrArray,
        #[serde(flatten, skip_serializing)]
        #[schemars(skip)]
        extra: Map<String, Value>,
    },
}

impl<'de> Deserialize<'de> for MoonPkgGenerate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Both forms share one object shape. Decode every known field before
        // selecting the command, so an unused field cannot hide a type error.
        #[derive(Deserialize)]
        struct Generate {
            input: StringOrArray,
            output: StringOrArray,
            #[serde(default, deserialize_with = "present_string")]
            command: Option<String>,
            #[serde(default, deserialize_with = "present_string")]
            rule: Option<String>,
            #[serde(flatten)]
            extra: Map<String, Value>,
        }
        fn present_string<'de, D: serde::Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Option<String>, D::Error> {
            String::deserialize(deserializer).map(Some)
        }
        let Generate {
            input,
            output,
            command,
            rule,
            extra,
        } = Generate::deserialize(deserializer)?;
        // Preserve the direct-command precedence when both are provided.
        if let Some(command) = command {
            Ok(Self::Direct {
                input,
                output,
                command,
                extra,
            })
        } else if let Some(rule) = rule {
            Ok(Self::Rule {
                input,
                output,
                rule,
                extra,
            })
        } else {
            Err(serde::de::Error::custom("expected `command` or `rule`"))
        }
    }
}

impl MoonPkgGenerate {
    pub fn input(&self) -> &StringOrArray {
        match self {
            MoonPkgGenerate::Direct { input, .. } | MoonPkgGenerate::Rule { input, .. } => input,
        }
    }

    pub fn output(&self) -> &StringOrArray {
        match self {
            MoonPkgGenerate::Direct { output, .. } | MoonPkgGenerate::Rule { output, .. } => output,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(untagged)]
pub enum StringOrArray {
    String(String),
    Array(Vec<String>),
}

/// Iterator over [`StringOrArray`]
pub enum StringOrArrayIter<'a> {
    String(std::iter::Once<&'a String>),
    Array(std::slice::Iter<'a, String>),
}

impl<'a> Iterator for StringOrArrayIter<'a> {
    type Item = &'a String;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            StringOrArrayIter::String(iter) => iter.next(),
            StringOrArrayIter::Array(iter) => iter.next(),
        }
    }
}

impl StringOrArray {
    pub fn iter(&self) -> StringOrArrayIter<'_> {
        match self {
            StringOrArray::String(s) => StringOrArrayIter::String(std::iter::once(s)),
            StringOrArray::Array(arr) => StringOrArrayIter::Array(arr.iter()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SubPackageInMoonPkg {
    pub files: Vec<String>,
    pub import: Vec<Import>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MoonPkg {
    pub name: Option<String>,
    #[serde(default)]
    pub experimental: bool,
    pub allow_experimental_packages: Option<AllowExperimentalPackages>,
    pub is_main: bool,
    pub force_link: bool,
    pub sub_package: Option<SubPackageInMoonPkg>,
    pub imports: Vec<Import>,
    pub wbtest_imports: Vec<Import>,
    pub test_imports: Vec<Import>,
    pub formatter: MoonPkgFormatter,

    pub link: Option<Link>,
    pub warn_list: Option<String>,
    pub alert_list: Option<String>,
    pub proof_enabled: bool,

    pub targets: Option<CondExprs>,

    pub pre_build: Option<Vec<MoonPkgGenerate>>,

    pub supported_targets: IndexSet<TargetBackend>,

    pub native_stub: Option<Vec<String>>,

    pub virtual_pkg: Option<VirtualPkg>,
    pub implement: Option<String>,
    pub overrides: Option<Vec<String>>,

    pub max_concurrent_tests: Option<u32>,

    pub regex_backend: Option<RegexBackend>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_rules: Option<Vec<MoonModRule>>,
}

impl MoonPkg {
    /// Nested configuration is shared by the JSON and DSL package decoders.
    pub(crate) fn unknown_fields(&self) -> Vec<String> {
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Import {
    Simple(String),
    Alias {
        path: String,
        alias: Option<String>,
        #[serde(default)]
        sub_package: bool,
        #[serde(default, alias = "import-all")]
        import_all: bool,
        /// Backends selected by an import block's `#cfg`; absent means unconditional.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        targets: Option<IndexSet<TargetBackend>>,
    },
}

impl Import {
    pub fn get_path(&self) -> &str {
        match self {
            Self::Simple(v) => v,
            Self::Alias { path, .. } => path,
        }
    }

    pub fn supports_backend(&self, backend: TargetBackend) -> bool {
        match self {
            Self::Alias {
                targets: Some(targets),
                ..
            } => targets.contains(&backend),
            _ => true,
        }
    }

    /// Legacy JSON treats an empty alias as omitted for ordinary imports.
    fn from_json(path: String, alias: Option<String>, import_all: bool) -> Self {
        let alias = alias.filter(|alias| import_all || !alias.is_empty());
        if alias.is_none() && !import_all {
            Self::Simple(path)
        } else {
            Self::Alias {
                path,
                alias,
                sub_package: false,
                import_all,
                targets: None,
            }
        }
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

/// Convert a parsed `moon.pkg` into the package model used by ordinary callers.
///
/// Whether `supported_targets` used the legacy or current spelling is migration
/// metadata needed by manifest loading, not part of normal package conversion.
pub fn convert_pkg_dsl_to_package(
    dsl: moon_pkg::Dsl,
    user_log: &UserLog,
) -> anyhow::Result<MoonPkg> {
    Ok(convert_pkg_dsl_to_package_with_supported_targets_decl(
        dsl,
        Path::new("moon.pkg"),
        true,
        user_log,
    )?
    .0)
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
                    crate::manifest::warn_unknown_fields(&value.unknown_fields(&k), path, user_log);
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

pub(crate) fn convert_pkg_dsl_to_package_with_supported_targets_decl(
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
    let name: Option<String> = options.read("name", None)?;
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
        crate::manifest::warn_unknown_fields(&unknown, path, user_log);
        if legacy_test_import_all.is_some() {
            user_log
                .warn("`test-import-all` in moon.pkg is deprecated; use explicit imports instead.");
        }
        imports.warn_duplicates(user_log);
    }
    let (is_main, force_link) = resolve_package_kind(
        pkgtype.as_ref(),
        name.as_deref(),
        is_main,
        link.as_ref(),
        emit_warnings,
        user_log,
    )?;
    let (supported_targets, supported_targets_decl_kind) =
        resolve_supported_targets(supported_targets.as_ref())?;
    validate_local_rules(local_rules.as_deref().unwrap_or_default())?;

    let package = MoonPkg {
        name: None,
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
        crate::manifest::warn_unknown_fields(&package.unknown_fields(), path, user_log);
    }
    Ok((package, supported_targets_decl_kind))
}

pub fn pkg_json_imports_to_imports(source: Option<PkgJSONImport>) -> Vec<Import> {
    match source {
        None => Vec::new(),
        Some(PkgJSONImport::Map(map)) => map
            .into_iter()
            .map(|(path, alias)| Import::from_json(path, alias, false))
            .collect(),
        Some(PkgJSONImport::List(list)) => list
            .into_iter()
            .map(|item| match item {
                PkgJSONImportItem::String(path) => Import::Simple(path),
                PkgJSONImportItem::Object {
                    path,
                    alias,
                    import_all,
                    ..
                } => Import::from_json(path, alias, import_all.unwrap_or(false)),
            })
            .collect(),
    }
}

pub fn convert_pkg_json_to_package_with_supported_targets_decl(
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
        j.name.as_deref(),
        j.is_main,
        j.link.as_ref(),
        emit_warnings,
        user_log,
    )?;
    let (supported_backends, supported_targets_decl_kind) =
        resolve_supported_targets(j.supported_targets.as_ref())?;
    validate_local_rules(j.rule.as_deref().unwrap_or_default())?;

    let result = MoonPkg {
        name: None,
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
    Ok((result, supported_targets_decl_kind))
}

fn resolve_package_kind(
    pkgtype: Option<&PkgType>,
    name: Option<&str>,
    is_main: Option<bool>,
    link: Option<&BoolOrLink>,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<(bool, bool)> {
    // Legacy `is-main`, including the deprecated `name == "main"` alias.
    let mut legacy_is_main = is_main.unwrap_or(false);
    if name == Some("main") {
        legacy_is_main = true;
        if emit_warnings {
            let warning = "The `name` field in `moon.pkg` is deprecated. Use `pkgtype(kind: \"executable\")` for an executable package.";
            user_log.warn(warning);
        }
    }
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
            let (want_main, want_force_link) = kind.to_flags();
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
        None => (legacy_is_main, legacy_force_link),
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

#[cfg(test)]
fn convert_test_pkg_dsl(
    dsl: moon_pkg::Dsl,
    emit_warnings: bool,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    convert_pkg_dsl_to_package_with_supported_targets_decl(
        dsl,
        Path::new("moon.pkg"),
        emit_warnings,
        &UserLog::new(log::LevelFilter::Error),
    )
}

#[cfg(test)]
fn convert_test_pkg_json(
    json: MoonPkgJSON,
    emit_warnings: bool,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    convert_pkg_json_to_package_with_supported_targets_decl(
        json,
        emit_warnings,
        &UserLog::new(log::LevelFilter::Error),
    )
}

#[cfg(test)]
fn assert_pkg_dsl_matches_json(source: &str, json: Value) {
    for emit_warnings in [false, true] {
        let (dsl_log, dsl_capture) = UserLog::captured(log::LevelFilter::Warn);
        let (json_log, json_capture) = UserLog::captured(log::LevelFilter::Warn);
        let (actual, actual_decl) = convert_pkg_dsl_to_package_with_supported_targets_decl(
            moon_pkg::parse(source).unwrap(),
            Path::new("moon.pkg"),
            emit_warnings,
            &dsl_log,
        )
        .unwrap();
        let (expected, expected_decl) = convert_pkg_json_to_package_with_supported_targets_decl(
            serde_json_lenient::from_value(json.clone()).unwrap(),
            emit_warnings,
            &json_log,
        )
        .unwrap();
        assert_eq!(
            serde_json_lenient::to_value(actual).unwrap(),
            serde_json_lenient::to_value(expected).unwrap(),
            "{source}",
        );
        assert_eq!(actual_decl, expected_decl, "{source}");
        assert_eq!(
            dsl_capture
                .take()
                .into_iter()
                .map(|w| w.message)
                .collect::<Vec<_>>(),
            json_capture
                .take()
                .into_iter()
                .map(|w| w.message)
                .collect::<Vec<_>>(),
            "{source}",
        );
    }
}

#[test]
fn convert_pkg_dsl_matches_json_fields_and_defaults() {
    assert_pkg_dsl_matches_json("", serde_json_lenient::json!({}));
    assert_pkg_dsl_matches_json(
        r#"
import { "example/first", "example/first" @again }
import { "example/second" }
import { "example/test" } for "test"
import { "example/whitebox" } for "wbtest"
pkgtype(kind: "foreign_library")
formatter(ignore: ["generated.mbt"])
warnings = "+1-2"
supported_targets = "all-js"
rule(name: "first", command: "first $input $output")
rule(name: "second", command: "second $input $output")
dev_build(rule: "first", input: "a", output: "b")
dev_build(input: "b", output: "c", command: "generate")
options(
  "is-main": false,
  link: { "wasm": { "heap-start-address": 2214592512 } },
  "proof-enabled": true,
  targets: { "native.mbt": "native" },
  "native-stub": ["stub.c"],
  virtual: { "has-default": true },
  implement: "example/virtual",
  overrides: ["example/impl"],
  "max-concurrent-tests": 8,
  "regex-backend": "table",
)
"#,
        serde_json_lenient::json!({
            "import": ["example/first", {"path": "example/first", "alias": "again"}, "example/second"],
            "test-import": ["example/test"],
            "wbtest-import": ["example/whitebox"],
            "pkgtype": {"kind": "foreign_library"},
            "formatter": {"ignore": ["generated.mbt"]},
            "warn-list": "+1-2",
            "supported-targets": "all-js",
            "rule": [
                {"name": "first", "command": "first $input $output"},
                {"name": "second", "command": "second $input $output"},
            ],
            "pre-build": [
                {"rule": "first", "input": "a", "output": "b"},
                {"input": "b", "output": "c", "command": "generate"},
            ],
            "is-main": false,
            "link": {"wasm": {"heap-start-address": 2214592512_u32}},
            "proof-enabled": true,
            "targets": {"native.mbt": "native"},
            "native-stub": ["stub.c"],
            "virtual": {"has-default": true},
            "implement": "example/virtual",
            "overrides": ["example/impl"],
            "max-concurrent-tests": 8,
            "regex-backend": "table",
        }),
    );
}

#[test]
fn experimental_package_configuration() {
    for experimental in [false, true] {
        for allowance in ["false", "true", "[]", r#"["example/experimental"]"#] {
            let source = format!(
                "experimental = {experimental}\noptions(\"allow-experimental-packages\": {allowance})"
            );
            let json = serde_json_lenient::from_str(&format!(
                r#"{{"experimental": {experimental}, "allow-experimental-packages": {allowance}}}"#
            ))
            .unwrap();
            assert_pkg_dsl_matches_json(&source, json);
        }
    }
}

#[test]
fn experimental_package_configuration_rejects_invalid_values() {
    for (key, value) in [
        ("experimental", r#""true""#),
        ("experimental", "[]"),
        ("experimental", "1"),
        ("allow-experimental-packages", r#""example/experimental""#),
        ("allow-experimental-packages", "[true]"),
        ("allow-experimental-packages", "{}"),
        ("allow-experimental-packages", "1"),
    ] {
        let source = if key == "experimental" {
            format!("experimental = {value}")
        } else {
            format!("options(\"{key}\": {value})")
        };
        assert!(
            convert_test_pkg_dsl(moon_pkg::parse(&source).unwrap(), false).is_err(),
            "{source}"
        );
        assert!(
            serde_json_lenient::from_str::<MoonPkgJSON>(&format!(r#"{{"{key}": {value}}}"#))
                .is_err(),
            "{key}: {value}"
        );
    }
}

#[test]
fn convert_pkg_dsl_preserves_option_aliases_and_validation() {
    for (key, value) in [
        ("allow_experimental_packages", "true"),
        ("is_main", "true"),
        ("warn_list", r#""+1""#),
        ("proof_enabled", "true"),
        (
            "pre_build",
            r#"[{"input": "a", "output": "b", "command": "generate"}]"#,
        ),
        ("native_stub", r#"["stub.c"]"#),
        ("virtual_pkg", r#"{"has-default": true}"#),
        ("max_concurrent_tests", "4294967295"),
        ("regex_backend", r#""runtime""#),
    ] {
        let alias = if key == "virtual_pkg" {
            "virtual".to_string()
        } else {
            key.replace('_', "-")
        };
        for spelling in [key, &alias] {
            let source = format!(r#"options("{spelling}": {value})"#);
            let json = serde_json_lenient::from_str(&format!(r#"{{"{key}": {value}}}"#)).unwrap();
            assert_pkg_dsl_matches_json(&source, json);

            let source = format!(r#"options("{spelling}": {{}})"#);
            let json =
                serde_json_lenient::from_str::<MoonPkgJSON>(&format!(r#"{{"{key}": {{}}}}"#));
            assert!(json.is_err(), "{key}");
            assert!(
                convert_test_pkg_dsl(moon_pkg::parse(&source).unwrap(), false).is_err(),
                "{source}"
            );
        }
        let source = format!(r#"options("{key}": {value}, "{alias}": {value})"#);
        let error = convert_test_pkg_dsl(moon_pkg::parse(&source).unwrap(), false).unwrap_err();
        assert_eq!(error.to_string(), format!("duplicate field `{key}`"));
    }
}

#[test]
fn convert_pkg_dsl_rejects_duplicate_options() {
    for (declaration, option, field) in [
        (
            r#"pkgtype(kind: "library")"#,
            r#"pkgtype: { "kind": "library" }"#,
            "pkgtype",
        ),
        (
            r#"formatter(ignore: ["a.mbt"])"#,
            r#"formatter: { "ignore": [] }"#,
            "formatter",
        ),
        (r#"rule(name: "a", command: "echo a")"#, "rule: []", "rule"),
        ("experimental = false", "experimental: true", "experimental"),
        (r#"warnings = "+1""#, r#"warnings: "+1""#, "warnings"),
        (r#"warnings = """#, r#"warn_list: "+1""#, "warn_list"),
        (r#"warnings = "+1""#, r#""warn-list": "-2""#, "warn_list"),
        (
            r#"supported_targets = "js""#,
            r#"supported_targets: "js""#,
            "supported_targets",
        ),
        (
            r#"supported_targets = "js""#,
            r#""supported-targets": "+native""#,
            "supported_targets",
        ),
        (
            "proof_enabled = true",
            r#""proof-enabled": false"#,
            "proof_enabled",
        ),
        (
            "virtual(has_default: true)",
            r#"virtual: { "has-default": false }"#,
            "virtual",
        ),
        (
            "virtual(has_default: true)",
            r#"virtual_pkg: { "has-default": true }"#,
            "virtual_pkg",
        ),
        (
            r#"virtual(implement: "example/lib")"#,
            r#"implement: "example/other""#,
            "implement",
        ),
    ] {
        for source in [
            format!("{declaration}\noptions({option})"),
            format!("options({option})\n{declaration}"),
        ] {
            let error = convert_test_pkg_dsl(moon_pkg::parse(&source).unwrap(), false).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("duplicate field `{field}`"),
                "{source}"
            );
        }
    }
}

#[test]
fn convert_pkg_dsl_accepts_legacy_options() {
    for (source, json) in [
        (
            r#"options(name: "main")"#,
            serde_json_lenient::json!({"name": "main"}),
        ),
        (
            r#"options(link: true)"#,
            serde_json_lenient::json!({"link": true}),
        ),
        (
            r#"supported_targets = ["wasm", "js"]"#,
            serde_json_lenient::json!({"supported-targets": ["wasm", "js"]}),
        ),
    ] {
        assert_pkg_dsl_matches_json(source, json);
    }
}

#[test]
fn ignore_unknown_package_fields() {
    for key in ["sub-package", "sub_package", "unknown-option"] {
        let json = format!(r#"{{"is-main": true, "{key}": {{"files": []}}}}"#);
        let dsl = moon_pkg::parse(&format!(
            r#"options("is-main": true, "{key}": {{"files": []}})"#
        ))
        .unwrap();
        for (pkg, _) in [
            convert_test_pkg_json(serde_json_lenient::from_str(&json).unwrap(), false).unwrap(),
            convert_test_pkg_dsl(dsl, false).unwrap(),
        ] {
            assert!(pkg.is_main);
            assert!(pkg.sub_package.is_none());
        }
    }
}

#[test]
fn ignore_unknown_import_fields() {
    for import_key in ["import", "wbtest-import", "test-import"] {
        for key in ["sub-package", "sub_package", "unknown-option"] {
            for value in ["true", "false"] {
                let imports = format!(r#"[{{"path": "example/dep", "{key}": {value}}}]"#);
                let json = format!(r#"{{"{import_key}": {imports}}}"#);
                let dsl =
                    moon_pkg::parse(&format!(r#"options("{import_key}": {imports})"#)).unwrap();
                for (pkg, _) in [
                    convert_test_pkg_json(serde_json_lenient::from_str(&json).unwrap(), false)
                        .unwrap(),
                    convert_test_pkg_dsl(dsl, false).unwrap(),
                ] {
                    let imports = pkg
                        .imports
                        .into_iter()
                        .chain(pkg.wbtest_imports)
                        .chain(pkg.test_imports)
                        .collect::<Vec<_>>();
                    assert!(
                        matches!(imports.as_slice(), [Import::Simple(path)] if path == "example/dep")
                    );
                }
            }
        }
    }
}

#[test]
fn convert_pkg_imports_normalize_aliases() {
    for source in [
        r#"["example/lib"]"#,
        r#"{"example/lib": null}"#,
        r#"{"example/lib": ""}"#,
        r#"[{"path": "example/lib"}]"#,
        r#"[{"path": "example/lib", "alias": ""}]"#,
        r#"[{"path": "example/lib", "sub-package": false}]"#,
        r#"[{"path": "example/lib", "alias": "", "sub-package": false}]"#,
    ] {
        let imports =
            pkg_json_imports_to_imports(Some(serde_json_lenient::from_str(source).unwrap()));
        assert!(matches!(&imports[..], [Import::Simple(path)] if path == "example/lib"));
    }
}

#[test]
fn convert_pkg_imports_preserve_aliases() {
    let dsl =
        crate::moon_pkg::parse(r#"import { "example/lib/default", "example/lib/named" @named }"#)
            .unwrap();
    let (dsl_pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
    for imports in [
        r#"["example/lib/default", {"path": "example/lib/named", "alias": "named"}]"#,
        r#"{"example/lib/default": null, "example/lib/named": "named"}"#,
    ] {
        let json = serde_json_lenient::from_str(&format!(r#"{{"import": {imports}}}"#)).unwrap();
        let (json_pkg, _) = convert_test_pkg_json(json, false).unwrap();
        assert_eq!(
            serde_json_lenient::to_value(json_pkg.imports).unwrap(),
            serde_json_lenient::to_value(&dsl_pkg.imports).unwrap(),
        );
    }
}

#[test]
fn convert_pkg_imports_preserve_import_all() {
    for (alias_syntax, alias) in [("", None), ("@named", Some("named"))] {
        let dsl =
            crate::moon_pkg::parse(&format!(r#"import {{ "example/lib" {alias_syntax} * }}"#,))
                .unwrap();
        let (dsl_pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
        assert!(matches!(
            &dsl_pkg.imports[..],
            [Import::Alias { alias: actual_alias, import_all: true, .. }]
                if actual_alias.as_deref() == alias
        ));
        let json = serde_json_lenient::from_value(serde_json_lenient::json!({
            "import": [{"path": "example/lib", "alias": alias, "import-all": true}],
        }))
        .unwrap();
        let (json_pkg, _) = convert_test_pkg_json(json, false).unwrap();
        assert_eq!(
            serde_json_lenient::to_value(json_pkg.imports).unwrap(),
            serde_json_lenient::to_value(dsl_pkg.imports).unwrap(),
        );
    }

    for source in [
        r#"{"example/lib": "*"}"#,
        r#"[{"path": "example/lib", "alias": "*"}]"#,
    ] {
        let imports =
            pkg_json_imports_to_imports(Some(serde_json_lenient::from_str(source).unwrap()));
        assert!(matches!(
            &imports[..],
            [Import::Alias { alias: Some(alias), import_all: false, .. }] if alias == "*"
        ));
    }
}

#[test]
fn convert_pkg_dsl_combines_repeated_import_blocks() {
    for (suffix, key) in [
        ("", "import"),
        ("for \"test\"", "test-import"),
        ("for \"wbtest\"", "wbtest-import"),
    ] {
        for first in ["", r#""example/lib/first""#] {
            let dsl = crate::moon_pkg::parse(&format!(
                r#"
import {{ {first} }} {suffix}
import {{ "example/lib/second" }} {suffix}
"#,
            ))
            .unwrap();
            let (pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
            let imports = match key {
                "import" => pkg.imports,
                "test-import" => pkg.test_imports,
                _ => pkg.wbtest_imports,
            };
            assert_eq!(
                imports.iter().map(Import::get_path).collect::<Vec<_>>(),
                if first.is_empty() {
                    vec!["example/lib/second"]
                } else {
                    vec!["example/lib/first", "example/lib/second"]
                },
            );
        }
    }
}

#[test]
fn convert_pkg_dsl_rejects_duplicate_import_option_spellings() {
    for kind in ["test", "wbtest"] {
        let dsl = crate::moon_pkg::parse(&format!(
            r#"options("{kind}-import": [], {kind}_import: [])"#,
        ))
        .unwrap();
        let error = convert_test_pkg_dsl(dsl, false).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("duplicate field `{kind}-import`"),
        );
    }
}

#[test]
fn convert_pkg_imports_preserve_repeated_packages() {
    for (suffix, key) in [
        ("", "import"),
        ("for \"test\"", "test-import"),
        ("for \"wbtest\"", "wbtest-import"),
    ] {
        let json = serde_json_lenient::from_value(serde_json_lenient::json!({
            key: ["example/lib", {"path": "example/lib", "alias": "other"}],
        }))
        .unwrap();
        let (json_pkg, _) = convert_test_pkg_json(json, false).unwrap();
        let dsl = crate::moon_pkg::parse(&format!(
            r#"import {{ "example/lib", "example/lib" @other }} {suffix}"#,
        ))
        .unwrap();
        let (dsl_pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
        for pkg in [json_pkg, dsl_pkg] {
            let imports = match key {
                "import" => pkg.imports,
                "test-import" => pkg.test_imports,
                _ => pkg.wbtest_imports,
            };
            assert_eq!(
                imports.iter().map(Import::get_path).collect::<Vec<_>>(),
                ["example/lib", "example/lib"],
            );
        }
    }
}

#[test]
fn convert_pkg_json_imports_are_unconditional() {
    let json = serde_json_lenient::from_str(
        r#"{
          "import": [{
            "path": "example/lib/platform",
            "alias": "platform",
            "targets": ["native"]
          }]
        }"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_json(json, false).unwrap();
    // Unknown JSON fields retain their existing behavior: they are ignored.
    for &backend in TargetBackend::all() {
        assert!(pkg.imports[0].supports_backend(backend));
    }
}

#[test]
fn convert_pkg_dsl_rejects_import_blocks_with_legacy_options() {
    for (source, field) in [
        (r#"import {} options("import": [])"#, "import"),
        (
            r#"import { "example/lib", "example/lib" } options("import": ["example/other"])"#,
            "import",
        ),
        (
            r#"import {} for "test" options(test_import: [])"#,
            "test-import",
        ),
        (
            r#"import { "example/lib" } for "test" options("test-import": [])"#,
            "test-import",
        ),
        (
            r#"import {} for "wbtest" options(wbtest_import: [])"#,
            "wbtest-import",
        ),
        (
            r#"import { "example/lib" } for "wbtest" options("wbtest-import": [])"#,
            "wbtest-import",
        ),
    ] {
        let (log, capture) = UserLog::captured(log::LevelFilter::Warn);
        let error = convert_pkg_dsl_to_package(moon_pkg::parse(source).unwrap(), &log).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("duplicate field `{field}`"),
            "{source}"
        );
        // Reject ambiguous declarations before producing duplicate-item warnings.
        assert!(capture.take().is_empty());
    }
}

#[test]
fn convert_pkg_dsl_options_preserve_import_aliases() {
    let dsl = crate::moon_pkg::parse(
        r#"options(
  test_import: ["example/lib/test"],
  wbtest_import: ["example/lib/wbtest"],
)"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
    assert_eq!(pkg.test_imports.len(), 1);
    assert_eq!(pkg.test_imports[0].get_path(), "example/lib/test");
    assert_eq!(pkg.wbtest_imports.len(), 1);
    assert_eq!(pkg.wbtest_imports[0].get_path(), "example/lib/wbtest");
}

#[test]
fn convert_pkg_dsl_conditional_imports() {
    let dsl = crate::moon_pkg::parse(
        r#"
import { "example/lib/common" }
#cfg(target = "native")
import { "example/lib/native" @platform }
#cfg(not(target = "native"))
import { "example/lib/portable" @platform }
#cfg(any(target = "native", target = "js"))
import { "example/lib/test" } for "test"
#cfg(all(target = "native", true))
import { "example/lib/wbtest" } for "wbtest"
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
    assert_eq!(
        pkg.imports.iter().map(Import::get_path).collect::<Vec<_>>(),
        [
            "example/lib/common",
            "example/lib/native",
            "example/lib/portable"
        ]
    );
    assert_eq!(pkg.test_imports[0].get_path(), "example/lib/test");
    assert_eq!(pkg.wbtest_imports[0].get_path(), "example/lib/wbtest");
    for &backend in TargetBackend::all() {
        assert!(pkg.imports[0].supports_backend(backend));
        assert_eq!(
            pkg.imports[1].supports_backend(backend),
            matches!(backend, Native | LLVM)
        );
        assert_eq!(
            pkg.imports[2].supports_backend(backend),
            !matches!(backend, Native | LLVM)
        );
        assert_eq!(
            pkg.test_imports[0].supports_backend(backend),
            matches!(backend, Native | LLVM | Js)
        );
        assert_eq!(
            pkg.wbtest_imports[0].supports_backend(backend),
            matches!(backend, Native | LLVM)
        );
    }
}

#[test]
fn convert_pkg_imports_warns_and_preserves_duplicate_items() {
    for key in ["import", "test-import", "wbtest-import"] {
        for alias in ["", "@named", "*", "@named *"] {
            for emit_warnings in [false, true] {
                let dsl = moon_pkg::parse(&format!(
                    "import {{ \"example/lib\" {alias}, \"example/lib\" {alias} }} {}",
                    match key {
                        "test-import" => "for \"test\"",
                        "wbtest-import" => "for \"wbtest\"",
                        _ => "",
                    },
                ))
                .unwrap();
                let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
                let (pkg, _) = convert_pkg_dsl_to_package_with_supported_targets_decl(
                    dsl,
                    Path::new("moon.pkg"),
                    emit_warnings,
                    &user_log,
                )
                .unwrap();
                let imports = match key {
                    "import" => pkg.imports,
                    "test-import" => pkg.test_imports,
                    _ => pkg.wbtest_imports,
                };
                assert_eq!(imports.len(), 2);
                let warnings = capture.take();
                assert_eq!(warnings.len(), usize::from(emit_warnings));
                if emit_warnings {
                    assert_eq!(
                        warnings[0].message,
                        format!("Duplicate import of package `example/lib` in `{key}`.",)
                    );
                }
            }
        }
        let json = serde_json_lenient::from_value(serde_json_lenient::json!({
            key: ["example/lib", {"path": "example/lib", "alias": ""}, {"path": "example/lib", "alias": "other"}],
        })).unwrap();
        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
        let (pkg, _) =
            convert_pkg_json_to_package_with_supported_targets_decl(json, true, &user_log).unwrap();
        let imports = match key {
            "import" => pkg.imports,
            "test-import" => pkg.test_imports,
            _ => pkg.wbtest_imports,
        };
        assert_eq!(imports.len(), 3);
        assert!(matches!(&imports[0], Import::Simple(_)));
        assert!(
            matches!(&imports[2], Import::Alias { alias: Some(alias), .. } if alias == "other")
        );
        assert_eq!(capture.take().len(), 1);
    }
}

#[test]
fn convert_pkg_dsl_import_options_allow_other_import_kinds() {
    assert_pkg_dsl_matches_json(
        r#"import { "example/lib" }
options(test_import: ["example/test"], wbtest_import: ["example/wbtest"])"#,
        serde_json_lenient::json!({
            "import": ["example/lib"],
            "test-import": ["example/test"],
            "wbtest-import": ["example/wbtest"],
        }),
    );
}

#[test]
fn convert_pkg_imports_preserves_alias_precedence() {
    let dsl = moon_pkg::parse(
        r#"import { "example/lib" @a, "example/lib" @b }
import { "example/lib" @a }"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
    let aliases = pkg
        .imports
        .iter()
        .map(|import| match import {
            Import::Alias {
                alias: Some(alias), ..
            } => alias.as_str(),
            _ => panic!("explicit alias was lost"),
        })
        .collect::<Vec<_>>();
    assert_eq!(aliases, ["a", "b", "a"]);
}

#[test]
fn convert_pkg_dsl_duplicate_warnings_respect_import_conditions() {
    for (suffix, key) in [
        ("", "import"),
        ("for \"test\"", "test-import"),
        ("for \"wbtest\"", "wbtest-import"),
    ] {
        for (first, second, overlaps) in [
            (r#"target = "native""#, r#"not(target = "native")"#, false),
            (r#"target = "native""#, r#"target = "llvm""#, true),
            (r#"not(target = "native")"#, r#"target = "llvm""#, false),
            (r#"target = "native""#, r#"not(target = "llvm")"#, true),
            ("false", "true", false),
            ("any()", "all()", false),
            ("true", r#"target = "native""#, true),
            (
                r#"any(target = "native", target = "js")"#,
                r#"target = "native""#,
                true,
            ),
            (r#"target = "native""#, r#"target = "native""#, true),
        ] {
            for alias in ["", "@named", "*", "@named *"] {
                for emit_warnings in [false, true] {
                    let dsl = moon_pkg::parse(&format!(
                        r#"
#cfg({first})
import {{ "example/lib" {alias} }} {suffix}
#cfg({second})
import {{ "example/lib" {alias} }} {suffix}
"#,
                    ))
                    .unwrap();
                    let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
                    let (pkg, _) = convert_pkg_dsl_to_package_with_supported_targets_decl(
                        dsl,
                        Path::new("moon.pkg"),
                        emit_warnings,
                        &user_log,
                    )
                    .unwrap();
                    let imports = match key {
                        "import" => pkg.imports,
                        "test-import" => pkg.test_imports,
                        _ => pkg.wbtest_imports,
                    };
                    assert_eq!(imports.len(), 2);
                    let warnings = capture.take();
                    assert_eq!(
                        warnings.len(),
                        usize::from(overlaps && emit_warnings),
                        "{first}, {second}, {alias}, {key}"
                    );
                    if overlaps && emit_warnings {
                        assert_eq!(
                            warnings[0].message,
                            format!("Duplicate import of package `example/lib` in `{key}`.")
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn convert_pkg_dsl_cfg_boolean_and_stacked_conditions() {
    let dsl = crate::moon_pkg::parse(
        r#"
#cfg(all())
import { "example/lib/all" }
#cfg(any())
import { "example/lib/none" }
#cfg(false)
import { "example/lib/false" }
#cfg(any(target = "native", target = "js"))
#cfg(not(target = "js"))
import "test" { "example/lib/native" * }
import { "example/lib/unconditional" }
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
    for &backend in TargetBackend::all() {
        assert!(pkg.imports[0].supports_backend(backend));
        assert!(!pkg.imports[1].supports_backend(backend));
        assert!(!pkg.imports[2].supports_backend(backend));
        assert!(pkg.imports[3].supports_backend(backend));
        assert_eq!(
            pkg.test_imports[0].supports_backend(backend),
            matches!(backend, Native | LLVM)
        );
    }
    assert!(matches!(
        &pkg.test_imports[0],
        Import::Alias {
            import_all: true,
            ..
        }
    ));
}

#[test]
fn convert_pkg_dsl_supports_u32_values() {
    let json = crate::moon_pkg::parse(
        r#"
options(
  link: { "wasm": { "heap-start-address": 2214592512 } },
)
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(json, false).unwrap();

    assert_eq!(
        pkg.link.unwrap().wasm.unwrap().heap_start_address,
        Some(2_214_592_512)
    );
}

#[test]
fn convert_pkg_dsl_supports_leading_zero_integers() {
    let json = crate::moon_pkg::parse(r#"options("max-concurrent-tests": 08)"#).unwrap();
    let (pkg, _) = convert_test_pkg_dsl(json, false).unwrap();

    assert_eq!(pkg.max_concurrent_tests, Some(8));
}

#[test]
fn convert_pkg_dsl_rejects_values_above_u32() {
    let json = crate::moon_pkg::parse(
        r#"
options(
  link: { "wasm": { "heap-start-address": 4294967297 } },
)
"#,
    )
    .unwrap();
    assert!(convert_test_pkg_dsl(json, false).is_err());
}

#[test]
fn convert_pkg_dsl_supports_supported_targets_shorthand() {
    let json = crate::moon_pkg::parse(r#"supported_targets = "js""#).unwrap();
    let (pkg, decl_kind) = convert_test_pkg_dsl(json, true).unwrap();

    assert_eq!(
        pkg.supported_targets.iter().copied().collect::<Vec<_>>(),
        vec![Js]
    );
    assert_eq!(decl_kind, SupportedTargetsDeclKind::Expr);
}

#[test]
fn legacy_main_warning_recommends_pkgtype() {
    let (log, capture) = UserLog::captured(log::LevelFilter::Warn);
    let pkg =
        convert_pkg_dsl_to_package(moon_pkg::parse(r#"options(name: "main")"#).unwrap(), &log)
            .unwrap();
    assert!(pkg.is_main);
    let warnings = capture.take();
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].message,
        "The `name` field in `moon.pkg` is deprecated. Use `pkgtype(kind: \"executable\")` for an executable package."
    );
}

#[test]
fn convert_pkgtype_derives_is_main_and_force_link() {
    let cases = [
        ("library", false, false),
        ("executable", true, false),
        ("foreign_library", false, true),
    ];
    for (kind, want_is_main, want_force_link) in cases {
        let src = format!(r#"pkgtype(kind: "{kind}")"#);
        let json = crate::moon_pkg::parse(&src).unwrap();
        let (pkg, _) = convert_test_pkg_dsl(json, false).unwrap();
        assert_eq!(pkg.is_main, want_is_main, "is_main for kind {kind}");
        assert_eq!(
            pkg.force_link, want_force_link,
            "force_link for kind {kind}"
        );
    }
}

#[test]
fn convert_pkgtype_defaults_to_library_when_absent() {
    let json = crate::moon_pkg::parse("").unwrap();
    let (pkg, _) = convert_test_pkg_dsl(json, false).unwrap();
    assert!(!pkg.is_main);
    assert!(!pkg.force_link);
}

#[test]
fn convert_pkgtype_conflicts_with_is_main() {
    let json = crate::moon_pkg::parse(
        r#"
pkgtype(kind: "library")
options("is-main": true)
"#,
    )
    .unwrap();
    let err = convert_test_pkg_dsl(json, false).unwrap_err();
    assert!(
        err.to_string().contains("conflicts with `is-main"),
        "unexpected error: {err}"
    );
}

#[test]
fn convert_pkgtype_conflicts_with_link_bool() {
    let json = crate::moon_pkg::parse(
        r#"
pkgtype(kind: "library")
options("link": true)
"#,
    )
    .unwrap();
    let err = convert_test_pkg_dsl(json, false).unwrap_err();
    assert!(
        err.to_string().contains("conflicts with `link"),
        "unexpected error: {err}"
    );
}

#[test]
fn convert_pkgtype_accepts_redundant_consistent_flags() {
    let json = crate::moon_pkg::parse(
        r#"
pkgtype(kind: "executable")
options("is-main": true)
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(json, false).unwrap();
    assert!(pkg.is_main);
    assert!(!pkg.force_link);
}

#[test]
fn convert_pkgtype_foreign_library_allows_structured_link() {
    // A structured `link: { ... }` is pure config, not a force-link signal,
    // so it must not conflict with `pkgtype`.
    let json = crate::moon_pkg::parse(
        r#"
pkgtype(kind: "foreign_library")
options("link": { "js": { "format": "esm" } })
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(json, false).unwrap();
    assert!(!pkg.is_main);
    assert!(pkg.force_link);
    assert!(pkg.link.is_some());
}

#[test]
fn convert_pkg_dsl_supports_proof_enabled() {
    let json = crate::moon_pkg::parse(
        r#"
options(
  "proof-enabled": true,
)
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(json, true).unwrap();

    assert!(pkg.proof_enabled);
}

#[test]
fn convert_pkg_dsl_rejects_removed_use_async_io() {
    let err = convert_test_pkg_dsl(
        crate::moon_pkg::parse("use_async_io = true").unwrap(),
        false,
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Unexpected key 'use_async_io' found in moon.pkg."
    );
}

#[test]
fn convert_pkg_dsl_supports_dev_build_rule() {
    let json = crate::moon_pkg::parse(
        r#"
dev_build(rule: "rule1", input: "abc", output: "def")
"#,
    )
    .unwrap();

    let (pkg, _) = convert_test_pkg_dsl(json, true).unwrap();
    let pre_build = pkg.pre_build.unwrap();
    let MoonPkgGenerate::Rule {
        rule,
        input,
        output,
        ..
    } = &pre_build[0]
    else {
        panic!("expected rule pre-build");
    };
    assert_eq!(rule, "rule1");
    let StringOrArray::String(input) = input else {
        panic!("expected string input");
    };
    assert_eq!(input, "abc");
    let StringOrArray::String(output) = output else {
        panic!("expected string output");
    };
    assert_eq!(output, "def");
}

#[test]
fn convert_pkg_dsl_supports_package_local_rules() {
    let json = crate::moon_pkg::parse(
        r#"
rule(name: "rule1", command: "exe1 $input -o $output")
dev_build(rule: "rule1", input: "abc", output: "def")
"#,
    )
    .unwrap();

    let (pkg, _) = convert_test_pkg_dsl(json, true).unwrap();
    let rules = pkg.local_rules.as_ref().expect("expected local rules");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "rule1");
    assert_eq!(rules[0].command, "exe1 $input -o $output");
}

#[test]
fn convert_pkg_dsl_rejects_duplicate_package_local_rule_names() {
    let json = crate::moon_pkg::parse(
        r#"
rule(name: "rule1", command: "exe1")
rule(name: "rule1", command: "exe2")
"#,
    )
    .unwrap();

    let err = convert_test_pkg_dsl(json, true).unwrap_err();
    assert!(
        err.to_string()
            .contains("Duplicate rule name `rule1` found in moon.pkg.")
    );
}

#[test]
fn convert_pkg_dsl_rejects_mixed_dev_build_and_pre_build() {
    for (option, field) in [
        ("dev_build", "dev_build"),
        ("pre-build", "pre_build"),
        ("pre_build", "pre_build"),
    ] {
        let source = format!(
            r#"
dev_build(rule: "rule1", input: "abc", output: "def")
options("{option}": [])
"#
        );
        let err = convert_test_pkg_dsl(moon_pkg::parse(&source).unwrap(), true).unwrap_err();
        assert_eq!(err.to_string(), format!("duplicate field `{field}`"));
    }
}

#[test]
fn convert_pkg_json_supports_proof_enabled_hyphenated() {
    let json: MoonPkgJSON = serde_json_lenient::from_str(
        r#"
{
  "proof-enabled": true
}
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_json(json, true).unwrap();

    assert!(pkg.proof_enabled);
}

#[test]
fn validate_pkg_json_schema() {
    let schema = schemars::schema_for!(MoonPkgJSON);
    let actual = &serde_json_lenient::to_string_pretty(&schema).unwrap();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../moonbuild/template/pkg.schema.json"
    );
    expect_test::expect_file![path].assert_eq(actual);

    let html_template_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../moonbuild/template/pkg_json_schema.html"
    );
    let html_template = std::fs::read_to_string(html_template_path).unwrap();
    let content = html_template.replace("const schema = {}", &format!("const schema = {actual}"));
    let html_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/manual/src/source/pkg_json_schema.html"
    );
    std::fs::write(html_path, &content).unwrap();

    let zh_html_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/manual-zh/src/source/pkg_json_schema.html"
    );
    std::fs::write(zh_html_path, content).unwrap();
}

#[test]
fn package_manifest_warnings_are_local_only() {
    let should_warn = |path: &std::path::Path| {
        !path
            .components()
            .any(|component| component.as_os_str() == crate::constants::DEP_PATH)
    };

    assert!(should_warn(std::path::Path::new(
        "/tmp/project/main/moon.pkg"
    )));
    assert!(!should_warn(std::path::Path::new(
        "/tmp/project/.mooncakes/user/pkg/moon.pkg"
    )));
}
