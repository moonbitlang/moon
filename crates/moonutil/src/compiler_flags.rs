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

use crate::moon_dir::MOON_DIRS;
use anyhow::Context;
use colored::Colorize;
use derive_builder::Builder;
use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

const ENV_MOON_CC: &str = "MOON_CC";
const ENV_MOON_AR: &str = "MOON_AR";

#[derive(Copy, Clone, Debug)]
pub enum CCKind {
    Msvc,     // cl.exe
    SystemCC, // cc
    Gcc,      // gcc
    Clang,    // clang
    Tcc,      // tcc
}

#[derive(Copy, Clone, Debug)]
pub enum ARKind {
    MsvcLib, // lib.exe
    GnuAr,   // ar
    LlvmAr,  // llvm-ar
    TccAr,   // tcc -ar
}

#[derive(Clone, Debug)]
pub struct CC {
    pub cc_kind: CCKind,
    pub cc_path: String,
    pub ar_kind: ARKind,
    pub ar_path: String,
    pub target_triple: Option<String>,
    pub is_env_override: bool, // Whether the cc is set by env MOON_CC
}

impl Default for CC {
    fn default() -> Self {
        NATIVE_CC().clone()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ToolchainSource {
    EnvOverride,
    PathProbe,
    PackageOverride,
}

#[derive(Clone, Debug)]
pub struct Toolchain {
    cc: CC,
    source: ToolchainSource,
}

impl Toolchain {
    pub fn from_env_override(cc: CC) -> Self {
        Self {
            cc,
            source: ToolchainSource::EnvOverride,
        }
    }

    pub fn from_path_probe(cc: CC) -> Self {
        Self {
            cc,
            source: ToolchainSource::PathProbe,
        }
    }

    pub fn from_package_override(cc: CC) -> Self {
        Self {
            cc,
            source: ToolchainSource::PackageOverride,
        }
    }

    pub fn from_cc(cc: CC) -> Self {
        if cc.is_env_override {
            Toolchain::from_env_override(cc)
        } else {
            Toolchain::from_path_probe(cc)
        }
    }

    pub fn cc(&self) -> &CC {
        &self.cc
    }

    pub fn source(&self) -> ToolchainSource {
        self.source
    }

    pub fn with_package_override(&self, package_cc: Option<&CC>) -> Toolchain {
        match (self.source, package_cc) {
            (ToolchainSource::EnvOverride, Some(_)) => {
                static WARN_ONCE: std::sync::Once = std::sync::Once::new();
                WARN_ONCE.call_once(|| {
                    eprintln!(
                        "{}: Both MOON_CC environment variable and user-specified CC are provided. \
                        MOON_CC takes precedence.",
                        "Warning".yellow().bold(),
                    );
                });
                self.clone()
            }
            (ToolchainSource::EnvOverride, None) | (_, None) => self.clone(),
            (_, Some(package_cc)) => Toolchain::from_package_override(package_cc.clone()),
        }
    }
}

// Used to detect the availability of libmoonbitrun.o on host system
#[cfg(any(target_os = "linux", target_os = "macos"))]
const CAN_USE_MOONBITRUN: bool = true;
// Currently, the distribution of libmoonbitrun.o is not available on Windows
// Once it's supported, we can set this to true but also need to
// correctly change the compiler flags
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
const CAN_USE_MOONBITRUN: bool = false;

impl CC {
    pub fn cc_name(&self) -> &'static str {
        match self.cc_kind {
            CCKind::Msvc => "cl.exe",
            CCKind::SystemCC => "cc",
            CCKind::Gcc => "gcc",
            CCKind::Clang => "clang",
            CCKind::Tcc => "tcc",
        }
    }

    pub fn ar_name(&self) -> &'static str {
        match self.ar_kind {
            ARKind::MsvcLib => "lib.exe",
            ARKind::GnuAr => "ar",
            ARKind::LlvmAr => "llvm-ar",
            ARKind::TccAr => "tcc",
        }
    }

    pub fn cc_path(&self) -> &str {
        &self.cc_path
    }

    fn new(
        cc_kind: CCKind,
        cc_path: String,
        ar_kind: ARKind,
        ar_path: String,
        target_triple: Option<String>,
    ) -> Self {
        CC {
            cc_kind,
            cc_path,
            ar_kind,
            ar_path,
            target_triple,
            is_env_override: false,
        }
    }

    fn parse_compiler_name(path: &Path) -> anyhow::Result<String> {
        let name = path.file_name().and_then(OsStr::to_str).context(
            "Invalid compiler path: path must point to a file with valid UTF-8 filename",
        )?;
        Ok(name.to_string())
    }

    fn strip_exe_suffix(name: &str) -> &str {
        name.strip_suffix(".exe").unwrap_or(name)
    }

