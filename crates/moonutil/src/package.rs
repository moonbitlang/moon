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

use indexmap::{IndexMap, IndexSet};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json_lenient::{Map, Value};

use crate::{
    cond_expr::CondExprs,
    module::MoonModRule,
    target::TargetBackend::{self, Js, LLVM, Native, Wasm, WasmGC},
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
        // checking that exactly one command source was provided.
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
        match (command, rule) {
            (Some(command), None) => Ok(Self::Direct {
                input,
                output,
                command,
                extra,
            }),
            (None, Some(rule)) => Ok(Self::Rule {
                input,
                output,
                rule,
                extra,
            }),
            (Some(_), Some(_)) => Err(serde::de::Error::custom(
                "`command` and `rule` cannot be used together",
            )),
            (None, None) => Err(serde::de::Error::custom("expected `command` or `rule`")),
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
