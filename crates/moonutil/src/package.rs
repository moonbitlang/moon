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

use std::path::PathBuf;

use anyhow::bail;
use colored::Colorize;
use indexmap::{IndexMap, IndexSet};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json_lenient::Value;

pub use crate::supported_targets::resolve_supported_targets;
use crate::{
    common::TargetBackend::{self, Js, LLVM, Native, Wasm, WasmGC},
    cond_expr::{CompileCondition, CondExprs},
    moon_pkg,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PackageJSON {
    pub is_main: bool,
    pub is_third_party: bool,
    pub root_path: String,
    pub root: String,
    pub rel: String,
    pub files: IndexMap<PathBuf, CompileCondition>,
    // white box test
    pub wbtest_files: IndexMap<PathBuf, CompileCondition>,
    // black box test
    pub test_files: IndexMap<PathBuf, CompileCondition>,
    // *.mbt.md
    pub mbt_md_files: IndexMap<PathBuf, CompileCondition>,
    pub deps: Vec<AliasJSON>,
    pub wbtest_deps: Vec<AliasJSON>,
    pub test_deps: Vec<AliasJSON>,
    pub artifact: String,
    pub supported_targets: Vec<TargetBackend>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AliasJSON {
    pub path: String,
    pub alias: String,
    pub fspath: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
pub struct MoonPkgFormatterJSON {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<std::collections::HashSet<String>>")]
    pub ignore: Option<IndexSet<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MoonPkgFormatter {
    pub ignore: IndexSet<String>,
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
        value: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(alias = "sub-package")]
        #[serde(rename(serialize = "sub-package"))]
        #[schemars(rename = "sub-package")]
        sub_package: Option<bool>,
    },
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BoolOrLink {
    Bool(bool),
    Link(Box<Link>),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubPackageInMoonPkgJSON {
    pub files: Vec<String>,
    pub import: Option<PkgJSONImport>,
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

    /// Specify whether this package is a main package or not
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "is-main")]
    #[serde(alias = "is_main")]
    #[serde(rename(serialize = "is-main"))]
    #[schemars(rename = "is-main")]
    pub is_main: Option<bool>,

    /// Specify whether this package is a sub package or not
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "sub-package")]
    #[serde(rename(serialize = "sub-package"))]
    #[schemars(rename = "sub-package")]
    pub sub_package: Option<SubPackageInMoonPkgJSON>,

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

    /// Whether to import all definitions from the package being tested
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "test-import-all")]
    #[schemars(rename = "test-import-all")]
    pub test_import_all: Option<bool>,

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
    #[serde(alias = "bin-name")]
    #[schemars(rename = "bin-name")]
    pub bin_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "bin-target")]
    #[schemars(rename = "bin-target")]
    pub bin_target: Option<String>,

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
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct VirtualPkg {
    #[serde(alias = "has-default")]
    #[schemars(rename = "has-default")]
    pub has_default: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[schemars(rename = "import-memory")]
pub struct ImportMemory {
    pub module: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[schemars(rename = "memory-limits")]
pub struct MemoryLimits {
    pub min: u32,
    pub max: u32,
}

#[derive(Debug, Clone)]
pub struct LinkDepItem {
    pub out: String,
    pub core_deps: Vec<String>, // need add parent's core files recursively
    pub package_full_name: String,
    pub package_sources: Vec<(String, String)>, // (pkgname, source_dir)
    pub package_path: PathBuf,
    pub link: Option<Link>,
    pub install_path: Option<PathBuf>,
    pub bin_name: Option<String>,

    pub stub_lib: Option<Vec<String>>,
}

#[rustfmt::skip]
impl LinkDepItem {
    pub fn wasm_exports(&self) -> Option<&[String]> { self.link.as_ref()?.wasm.as_ref()?.exports.as_deref() }
    pub fn wasm_export_memory_name(&self) -> Option<&str> { self.link.as_ref()?.wasm.as_ref()?.export_memory_name.as_deref() }
    pub fn wasm_import_memory(&self) -> Option<&ImportMemory> { self.link.as_ref()?.wasm.as_ref()?.import_memory.as_ref() }
    pub fn wasm_memory_limits(&self) -> Option<&MemoryLimits> { self.link.as_ref()?.wasm.as_ref()?.memory_limits.as_ref() }
    pub fn wasm_shared_memory(&self) -> Option<bool> { self.link.as_ref()?.wasm.as_ref()?.shared_memory }
    pub fn wasm_heap_start_address(&self) -> Option<u32> { self.link.as_ref()?.wasm.as_ref()?.heap_start_address }
    pub fn wasm_link_flags(&self) -> Option<&[String]> { self.link.as_ref()?.wasm.as_ref()?.flags.as_deref() }

    pub fn wasm_gc_exports(&self) -> Option<&[String]> { self.link.as_ref()?.wasm_gc.as_ref()?.exports.as_deref() }
    pub fn wasm_gc_export_memory_name(&self) -> Option<&str> { self.link.as_ref()?.wasm_gc.as_ref()?.export_memory_name.as_deref() }
    pub fn wasm_gc_import_memory(&self) -> Option<&ImportMemory> { self.link.as_ref()?.wasm_gc.as_ref()?.import_memory.as_ref() }
    pub fn wasm_gc_memory_limits(&self) -> Option<&MemoryLimits> { self.link.as_ref()?.wasm_gc.as_ref()?.memory_limits.as_ref() }
    pub fn wasm_gc_shared_memory(&self) -> Option<bool> { self.link.as_ref()?.wasm_gc.as_ref()?.shared_memory }
    pub fn wasm_gc_link_flags(&self) -> Option<&[String]> { self.link.as_ref()?.wasm_gc.as_ref()?.flags.as_deref() }

    pub fn js_exports(&self) -> Option<&[String]> { self.link.as_ref()?.js.as_ref()?.exports.as_deref() }

    pub fn native_exports(&self) -> Option<&[String]> { self.link.as_ref()?.native.as_ref()?.exports.as_deref() }

    pub fn exports(&self, b: TargetBackend) -> Option<&[String]> {
        match b {
            Wasm => self.wasm_exports(),
            WasmGC => self.wasm_gc_exports(),
            Js => self.js_exports(),
            Native => self.native_exports(),
            LLVM => None,
        }
    }

    pub fn export_memory_name(&self, b: TargetBackend) -> Option<&str> {
        match b {
            Wasm => self.wasm_export_memory_name(),
            WasmGC => self.wasm_gc_export_memory_name(),
            Js => None,
            Native => None,
            LLVM => None,
        }
    }

    pub fn heap_start_address(&self, b: TargetBackend) -> Option<u32> {
        match b {
            Wasm => self.wasm_heap_start_address(),
            WasmGC => None,
            Js => None,
            Native => None,
            LLVM => None,
        }
    }

    pub fn import_memory(&self, b: TargetBackend) -> Option<&ImportMemory> {
        match b {
            Wasm => self.wasm_import_memory(),
            WasmGC => self.wasm_gc_import_memory(),
            Js => None,
            Native => None,
            LLVM => None,
        }
    }

    pub fn memory_limits(&self, b:TargetBackend) -> Option<&MemoryLimits> {
        match b {
            Wasm => self.wasm_memory_limits(),
            WasmGC => self.wasm_gc_memory_limits(),
            Js => None,
            Native => None,
            LLVM => None,
        }
    }

    pub fn shared_memory(&self, b: TargetBackend) -> Option<bool> {
        match b {
            Wasm => self.wasm_shared_memory(),
            WasmGC => self.wasm_gc_shared_memory(),
            Js => None,
            Native => None,
            LLVM => None,
        }
    }

    pub fn link_flags(&self, b: TargetBackend) -> Option<&[String]> {
        match b {
            Wasm => self.wasm_link_flags(),
            WasmGC => self.wasm_gc_link_flags(),
            Js => None,
            Native => None,
            LLVM => None,
        }
    }

}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct WasmLinkConfig {
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
}

/// Native C/C++ compilation and linking configuration for MoonBit packages.
///
/// Controls how C stub files and main executables are compiled and linked.
/// The build system uses these flags differently depending on compilation mode:
/// normal mode creates static libraries, while TCC mode creates dynamic libraries.
#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct NativeLinkConfig {
    // FIXME: We have no way to force link a native library when not `is_main`
    /// Function exports for the final native executable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exports: Option<Vec<String>>,

    /// Custom C compiler for main MoonBit-generated C code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<String>,

    /// Compiler flags for main MoonBit-generated C code (whitespace-separated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc_flags: Option<String>,

    /// Linker flags for main executable (also used for stub dynamic libraries in TCC mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc_link_flags: Option<String>,

    /// Custom C compiler for C stub files
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub_cc: Option<String>,

    /// Compiler flags for C stub compilation (whitespace-separated)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub_cc_flags: Option<String>,

    /// Linker flags for C stub linking (only used in TCC mode for dynamic libraries)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub_cc_link_flags: Option<String>,

    /// Compiled stub object files as dependencies for the executable
    ///
    /// (should not be present in the `pkg.json`, generated and populated later)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub stub_lib_deps: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub struct WasmGcLinkConfig {
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

    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_string_constants: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct JsLinkConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exports: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<JsFormat>,
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

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct MoonPkgGenerate {
    pub input: StringOrArray,
    pub output: StringOrArray,
    pub command: String,
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
    pub is_main: bool,
    pub force_link: bool,
    pub sub_package: Option<SubPackageInMoonPkg>,
    pub imports: Vec<Import>,
    pub wbtest_imports: Vec<Import>,
    pub test_imports: Vec<Import>,
    pub formatter: MoonPkgFormatter,

    pub link: Option<Link>,
    pub warn_list: Option<String>,
    pub proof_enabled: bool,

    pub targets: Option<CondExprs>,

    pub pre_build: Option<Vec<MoonPkgGenerate>>,

    pub bin_name: Option<String>,
    pub bin_target: Option<TargetBackend>,

    pub supported_targets: IndexSet<TargetBackend>,

    pub native_stub: Option<Vec<String>>,

    pub virtual_pkg: Option<VirtualPkg>,
    pub implement: Option<String>,
    pub overrides: Option<Vec<String>>,

    pub max_concurrent_tests: Option<u32>,

    pub regex_backend: Option<RegexBackend>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Import {
    Simple(String),
    Alias {
        path: String,
        alias: Option<String>,
        sub_package: bool,
    },
}

impl Import {
    pub fn get_path(&self) -> &str {
        match self {
            Self::Simple(v) => v,
            Self::Alias {
                path,
                alias: _,
                sub_package: _,
            } => path,
        }
    }
}

/// Convert moon.pkg DSL (with `options` key) to MoonPkg struct
pub fn convert_pkg_dsl_to_package(dsl: moon_pkg::Dsl) -> anyhow::Result<MoonPkg> {
    Ok(convert_pkg_dsl_to_package_with_supported_targets_decl(dsl, true)?.0)
}

pub fn convert_pkg_dsl_to_package_with_supported_targets_decl(
    dsl: moon_pkg::Dsl,
    emit_warnings: bool,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    // It will validate the top-level keys and merge `options` into the root level.
    // Might be removed in the future, after we remove the moon.pkg.json and have an
    // AST to represent moon.pkg files.

    // Top-level DSL keys accepted in `moon.pkg`; the boolean says whether
    // repeated entries should be collected as a JSON array instead of rejected.
    let toplevel_keys = std::collections::HashMap::from([
        ("import", false),
        ("wbtest-import", false),
        ("test-import", false),
        ("options", false),
        ("warnings", false),
        ("supported_targets", false),
    ]);
    let mut map = serde_json_lenient::Map::new();
    for (key, value) in dsl.iter() {
        let Some(&allow_duplicate) = toplevel_keys.get(key) else {
            bail!("Unexpected key '{}' found in moon.pkg.", key);
        };
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
    if let Value::Object(options) = map.remove("options").unwrap_or_default() {
        for (k, v) in options {
            map.insert(k, v);
        }
    }
    if let Some(warnings) = map.remove("warnings") {
        let warnings = match warnings {
            Value::String(s) => s,
            _ => String::new(),
        };
        let legacy_warn_list = match map.remove("warn-list") {
            Some(Value::String(s)) => s,
            _ => String::new(),
        };
        let merged = format!("{warnings}{legacy_warn_list}");
        if !merged.is_empty() {
            map.insert(String::from("warn-list"), Value::String(merged));
        }
    }
    let supported_targets = map.remove("supported_targets");
    let legacy_supported_targets = map.remove("supported-targets");
    match (supported_targets, legacy_supported_targets) {
        (Some(supported_targets), Some(_)) => {
            if emit_warnings {
                eprintln!(
                    "{}",
                    "Warning: Both `supported_targets = ...` and `options(\"supported-targets\": ...)` are set in `moon.pkg`. Using `supported_targets` and ignoring the old `options(\"supported-targets\")` value."
                        .yellow()
                        .bold()
                );
            }
            map.insert(String::from("supported-targets"), supported_targets);
        }
        (Some(supported_targets), None) => {
            map.insert(String::from("supported-targets"), supported_targets);
        }
        (None, Some(legacy_supported_targets)) => {
            if emit_warnings {
                eprintln!(
                    "{}",
                    "Warning: `options(\"supported-targets\": ...)` in `moon.pkg` is deprecated. Please use `supported_targets = ...` instead."
                        .yellow()
                        .bold()
                );
            }
            map.insert(String::from("supported-targets"), legacy_supported_targets);
        }
        (None, None) => {}
    }
    let json = Value::Object(map);
    let pkg_json: MoonPkgJSON = serde_json_lenient::from_value(json)?;
    convert_pkg_json_to_package_with_supported_targets_decl(pkg_json, emit_warnings)
}

pub fn pkg_json_imports_to_imports(source: Option<PkgJSONImport>) -> Vec<Import> {
    let mut imports = vec![];
    if let Some(im) = source {
        match im {
            PkgJSONImport::Map(m) => {
                for (k, v) in m.into_iter() {
                    match &v {
                        None => imports.push(Import::Simple(k)),
                        Some(p) => {
                            if p.is_empty() {
                                imports.push(Import::Simple(k));
                            } else {
                                imports.push(Import::Alias {
                                    path: k,
                                    alias: v,
                                    sub_package: false,
                                })
                            }
                        }
                    }
                }
            }
            PkgJSONImport::List(l) => {
                for item in l.into_iter() {
                    match item {
                        PkgJSONImportItem::String(s) => imports.push(Import::Simple(s)),
                        PkgJSONImportItem::Object {
                            path,
                            alias,
                            value: _,
                            sub_package,
                        } => match (alias, sub_package) {
                            (None, None) => imports.push(Import::Simple(path)),
                            (Some(alias), None) if alias.is_empty() => {
                                imports.push(Import::Simple(path))
                            }
                            (Some(alias), _) => imports.push(Import::Alias {
                                path,
                                alias: Some(alias),
                                sub_package: sub_package.unwrap_or(false),
                            }),
                            (_, Some(sub_package)) => imports.push(Import::Alias {
                                path: path.clone(),
                                alias: Some(path.split('/').next_back().unwrap().to_string()),
                                sub_package,
                            }),
                        },
                    }
                }
            }
        }
    };
    imports
}

pub fn convert_pkg_json_to_package_with_supported_targets_decl(
    j: MoonPkgJSON,
    emit_warnings: bool,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    let get_imports =
        |source: Option<PkgJSONImport>| -> Vec<Import> { pkg_json_imports_to_imports(source) };

    let sub_package = j.sub_package.map(|s| SubPackageInMoonPkg {
        files: s.files,
        import: get_imports(s.import),
    });
    let imports = get_imports(j.import);
    let wbtest_imports = get_imports(j.wbtest_import);
    let test_imports = get_imports(j.test_import);
    let formatter_cfg = j.formatter.unwrap_or_default();
    let formatter = MoonPkgFormatter {
        ignore: formatter_cfg.ignore.unwrap_or_default(),
    };

    let mut is_main = j.is_main.unwrap_or(false);
    if let Some(name) = &j.name
        && name == "main"
    {
        is_main = true;
        if emit_warnings {
            eprintln!(
                "{}",
                "Warning: The `name` field in `moon.pkg` is now deprecated. For the main package, please use `\"is-main\": true` instead. Refer to the latest documentation at https://www.moonbitlang.com/docs/build-system-tutorial for more information."
                    .yellow()
                    .bold()
            );
        }
    }
    let force_link = match &j.link {
        None => false,
        Some(BoolOrLink::Bool(b)) => *b,
        Some(BoolOrLink::Link(_)) => false,
    };

    let bin_target = j
        .bin_target
        .as_ref()
        .map(|s| TargetBackend::str_to_backend(s))
        .transpose()?;

    let (supported_backends, supported_targets_decl_kind) =
        resolve_supported_targets(j.supported_targets.as_ref())?;

    let result = MoonPkg {
        name: None,
        is_main,
        force_link,
        sub_package,
        imports,
        wbtest_imports,
        test_imports,
        formatter,
        link: match j.link {
            None => None,
            Some(BoolOrLink::Bool(_)) => None,
            Some(BoolOrLink::Link(l)) => Some(*l),
        },
        warn_list: j.warn_list,
        proof_enabled: j.proof_enabled.unwrap_or(false),
        targets: j.targets,
        pre_build: j.pre_build,
        bin_name: j.bin_name,
        bin_target,
        supported_targets: supported_backends,
        native_stub: j.native_stub,
        virtual_pkg: j.virtual_pkg,
        implement: j.implement,
        overrides: j.overrides,
        max_concurrent_tests: j.max_concurrent_tests,
        regex_backend: j.regex_backend,
    };
    Ok((result, supported_targets_decl_kind))
}

#[test]
fn convert_pkg_dsl_supports_supported_targets_shorthand() {
    let json = crate::moon_pkg::parse(r#"supported_targets = "js""#).unwrap();
    let (pkg, decl_kind) =
        convert_pkg_dsl_to_package_with_supported_targets_decl(json, true).unwrap();

    assert_eq!(
        pkg.supported_targets.iter().copied().collect::<Vec<_>>(),
        vec![Js]
    );
    assert_eq!(decl_kind, SupportedTargetsDeclKind::Expr);
}

#[test]
fn convert_pkg_dsl_prefers_supported_targets_over_options_supported_targets() {
    let json = crate::moon_pkg::parse(
        r#"
supported_targets = "js"
options(
  "supported-targets": "+native",
)
"#,
    )
    .unwrap();
    let (pkg, decl_kind) =
        convert_pkg_dsl_to_package_with_supported_targets_decl(json, true).unwrap();

    assert_eq!(
        pkg.supported_targets.iter().copied().collect::<Vec<_>>(),
        vec![Js]
    );
    assert_eq!(decl_kind, SupportedTargetsDeclKind::Expr);
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
    let (pkg, _) = convert_pkg_dsl_to_package_with_supported_targets_decl(json, true).unwrap();

    assert!(pkg.proof_enabled);
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
    let (pkg, _) = convert_pkg_json_to_package_with_supported_targets_decl(json, true).unwrap();

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
            .any(|component| component.as_os_str() == crate::common::DEP_PATH)
    };

    assert!(should_warn(std::path::Path::new(
        "/tmp/project/main/moon.pkg"
    )));
    assert!(!should_warn(std::path::Path::new(
        "/tmp/project/.mooncakes/user/pkg/moon.pkg"
    )));
}
