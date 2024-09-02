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

pub use crate::dirs::check_moon_mod_exists;
use crate::module::{MoonMod, MoonModJSON};
use crate::package::{convert_pkg_json_to_package, MoonPkg, MoonPkgJSON};
use anyhow::{bail, Context};
use clap::ValueEnum;
use colored::Colorize;
use fs4::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::ErrorKind;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub const MOON_MOD_JSON: &str = "moon.mod.json";
pub const MOON_PKG_JSON: &str = "moon.pkg.json";
pub const MOON_PID_NAME: &str = ".moon.pid";
pub const MOONBITLANG_CORE: &str = "moonbitlang/core";

pub const MOON_TEST_DELIMITER_BEGIN: &str = "----- BEGIN MOON TEST RESULT -----";
pub const MOON_TEST_DELIMITER_END: &str = "----- END MOON TEST RESULT -----";

pub const MOON_COVERAGE_DELIMITER_BEGIN: &str = "----- BEGIN MOONBIT COVERAGE -----";
pub const MOON_COVERAGE_DELIMITER_END: &str = "----- END MOONBIT COVERAGE -----";

pub const MOON_LOCK: &str = ".moon-lock";

pub const WATCH_MODE_DIR: &str = "watch";

pub const MOON_SNAPSHOT_DELIMITER_BEGIN: &str = "----- BEGIN MOONBIT SNAPSHOT TESTING -----";
pub const MOON_SNAPSHOT_DELIMITER_END: &str = "----- END MOONBIT SNAPSHOT TESTING -----";

pub fn startswith_and_trim(s: &str, t: &str) -> String {
    if s.starts_with(t) {
        s.replacen(t, "", 1)
    } else {
        s.into()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("`source` should not be empty")]
    EmptyString,
    #[error("`source` should not contain invalid chars `{0:?}`")]
    ContainInvalidChars(Vec<char>),
    #[error("`source` not a subdirectory of the parent directory")]
    NotSubdirectory,
}

fn is_valid_folder_name(folder_name: &str) -> Result<(), SourceError> {
    if folder_name.is_empty() {
        return Err(SourceError::EmptyString);
    }

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
    #[error("`source` bad format")]
    Source(#[from] SourceError),
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
    Ok(j.into())
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
pub enum SurfaceTarget {
    Wasm,
    #[default]
    WasmGC,
    Js,
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
            SurfaceTarget::All => {
                result.insert(TargetBackend::Wasm);
                result.insert(TargetBackend::WasmGC);
                result.insert(TargetBackend::Js);
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
    pub output_json: bool,
    pub no_parallelize: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TestOpt {
    pub filter_package: Option<HashSet<PathBuf>>,
    pub filter_file: Option<String>,
    pub filter_index: Option<u32>,
    pub limit: u32,
    pub test_failure_json: bool,
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

#[derive(serde::Serialize, Clone)]
pub struct TestArtifacts {
    pub artifacts_path: Vec<PathBuf>,
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

pub fn get_moonrun_version() -> String {
    let output = std::process::Command::new("moonrun")
        .arg("--version")
        .output();
    if let Ok(output) = &output {
        if output.status.success() {
            return std::str::from_utf8(&output.stdout)
                .unwrap()
                .trim()
                .to_string();
        }
    }
    println!("{}: failed to get moonrun version", "error".red().bold());
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
                println!(
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

#[derive(Serialize, Deserialize, Debug)]
pub struct TestInfo {
    pub index: usize,
    pub func: String,
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MooncGenTestInfo {
    pub tests: HashMap<String, Vec<TestInfo>>,
    pub no_args_tests: HashMap<String, Vec<TestInfo>>,
    pub with_args_tests: HashMap<String, Vec<TestInfo>>,
}

impl MooncGenTestInfo {
    pub fn to_mbt(&self) -> String {
        let mut result = String::new();
        let default_name = "".to_string();

        result.push_str("let no_args_tests = {\n");
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

        result.push_str("let with_args_tests = {\n");
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
