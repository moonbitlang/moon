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
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use colored::Colorize;
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    common::{
        FileName, GeneratedTestDriver, TargetBackend, TargetBackend::Js, TargetBackend::Native,
        TargetBackend::Wasm, TargetBackend::WasmGC, TargetBackend::LLVM,
    },
    cond_expr::{CompileCondition, CondExpr, RawTargets},
    path::{ImportComponent, PathComponent},
};

#[derive(Debug, Clone)]
pub struct Package {
    pub is_main: bool,
    pub need_link: bool,
    pub is_third_party: bool,
    // Absolute fs path to the root directory of the package, already consider
    // `source` field in moon.mod.json
    // e.g. after `moon add moonbitlang/x`
    // root_path of package `moonbitlang/x/stack` is
    // $WORKSPACE/.mooncakes/moonbitlang/x/`{source}`
    pub root_path: PathBuf,
    // moonbitlang/x
    pub root: PathComponent,
    // stack
    pub rel: PathComponent,
    // *.mbt (exclude the following)
    pub files: IndexMap<PathBuf, CompileCondition>,
    //  *_wbtest.mbt
    pub wbtest_files: IndexMap<PathBuf, CompileCondition>,
    //  *_test.mbt
    pub test_files: IndexMap<PathBuf, CompileCondition>,
    pub mbt_md_files: IndexMap<PathBuf, CompileCondition>,
    pub files_contain_test_block: Vec<PathBuf>,
    pub imports: Vec<ImportComponent>,
    pub wbtest_imports: Vec<ImportComponent>,
    pub test_imports: Vec<ImportComponent>,
    pub generated_test_drivers: Vec<GeneratedTestDriver>,
    pub artifact: PathBuf,

    pub link: Option<Link>,

    // moon.mod.json + moon.pkg.json + cli passing value
    pub warn_list: Option<String>,
    pub alert_list: Option<String>,

    pub targets: Option<IndexMap<FileName, CondExpr>>,
    pub pre_build: Option<Vec<MoonPkgGenerate>>,

    pub patch_file: Option<PathBuf>,
    pub no_mi: bool,

    pub doc_test_patch_file: Option<PathBuf>,

    pub install_path: Option<PathBuf>,

    pub bin_name: Option<String>,

    pub bin_target: TargetBackend,

    pub enable_value_tracing: bool,

    pub supported_targets: HashSet<TargetBackend>,

    pub native_stub: Option<Vec<String>>,
}

impl Package {
    pub fn full_name(&self) -> String {
        if self.rel.full_name().is_empty() {
            self.root.full_name()
        } else {
            format!("{}/{}", self.root.full_name(), self.rel.full_name())
        }
    }

    pub fn last_name(&self) -> &str {
        if self.rel.components.is_empty() {
            self.root.components.last().unwrap()
        } else {
            self.rel.components.last().unwrap()
        }
    }

    pub fn full_components(&self) -> PathComponent {
        let mut comps = self.root.components.clone();
        comps.extend(self.rel.components.iter().cloned());
        PathComponent { components: comps }
    }

    pub fn get_all_files(&self) -> Vec<String> {
        let mut files =
            Vec::with_capacity(self.files.len() + self.test_files.len() + self.wbtest_files.len());
        files.extend(
            self.files
                .keys()
                .chain(self.test_files.keys())
                .chain(self.wbtest_files.keys())
                .map(|x| x.file_name().unwrap().to_str().unwrap().to_string()),
        );
        files
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
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
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct AliasJSON {
    pub path: String,
    pub alias: String,
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
        alias: Option<String>,
        value: Option<Vec<String>>,
    },
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BoolOrLink {
    Bool(bool),
    Link(Box<Link>),
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

    /// Warn list setting of the package
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "warn-list")]
    #[serde(alias = "warn_list")]
    #[schemars(rename = "warn-list")]
    pub warn_list: Option<String>,