    fn probe_target_triple(cc_path: &Path, cc_kind: CCKind) -> Option<String> {
        if matches!(cc_kind, CCKind::Msvc) {
            return None;
        }

        let output = Command::new(cc_path).arg("-dumpmachine").output().ok()?;
        if !output.status.success() {
            return None;
        }

        let triple = String::from_utf8_lossy(&output.stdout);
        let triple = triple.lines().next()?.trim().to_ascii_lowercase();
        if triple.is_empty() {
            None
        } else {
            Some(triple)
        }
    }

    fn replace_compiler_suffix(name: &str, from_suffix: &str, to_suffix: &str) -> Option<String> {
        let name_lower = name.to_ascii_lowercase();
        let (stem, ext) = if name_lower.ends_with(".exe") {
            let stem_len = name.len().checked_sub(4)?;
            let stem = name.get(..stem_len)?;
            let ext = name.get(stem_len..)?;
            (stem, ext)
        } else {
            (name, "")
        };
        let stem_lower = stem.to_ascii_lowercase();
        if !stem_lower.ends_with(from_suffix) {
            return None;
        }
        let prefix_len = stem.len() - from_suffix.len();
        let prefix = stem.get(..prefix_len)?;
        Some(format!("{prefix}{to_suffix}{ext}"))
    }

