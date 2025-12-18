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
pub use crate::dirs::check_moon_mod_exists;
use crate::module::{MoonMod, MoonModJSON};
use crate::moon_pkg;
use crate::mooncakes::ModuleName;
use crate::package::{
    MoonPkg, MoonPkgJSON, Package, VirtualPkg, convert_pkg_dsl_to_package,
    convert_pkg_json_to_package,
};
use crate::path::PathComponent;
use anyhow::{Context, bail};
use clap::ValueEnum;
use fs4::fs_std::FileExt;
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::hash::Hash;
use std::io::ErrorKind;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

pub const MOON_MOD_JSON: &str = "moon.mod.json";
pub const MOON_PKG_JSON: &str = "moon.pkg.json";
pub const MOON_PKG: &str = "moon.pkg";
pub const MBTI_GENERATED: &str = "pkg.generated.mbti";
pub const MBTI_USER_WRITTEN: &str = "pkg.mbti";
pub const MOONBITLANG_CORE: &str = "moonbitlang/core";
pub const MOONBITLANG_CORE_BUILTIN: &str = "moonbitlang/core/builtin";
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

pub const MOON_SNAPSHOT_DELIMITER_BEGIN: &str = "----- BEGIN MOONBIT SNAPSHOT TESTING -----";
pub const MOON_SNAPSHOT_DELIMITER_END: &str = "----- END MOONBIT SNAPSHOT TESTING -----";

pub const TEST_INFO_FILE: &str = "test_info.json";

pub const WHITEBOX_TEST_PATCH: &str = "_wbtest.json";
pub const BLACKBOX_TEST_PATCH: &str = "_test.json";

pub const DOT_MBT_DOT_MD: &str = ".mbt.md";
pub const DOT_MBL: &str = ".mbl";
pub const DOT_MBY: &str = ".mby";

pub const MOON_BIN_DIR: &str = "__moonbin__";

pub const MOONCAKE_BIN: &str = "$mooncake_bin";
pub const MOD_DIR: &str = "$mod_dir";
pub const PKG_DIR: &str = "$pkg_dir";

pub const SINGLE_FILE_TEST_PACKAGE: &str = "moon/test/single";
pub const SINGLE_FILE_TEST_MODULE: &str = "moon/test";

pub const SUB_PKG_POSTFIX: &str = "_sub";

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

#[derive(Debug, Serialize, Deserialize)]
pub struct PatchJSON {
    pub drops: Vec<String>,
    pub patches: Vec<PatchItem>,
}

#[derive(Debug, Serialize, Deserialize)]
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

fn read_package_from_json(path: &Path) -> anyhow::Result<MoonPkg> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let j = serde_json_lenient::from_reader(reader).context(format!("Failed to parse {path:?}"))?;
    convert_pkg_json_to_package(j)
}

/// Reads a moon.pkg from the given path.
fn read_package_from_dsl(path: &Path) -> anyhow::Result<MoonPkg> {
    let file = File::open(path)?;
    let str = std::io::read_to_string(file)?;
    let json = moon_pkg::parse(&str)?;
    let j = serde_json_lenient::from_value(json)?;
    convert_pkg_dsl_to_package(j)
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

pub fn read_module_desc_file_in_dir(dir: &Path) -> anyhow::Result<MoonMod> {
    if !dir.join(MOON_MOD_JSON).exists() {
        bail!("`{:?}` does not exist", dir.join(MOON_MOD_JSON));
    }
    Ok(read_module_from_json(&dir.join(MOON_MOD_JSON))?)
}

pub fn read_package_desc_file_in_dir(dir: &Path) -> anyhow::Result<MoonPkg> {
    if dir.join(MOON_PKG).exists() {
        read_package_from_dsl(&dir.join(MOON_PKG))
    } else if dir.join(MOON_PKG_JSON).exists() {
        read_package_from_json(&dir.join(MOON_PKG_JSON))
            .context(format!("Failed to load {:?}", dir.join(MOON_PKG_JSON)))
    } else {
        bail!("`{:?}` does not exist", dir.join(MOON_PKG_JSON));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Default)]
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

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Serialize, Deserialize, Default,
)]
pub enum SurfaceTarget {
    Wasm,
    #[default]
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

#[derive(Debug, Clone, Default)]
pub struct BuildPackageFlags {
    pub debug_flag: bool,
    pub strip_flag: bool,
    pub source_map: bool,
    pub enable_coverage: bool,
    // treat all warnings as errors
    pub deny_warn: bool,
    pub target_backend: TargetBackend,
    pub warn_list: Option<String>,
    pub alert_list: Option<String>,
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
            alert_list: None,
            enable_value_tracing: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
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

impl MoonbuildOpt {
    pub fn get_package_filter(&self) -> Option<impl Fn(&Package) -> bool + '_> {
        self.test_opt.as_ref().map(|opt| opt.get_package_filter())
    }
}

#[derive(Debug, Clone, Default)]
pub struct BuildOpt {
    pub install_path: Option<PathBuf>,

