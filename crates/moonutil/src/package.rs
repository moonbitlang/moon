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

use std::{collections::HashSet, path::PathBuf};

use anyhow::bail;
use indexmap::{IndexMap, IndexSet};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json_lenient::Value;

pub use crate::supported_targets::resolve_supported_targets;
use crate::{
    cond_expr::{CompileCondition, CondExprs},
    module::MoonModRule,
    moon_pkg,
    target::TargetBackend::{self, Js, LLVM, Native, Wasm, WasmGC},
    user_log::UserLog,
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
    pub check_command: Option<Vec<String>>,
    pub wbtest_check_command: Option<Vec<String>>,
    pub test_check_command: Option<Vec<String>>,
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

/// The kind of a package, declared via `pkgtype(kind: "...")` in `moon.pkg`.
///
/// This supersedes the deprecated `is-main` and `link: true` flags:
/// - `library` (the default): a normal library, emits a `.core` consumed by
///   other MoonBit packages.
/// - `executable`: has a `main` and links to a runnable artifact
///   (equivalent to the deprecated `is-main: true`).
/// - `foreign_library`: a non-main library force-linked into a standalone,
///   foreign-consumable artifact (equivalent to the deprecated `link: true`).
///   Not yet supported on the native/LLVM backends.
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

    /// Legacy linker flags for C stubs, retained for manifest compatibility
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

    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_string_constants: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema)]