    fn resolve_tool_path(cc_path: &Path, tool: &str) -> String {
        let tool_path = Path::new(tool);
        let has_non_empty_parent = tool_path
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty());
        if tool_path.is_absolute() || has_non_empty_parent {
            tool.to_string()
        } else if let Some(cc_dir) = cc_path.parent() {
            cc_dir.join(tool).display().to_string()
        } else {
            tool.to_string()
        }
    }

    fn probe_prog_name(cc_path: &Path, name: &str) -> Option<String> {
        let output = Command::new(cc_path)
            .arg(format!("-print-prog-name={name}"))
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let prog = String::from_utf8_lossy(&output.stdout);
        let prog = prog.lines().next()?.trim();
        (!prog.is_empty()).then(|| prog.to_string())
    }

    fn resolve_reported_prog_path(prog: &str) -> Option<String> {
        let prog_path = Path::new(prog);
        let has_non_empty_parent = prog_path
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty());

        if prog_path.is_absolute() || has_non_empty_parent {
            if prog_path.is_file() {
                return Some(prog.to_string());
            }

            #[cfg(windows)]
            if prog_path.extension().is_none() {
                let exe_path = prog_path.with_extension("exe");
                if exe_path.is_file() {
                    return Some(exe_path.display().to_string());
                }
            }

            return None;
        }

        which::which(prog)
            .ok()
            .map(|path| path.display().to_string())
    }

    fn probe_existing_prog_name(cc_path: &Path, name: &str) -> Option<String> {
        let prog = CC::probe_prog_name(cc_path, name)?;
        CC::resolve_reported_prog_path(&prog)
    }

    fn is_llvm_ar_name(ar_name_or_path: &str) -> bool {
        let file_name = Path::new(ar_name_or_path)
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or(ar_name_or_path)
            .to_ascii_lowercase();
        CC::strip_exe_suffix(&file_name) == "llvm-ar"
    }
    pub fn try_from_cc_path_and_kind(
        ar_name: &str,
        cc_path: &Path,
        cc_kind: CCKind,
    ) -> anyhow::Result<Self> {
        let (ar_kind, ar_path) = match cc_kind {
            CCKind::Msvc => (ARKind::MsvcLib, CC::resolve_tool_path(cc_path, "lib")),
            CCKind::SystemCC => (ARKind::GnuAr, CC::resolve_tool_path(cc_path, ar_name)),
            CCKind::Gcc => (ARKind::GnuAr, CC::resolve_tool_path(cc_path, ar_name)),
            CCKind::Clang => {
                let ar_kind = if CC::is_llvm_ar_name(ar_name) {
                    ARKind::LlvmAr
                } else {
                    ARKind::GnuAr
                };
                (ar_kind, CC::resolve_tool_path(cc_path, ar_name))
            }
            CCKind::Tcc => (ARKind::TccAr, cc_path.display().to_string()),
        };
        let target_triple = CC::probe_target_triple(cc_path, cc_kind);
        Ok(CC::new(
            cc_kind,
            cc_path.display().to_string(),
            ar_kind,
            ar_path,
            target_triple,
        ))
    }

    pub fn try_from_path_with_ar(cc: &str, ar: &str) -> anyhow::Result<Self> {
        let path = PathBuf::from(cc);
        let name = CC::parse_compiler_name(&path)?;
        let name_lower = name.to_ascii_lowercase();
        let stem = CC::strip_exe_suffix(&name_lower);
        if stem.ends_with("cl") {
            CC::try_from_cc_path_and_kind(ar, &path, CCKind::Msvc)
        } else if stem.ends_with("gcc") {
            CC::try_from_cc_path_and_kind(ar, &path, CCKind::Gcc)
        } else if stem.ends_with("clang") {
            CC::try_from_cc_path_and_kind(ar, &path, CCKind::Clang)
        } else if stem.ends_with("tcc") {
            CC::try_from_cc_path_and_kind(ar, &path, CCKind::Tcc)
        } else if stem.ends_with("cc") {
            CC::try_from_cc_path_and_kind(ar, &path, CCKind::SystemCC)
        } else {
            // assume it's a system cc
            CC::try_from_cc_path_and_kind(ar, &path, CCKind::SystemCC)
        }
    }

    pub fn try_from_path(cc: &str) -> anyhow::Result<Self> {
        let path = PathBuf::from(cc);
        let name = CC::parse_compiler_name(&path)?;
        let name_lower = name.to_ascii_lowercase();
        let stem = CC::strip_exe_suffix(&name_lower);
        let replaced_ar =
            |s: &str| CC::replace_compiler_suffix(&name, s, "ar").unwrap_or_else(|| "ar".into());
        if stem.ends_with("cl") {
            CC::try_from_cc_path_and_kind("lib.exe", &path, CCKind::Msvc)
        } else if stem.ends_with("gcc") {
            CC::try_from_cc_path_and_kind(&replaced_ar("gcc"), &path, CCKind::Gcc)
        } else if stem.ends_with("clang") {
            if let Some(ar) = CC::probe_existing_prog_name(&path, "ar") {
                CC::try_from_cc_path_and_kind(&ar, &path, CCKind::Clang)
            } else if let Some(llvm_ar) = CC::probe_existing_prog_name(&path, "llvm-ar") {
                CC::try_from_cc_path_and_kind(&llvm_ar, &path, CCKind::Clang)
            } else {
                CC::try_from_cc_path_and_kind(&replaced_ar("clang"), &path, CCKind::Clang)
            }
        } else if stem.ends_with("tcc") {
            CC::try_from_cc_path_and_kind("", &path, CCKind::Tcc)
        } else if stem.ends_with("cc") {
            CC::try_from_cc_path_and_kind(&replaced_ar("cc"), &path, CCKind::SystemCC)
        } else {
            // assume it's a system cc
            CC::try_from_cc_path_and_kind("ar", &path, CCKind::SystemCC)
        }
    }

    fn try_from_detected_path(cc_path: &Path, cc_kind: CCKind) -> anyhow::Result<Self> {
        if matches!(cc_kind, CCKind::Clang)
            && let Some(cc) = cc_path.to_str()
        {
            return CC::try_from_path(cc);
        }

        let ar_name = match cc_kind {
            CCKind::Msvc => "lib.exe",
            CCKind::Tcc => "",
            CCKind::SystemCC | CCKind::Gcc | CCKind::Clang => "ar",
        };
        CC::try_from_cc_path_and_kind(ar_name, cc_path, cc_kind)
    }

    pub fn is_gcc_like(&self) -> bool {
        matches!(
            self.cc_kind,
            CCKind::SystemCC | CCKind::Gcc | CCKind::Clang | CCKind::Tcc
        )
    }

    pub fn is_full_featured_gcc_like(&self) -> bool {
        matches!(self.cc_kind, CCKind::SystemCC | CCKind::Gcc | CCKind::Clang)
    }

    pub fn is_msvc(&self) -> bool {
        matches!(self.cc_kind, CCKind::Msvc)
    }

    pub fn is_tcc(&self) -> bool {
        matches!(self.cc_kind, CCKind::Tcc)
    }

    pub fn targets_msvc(&self) -> bool {
        self.target_triple
            .as_deref()
            .is_some_and(|target| target.contains("msvc"))
    }

    pub fn should_link_libm(&self) -> bool {
        self.is_full_featured_gcc_like() && !self.targets_msvc()
    }

    pub fn is_libmoonbitrun_o_available(&self) -> bool {
        // If users set MOON_CC, we believe they know what they are doing
        // And we conservatively disable libmoonbitrun.o
        CAN_USE_MOONBITRUN && !self.is_msvc() && !self.is_env_override
    }

    // Constructors for TCC toolchain

    /// Create a CC configured for the internal TCC shipped with Moon.
    /// Resolves MOON_DIRS.internal_tcc_path via which::which.
    pub fn internal_tcc() -> anyhow::Result<Self> {
        let cc_path =
            which::which(&MOON_DIRS.internal_tcc_path).context("internal tcc not found")?;
        CC::try_from_cc_path_and_kind("", &cc_path, CCKind::Tcc)
    }
}

pub static ENV_CC: std::sync::LazyLock<Option<CC>> = std::sync::LazyLock::new(|| {
    let env_cc = env::var(ENV_MOON_CC);
    let env_ar = env::var(ENV_MOON_AR);

    match (env_cc, env_ar) {
        (Ok(cc), Ok(ar)) => {
            let cc = CC::try_from_path_with_ar(&cc, &ar)
                .context(format!("failed to parse cc from env {ENV_MOON_CC}"))
                .unwrap();
            Some(CC {
                is_env_override: true,
                ..cc
            })
        }
        (Ok(cc), _) => {
            let cc = CC::try_from_path(&cc)
                .context(format!("failed to parse cc from env {ENV_MOON_CC}"))
                .unwrap();
            Some(CC {
                is_env_override: true,
                ..cc
            })
        }
        _ => None,
    }
});