    pub filter_package: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CheckOpt {
    pub package_name_filter: Option<String>,
    pub patch_file: Option<PathBuf>,
    pub no_mi: bool,
    pub explain: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TestOpt {
    pub filter_package: Option<HashSet<String>>,
    pub filter_file: Option<String>,
    pub filter_index: Option<u32>,
    pub filter_doc_index: Option<u32>,
    pub limit: u32,
    pub test_failure_json: bool,
    pub display_backend_hint: Option<()>, // use Option to avoid if else
    pub patch_file: Option<PathBuf>,
}

impl TestOpt {
    pub fn get_package_filter(&self) -> impl Fn(&Package) -> bool + '_ {
        move |pkg| {
            if let Some(ref filter_package) = self.filter_package {
                filter_package.contains(&pkg.full_name())
            } else {
                true
            }
        }
    }
}

#[derive(serde::Serialize, Clone)]
pub struct TestArtifacts {
    pub artifacts_path: Vec<PathBuf>,
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

#[derive(Debug, Clone, Default)]
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

pub const DEP_PATH: &str = ".mooncakes";

pub const IGNORE_DIRS: &[&str] = &["target", ".git", "node_modules", DEP_PATH];

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

#[derive(Debug, Clone, PartialEq, Eq, Copy, Default)]
pub enum RunMode {
    Bench,
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
            Self::Bench => "bench",
            Self::Build | Self::Run => "build",
            Self::Check => "check",
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

#[derive(Debug, ValueEnum, Clone, Hash, Eq, PartialEq, Copy, Ord, PartialOrd)]
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

#[derive(Debug, Default, ValueEnum, Clone, Hash, Eq, PartialEq, Copy, Ord, PartialOrd)]
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

pub const INTERNAL_TEST_DRIVER: &str = "__generated_driver_for_internal_test.mbt";
pub const WHITEBOX_TEST_DRIVER: &str = "__generated_driver_for_whitebox_test.mbt";
pub const BLACKBOX_TEST_DRIVER: &str = "__generated_driver_for_blackbox_test.mbt";

pub type FileName = String;
pub type TestName = String;
pub type TestBlockIndex = u32;

#[derive(Serialize, Deserialize, Debug, Clone)]
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MooncGenTestInfo {
    pub no_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    pub with_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    #[serde(default)] // for backward compatibility
    pub with_bench_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    #[serde(default)]
    pub async_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
}

impl MbtTestInfo {
    pub fn has_skip(&self) -> bool {
        self.attrs.iter().any(|attr| attr.starts_with("#skip"))
    }
}

impl MooncGenTestInfo {
    /// Convert part of the driver metadata into MoonBit declaraction code for
    /// the test driver to use.
    pub fn section_to_mbt(
        var_name: &str,
        section: &IndexMap<FileName, Vec<MbtTestInfo>>,
    ) -> String {
        use std::fmt::Write;

        let mut result = String::new();
        let default_name = "";

        // Writing to string cannot fail, so unwrap() is safe here.
        writeln!(result, "let {var_name} = {{").unwrap();
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

pub fn set_native_backend_link_flags(
    run_mode: RunMode,
    target_backend: TargetBackend,
    module: &mut crate::module::ModuleDB,
) -> anyhow::Result<Vec<String>> {
    let mut all_stubs = Vec::new();
    match run_mode {
        // need link-core for build, test, bench, and run
        RunMode::Build | RunMode::Test | RunMode::Bench | RunMode::Run => {
            if matches!(target_backend, TargetBackend::Native | TargetBackend::LLVM) {
                let mut link_configs = HashMap::new();

                let all_pkgs = module.get_all_packages();

                for (_, pkg) in all_pkgs {
                    let existing_native = pkg.link.as_ref().and_then(|link| link.native.as_ref());

                    let mut native_config = existing_native.cloned().unwrap_or_default();

                    let mut stub_lib = Vec::new();
                    module
                        .get_filtered_packages_and_its_deps_by_pkgname(pkg.full_name().as_str())
                        .unwrap()
                        .iter()
                        .for_each(|(_, pkg)| {
                            if pkg.stub_lib.is_some() {
                                stub_lib.push(
                                    pkg.artifact
                                        .with_file_name(format!("lib{}.{}", pkg.last_name(), A_EXT))
                                        .display()
                                        .to_string(),
                                );
                                all_stubs.push(
                                    pkg.artifact
                                        .with_file_name(format!(
                                            "lib{}.{}",
                                            pkg.last_name(),
                                            DYN_EXT
                                        ))
                                        .display()
                                        .to_string(),
                                );
                            }
                        });

                    if !stub_lib.is_empty() {
                        native_config.stub_lib_deps = Some(stub_lib);
                    }

                    link_configs.insert(
                        pkg.full_name(),
                        Some(crate::package::Link {
                            native: Some(native_config),
                            ..Default::default()
                        }),
                    );
                }

                for (pkgname, link_config) in link_configs {
                    module.get_package_by_name_mut_safe(&pkgname).unwrap().link = link_config;
                }
            }
            Ok(all_stubs)
        }
        // don't use wildcard here to avoid possible mishandling if we add more modes in the future
        RunMode::Bundle | RunMode::Check | RunMode::Format => Ok(all_stubs),
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

pub fn execute_postadd_script(dir: &Path) -> anyhow::Result<()> {
    if std::env::var("MOON_IGNORE_POSTADD").is_ok() {
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

pub fn gen_moonbitlang_abort_pkg(moonc_opt: &MooncOpt) -> Package {
    let path_comp = PathComponent {
        components: vec!["moonbitlang".to_string(), "core".to_string()],
    };

    let module_root = crate::moon_dir::core();
    let root_path = module_root.join("abort");

    Package {
        is_main: false,
        force_link: false,
        is_third_party: true,
        root_path: root_path.clone(),
        root: path_comp,
        rel: PathComponent {
            components: vec!["abort".to_string()],
        },
        files: IndexMap::from([(root_path.join("abort.mbt"), CompileCondition::default())]),
        wbtest_files: IndexMap::new(),
        test_files: IndexMap::new(),
        mbt_md_files: IndexMap::new(),
        files_contain_test_block: vec![],
        formatter_ignore: IndexSet::new(),
        with_sub_package: None,
        is_sub_package: false,
        imports: vec![],
        wbtest_imports: vec![],
        test_imports: vec![],
        generated_test_drivers: vec![],
        artifact: crate::moon_dir::core_bundle(moonc_opt.link_opt.target_backend)
            .join("abort")
            .join("abort.core"),
        link: None,
        warn_list: None,
        alert_list: None,
        targets: None,
        pre_build: None,
        patch_file: None,
        no_mi: false,
        install_path: None,
        bin_name: None,
        bin_target: moonc_opt.link_opt.target_backend,
        enable_value_tracing: false,
        supported_targets: indexmap::IndexSet::from_iter([moonc_opt.link_opt.target_backend]),
        stub_lib: None,
        virtual_pkg: Some(VirtualPkg { has_default: true }),
        virtual_mbti_file: Some(root_path.join("abort.mbti")),
        implement: None,
        overrides: None,
        link_flags: None,
        link_libs: vec![],
        link_search_paths: vec![],
        module_root: module_root.into(),
        max_concurrent_tests: None,
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct MbtMdHeader {
    pub moonbit: Option<MbtMdSection>,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct MbtMdSection {
    pub deps: Option<IndexMap<String, crate::dependency::SourceDependencyInfoJson>>,
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
