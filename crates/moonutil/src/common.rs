use anyhow::{bail, Context};
use colored::Colorize;
use fs4::FileExt;
use indexmap::IndexMap;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub const MOON_MOD: &str = "moon.mod";
pub const MOON_PKG: &str = "moon.pkg";
pub const MOON_MOD_JSON: &str = "moon.mod.json";
pub const MOON_PKG_JSON: &str = "moon.pkg.json";
pub const MOON_PID_NAME: &str = ".moon.pid";

pub const RSP_THRESHOLD: usize = 8196;

pub const MOON_TEST_DELIMITER_BEGIN: &str = "----- BEGIN MOON TEST RESULT -----";
pub const MOON_TEST_DELIMITER_END: &str = "----- END MOON TEST RESULT -----";

pub const MOON_COVERAGE_DELIMITER_BEGIN: &str = "----- BEGIN MOONBIT COVERAGE -----";
pub const MOON_COVERAGE_DELIMITER_END: &str = "----- END MOONBIT COVERAGE -----";

pub const MOON_LOCK: &str = ".moon-lock";

mod dependency;
pub use dependency::{DependencyInfo, DependencyInfoJson};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Module {
    pub name: String,
    pub version: Option<Version>,
    pub deps: IndexMap<String, DependencyInfo>,
    pub readme: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub description: Option<String>,

    pub compile_flags: Option<Vec<String>>,
    pub link_flags: Option<Vec<String>>,
    pub checksum: Option<String>,

    /// Fields not covered by the info above, which should be left as-is.
    #[serde(flatten)]
    pub ext: serde_json_lenient::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LinkConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exports: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "export-memory-name")]
    pub export_memory_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<Vec<String>>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JsLinkConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exports: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<JsFormat>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Link {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasm: Option<LinkConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "wasm-gc")]
    pub wasm_gc: Option<LinkConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub js: Option<JsLinkConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Package {
    pub name: Option<String>,
    pub is_main: bool,
    pub need_link: bool,
    pub imports: Vec<Import>,
    pub test_imports: Vec<Import>,

    pub link: Option<Link>,
    pub warn_list: Option<String>,
    pub alert_list: Option<String>,
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

pub fn get_mbt_files(dir: &Path) -> Vec<String> {
    let mut moons = vec![];
    let entries = std::fs::read_dir(dir).unwrap();
    for entry in entries.flatten() {
        if let Ok(t) = entry.file_type() {
            if (t.is_file() || t.is_symlink())
                && entry.path().extension().is_some()
                && entry.path().extension().unwrap() == "mbt"
            {
                moons.push(entry.path().display().to_string())
            }
        }
    }
    moons
}

pub fn get_short_and_long_name(module_name: &str, path: &str) -> (String, String) {
    let short = Path::new(path)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let long = startswith_and_trim(path, module_name);
    (short, long)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PackageListItem {
    pub name: String,
    pub path: String,
    pub deps: Vec<String>,
}

pub fn startswith_and_trim(s: &str, t: &str) -> String {
    if s.starts_with(t) {
        s.replacen(t, "", 1)
    } else {
        s.into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MoonModJSON {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<Version>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub deps: Option<IndexMap<String, DependencyInfoJson>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub readme: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub compile_flags: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_flags: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,

    /// Fields not covered by the info above, which should be left as-is.
    #[serde(flatten)]
    pub ext: serde_json_lenient::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum PkgJSONImport {
    Map(IndexMap<String, Option<String>>),
    List(Vec<PkgJSONImportItem>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum PkgJSONImportItem {
    String(String),
    Object { path: String, alias: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BoolOrLink {
    Bool(bool),
    Link(Link),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MoonPkgJSON {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "is-main")]
    #[serde(alias = "is_main")]
    #[serde(rename(serialize = "is-main"))]
    pub is_main: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub import: Option<PkgJSONImport>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "test-import")]
    #[serde(alias = "test_import")]
    pub test_import: Option<PkgJSONImport>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<BoolOrLink>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "warn-list")]
    #[serde(alias = "warn_list")]
    pub warn_list: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "alert-list")]
    #[serde(alias = "alert_list")]
    pub alert_list: Option<String>,
}

pub fn convert_mod_json_to_module(j: MoonModJSON) -> anyhow::Result<Module> {
    let deps = match j.deps {
        None => IndexMap::new(),
        Some(d) => d.into_iter().map(|(k, v)| (k, v.into())).collect(),
    };

    Ok(Module {
        name: j.name,
        version: j.version,
        deps,
        readme: j.readme,
        repository: j.repository,
        license: j.license,
        keywords: j.keywords,
        description: j.description,

        compile_flags: j.compile_flags,
        link_flags: j.link_flags,
        checksum: j.checksum,
        ext: j.ext,
    })
}

pub fn convert_module_to_mod_json(m: Module) -> MoonModJSON {
    MoonModJSON {
        name: m.name,
        version: m.version,
        deps: Some(m.deps.into_iter().map(|(k, v)| (k, v.into())).collect()),
        readme: m.readme,
        repository: m.repository,
        license: m.license,
        keywords: m.keywords,
        description: m.description,

        compile_flags: m.compile_flags,
        link_flags: m.link_flags,
        checksum: m.checksum,
        ext: m.ext,
    }
}

impl TryFrom<MoonModJSON> for Module {
    type Error = anyhow::Error;

    fn try_from(val: MoonModJSON) -> Result<Self, Self::Error> {
        convert_mod_json_to_module(val)
    }
}

impl From<Module> for MoonModJSON {
    fn from(val: Module) -> Self {
        convert_module_to_mod_json(val)
    }
}

pub fn convert_pkg_json_to_package(j: MoonPkgJSON) -> anyhow::Result<Package> {
    let mut imports: Vec<Import> = vec![];
    if let Some(im) = j.import {
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
                        PkgJSONImportItem::Object { path, alias } => {
                            if alias.is_empty() {
                                imports.push(Import::Simple(path));
                            } else {
                                imports.push(Import::Alias { path, alias })
                            }
                        }
                    }
                }
            }
        }
    };

    let mut test_imports: Vec<Import> = vec![];
    if let Some(im) = j.test_import {
        match im {
            PkgJSONImport::Map(m) => {
                for (k, v) in m.into_iter() {
                    match &v {
                        None => test_imports.push(Import::Simple(k)),
                        Some(p) => {
                            if p.is_empty() {
                                test_imports.push(Import::Simple(k));
                            } else {
                                test_imports.push(Import::Alias {
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
                        PkgJSONImportItem::String(s) => test_imports.push(Import::Simple(s)),
                        PkgJSONImportItem::Object { path, alias } => {
                            if alias.is_empty() {
                                test_imports.push(Import::Simple(path));
                            } else {
                                test_imports.push(Import::Alias { path, alias })
                            }
                        }
                    }
                }
            }
        }
    };

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

    let result = Package {
        name: None,
        is_main,
        need_link,
        imports,
        test_imports,
        link: match j.link {
            None => None,
            Some(BoolOrLink::Bool(_)) => None,
            Some(BoolOrLink::Link(l)) => Some(l),
        },
        warn_list: j.warn_list,
        alert_list: j.alert_list,
    };
    Ok(result)
}

pub fn read_module_from_json(path: &Path) -> anyhow::Result<Module> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let j =
        serde_json_lenient::from_reader(reader).context(format!("Failed to parse {:?}", path))?;
    convert_mod_json_to_module(j)
}

fn read_package_from_json(path: &Path) -> anyhow::Result<Package> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let j =
        serde_json_lenient::from_reader(reader).context(format!("Failed to parse {:?}", path))?;
    convert_pkg_json_to_package(j)
}

pub fn write_module_json_to_file(m: &MoonModJSON, source_dir: &Path) -> anyhow::Result<()> {
    let p = source_dir.join(MOON_MOD_JSON);
    let file = File::create(p)?;
    let mut writer = BufWriter::new(file);
    serde_json_lenient::to_writer_pretty(&mut writer, &m)?;
    Ok(())
}

pub fn write_package_json_to_file(pkg: &MoonPkgJSON, path: &Path) -> anyhow::Result<()> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    serde_json_lenient::to_writer_pretty(&mut writer, &pkg)?;
    Ok(())
}

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::str::FromStr;