pub static DETECTED_CC: std::sync::LazyLock<CC> = std::sync::LazyLock::new(|| {
    use CCKind::*;

    let (cc_kind, cc_path) = if let Ok(cc) = which::which("cl") {
        (Msvc, cc)
    } else if let Ok(cc) = which::which("cc") {
        (SystemCC, cc)
    } else if let Ok(cc) = which::which("gcc") {
        (Gcc, cc)
    } else if let Ok(cc) = which::which("clang") {
        (Clang, cc)
    } else {
        let cc = which::which(&MOON_DIRS.internal_tcc_path)
            .context("internal tcc not found")
            .unwrap();
        (Tcc, cc)
    };

    CC::try_from_detected_path(&cc_path, cc_kind)
        .context("failed to detect native C toolchain")
        .unwrap()
});

#[allow(non_snake_case)]
pub fn NATIVE_CC() -> &'static CC {
    if let Some(env_cc) = ENV_CC.as_ref() {
        env_cc
    } else {
        &DETECTED_CC
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum OutputType {
    Object,     // .o or .obj
    SharedLib,  // .so or .dll or .dylib
    Executable, // .exe or no extension
}

#[derive(Clone, Copy, PartialEq)]
pub enum OptLevel {
    Speed,
    Size,
    Debug,
    None,
}

#[derive(Clone, Builder)]
#[builder(setter(into))]
pub struct CCConfig {
    #[builder(default = false)]
    // indicates -g for gcc-like compilers
    // we don't set /Zi as it will have concurrency problem
    // like multiple msvc instances race to write the same .pdb file
    pub debug_info: bool,
    // TCC cannot link libmoonbitrun.o
    pub link_moonbitrun: bool,
    #[builder(default = false)]
    // Define MOONBIT_NATIVE_NO_SYS_HEADER
    // Usually used with TCC
    // TCC may not be able to handle the system header
    pub no_sys_header: bool,
    #[builder(default = OutputType::Object)]
    pub output_ty: OutputType,
    #[builder(default = OptLevel::Speed)]
    pub opt_level: OptLevel,
    // Define MOONBIT_USE_SHARED_RUNTIME
    // It's non-op on Linux and MacOS
    // But on Windows, it will mark runtime function declarations
    // with extra __declspec(dllimport)
    // This is needed to use the shared runtime
    pub define_use_shared_runtime_macro: bool,
}

#[derive(Clone, Builder)]
#[builder(setter(into))]
pub struct LinkerConfig<P: AsRef<Path>> {
    #[builder(default = false)]
    pub link_moonbitrun: bool,
    #[builder(default = OutputType::Executable)]
    pub output_ty: OutputType,
    #[builder(default = None)]
    // This is the parent directory to the shared runtime library
    pub link_shared_runtime: Option<P>,
}

#[derive(Clone, Builder)]
#[builder(setter(into))]
pub struct ArchiverConfig {
    #[builder(default = false)]
    pub archive_moonbitrun: bool,
}

/// Resolve the C compiler to use from global state
pub fn resolve_cc(cc: &CC, user_cc: Option<&CC>) -> CC {
    if ENV_CC.is_some() && user_cc.is_some() {
        eprintln!(
            "{}: Both MOON_CC environment variable and user-specified CC are provided. \
            MOON_CC takes precedence.",
            "Warning".yellow().bold(),
        );
    }
    ENV_CC
        .clone()
        .unwrap_or_else(|| user_cc.cloned().unwrap_or_else(|| cc.clone()))
}

// Struct to hold path configuration for commands
#[derive(Clone)]
pub struct CompilerPaths {
    pub include_path: String,
    pub lib_path: String,
}

impl CompilerPaths {
    pub fn from_moon_dirs() -> Self {
        Self {
            include_path: MOON_DIRS.moon_include_path.display().to_string(),
            lib_path: MOON_DIRS.moon_lib_path.display().to_string(),
        }
    }
}

// Helper functions for archiver command building
fn add_archiver_flags(cc: &CC, buf: &mut Vec<String>, dest: &str) {
    if cc.is_msvc() {
        buf.push("/nologo".to_string());
        buf.push(format!("/Out:{dest}"));
    } else if cc.is_tcc() {
        // tcc don't have separate ar command
        // just use tcc -ar
        buf.push("-ar".to_string());
        buf.push("rcs".to_string());
        buf.push(dest.to_string());
    } else if cc.is_full_featured_gcc_like() {
        buf.push("-r".to_string());
        buf.push("-c".to_string());
        buf.push("-s".to_string());
        buf.push(dest.to_string());
    } else {
        panic!("Unsupported archiver");
    }
}

// Archiver compiler-specific handling for moonbitrun
fn add_archiver_moonbitrun_with_warnings(cc: &CC, buf: &mut Vec<String>, config: &ArchiverConfig) {
    if cc.is_libmoonbitrun_o_available() && config.archive_moonbitrun {
        if cc.is_tcc() {
            eprintln!(
                "{}: Cannot archive libmoonbitrun.o when using tcc",
                "Warning".yellow().bold(),
            );
        } else {
            buf.push(
                MOON_DIRS
                    .moon_lib_path
                    .join("libmoonbitrun.o")
                    .display()
                    .to_string(),
            );
        }
    }
}

pub fn make_archiver_command<S>(
    cc: CC,
    user_cc: Option<CC>,
    config: ArchiverConfig,
    src: &[S],
    dest: &str,
) -> Vec<String>
where
    S: AsRef<str>,
{
    let resolved_cc = resolve_cc(&cc, user_cc.as_ref());
    make_archiver_command_pure(resolved_cc, config, src, dest)
}

pub fn make_archiver_command_pure<S>(
    cc: CC,
    config: ArchiverConfig,
    src: &[S],
    dest: &str,
) -> Vec<String>
where
    S: AsRef<str>,
{
    let mut buf = vec![cc.ar_path.clone()];

    add_archiver_flags(&cc, &mut buf, dest);
    add_archiver_moonbitrun_with_warnings(&cc, &mut buf, &config);
    buf.extend(src.iter().map(|s| s.as_ref().to_string()));

    buf
}

// Helper functions for linker command building
fn add_linker_output_flags(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<impl AsRef<Path>>,
    dest: &str,
) {
    if cc.is_msvc() {
        match config.output_ty {
            OutputType::SharedLib | OutputType::Executable => {
                // /F(executable)
                buf.push(format!("/Fe{dest}"));
            }
            _ => panic!("Linker only supports shared lib, executable and static lib"),
        }
    } else if cc.is_gcc_like() {
        buf.push("-o".to_string());
        buf.push(dest.to_string());
    }
}

fn add_linker_library_paths<P: AsRef<Path>>(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<P>,
    lpath: &str,
) {
    if cc.is_tcc() {
        buf.push(format!("-L{lpath}"));
    }
    if cc.is_gcc_like()
        && let Some(dyn_lib_path) = config.link_shared_runtime.as_ref()
    {
        buf.push(format!("-L{}", dyn_lib_path.as_ref().display()));
    }
}

fn add_linker_intermediate_dir_flags(cc: &CC, buf: &mut Vec<String>, dest_dir: &str) {
    // MSVC may throw intermediate files into current directory
    // Explicitly set the output directory of these files
    if cc.is_msvc() {
        // /F(object)
        buf.push(format!("/Fo{dest_dir}\\"));
    }
}

fn add_linker_shared_lib_flags(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<impl AsRef<Path>>,
) {
    if config.output_ty == OutputType::SharedLib {
        if cc.is_msvc() {
            buf.push("/LD".to_string());
        } else if cc.is_gcc_like() {
            buf.push("-shared".to_string());
            buf.push("-fPIC".to_string());
        }
    }
}

// Linker compiler-specific flags
fn add_linker_msvc_specific_flags(cc: &CC, buf: &mut Vec<String>, has_user_flags: bool) {
    if cc.is_msvc() && !has_user_flags {
        buf.push("/nologo".to_string());
    }
}

// Linker compiler-specific handling for moonbitrun
fn add_linker_moonbitrun_with_warnings(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<impl AsRef<Path>>,
) {
    if config.link_moonbitrun && cc.is_libmoonbitrun_o_available() {
        if cc.is_tcc() {
            eprintln!(
                "{}: Cannot link libmoonbitrun.o when using tcc",
                "Warning".yellow().bold(),
            );
        } else {
            buf.push(
                MOON_DIRS
                    .moon_lib_path
                    .join("libmoonbitrun.o")
                    .display()
                    .to_string(),
            );
        }
    }
}

fn add_linker_common_libraries<P: AsRef<Path>>(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<P>,
) {
    if cc.is_gcc_like() {
        if cc.should_link_libm() {
            buf.push("-lm".to_string());
        }
        if let Some(dyn_lib_path) = config.link_shared_runtime.as_ref() {
            buf.push("-lruntime".to_string());
            buf.push(format!("-Wl,-rpath,{}", dyn_lib_path.as_ref().display()));
        }
    }
}

fn add_linker_msvc_runtime<P: AsRef<Path>>(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<P>,
    lpath: &str,
) {
    if cc.is_msvc() {
        if let Some(dyn_lib_path) = config.link_shared_runtime.as_ref() {
            buf.push(
                dyn_lib_path
                    .as_ref()
                    .join("libruntime.lib")
                    .display()
                    .to_string(),
            );
        }
        buf.push("/link".to_string());
        buf.push(format!("/LIBPATH:{lpath}"));
    }
}

pub fn make_linker_command<S, P>(
    cc: CC,
    user_cc: Option<CC>,
    config: LinkerConfig<P>,
    user_link_flags: &[S],
    src: &[S],
    dest_dir: &str,
    dest: &str,
) -> Vec<String>
where
    S: AsRef<str>,
    P: AsRef<Path>,
{
    let resolved_cc = resolve_cc(&cc, user_cc.as_ref());
    let lib_path = &MOON_DIRS.moon_lib_path.display().to_string();
    make_linker_command_pure(
        resolved_cc,
        config,
        user_link_flags,
        src,
        dest_dir,
        dest,
        lib_path,
    )
}

pub fn make_linker_command_pure<S, P>(
    cc: CC,
    config: LinkerConfig<P>,
    user_link_flags: &[S],
    src: &[S],
    dest_dir: &str,
    dest: &str,
    lpath: &str,
) -> Vec<String>
where
    S: AsRef<str>,
    P: AsRef<Path>,
{
    let mut buf = vec![cc.cc_path.clone()];
    // if user_link_flags is set, we only set necessary flags
    // that are tightly coupled with the paths and output types
    // as user cannot easily specify them in the configuration file
    let has_user_flags = !user_link_flags.is_empty();

    add_linker_output_flags(&cc, &mut buf, &config, dest);
    add_linker_library_paths(&cc, &mut buf, &config, lpath);
    add_linker_intermediate_dir_flags(&cc, &mut buf, dest_dir);
    add_linker_shared_lib_flags(&cc, &mut buf, &config);

    // Linker compiler-specific flags
    add_linker_msvc_specific_flags(&cc, &mut buf, has_user_flags);

    add_linker_moonbitrun_with_warnings(&cc, &mut buf, &config);

    buf.extend(src.iter().map(|s| s.as_ref().to_string()));

    add_linker_common_libraries(&cc, &mut buf, &config);
    add_linker_msvc_runtime(&cc, &mut buf, &config, lpath);

    buf.extend(user_link_flags.iter().map(|s| s.as_ref().to_string()));

    buf
}

// Helper functions for CC command building
fn add_cc_output_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig, dest: Option<&str>) {
    let Some(dest) = dest else {
        return;
    };
    if cc.is_msvc() {
        match config.output_ty {
            OutputType::Object => {
                buf.push(format!("/Fo{dest}"));
            }
            OutputType::SharedLib | OutputType::Executable => {
                buf.push(format!("/Fe{dest}"));
            }
        }
    } else if cc.is_gcc_like() {
        buf.push("-o".to_string());
        buf.push(dest.to_string());
    }
}

