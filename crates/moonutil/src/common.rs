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
use crate::package::{convert_pkg_json_to_package, MoonPkg, MoonPkgJSON, Package};
use anyhow::{bail, Context};
use clap::ValueEnum;
use fs4::fs_std::FileExt;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::ErrorKind;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use which::which;

pub const MOON_MOD_JSON: &str = "moon.mod.json";
pub const MOON_PKG_JSON: &str = "moon.pkg.json";
pub const MOON_PID_NAME: &str = ".moon.pid";
pub const MOONBITLANG_CORE: &str = "moonbitlang/core";
pub const MOONBITLANG_COVERAGE: &str = "moonbitlang/core/coverage";

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

pub const MOON_DOC_TEST_POSTFIX: &str = "__moonbit_internal_doc_test";

pub const MOON_BIN_DIR: &str = "__moonbin__";

pub const MOONCAKE_BIN: &str = "$mooncake_bin";
pub const MOD_DIR: &str = "$mod_dir";
pub const PKG_DIR: &str = "$pkg_dir";

pub const O_EXT: &str = if cfg!(windows) { "obj" } else { "o" };

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

pub fn read_module_desc_file_in_dir(dir: &Path) -> anyhow::Result<MoonMod> {
    if !dir.join(MOON_MOD_JSON).exists() {
        bail!("`{:?}` does not exist", dir.join(MOON_MOD_JSON));
    }
    Ok(read_module_from_json(&dir.join(MOON_MOD_JSON))?)
}