pub struct JsLinkConfig {
    /// **Deprecated:** use the `#export_name` attribute in MoonBit source instead.
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
#[serde(untagged)]
pub enum MoonPkgGenerate {
    Direct {
        input: StringOrArray,
        output: StringOrArray,
        command: String,
    },
    Rule {
        rule: String,
        input: StringOrArray,
        output: StringOrArray,
    },
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

    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_rules: Option<Vec<MoonModRule>>,
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
        /// Backends selected by import blocks; absent means unconditional.
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
    fn from_json(path: String, alias: Option<String>, sub_package: bool) -> Self {
        let alias = alias.filter(|alias| sub_package || !alias.is_empty());
        if alias.is_none() && !sub_package {
            Self::Simple(path)
        } else {
            Self::Alias {
                path,
                alias,
                sub_package,
                targets: None,
            }
        }
    }
}

/// Imports decoded from either manifest syntax, before legacy options override
/// DSL blocks and normalization combines their conditions and repeated items.
#[derive(Default)]
struct PackageImports {
    regular: Vec<Import>,
    whitebox: Vec<Import>,
    blackbox: Vec<Import>,
}

impl PackageImports {
    fn take_from_json(json: &mut MoonPkgJSON) -> Self {
        Self {
            regular: pkg_json_imports_to_imports(json.import.take()),
            whitebox: pkg_json_imports_to_imports(json.wbtest_import.take()),
            blackbox: pkg_json_imports_to_imports(json.test_import.take()),
        }
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

/// Combine blocks per backend, then share identical ordered import lists.
/// Default aliases stay unresolved until the package's module boundary is known.
fn normalize_imports(
    imports: Vec<Import>,
    kind: &str,
    emit_warnings: bool,
    user_log: &UserLog,
) -> Vec<Import> {
    let mut by_backend = TargetBackend::all()
        .iter()
        .map(|&backend| (backend, Vec::new()))
        .collect::<Vec<_>>();
    let mut active = HashSet::new();
    let mut duplicates = IndexMap::<_, IndexSet<TargetBackend>>::new();
    for import in &imports {
        let (path, alias, sub_package) = match import {
            Import::Simple(path) => (path.as_str(), None, false),
            Import::Alias {
                path,
                alias,
                sub_package,
                ..
            } => (path.as_str(), alias.as_deref(), *sub_package),
        };
        for (backend, items) in &mut by_backend {
            if !import.supports_backend(*backend) {
                continue;
            }
            if !active.insert((path, *backend)) {
                duplicates.entry(path).or_default().insert(*backend);
            }
            // Keep source order and duplicate items: existing dependency
            // resolution uses both first insertion order and final aliases.
            items.push((path, alias, sub_package));
        }
    }
    if emit_warnings {
        for (path, targets) in duplicates {
            let targets = TargetBackend::all()
                .iter()
                .filter(|target| targets.contains(*target))
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            user_log.warn(format!(
                "Duplicate import of package `{path}` in `{kind}` for targets [{targets}]."
            ));
        }
    }
    let mut combined = IndexMap::<_, IndexSet<TargetBackend>>::new();
    for (backend, items) in by_backend {
        combined.entry(items).or_default().insert(backend);
    }
    combined
        .into_iter()
        .flat_map(|(items, targets)| {
            let targets = (targets.len() != TargetBackend::all().len()).then_some(targets);
            items.into_iter().map(move |(path, alias, sub_package)| {
                if alias.is_none() && !sub_package && targets.is_none() {
                    Import::Simple(path.to_owned())
                } else {
                    Import::Alias {
                        path: path.to_owned(),
                        alias: alias.map(str::to_owned),
                        sub_package,
                        targets: targets.clone(),
                    }
                }
            })
        })
        .collect()
}

/// Convert a parsed `moon.pkg` into the package model used by ordinary callers.
///
/// Whether `supported_targets` used the legacy or current spelling is migration
/// metadata needed by manifest loading, not part of normal package conversion.
pub fn convert_pkg_dsl_to_package(
    dsl: moon_pkg::Dsl,
    user_log: &UserLog,
) -> anyhow::Result<MoonPkg> {
    Ok(convert_pkg_dsl_to_package_with_supported_targets_decl(dsl, true, user_log)?.0)
}

pub(crate) fn convert_pkg_dsl_to_package_with_supported_targets_decl(
    dsl: moon_pkg::Dsl,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    // TODO: Remove the legacy JSON adapter for non-import options once moon.pkg
    // conversion has a typed AST.

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
    ]);
    let mut map = serde_json_lenient::Map::new();
    let mut imports = PackageImports::default();
    for (key, value) in dsl.iter() {
        let Some(&allow_duplicate) = toplevel_keys.get(key) else {
            bail!("Unexpected key '{}' found in moon.pkg.", key);
        };
        if let Some((_, imports)) = imports.get_mut(key) {
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
    if let Value::Object(options) = map.remove("options").unwrap_or_default() {
        let mut seen_import_keys = HashSet::new();
        for (k, v) in options {
            if let Some((key, imports)) = imports.get_mut(&k) {
                if !seen_import_keys.insert(key) {
                    bail!("Duplicate key '{key}' found in moon.pkg options.");
                }
                *imports = pkg_json_imports_to_imports(serde_json_lenient::from_value(v)?);
            } else {
                map.insert(k, v);
            }
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
                let warning = "Both `supported_targets = ...` and `options(\"supported-targets\": ...)` are set in `moon.pkg`. Using `supported_targets` and ignoring the old `options(\"supported-targets\")` value.";
                user_log.warn(warning);
            }
            map.insert(String::from("supported-targets"), supported_targets);
        }
        (Some(supported_targets), None) => {
            map.insert(String::from("supported-targets"), supported_targets);
        }
        (None, Some(legacy_supported_targets)) => {
            if emit_warnings {
                let warning = "`options(\"supported-targets\": ...)` in `moon.pkg` is deprecated. Please use `supported_targets = ...` instead.";
                user_log.warn(warning);
            }
            map.insert(String::from("supported-targets"), legacy_supported_targets);
        }
        (None, None) => {}
    }
    let dev_build = map.remove("dev_build");
    if let Some(v) = dev_build {
        if map.contains_key("pre-build") {
            bail!("`dev_build` cannot be used together with `pre-build` in moon.pkg.");
        }
        map.insert(String::from("pre-build"), v);
    }
    let json = Value::Object(map);
    let pkg_json: MoonPkgJSON = serde_json_lenient::from_value(json)?;
    convert_package_with_imports(pkg_json, imports, emit_warnings, user_log)
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
                    sub_package,
                    ..
                } => Import::from_json(path, alias, sub_package.unwrap_or(false)),
            })
            .collect(),
    }
}

pub fn convert_pkg_json_to_package_with_supported_targets_decl(
    mut j: MoonPkgJSON,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    let imports = PackageImports::take_from_json(&mut j);
    convert_package_with_imports(j, imports, emit_warnings, user_log)
}