fn add_cc_include_and_lib_paths(cc: &CC, buf: &mut Vec<String>, ipath: &str, lpath: &str) {
    if cc.is_msvc() {
        buf.push(format!("/I{ipath}"));
    } else if cc.is_tcc() {
        buf.push(format!("-I{ipath}"));
        buf.push(format!("-L{lpath}"));
    } else if cc.is_gcc_like() {
        buf.push(format!("-I{ipath}"));
    }
}

fn add_cc_intermediate_dir_flags(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &CCConfig,
    dest_dir: &str,
) {
    // MSVC may throw intermediate files into current directory
    // Explicitly set the output directory of these files
    if cc.is_msvc() && config.output_ty != OutputType::Object {
        buf.push(format!("/Fo{dest_dir}\\"));
    }
}

fn add_cc_debug_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if config.debug_info {
        if cc.is_gcc_like() {
            buf.push("-g".to_string());
        } else if cc.is_msvc() {
            buf.push("/Z7".to_string());
        }
    }
}

fn add_cc_shared_lib_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if config.output_ty == OutputType::SharedLib {
        if cc.is_msvc() {
            buf.push("/LD".to_string());
        } else if cc.is_gcc_like() {
            buf.push("-shared".to_string());
            buf.push("-fPIC".to_string());
        }
    }
}