pub fn read_package_desc_file_in_dir(dir: &Path) -> anyhow::Result<MoonPkg> {
    if !dir.join(MOON_PKG_JSON).exists() {
        bail!("`{:?}` does not exist", dir.join(MOON_PKG_JSON));
    }
    read_package_from_json(&dir.join(MOON_PKG_JSON))
        .context(format!("Failed to load {:?}", dir.join(MOON_PKG_JSON)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Default)]
#[repr(u8)]
pub enum OutputFormat {
    #[default]
    Wat,
    Wasm,
    Js,
    Native,
}

impl OutputFormat {
    pub fn to_str(&self) -> &str {
        match self {
            OutputFormat::Wat => "wat",
            OutputFormat::Wasm => "wasm",
            OutputFormat::Js => "js",
            OutputFormat::Native => "c",
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
            SurfaceTarget::All => {
                result.insert(TargetBackend::Wasm);
                result.insert(TargetBackend::WasmGC);
                result.insert(TargetBackend::Js);
                // todo: enable native backend
                // result.insert(TargetBackend::Native);
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
        }
    }

    pub fn to_extension(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm",
            Self::Js => "js",
            Self::Native => "exe",
        }
    }

    pub fn to_artifact(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm",
            Self::Js => "js",
            Self::Native => "c",
        }
    }

    pub fn to_dir_name(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm-gc",
            Self::Js => "js",
            Self::Native => "native",
        }
    }

    pub fn to_backend_ext(self) -> &'static str {
        match self {
            Self::Wasm => "wasm",
            Self::WasmGC => "wasm-gc",
            Self::Js => "js",
            Self::Native => "native",
        }
    }

    pub fn str_to_backend(s: &str) -> anyhow::Result<Self> {
        match s {
            "wasm" => Ok(Self::Wasm),
            "wasm-gc" => Ok(Self::WasmGC),
            "js" => Ok(Self::Js),
            "native" => Ok(Self::Native),
            _ => bail!(
                "invalid backend: {}, only support wasm, wasm-gc, js, native",
                s
            ),
        }
    }

    pub fn hashset_to_string(backends: &HashSet<TargetBackend>) -> String {
        let mut backends = backends
            .iter()
            .map(|b| b.to_flag().to_string())
            .collect::<Vec<_>>();
        backends.sort();
        format!("[{}]", backends.join(", "))
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
    pub output_json: bool,
    pub no_parallelize: bool,
    pub build_graph: bool,
    /// Max parallel tasks to run in n2; `None` to use default
    pub parallelism: Option<usize>,
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
    pub package_path: Option<PathBuf>,
    pub patch_file: Option<PathBuf>,
    pub no_mi: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TestOpt {
    pub filter_package: Option<HashSet<String>>,
    pub filter_file: Option<String>,
    pub filter_index: Option<u32>,
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
    let output = std::process::Command::new("moonc").arg("-v").output();
    match output {
        Ok(output) => {
            if output.status.success() {
                return Ok(std::str::from_utf8(&output.stdout)?.trim().to_string());
            } else {
                anyhow::bail!(
                    "failed to get moonc version: {}",
                    std::str::from_utf8(&output.stderr)?
                );
            }
        }
        Err(e) => anyhow::bail!("failed to get moonc version: {}", e),
    }
}

pub fn get_moonrun_version() -> anyhow::Result<String> {
    let output = std::process::Command::new("moonrun")
        .arg("--version")
        .output();
    match output {
        Ok(output) => {
            if output.status.success() {
                return Ok(std::str::from_utf8(&output.stdout)?.trim().to_string());
            } else {
                anyhow::bail!(
                    "failed to get moonrun version: {}",
                    std::str::from_utf8(&output.stderr)?
                );
            }
        }
        Err(e) => anyhow::bail!("failed to get moonrun version: {}", e),
    }
}

#[test]
fn test_get_version() {
    let v = get_moon_version();
    println!("moon_version: {}", v);
    assert!(!v.is_empty());
    let v = get_moonc_version().unwrap();
    println!("moonc_version: {}", v);
    assert!(!v.is_empty());
}

pub struct FileLock {
    _file: std::fs::File,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        self._file.unlock().unwrap();
    }
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

#[derive(Debug, ValueEnum, Clone)]
pub enum DriverKind {
    Internal,
    Whitebox,
    Blackbox,
}

impl DriverKind {
    pub fn to_string(&self) -> &'static str {
        match self {
            Self::Internal => "internal",
            Self::Whitebox => "whitebox",
            Self::Blackbox => "blackbox",
        }
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
    pub index: TestBlockIndex,
    pub func: String,
    pub name: Option<TestName>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MooncGenTestInfo {
    pub no_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
    pub with_args_tests: IndexMap<FileName, Vec<MbtTestInfo>>,
}

impl MooncGenTestInfo {
    pub fn to_mbt(&self) -> String {
        let mut result = String::new();
        let default_name = "".to_string();

        result.push_str("let moonbit_test_driver_internal_no_args_tests = {\n");
        for (file, tests) in &self.no_args_tests {
            result.push_str(&format!("  \"{}\": {{\n", file));
            for test in tests {
                result.push_str(&format!(
                    "    {}: ({}, [\"{}\"]),\n",
                    test.index,
                    test.func,
                    test.name.as_ref().unwrap_or(&default_name)
                ));
            }
            result.push_str("  },\n");
        }
        result.push_str("}\n\n");

        result.push_str("let moonbit_test_driver_internal_with_args_tests = {\n");
        for (file, tests) in &self.with_args_tests {
            result.push_str(&format!("  \"{}\": {{\n", file));
            for test in tests {
                result.push_str(&format!(
                    "    {}: ({}, [\"{}\"]),\n",
                    test.index,
                    test.func,
                    test.name.as_ref().unwrap_or(&default_name)
                ));
            }
            result.push_str("  },\n");
        }
        result.push_str("}\n");

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
    release: bool,
    target_backend: Option<TargetBackend>,
    module: &mut crate::module::ModuleDB,
) -> anyhow::Result<()> {
    match run_mode {
        // need link-core for build, test and run
        RunMode::Build | RunMode::Test | RunMode::Run => {
            if target_backend == Some(TargetBackend::Native) {
                // check if c compiler exists in PATH
                #[cfg(unix)]
                let compiler = "cc";
                #[cfg(windows)]
                let compiler = "cl";

                let moonc_path = which::which("moonc").context("moonc not found in PATH")?;
                let moon_home = moonc_path.parent().unwrap().parent().unwrap();
                let moon_include_path = moon_home.join("include");
                let moon_lib_path = moon_home.join("lib");

                // libmoonbitrun.o should under $MOON_HOME/lib
                let libmoonbitrun_path = moon_home.join("lib").join("libmoonbitrun.o");

                let get_default_cc_flags = || -> Option<String> {
                    #[cfg(unix)]
                    return Some(format!(
                        "-I{} -O2 {} -fwrapv -fno-strict-aliasing",
                        moon_include_path.display(),
                        libmoonbitrun_path.display()
                    ));
                    #[cfg(windows)]
                    return Some(format!("-I{}", moon_include_path.display()));
                };

                let get_default_cc_link_flag = || -> Option<String> {
                    #[cfg(unix)]
                    return Some("-lm".to_string());
                    #[cfg(windows)]
                    return None;
                };

                let mut link_configs = HashMap::new();

                let all_pkgs = module.get_all_packages();
                for (_, pkg) in all_pkgs {
                    let existing_native = pkg.link.as_ref().and_then(|link| link.native.as_ref());

                    let mut native_config = match existing_native {
                        Some(n) => crate::package::NativeLinkConfig {
                            exports: n.exports.clone(),
                            cc: n.cc.clone().or(Some(compiler.to_string())),
                            cc_flags: n
                                .cc_flags
                                .as_ref()
                                .map(|cc_flags| {
                                    format!(
                                        "-I{} -fwrapv -fno-strict-aliasing {}",
                                        moon_include_path.display(),
                                        cc_flags
                                    )
                                })
                                .or(get_default_cc_flags()),
                            cc_link_flags: n.cc_link_flags.clone().or(get_default_cc_link_flag()),
                            native_stub_deps: None,
                        },
                        None if (release
                            || pkg.native_stub.is_some()
                            || which(compiler).is_ok()) =>
                        {
                            crate::package::NativeLinkConfig {
                                exports: None,
                                cc: Some(compiler.to_string()),
                                cc_flags: get_default_cc_flags(),
                                cc_link_flags: get_default_cc_link_flag(),
                                native_stub_deps: None,
                            }
                        }
                        None => crate::package::NativeLinkConfig {
                            exports: None,
                            cc: Some(
                                moon_home
                                    .join("bin")
                                    .join("internal")
                                    .join("tcc")
                                    .display()
                                    .to_string(),
                            ),
                            cc_flags: Some(format!(
                                "-L{} -I{} -DMOONBIT_NATIVE_NO_SYS_HEADER",
                                moon_lib_path.display(),
                                moon_include_path.display()
                            )),
                            cc_link_flags: None,
                            native_stub_deps: None,
                        },
                    };

                    let mut native_stub_o = Vec::new();
                    module
                        .get_filtered_packages_and_its_deps_by_pkgname(pkg.full_name().as_str())
                        .unwrap()
                        .iter()
                        .for_each(|(_, pkg)| {
                            if pkg.native_stub.is_some() {
                                native_stub_o
                                    .push(pkg.artifact.with_extension(O_EXT).display().to_string());
                            }
                        });

                    if !native_stub_o.is_empty() {
                        native_config.native_stub_deps = Some(native_stub_o);
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
            Ok(())
        }
        _ => Ok(()),
    }
}