    /// Alert list setting of the package
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "alert-list")]
    #[serde(alias = "alert_list")]
    #[schemars(rename = "alert-list")]
    pub alert_list: Option<String>,

    /// Conditional compilation targets
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "targets")]
    #[schemars(rename = "targets")]
    #[schemars(with = "Option<std::collections::HashMap<String, StringOrArray>>")]
    pub targets: Option<RawTargets>,

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

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "supported-targets")]
    #[schemars(rename = "supported-targets")]
    pub supported_targets: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "native-stub")]
    #[schemars(rename = "native-stub")]
    pub native_stub: Option<Vec<String>>,
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

    pub native_stub: Option<Vec<String>>,
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

    pub fn native_cc(&self, b: TargetBackend) -> Option<&str> {
        match b {
            Native => self.link.as_ref()?.native.as_ref()?.cc.as_deref(),
            _ => None,
        }
    }

    pub fn native_cc_flags(&self, b: TargetBackend) -> Option<&str> {
        match b {
            Native => self.link.as_ref()?.native.as_ref()?.cc_flags.as_deref(),
            _ => None,
        }
    }

    pub fn native_cc_link_flags(&self, b: TargetBackend) -> Option<&str> {
        match b {
            Native => self.link.as_ref()?.native.as_ref()?.cc_link_flags.as_deref(),
            _ => None,
        }
    }

    pub fn native_stub_deps(&self) -> Option<&[String]> {
        self.link.as_ref()?.native.as_ref()?.native_stub_deps.as_deref()
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

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub struct NativeLinkConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exports: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc_flags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc_link_flags: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub_cc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub_cc_flags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub_cc_link_flags: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub native_stub_deps: Option<Vec<String>>,
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

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize, JsonSchema,
)]
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

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MoonPkg {
    pub name: Option<String>,
    pub is_main: bool,
    pub need_link: bool,
    pub imports: Vec<Import>,
    pub wbtest_imports: Vec<Import>,
    pub test_imports: Vec<Import>,

    pub link: Option<Link>,
    pub warn_list: Option<String>,
    pub alert_list: Option<String>,

    pub targets: Option<RawTargets>,

    pub pre_build: Option<Vec<MoonPkgGenerate>>,

    pub bin_name: Option<String>,
    pub bin_target: TargetBackend,

    pub supported_targets: HashSet<TargetBackend>,

    pub native_stub: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum Import {
    Simple(String),
    Alias { path: String, alias: String },
}

impl Import {
    pub fn get_path(&self) -> &str {
        match self {
            Self::Simple(v) => v,
            Self::Alias { path, alias: _ } => path,
        }
    }
}

pub fn convert_pkg_json_to_package(j: MoonPkgJSON) -> anyhow::Result<MoonPkg> {
    let get_imports = |source: Option<PkgJSONImport>| -> Vec<Import> {
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
                                        alias: v.unwrap(),
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
                            } => match alias {
                                None => imports.push(Import::Simple(path)),
                                Some(alias) if alias.is_empty() => {
                                    imports.push(Import::Simple(path))
                                }
                                Some(alias) => imports.push(Import::Alias { path, alias }),
                            },
                        }
                    }
                }
            }
        };
        imports
    };

    let imports = get_imports(j.import);
    let wbtest_imports = get_imports(j.wbtest_import);
    let test_imports = get_imports(j.test_import);

    let mut is_main = j.is_main.unwrap_or(false);
    if let Some(name) = &j.name {
        if name == "main" {
            is_main = true;
            eprintln!(
                "{}",
                "Warning: The `name` field in `moon.pkg.json` is now deprecated. For the main package, please use `\"is-main\": true` instead. Refer to the latest documentation at https://www.moonbitlang.com/docs/build-system-tutorial for more information.".yellow()
                    .bold()
            );
        }
    }
    let need_link = match &j.link {
        None => false,
        Some(BoolOrLink::Bool(b)) => *b,
        Some(BoolOrLink::Link(_)) => true,
    };

    // TODO: check on the fly
    let mut alias_dedup: HashSet<String> = HashSet::new();
    for item in imports.iter() {
        let alias = match item {
            Import::Simple(p) => {
                let alias = Path::new(p)
                    .file_stem()
                    .context(format!("failed to get alias of `{}`", p))?
                    .to_str()
                    .unwrap()
                    .to_string();
                alias
            }
            Import::Alias { path: _path, alias } => alias.clone(),
        };
        if alias_dedup.contains(&alias) {
            bail!("Duplicate alias `{}`", alias);
        } else {
            alias_dedup.insert(alias.clone());
        }
    }

    // TODO: check on the fly
    let mut alias_dedup: HashSet<String> = HashSet::new();
    for item in wbtest_imports.iter() {
        let alias = match item {
            Import::Simple(p) => {
                let alias = Path::new(p)
                    .file_stem()
                    .context(format!("failed to get alias of `{}`", p))?
                    .to_str()
                    .unwrap()
                    .to_string();
                alias
            }
            Import::Alias { path: _path, alias } => alias.clone(),
        };
        if alias_dedup.contains(&alias) {
            bail!("Duplicate alias `{}`", alias);
        } else {
            alias_dedup.insert(alias.clone());
        }
    }

    // TODO: check on the fly
    let mut alias_dedup: HashSet<String> = HashSet::new();
    for item in test_imports.iter() {
        let alias = match item {
            Import::Simple(p) => {
                let alias = Path::new(p)
                    .file_stem()
                    .context(format!("failed to get alias of `{}`", p))?
                    .to_str()
                    .unwrap()
                    .to_string();
                alias
            }
            Import::Alias { path: _path, alias } => alias.clone(),
        };
        if alias_dedup.contains(&alias) {
            bail!("Duplicate alias `{}`", alias);
        } else {
            alias_dedup.insert(alias.clone());
        }
    }

    let bin_target = if let Some(ref b) = j.bin_target {
        TargetBackend::str_to_backend(b)?
    } else {
        TargetBackend::WasmGC
    };

    let mut supported_backends = HashSet::new();
    if let Some(ref b) = j.supported_targets {
        for backend in b.iter() {
            supported_backends.insert(TargetBackend::str_to_backend(backend)?);
        }
    } else {
        // if supported_backends in moon.pkg.json is not set, then set it to all backends
        supported_backends.extend(vec![
            TargetBackend::Wasm,
            TargetBackend::WasmGC,
            TargetBackend::Js,
            TargetBackend::Native,
            TargetBackend::LLVM,
        ]);
    };

    let result = MoonPkg {
        name: None,
        is_main,
        need_link,
        imports,
        wbtest_imports,
        test_imports,
        link: match j.link {
            None => None,
            Some(BoolOrLink::Bool(_)) => None,
            Some(BoolOrLink::Link(l)) => Some(*l),
        },
        warn_list: j.warn_list,
        alert_list: j.alert_list,
        targets: j.targets,
        pre_build: j.pre_build,
        bin_name: j.bin_name,
        bin_target,
        supported_targets: supported_backends,
        native_stub: j.native_stub,
    };
    Ok(result)
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
    let content = html_template.replace("const schema = {}", &format!("const schema = {}", actual));
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