fn add_cc_compile_only_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if config.output_ty == OutputType::Object {
        if cc.is_msvc() {
            buf.push("/c".to_string());
        } else if cc.is_gcc_like() {
            buf.push("-c".to_string());
        }
    }
}

// Compiler-specific flags grouped together
fn add_cc_msvc_specific_flags(cc: &CC, buf: &mut Vec<String>, has_user_flags: bool) {
    if !cc.is_msvc() {
        return;
    }

    // MSVC-specific misc options
    if !has_user_flags {
        buf.push("/utf-8".to_string());
        buf.push("/wd4819".to_string());
        buf.push("/nologo".to_string());
    }
}

fn add_cc_gcc_like_specific_flags(cc: &CC, buf: &mut Vec<String>) {
    // the below flags are needed, ref: https://github.com/moonbitlang/core/issues/1594#issuecomment-2649652455
    if cc.is_full_featured_gcc_like() {
        buf.push("-fwrapv".to_string());
        buf.push("-fno-strict-aliasing".to_string());
        // Apple clang is usually detected as SystemCC on macOS.
        if matches!(cc.cc_kind, CCKind::Clang)
            || (cfg!(target_os = "macos") && matches!(cc.cc_kind, CCKind::SystemCC))
        {
            buf.push("-Wno-unused-value".to_string());
        }
    }
}