fn convert_package_with_imports(
    j: MoonPkgJSON,
    imports: PackageImports,
    emit_warnings: bool,
    user_log: &UserLog,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    let sub_package = j.sub_package.map(|s| SubPackageInMoonPkg {
        files: s.files,
        import: normalize_imports(
            pkg_json_imports_to_imports(s.import),
            "sub-package.import",
            emit_warnings,
            user_log,
        ),
    });
    let imports_regular = normalize_imports(imports.regular, "import", emit_warnings, user_log);
    let wbtest_imports =
        normalize_imports(imports.whitebox, "wbtest-import", emit_warnings, user_log);
    let test_imports = normalize_imports(imports.blackbox, "test-import", emit_warnings, user_log);
    let formatter_cfg = j.formatter.unwrap_or_default();
    let formatter = MoonPkgFormatter {
        ignore: formatter_cfg.ignore.unwrap_or_default(),
    };

    // Legacy `is-main`, including the deprecated `name == "main"` alias.
    let mut legacy_is_main = j.is_main.unwrap_or(false);
    if let Some(name) = &j.name
        && name == "main"
    {
        legacy_is_main = true;
        if emit_warnings {
            let warning = "The `name` field in `moon.pkg` is now deprecated. For the main package, please use `\"is-main\": true` instead. Refer to the latest documentation at https://www.moonbitlang.com/docs/build-system-tutorial for more information.";
            user_log.warn(warning);
        }
    }
    // Legacy `force_link` from the boolean `link: true` (a structured
    // `link: { ... }` config is not a force-link signal).
    let legacy_force_link = match &j.link {
        None => false,
        Some(BoolOrLink::Bool(b)) => *b,
        Some(BoolOrLink::Link(_)) => false,
    };

    // `pkgtype` is the source of truth when present. The legacy `is-main` /
    // `link: true` flags are honored only as a fallback during migration; an
    // explicitly-set legacy flag that contradicts `pkgtype` is a hard error,
    // while a redundant-but-consistent one only warns.
    let (is_main, force_link) = match j.pkgtype.as_ref().map(|p| p.kind) {
        Some(kind) => {
            let (want_main, want_force_link) = kind.to_flags();
            if let Some(is_main_flag) = j.is_main {
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
            if let Some(BoolOrLink::Bool(link_flag)) = &j.link {
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

    let bin_target = j
        .bin_target
        .as_ref()
        .map(|s| TargetBackend::str_to_backend(s))
        .transpose()?;

    let (supported_backends, supported_targets_decl_kind) =
        resolve_supported_targets(j.supported_targets.as_ref())?;

    if let Some(rules) = &j.rule {
        let mut names = HashSet::new();
        for rule in rules {
            if !names.insert(rule.name.as_str()) {
                bail!("Duplicate rule name `{}` found in moon.pkg.", rule.name);
            }
        }
    }

    let result = MoonPkg {
        name: None,
        is_main,
        force_link,
        sub_package,
        imports: imports_regular,
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
        local_rules: j.rule,
    };
    Ok((result, supported_targets_decl_kind))
}

#[cfg(test)]
fn convert_test_pkg_dsl(
    dsl: moon_pkg::Dsl,
    emit_warnings: bool,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    convert_pkg_dsl_to_package_with_supported_targets_decl(
        dsl,
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
fn convert_pkg_imports_preserve_aliases_and_subpackages() {
    let dsl = crate::moon_pkg::parse(
        r#"import { "example/lib/default", "example/lib/named" @named, "example/lib/all" * }"#,
    )
    .unwrap();
    let (dsl_pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
    for imports in [
        r#"["example/lib/default", {"path": "example/lib/named", "alias": "named"}, {"path": "example/lib/all", "alias": "*"}]"#,
        r#"{"example/lib/default": null, "example/lib/named": "named", "example/lib/all": "*"}"#,
    ] {
        let json = serde_json_lenient::from_str(&format!(r#"{{"import": {imports}}}"#)).unwrap();
        let (json_pkg, _) = convert_test_pkg_json(json, false).unwrap();
        assert_eq!(
            serde_json_lenient::to_value(json_pkg.imports).unwrap(),
            serde_json_lenient::to_value(&dsl_pkg.imports).unwrap(),
        );
    }

    let json = serde_json_lenient::from_str(
        r#"{
          "import": [{"path": "a/b/v2", "sub-package": true}],
          "sub-package": {"files": ["part.mbt"], "import": ["example/lib"]}
        }"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_json(json, false).unwrap();
    assert!(matches!(
        &pkg.imports[0],
        Import::Alias {
            alias: None,
            sub_package: true,
            ..
        }
    ));
    assert_eq!(pkg.sub_package.unwrap().import[0].get_path(), "example/lib");
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
fn convert_pkg_dsl_import_options_override_blocks() {
    for (suffix, key) in [
        ("", "import"),
        ("for \"test\"", "test-import"),
        ("for \"test\"", "test_import"),
        ("for \"wbtest\"", "wbtest-import"),
        ("for \"wbtest\"", "wbtest_import"),
    ] {
        for replacement in [r#"["example/lib/replacement"]"#, "[]"] {
            let dsl = crate::moon_pkg::parse(&format!(
                r#"
import {{ "example/lib/original" }} {suffix}
options("{key}": {replacement})
"#,
            ))
            .unwrap();
            let (pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
            let imports = match suffix {
                "" => pkg.imports,
                "for \"test\"" => pkg.test_imports,
                _ => pkg.wbtest_imports,
            };
            let expected = if replacement == "[]" {
                vec![]
            } else {
                vec!["example/lib/replacement"]
            };
            assert_eq!(
                imports.iter().map(Import::get_path).collect::<Vec<_>>(),
                expected
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
            format!("Duplicate key '{kind}-import' found in moon.pkg options."),
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
fn convert_pkg_dsl_drops_inactive_imports() {
    for (prefix, suffix) in [
        ("", ""),
        ("", "for \"test\""),
        ("", "for \"wbtest\""),
        ("\"test\"", ""),
        ("\"wbtest\"", ""),
    ] {
        let dsl = crate::moon_pkg::parse(&format!(
            r#"#cfg(false) import {prefix} {{ "example/dep" }} {suffix}"#,
        ))
        .unwrap();
        let (pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
        assert!(pkg.imports.is_empty());
        assert!(pkg.test_imports.is_empty());
        assert!(pkg.wbtest_imports.is_empty());
    }
}

#[test]
fn convert_pkg_dsl_combines_import_conditions() {
    for (suffix, key) in [
        ("", "imports"),
        ("for \"test\"", "test_imports"),
        ("for \"wbtest\"", "wbtest_imports"),
    ] {
        for (first, second, counts, warning) in [
            (
                "target = \"native\"",
                "target = \"js\"",
                [0, 0, 1, 1, 0],
                false,
            ),
            (
                "any(target = \"native\", target = \"js\")",
                "target = \"native\"",
                [0, 0, 1, 2, 0],
                true,
            ),
            (
                "target = \"native\"",
                "not(target = \"native\")",
                [1, 1, 1, 1, 1],
                false,
            ),
            ("true", "target = \"native\"", [1, 1, 1, 2, 1], true),
        ] {
            let dsl = moon_pkg::parse(&format!(
                r#"
#cfg({first})
import {{ "example/lib" @lib }} {suffix}
#cfg({second})
import {{ "example/lib" @lib }} {suffix}
"#,
            ))
            .unwrap();
            let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
            let pkg = convert_pkg_dsl_to_package(dsl, &user_log).unwrap();
            let imports = match key {
                "imports" => pkg.imports,
                "test_imports" => pkg.test_imports,
                _ => pkg.wbtest_imports,
            };
            for (&backend, expected) in TargetBackend::all().iter().zip(counts) {
                assert_eq!(
                    imports
                        .iter()
                        .filter(|import| import.supports_backend(backend))
                        .count(),
                    expected
                );
            }
            assert!(imports.iter().all(|import| matches!(import, Import::Alias { alias: Some(alias), .. } if alias == "lib")));
            if !warning {
                assert_eq!(imports.len(), 1);
            }
            let warnings = capture.take();
            assert_eq!(warnings.len(), usize::from(warning));
            if warning {
                assert_eq!(
                    warnings[0].message,
                    format!(
                        "Duplicate import of package `example/lib` in `{}` for targets [native].",
                        key.replace('_', "-").trim_end_matches('s'),
                    )
                );
            }
        }
    }
}

#[test]
fn convert_pkg_imports_warns_and_preserves_duplicate_items() {
    for key in ["import", "test-import", "wbtest-import"] {
        for alias in ["", "@named", "*"] {
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
                        format!(
                            "Duplicate import of package `example/lib` in `{key}` for targets [wasm, wasm-gc, js, native, llvm].",
                        )
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
fn convert_pkg_dsl_overrides_imports_before_normalization() {
    let dsl = moon_pkg::parse(
        r#"
#cfg(target = "native")
import { "example/unused", "example/unused" }
options("import": ["example/replacement"])
"#,
    )
    .unwrap();
    let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
    let pkg = convert_pkg_dsl_to_package(dsl, &user_log).unwrap();
    assert!(matches!(&pkg.imports[..], [Import::Simple(path)] if path == "example/replacement"));
    assert!(capture.take().is_empty());
}

#[test]
fn convert_pkg_dsl_preserves_distinct_conditional_aliases() {
    for (second, overlaps) in [
        ("target = \"native\"", true),
        ("not(target = \"native\")", false),
    ] {
        let dsl = moon_pkg::parse(&format!(
            r#"
#cfg(target = "native")
import {{ "example/lib" @first }}
#cfg({second})
import {{ "example/lib" @second }}
import {{ "example/lib" }} for "test"
import {{ "example/lib" }} for "wbtest"
"#
        ))
        .unwrap();
        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
        let pkg = convert_pkg_dsl_to_package(dsl, &user_log).unwrap();
        assert_eq!(pkg.imports.len(), 2);
        for &backend in TargetBackend::all() {
            let aliases = pkg
                .imports
                .iter()
                .filter(|import| import.supports_backend(backend))
                .map(|import| match import {
                    Import::Alias {
                        alias: Some(alias), ..
                    } => alias.as_str(),
                    _ => panic!("explicit alias was lost"),
                })
                .collect::<Vec<_>>();
            assert_eq!(
                aliases,
                match (backend == Native, overlaps) {
                    (true, true) => vec!["first", "second"],
                    (true, false) => vec!["first"],
                    (false, true) => vec![],
                    (false, false) => vec!["second"],
                }
            );
            assert!(pkg.test_imports[0].supports_backend(backend));
            assert!(pkg.wbtest_imports[0].supports_backend(backend));
        }
        assert_eq!(capture.take().len(), usize::from(overlaps));
    }
}

#[test]
fn convert_pkg_json_imports_ignore_unknown_conditions() {
    let json = serde_json_lenient::from_str(
        r#"{
        "import": [{"path": "example/lib", "targets": ["Native"]}]
    }"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_json(json, false).unwrap();
    assert!(matches!(&pkg.imports[..], [Import::Simple(path)] if path == "example/lib"));
}

#[test]
fn convert_pkg_imports_preserves_alias_precedence() {
    for (source, conditional) in [
        (
            r#"import { "example/lib" @a, "example/lib" @b, "example/lib" @a }"#,
            false,
        ),
        (
            r#"
#cfg(target = "native")
import { "example/lib" @a }
import { "example/lib" @b }
#cfg(not(target = "native"))
import { "example/lib" @a }
"#,
            true,
        ),
    ] {
        let pkg = convert_pkg_dsl_to_package(
            moon_pkg::parse(source).unwrap(),
            &UserLog::new(log::LevelFilter::Error),
        )
        .unwrap();
        for &backend in TargetBackend::all() {
            // Dependency resolution retains the final edge for each package.
            let last = pkg
                .imports
                .iter()
                .rfind(|import| import.supports_backend(backend))
                .unwrap();
            let expected = if conditional && backend == Native {
                "b"
            } else {
                "a"
            };
            assert!(matches!(last, Import::Alias { alias: Some(alias), .. } if alias == expected));
        }
    }
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
    let (pkg, decl_kind) = convert_test_pkg_dsl(json, true).unwrap();

    assert_eq!(
        pkg.supported_targets.iter().copied().collect::<Vec<_>>(),
        vec![Js]
    );
    assert_eq!(decl_kind, SupportedTargetsDeclKind::Expr);
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
    let json = crate::moon_pkg::parse(
        r#"
dev_build(rule: "rule1", input: "abc", output: "def")
options(
  "pre-build": [],
)
"#,
    )
    .unwrap();

    let err = convert_test_pkg_dsl(json, true).unwrap_err();
    assert!(
        err.to_string()
            .contains("`dev_build` cannot be used together with `pre-build`")
    );
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