pub fn check_moon_pkg_exist(dir: &Path) -> bool {
    let moon_pkg_path = dir.join(MOON_PKG);
    let moon_pkg_json_path = dir.join(MOON_PKG_JSON);
    moon_pkg_path.exists() || moon_pkg_json_path.exists()
}

pub fn read_module_desc_file_in_dir(dir: &Path) -> anyhow::Result<Module> {
    if !dir.join(MOON_MOD_JSON).exists() {
        bail!("`{:?}` does not exist", dir.join(MOON_MOD_JSON));
    }
    read_module_from_json(&dir.join(MOON_MOD_JSON))
}

pub fn read_package_desc_file_in_dir(dir: &Path) -> anyhow::Result<Package> {
    if !dir.join(MOON_PKG_JSON).exists() {
        bail!("`{:?}` does not exist", dir.join(MOON_PKG_JSON));
    }
    read_package_from_json(&dir.join(MOON_PKG_JSON))
        .context(format!("Failed to load {:?}", dir.join(MOON_PKG_JSON)))
}

use clap::ValueEnum;

pub use crate::dirs::check_moon_mod_exists;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
#[repr(u8)]
pub enum OutputFormat {
    Wat,
    Wasm,
    Js,
}

impl OutputFormat {
    pub fn to_str(&self) -> &str {
        match self {
            OutputFormat::Wat => "wat",
            OutputFormat::Wasm => "wasm",
            OutputFormat::Js => "js",
        }
    }
}

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Serialize, Deserialize, Default,
)]
#[repr(u8)]
pub enum TargetBackend {
    Wasm,
    #[default]
    WasmGC,
    Js,
}