fn add_cc_tcc_specific_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if !cc.is_tcc() {
        return;
    }

    if config.no_sys_header {
        buf.push("-DMOONBIT_NATIVE_NO_SYS_HEADER".to_string());
    } else {
        eprintln!(
            "{}: Use tcc without set MOONBIT_NATIVE_NO_SYS_HEADER.",
            "Warning".yellow().bold(),
        );
    }
}

fn add_cc_optimization_flags(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &CCConfig,
    has_user_flags: bool,
) {
    if !has_user_flags {
        match config.opt_level {
            OptLevel::Speed => {
                if cc.is_msvc() {
                    buf.push("/O2".to_string());
                } else if cc.is_full_featured_gcc_like() {
                    buf.push("-O2".to_string());
                }
            }
            OptLevel::Size => {
                if cc.is_msvc() {
                    buf.push("/Os".to_string());
                } else if cc.is_full_featured_gcc_like() {
                    buf.push("-Os".to_string());
                }
            }
            OptLevel::Debug => {
                if cc.is_msvc() {
                    buf.push("/Od".to_string());
                } else if cc.is_full_featured_gcc_like() {
                    buf.push("-Og".to_string());
                }
            }
            OptLevel::None => {
                if cc.is_msvc() {
                    buf.push("/Od".to_string());
                } else if cc.is_full_featured_gcc_like() {
                    buf.push("-O0".to_string());
                }
            }
        }
    }
}

fn add_cc_shared_runtime_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    // always set this even if user_cc_flags is set
    // user cannot easily know when we use shared runtime
    if config.define_use_shared_runtime_macro {
        if cc.is_msvc() {
            buf.push("/DMOONBIT_USE_SHARED_RUNTIME".to_string());
        } else if cc.is_gcc_like() {
            buf.push("-fPIC".to_string());
            buf.push("-DMOONBIT_USE_SHARED_RUNTIME".to_string());
        }
    }
}

// CC compiler-specific handling for moonbitrun
fn add_cc_moonbitrun_with_warnings(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if config.output_ty != OutputType::Object
        && config.link_moonbitrun
        && cc.is_libmoonbitrun_o_available()
    {
        if cc.is_tcc() {
            eprintln!(
                "{}: Cannot link libmoonbitrun.o when using tcc",
                "Warning".yellow().bold(),
            );
        } else {
            buf.push(
                MOON_DIRS
                    .moon_lib_path
                    .join("libmoonbitrun.o")
                    .display()
                    .to_string(),
            );
        }
    }
}

fn add_cc_common_libraries(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if cc.should_link_libm() && config.output_ty != OutputType::Object {
        buf.push("-lm".to_string());
    }
}

fn add_cc_msvc_linker_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig, lpath: &str) {
    if cc.is_msvc() && config.output_ty != OutputType::Object {
        buf.push("/link".to_string());
        buf.push(format!("/LIBPATH:{lpath}"));
    }
}

pub fn make_cc_command<S>(
    cc: CC,
    user_cc: Option<CC>,
    config: CCConfig,
    user_cc_flags: &[S],
    src: impl IntoIterator<Item = impl Into<String>>,
    intermediate_dir: &str,
    dest: &str,
) -> Vec<String>
where
    S: AsRef<str>,
{
    let resolved_cc = resolve_cc(&cc, user_cc.as_ref());
    let paths = CompilerPaths::from_moon_dirs();
    make_cc_command_pure(
        resolved_cc,
        config,
        user_cc_flags,
        src,
        intermediate_dir,
        Some(dest),
        &paths,
    )
}

pub fn make_cc_command_pure<S>(
    cc: CC,
    config: CCConfig,
    user_cc_flags: &[S],
    src: impl IntoIterator<Item = impl Into<String>>,
    intermediate_dir: &str,
    dest: Option<&str>,
    paths: &CompilerPaths,
) -> Vec<String>
where
    S: AsRef<str>,
{
    let mut buf = vec![cc.cc_path.clone()];

    // if user_cc_flags is set, we only set necessary flags
    // that are tightly coupled with the paths and output types
    // as user cannot easily specify them in the configuration file
    let has_user_flags = !user_cc_flags.is_empty();

    add_cc_output_flags(&cc, &mut buf, &config, dest);
    add_cc_include_and_lib_paths(&cc, &mut buf, &paths.include_path, &paths.lib_path);
    add_cc_intermediate_dir_flags(&cc, &mut buf, &config, intermediate_dir);
    add_cc_debug_flags(&cc, &mut buf, &config);
    add_cc_shared_lib_flags(&cc, &mut buf, &config);
    add_cc_compile_only_flags(&cc, &mut buf, &config);

    // Compiler-specific flags
    add_cc_msvc_specific_flags(&cc, &mut buf, has_user_flags);
    add_cc_gcc_like_specific_flags(&cc, &mut buf);
    add_cc_tcc_specific_flags(&cc, &mut buf, &config);

    add_cc_optimization_flags(&cc, &mut buf, &config, has_user_flags);
    add_cc_shared_runtime_flags(&cc, &mut buf, &config);
    add_cc_moonbitrun_with_warnings(&cc, &mut buf, &config);

    buf.extend(src.into_iter().map(|s| s.into()));

    add_cc_common_libraries(&cc, &mut buf, &config);
    buf.extend(user_cc_flags.iter().map(|s| s.as_ref().to_string()));
    add_cc_msvc_linker_flags(&cc, &mut buf, &config, &paths.lib_path);

    buf
}
#[cfg(test)]
mod tests {
    use super::*;