impl TargetBackend {
    pub fn to_flag(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm-gc",
            Self::Js => "js",
        }
    }

    pub fn to_extension(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm",
            Self::Js => "js",
        }
    }

    pub fn to_dir_name(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm-gc",
            Self::Js => "js",
        }
    }

    pub fn to_backend_ext(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm-gc",
            Self::Js => "js",
        }
    }
}

impl FromStr for TargetBackend {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "wasm" => Ok(Self::Wasm),
            "wasm-gc" => Ok(Self::WasmGC),
            "js" => Ok(Self::Js),
            _ => Err("invalid target backend"),
        }
    }
}

pub fn is_slash(c: char) -> bool {
    c == '/' || c == '\\'
}

#[derive(Debug, Clone)]
pub struct BuildPackageFlags {
    pub debug_flag: bool,
    pub source_map: bool,
    pub enable_coverage: bool,
    // key: package name, value: warn_list for this package
    pub warn_lists: HashMap<String, Option<String>>,
    pub alert_lists: HashMap<String, Option<String>>,
    // treat all warnings as errors
    pub deny_warn: bool,
    pub target_backend: TargetBackend,
}

impl Default for BuildPackageFlags {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildPackageFlags {
    pub fn new() -> Self {
        Self {
            debug_flag: false,
            source_map: false,
            enable_coverage: false,
            warn_lists: HashMap::new(),
            alert_lists: HashMap::new(),
            deny_warn: false,
            target_backend: TargetBackend::default(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LinkCoreFlags {
    pub debug_flag: bool,
    pub source_map: bool,
    pub output_format: OutputFormat,
    pub target_backend: TargetBackend,
}

impl Default for LinkCoreFlags {
    fn default() -> Self {
        Self::new()
    }
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

#[derive(Debug, Clone, Default)]
pub struct MoonbuildOpt {
    pub source_dir: PathBuf,
    pub target_dir: PathBuf,
    pub test_opt: Option<TestOpt>,
    pub sort_input: bool,
    pub run_mode: RunMode,
    pub fmt_opt: Option<FmtOpt>,
    pub args: Vec<String>,
    pub verbose: bool,
    pub quiet: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TestOpt {
    pub filter_package: Option<HashSet<PathBuf>>,
    pub filter_file: Option<String>,
    pub filter_index: Option<u32>,
}

impl TestOpt {
    pub fn to_command(&self) -> Vec<String> {
        let mut command_str = Vec::new();
        if let Some(filter_package) = &self.filter_package {
            command_str.push("--package".into());
            filter_package.iter().for_each(|pkg| {
                command_str.push(pkg.display().to_string());
            });
        }
        if let Some(filter_file) = &self.filter_file {
            command_str.push("--file".into());
            command_str.push(filter_file.into());
        }
        if let Some(filter_index) = self.filter_index {
            command_str.push("--index".into());
            command_str.push(filter_index.to_string());
        }
        command_str
    }
}

#[derive(Debug, Clone, Default)]
pub struct FmtOpt {
    pub check: bool,
}

#[derive(Debug, Clone)]
pub struct MooncOpt {
    pub build_opt: BuildPackageFlags,
    pub link_opt: LinkCoreFlags,
    pub extra_build_opt: Vec<String>,
    pub extra_link_opt: Vec<String>,
    pub nostd: bool,
    pub render: bool,
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
            render: false,
        }
    }
}

pub const DEP_PATH: &str = ".mooncakes";

pub const IGNORE_DIRS: &[&str] = &["target", ".git", DEP_PATH];

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
    fn mkdir_p(&self);

    /// Returns a list of all files and directories underneath the given
    /// directory, recursively, including the starting path.
    fn ls_r(&self) -> Vec<PathBuf>;
}

impl CargoPathExt for Path {
    fn rm_rf(&self) {
        let meta = match self.symlink_metadata() {
            Ok(meta) => meta,
            Err(e) => {
                if e.kind() == ErrorKind::NotFound {
                    return;
                }
                panic!("failed to remove {:?}, could not read: {:?}", self, e);
            }
        };
        // There is a race condition between fetching the metadata and
        // actually performing the removal, but we don't care all that much
        // for our tests.
        if meta.is_dir() {
            if let Err(e) = fs::remove_dir_all(self) {
                panic!("failed to remove {:?}: {:?}", self, e)
            }
        } else if let Err(e) = fs::remove_file(self) {
            panic!("failed to remove {:?}: {:?}", self, e)
        }
    }

    fn mkdir_p(&self) {
        fs::create_dir_all(self)
            .unwrap_or_else(|e| panic!("failed to mkdir_p {}: {}", self.display(), e))
    }

    fn ls_r(&self) -> Vec<PathBuf> {
        walkdir::WalkDir::new(self)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.map(|e| e.path().to_owned()).ok())
            .collect()
    }
}

pub fn get_src_dst_dir(matches: &clap::ArgMatches) -> anyhow::Result<(PathBuf, PathBuf)> {
    let default_source_dir = dunce::canonicalize(PathBuf::from(".")).unwrap();
    let source_dir = matches
        .get_one::<PathBuf>("source-dir")
        .unwrap_or(&default_source_dir);
    let default_target_dir = source_dir.join("target");

    if !check_moon_mod_exists(source_dir) {
        bail!("could not find `{}`", MOON_MOD_JSON);
    }

    let target_dir = matches
        .get_one::<PathBuf>("target-dir")
        .unwrap_or(&default_target_dir);
    if !target_dir.exists() {
        std::fs::create_dir_all(target_dir).context("failed to create target directory")?;
    }

    let target_dir = dunce::canonicalize(target_dir).context("failed to set target directory")?;
    Ok((source_dir.into(), target_dir))
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Default)]
pub enum RunMode {
    #[default]
    Build,
    Check,
    Run,
    Test,
    Bundle,
    Format,
}

impl RunMode {
    pub fn to_dir_name(self) -> &'static str {
        match self {
            Self::Build | Self::Run => "build",
            Self::Check => "check",
            Self::Test => "test",
            Self::Bundle => "bundle",
            Self::Format => "format",
        }
    }
}

pub fn backend_filter(files: &[PathBuf], backend: TargetBackend) -> Vec<std::path::PathBuf> {
    files
        .iter()
        .filter(|f| {
            let stem = f.file_stem().unwrap().to_str().unwrap();
            let dot = stem.rfind('.');
            match dot {
                None => true,
                Some(idx) => {
                    let (_, backend_ext) = stem.split_at(idx + 1);
                    backend_ext == backend.to_backend_ext()
                }
            }
        })
        .cloned()
        .collect()
}

#[test]
fn test_backend_filter() {
    use expect_test::expect;

    let files = [
        PathBuf::from("a.mbt"),
        PathBuf::from("a_test.mbt"),
        PathBuf::from("b.mbt"),
        PathBuf::from("b_test.mbt"),
        PathBuf::from("x.js.mbt"),
        PathBuf::from("x_test.js.mbt"),
        PathBuf::from("x.wasm.mbt"),
        PathBuf::from("x_test.wasm.mbt"),
        PathBuf::from("x.wasm-gc.mbt"),
        PathBuf::from("x_test.wasm-gc.mbt"),
    ];

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
    .assert_debug_eq(&backend_filter(&files, TargetBackend::Js));
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
    .assert_debug_eq(&backend_filter(&files, TargetBackend::Wasm));
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
    .assert_debug_eq(&backend_filter(&files, TargetBackend::WasmGC));
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

pub fn get_moonc_version() -> String {
    let output = std::process::Command::new("moonc").arg("-v").output();
    if let Ok(output) = &output {
        if output.status.success() {
            return std::str::from_utf8(&output.stdout)
                .unwrap()
                .trim()
                .to_string();
        }
    }
    println!("{}: failed to get moonc version", "error".red().bold());
    std::process::exit(1);
}

#[test]
fn test_get_version() {
    let v = get_moon_version();
    println!("moon_version: {}", v);
    assert!(!v.is_empty());
    let v = get_moonc_version();
    println!("moonc_version: {}", v);
    assert!(!v.is_empty());
}

pub mod render {

    use ariadne::Fmt;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    pub struct MooncDiagnostic {
        pub level: String,
        #[serde(alias = "loc")]
        pub location: Location,
        pub message: String,
        pub error_code: u32,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Location {
        pub start: Position,
        pub end: Position,
        pub path: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct Position {
        pub line: usize,
        pub col: usize,
        pub offset: isize,
    }

    impl MooncDiagnostic {
        pub fn render(&self) {
            let (kind, color) = self.get_level_and_color();

            // for no-location diagnostic, like Missing main function in the main package(4067)
            if self.location.path.is_empty() {
                println!(
                    "{}",
                    format!("[{}] {}: {}", self.error_code, kind, self.message).fg(color)
                );
            } else {
                let source_file_path = &self.location.path;
                let source_file = std::fs::read_to_string(source_file_path)
                    .unwrap_or_else(|_| panic!("failed to read {}", source_file_path));

                ariadne::Report::build(kind, source_file_path, self.location.start.offset as usize)
                    .with_code(self.error_code)
                    .with_message(&self.message)
                    .with_label(
                        ariadne::Label::new((
                            source_file_path,
                            self.location.start.offset as usize..self.location.end.offset as usize,
                        ))
                        .with_message((&self.message).fg(color))
                        .with_color(color),
                    )
                    .finish()
                    .print((source_file_path, ariadne::Source::from(source_file)))
                    .unwrap();
            }
        }

        fn get_level_and_color(&self) -> (ariadne::ReportKind, ariadne::Color) {
            if self.level == "error" {
                (ariadne::ReportKind::Error, ariadne::Color::Red)
            } else if self.level == "warning" {
                (ariadne::ReportKind::Warning, ariadne::Color::BrightYellow)
            } else {
                (ariadne::ReportKind::Advice, ariadne::Color::Blue)
            }
        }
    }
}

pub struct FileLock {
    _file: std::fs::File,
}

impl FileLock {
    pub fn lock(path: &std::path::Path) -> std::io::Result<Self> {
        let file = match std::fs::File::create(path.join(MOON_LOCK)) {
            Ok(f) => f,
            Err(e) => return Err(e),
        };
        match file.try_lock_exclusive() {
            Ok(_) => Ok(FileLock { _file: file }),
            Err(_) => {
                println!("Blocking waiting for file lock ...");
                // Move console cursor up by one line to overprint the above tip
                print!("\x1b[1A");
                file.lock_exclusive()
                    .map_err(|e| std::io::Error::new(e.kind(), "failed to lock target dir"))?;
                Ok(FileLock { _file: file })
            }
        }
    }
}

pub mod gen {

    use crate::common::MOON_PKG_JSON;
    use anyhow::bail;
    use indexmap::map::IndexMap;
    use petgraph::graph::DiGraph;
    use serde::{Deserialize, Serialize};
    use std::collections::HashSet;
    use std::fmt::Debug;
    use std::{
        fmt::Formatter,
        path::{Component, Path, PathBuf},
    };

    use super::MooncOpt;

    #[allow(unused)]
    const IGNORE_DIRS: &[&str] = &["target", ".git"];

    #[derive(Clone)]
    pub struct PathComponent {
        pub components: Vec<String>,
    }

    impl PathComponent {
        pub fn len(&self) -> usize {
            self.components.len()
        }

        pub fn is_empty(&self) -> bool {
            self.components.is_empty()
        }

        pub fn is_internal(&self) -> bool {
            self.components.iter().any(|x| x == "internal")
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_path_component_1() {
        let pc = PathComponent { components: vec![] };
        assert!(pc.full_name() == "");
        let pc = PathComponent {
            components: vec!["a".into()],
        };
        assert!(pc.full_name() == "a");
        let pc = PathComponent {
            components: vec!["a".into(), "b".into()],
        };
        assert!(pc.full_name() == "a/b");
    }

    #[test]
    fn test_import_component_1() {
        let ic = ImportComponent {
            path: ImportPath {
                module_name: "a/b".into(),
                rel_path: PathComponent { components: vec![] },
                is_3rd: true,
            },
            alias: None,
        };
        assert!(ic.path.make_full_path() == "a/b");
        let ic = ImportComponent {
            path: ImportPath {
                module_name: "a".into(),
                rel_path: PathComponent {
                    components: vec!["b".into()],
                },
                is_3rd: true,
            },
            alias: None,
        };
        assert!(ic.path.make_full_path() == "a/b");
    }

    impl Debug for PathComponent {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(f, "#[{}]", self.components.join("|"))
        }
    }

    impl PathComponent {
        pub fn short_name(&self) -> &str {
            if self.components.is_empty() {
                ""
            } else {
                self.components.last().unwrap()
            }
        }

        pub fn full_name(&self) -> String {
            self.components.join("/")
        }

        pub fn fs_full_name(&self) -> String {
            #[cfg(unix)]
            return self.components.join("/");

            #[cfg(windows)]
            return self.components.join("\\");
        }

        pub fn from_path(p: &Path) -> anyhow::Result<PathComponent> {
            let mut comps = vec![];
            for comp in p.components() {
                match comp {
                    Component::Normal(s) => {
                        comps.push(s.to_str().unwrap().to_string());
                    }
                    _ => {
                        bail!("invalid package path `{:?}`", p)
                    }
                }
            }
            Ok(Self { components: comps })
        }
    }

    impl std::str::FromStr for PathComponent {
        type Err = anyhow::Error;
        // like a/b/c
        fn from_str(p: &str) -> anyhow::Result<PathComponent> {
            let buf = PathBuf::from(p);
            PathComponent::from_path(&buf)
        }
    }

    #[derive(Clone)]
    pub struct ImportPath {
        pub module_name: String,
        pub rel_path: PathComponent,
        pub is_3rd: bool,
    }

    impl ImportPath {
        pub fn make_full_path(&self) -> String {
            let mut p = self.module_name.clone();
            if !self.rel_path.components.is_empty() {
                p.push('/');
                p.push_str(&self.rel_path.full_name())
            }
            p
        }

        fn make_rel_path(&self) -> String {
            self.rel_path.full_name()
        }
    }

    impl Debug for ImportPath {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "{}({}){}",
                if self.is_3rd { "*" } else { "" },
                self.module_name,
                self.rel_path.full_name()
            )
        }
    }

    #[derive(Clone)]
    pub struct ImportComponent {
        pub path: ImportPath,
        pub alias: Option<String>,
    }

    impl ImportComponent {
        pub fn full_components(&self) -> PathComponent {
            let mut components: Vec<String> = PathBuf::from(&self.path.module_name)
                .components()
                .map(|x| x.as_os_str().to_str().unwrap().to_string())
                .collect();
            components.extend(self.path.rel_path.components.iter().cloned());
            PathComponent { components }
        }
    }

    impl Debug for ImportComponent {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            match &self.alias {
                None => {
                    write!(f, "import {:?}", self.path)
                }
                Some(alias) => {
                    write!(f, "import {:?} as {}", self.path, alias)
                }
            }
        }
    }

    #[derive(Debug, Clone)]
    pub enum GeneratedTestDriver {
        InternalTest(PathBuf),
        UnderscoreTest(PathBuf),
    }

    #[derive(Debug, Clone)]
    pub struct Package {
        pub is_main: bool,
        pub need_link: bool,
        pub is_third_party: bool,
        pub root_path: PathBuf,
        pub root: PathComponent,
        pub rel: PathComponent,
        // *.mbt
        pub files: Vec<PathBuf>,
        //  *_test.mbt
        pub test_files: Vec<PathBuf>,
        pub files_contain_test_block: Vec<PathBuf>,
        pub imports: Vec<ImportComponent>,
        pub test_imports: Vec<ImportComponent>,
        pub generated_test_drivers: Vec<GeneratedTestDriver>,
        pub artifact: PathBuf,

        pub link: Option<crate::common::Link>,
        pub warn_list: Option<String>,
        pub alert_list: Option<String>,
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
    }

    #[derive(Debug, Clone)]
    pub struct ModuleDB {
        pub source_dir: PathBuf,
        pub name: String,
        pub packages: IndexMap<String, Package>,
        pub entries: Vec<usize>, // index of entry packages
        pub deps: Vec<String>,
        pub graph: DiGraph<String, usize>,
        pub backend: String,
    }

    impl ModuleDB {
        pub fn make_pkg_import_path(&self, pkg_idx: usize) -> String {
            let pkg = &self.packages[pkg_idx];

            let p = ImportPath {
                module_name: self.name.clone(),
                rel_path: pkg.rel.clone(),
                is_3rd: false,
            };

            p.make_full_path()
        }

        pub fn get_package_dir(&self, index: usize) -> PathBuf {
            self.source_dir
                .join(self.packages[index].rel.fs_full_name())
        }

        pub fn make_pkg_core_path(&self, target_dir: &Path, pkg_full_name: &str) -> PathBuf {
            let pkg = &self.packages[pkg_full_name];
            target_dir
                .join(pkg.rel.fs_full_name())
                .join(format!("{}.core", pkg.rel.short_name()))
        }

        pub fn make_pkg_mi_path(&self, target_dir: &Path, pkg_idx: usize) -> PathBuf {
            let pkg = &self.packages[pkg_idx];
            target_dir
                .join(pkg.rel.fs_full_name())
                .join(format!("{}.mi", pkg.rel.short_name()))
        }

        pub fn get_pkg_mi_deps(&self, target_dir: &Path, pkg_idx: usize) -> Vec<String> {
            let mut deps: Vec<String> = vec![];
            let pkg = &self.packages[pkg_idx];
            for dep in pkg.imports.iter() {
                let mi_path = target_dir
                    .join(dep.path.make_rel_path())
                    .join(format!("{}.mi", dep.path.rel_path.short_name()));

                deps.push(mi_path.display().to_string());
            }
            deps
        }

        pub fn get_pkg_mi_deps_with_alias(&self, target_dir: &Path, pkg_idx: usize) -> Vec<String> {
            let mut deps: Vec<String> = vec![];
            let pkg = &self.packages[pkg_idx];
            for dep in pkg.imports.iter() {
                let alias = if let Some(a) = &dep.alias {
                    a.clone()
                } else {
                    dep.path.rel_path.short_name().into()
                };
                let mi_path = target_dir
                    .join(dep.path.make_rel_path())
                    .join(format!("{}.mi", dep.path.rel_path.short_name()));

                deps.push(format!("{}:{}", mi_path.display(), alias));
            }
            deps
        }

        pub fn make_output_path(
            &self,
            target_dir: &Path,
            pkg_idx: usize,
            moonc_opt: &MooncOpt,
        ) -> PathBuf {
            let pkg = &self.packages[pkg_idx];
            target_dir.join(pkg.rel.fs_full_name()).join(format!(
                "{}.{}",
                pkg.rel.short_name(),
                moonc_opt.link_opt.output_format.to_str()
            ))
        }

        fn get_core_dep_rec(
            &self,
            visited: &mut HashSet<String>,
            target_dir: &Path,
            pkg_full_name: &str,
            cores: &mut Vec<PathBuf>,
        ) {
            if visited.contains(pkg_full_name) {
                return;
            }
            visited.insert(pkg_full_name.into());
            let c = self.make_pkg_core_path(target_dir, pkg_full_name);
            cores.push(c);
            let pkg = &self.packages[pkg_full_name];
            for d in pkg.imports.iter() {
                let pkgname = d.path.make_full_path();
                if self.packages.contains_key(&pkgname) {
                    self.get_core_dep_rec(visited, target_dir, &pkgname, cores);
                }
            }
        }

        pub fn get_all_dep_cores(&self, target_dir: &Path, pkg_full_name: &str) -> Vec<PathBuf> {
            let mut cores = vec![];
            let mut visited = HashSet::<String>::new();
            self.get_core_dep_rec(&mut visited, target_dir, pkg_full_name, &mut cores);
            cores.sort();
            cores.dedup();
            cores
        }
    }

    fn x_can_import_y(x: &PathComponent, y: &PathComponent) -> bool {
        if !y.is_internal() {
            return true;
        }
        let mut i = 0;
        let mut j = 0;
        let internal_index = y.components.iter().position(|c| *c == "internal").unwrap();

        while i < x.len() && j < internal_index {
            if x.components[i] != y.components[j] {
                return false;
            }
            i += 1;
            j += 1;
        }
        true
    }

    #[test]
    fn test_internal() {
        let x = PathComponent {
            components: vec!["a".to_string(), "b".to_string()],
        };
        let y = PathComponent {
            components: vec!["a".to_string(), "b".to_string(), "internal".to_string()],
        };
        assert!(x_can_import_y(&x, &y));

        let x = PathComponent {
            components: vec!["x".to_string(), "y".to_string()],
        };
        let y = PathComponent {
            components: vec!["a".to_string(), "b".to_string(), "internal".to_string()],
        };
        assert!(!x_can_import_y(&x, &y));
    }

    impl ModuleDB {
        pub fn validate(&self) -> anyhow::Result<()> {
            let mut errors = vec![];
            for (_, pkg) in &self.packages {
                for item in pkg.imports.iter().chain(pkg.test_imports.iter()) {
                    let imported = item.path.make_full_path();
                    if !x_can_import_y(&pkg.full_components(), &item.full_components()) {
                        errors.push(format!(
                            "{}: cannot import internal package `{}` in `{}`",
                            self.source_dir
                                .join(pkg.rel.fs_full_name())
                                .join(MOON_PKG_JSON)
                                .display(),
                            imported,
                            pkg.full_name()
                        ))
                    }
                    if !self.packages.contains_key(&imported) {
                        errors.push(format!(
                            "{}: cannot import `{}` in `{}`, no such package",
                            self.source_dir
                                .join(pkg.rel.fs_full_name())
                                .join(MOON_PKG_JSON)
                                .display(),
                            imported,
                            pkg.full_name(),
                        ));
                    }
                }
            }
            if !errors.is_empty() {
                bail!("{}", errors.join("\n"));
            }
            Ok(())
        }
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "kebab-case")]
    pub struct PackageJSON {
        pub is_main: bool,
        pub is_third_party: bool,
        pub root: String,
        pub rel: String,
        pub files: Vec<String>,
        pub test_files: Vec<String>,
        pub deps: Vec<AliasJSON>,
        pub test_deps: Vec<AliasJSON>,
        pub artifact: String,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    pub struct AliasJSON {
        pub path: String,
        pub alias: String,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    pub struct ModuleDBJSON {
        pub source_dir: String,
        pub name: String,
        pub packages: Vec<PackageJSON>,
        pub deps: Vec<String>,
        pub backend: String,
    }

    pub fn convert_mdb_to_json(module: &ModuleDB) -> ModuleDBJSON {
        let mut pkgs = vec![];
        for (_, pkg) in &module.packages {
            let files = pkg.files.iter().map(|f| f.display().to_string()).collect();
            let test_files = pkg
                .test_files
                .iter()
                .map(|f| f.display().to_string())
                .collect();
            let mut deps = vec![];
            for dep in &pkg.imports {
                let alias = match &dep.alias {
                    None => {
                        let alias = dep.path.rel_path.components.last();
                        match alias {
                            None => dep.path.module_name.split('/').last().unwrap().to_string(),
                            Some(x) => x.to_string(),
                        }
                    }
                    Some(x) => x.to_string(),
                };
                deps.push(AliasJSON {
                    path: dep.path.make_full_path(),
                    alias,
                });
            }

            let mut test_deps = vec![];
            for dep in &pkg.test_imports {
                let alias = match &dep.alias {
                    None => {
                        let alias = dep.path.rel_path.components.last();
                        match alias {
                            None => dep.path.module_name.split('/').last().unwrap().to_string(),
                            Some(x) => x.to_string(),
                        }
                    }
                    Some(x) => x.to_string(),
                };
                test_deps.push(AliasJSON {
                    path: dep.path.make_full_path(),
                    alias,
                });
            }

            pkgs.push(PackageJSON {
                is_main: pkg.is_main,
                is_third_party: pkg.is_third_party,
                root: pkg.root.full_name(),
                rel: pkg.rel.full_name(),
                files,
                test_files,
                deps,
                test_deps,
                artifact: pkg
                    .artifact
                    .with_extension("mi")
                    .to_str()
                    .unwrap()
                    .to_string(),
            })
        }
        let mut deps = vec![];
        for dep in &module.deps {
            deps.push(dep.clone());
        }
        ModuleDBJSON {
            source_dir: module.source_dir.display().to_string(),
            name: module.name.clone(),
            packages: pkgs,
            deps,
            backend: module.backend.clone(),
        }
    }

    pub mod util {
        use petgraph::graph::NodeIndex;

        pub fn get_example_cycle(
            m: &petgraph::graph::DiGraph<String, usize>,
            n: petgraph::prelude::NodeIndex,
        ) -> Vec<petgraph::prelude::NodeIndex> {
            // the parent of each node in the spanning tree
            let mut spanning_tree = vec![NodeIndex::default(); m.capacity().0];
            // we find a cycle via dfs from our starting point
            let res = petgraph::visit::depth_first_search(&m, [n], |ev| match ev {
                petgraph::visit::DfsEvent::TreeEdge(parent, n) => {
                    spanning_tree[n.index()] = parent;
                    petgraph::visit::Control::Continue
                }
                petgraph::visit::DfsEvent::BackEdge(u, v) => {
                    if v == n {
                        // Cycle found! Bail out of the search.
                        petgraph::visit::Control::Break(u)
                    } else {
                        // This is not the cycle we are looking for.
                        petgraph::visit::Control::Continue
                    }
                }
                _ => {
                    // Continue the search.
                    petgraph::visit::Control::Continue
                }
            });
            let res = res.break_value().expect("The cycle should be found");
            let mut cycle = vec![n];
            let mut curr_node = res;
            loop {
                cycle.push(curr_node);
                if curr_node == n {
                    break;
                }
                curr_node = spanning_tree[curr_node.index()]; // get parent
            }
            cycle.reverse(); // the cycle was pushed in reverse order
            cycle
        }
    }
}