    fn fake_cc(kind: CCKind, target_triple: Option<&str>) -> CC {
        CC {
            cc_kind: kind,
            cc_path: "cc".to_string(),
            ar_kind: ARKind::GnuAr,
            ar_path: "ar".to_string(),
            target_triple: target_triple.map(str::to_string),
            is_env_override: false,
        }
    }

    fn executable_cc_config() -> CCConfig {
        CCConfig {
            debug_info: false,
            link_moonbitrun: false,
            no_sys_header: false,
            output_ty: OutputType::Executable,
            opt_level: OptLevel::Speed,
            define_use_shared_runtime_macro: false,
        }
    }

    #[test]
    fn clang_msvc_target_does_not_link_libm() {
        let cc = fake_cc(CCKind::Clang, Some("x86_64-pc-windows-msvc"));

        let mut cc_flags = vec![];
        add_cc_common_libraries(&cc, &mut cc_flags, &executable_cc_config());
        assert!(!cc_flags.iter().any(|f| f == "-lm"));

        let linker_config = LinkerConfig::<&Path> {
            link_moonbitrun: false,
            output_ty: OutputType::Executable,
            link_shared_runtime: None,
        };
        let mut linker_flags = vec![];
        add_linker_common_libraries(&cc, &mut linker_flags, &linker_config);
        assert!(!linker_flags.iter().any(|f| f == "-lm"));
    }

    #[test]
    fn clang_gnu_target_keeps_linking_libm() {
        let cc = fake_cc(CCKind::Clang, Some("x86_64-unknown-linux-gnu"));

        let mut cc_flags = vec![];
        add_cc_common_libraries(&cc, &mut cc_flags, &executable_cc_config());
        assert!(cc_flags.iter().any(|f| f == "-lm"));

        let linker_config = LinkerConfig::<&Path> {
            link_moonbitrun: false,
            output_ty: OutputType::Executable,
            link_shared_runtime: None,
        };
        let mut linker_flags = vec![];
        add_linker_common_libraries(&cc, &mut linker_flags, &linker_config);
        assert!(linker_flags.iter().any(|f| f == "-lm"));
    }

    #[test]
    fn try_from_path_recognizes_clang_exe() {
        let cc = CC::try_from_path("C:/llvm/bin/clang.exe").expect("parse clang.exe");
        assert!(matches!(cc.cc_kind, CCKind::Clang));
    }

    fn normalize_path_separators(path: &str) -> String {
        path.replace('\\', "/")
    }

    #[test]
    fn try_from_path_keeps_original_casing_for_fallback_archiver_name() {
        let cc =
            CC::try_from_path("/LLVM/bin/X86_64-W64-MINGW32-CLANG.ExE").expect("parse clang path");
        assert_eq!(
            normalize_path_separators(&cc.ar_path),
            "/LLVM/bin/X86_64-W64-MINGW32-ar.ExE"
        );
    }

    #[test]
    fn try_from_path_uses_clang_reported_archiver_when_available() {
        let clang_path = match which::which("clang") {
            Ok(path) => path,
            Err(err) => {
                if cfg!(windows) {
                    panic!("clang should exist on Windows CI for this regression test: {err}");
                }
                return;
            }
        };
        let Some(clang_path_str) = clang_path.to_str() else {
            return;
        };
        let reported_ar = match CC::probe_existing_prog_name(&clang_path, "ar") {
            Some(ar) => ar,
            None => {
                if cfg!(windows) {
                    panic!("clang -print-prog-name=ar should work on Windows CI");
                }
                return;
            }
        };

        let cc = CC::try_from_path(clang_path_str).expect("parse real clang path");
        assert_eq!(cc.ar_path, CC::resolve_tool_path(&clang_path, &reported_ar));
    }

    #[cfg(windows)]
    #[test]
    fn resolve_reported_prog_path_accepts_windows_exe_without_suffix() {
        let dir = std::env::temp_dir().join(format!(
            "moonutil-reported-prog-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();

        let reported = dir.join("bin").join("llvm-ar");
        let exe_path = reported.with_extension("exe");
        std::fs::write(&exe_path, []).unwrap();

        assert_eq!(
            CC::resolve_reported_prog_path(&reported.display().to_string()),
            Some(exe_path.display().to_string())
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
