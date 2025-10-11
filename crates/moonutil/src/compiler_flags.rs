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
};

const ENV_MOON_CC: &str = "MOON_CC";
const ENV_MOON_AR: &str = "MOON_AR";

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum CCKind {
    Msvc,     // cl.exe
    SystemCC, // cc
    Gcc,      // gcc
    Clang,    // clang
    Tcc,      // tcc
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ARKind {
    MsvcLib, // lib.exe
    GnuAr,   // ar
    LlvmAr,  // llvm-ar
    TccAr,   // tcc -ar
}

#[derive(Clone, PartialEq, Eq)]
pub struct CC {
    pub cc_kind: CCKind,
    pub cc_path: String,
    pub ar_kind: ARKind,
    pub ar_path: String,
    pub is_env_override: bool, // Whether the cc is set by env MOON_CC
}

impl Default for CC {
    fn default() -> Self {
        NATIVE_CC().clone()
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

    fn new(cc_kind: CCKind, cc_path: String, ar_kind: ARKind, ar_path: String) -> Self {
        CC {
            cc_kind,
            cc_path,
            ar_kind,
            ar_path,
            is_env_override: false,
        }
    }

    pub fn try_from_cc_path_and_kind(
        ar_name: &str,
        cc_path: &Path,
        cc_kind: CCKind,
    ) -> anyhow::Result<Self> {
        let cc_dir = cc_path
            .parent()
            .context("cc_path should have a parent directory")?;
        let (ar_kind, ar_path) = match cc_kind {
            CCKind::Msvc => {
                let ar = cc_dir.join("lib");
                (ARKind::MsvcLib, ar.display().to_string())
            }
            CCKind::SystemCC => {
                let ar = cc_dir.join(ar_name);
                (ARKind::GnuAr, ar.display().to_string())
            }
            CCKind::Gcc => {
                let ar = cc_dir.join(ar_name);
                (ARKind::GnuAr, ar.display().to_string())
            }
            CCKind::Clang => {
                let ar = cc_dir.join(ar_name);
                (ARKind::GnuAr, ar.display().to_string())
            }
            CCKind::Tcc => (ARKind::TccAr, cc_path.display().to_string()),
        };
        Ok(CC::new(
            cc_kind,
            cc_path.display().to_string(),
            ar_kind,
            ar_path,
        ))
    }

    pub fn try_from_path_with_ar(cc: &str, ar: &str) -> anyhow::Result<Self> {
        let path = PathBuf::from(cc);
        let name = path.file_name().and_then(OsStr::to_str).context(
            "Invalid compiler path: path must point to a file with valid UTF-8 filename",
        )?;
        if name.ends_with("cl") {
            CC::try_from_cc_path_and_kind(ar, &path, CCKind::Msvc)
        } else if name.ends_with("gcc") {
            CC::try_from_cc_path_and_kind(ar, &path, CCKind::Gcc)
        } else if name.ends_with("clang") {
            CC::try_from_cc_path_and_kind(ar, &path, CCKind::Clang)
        } else if name.ends_with("tcc") {
            CC::try_from_cc_path_and_kind(ar, &path, CCKind::Tcc)
        } else if name.ends_with("cc") {
            CC::try_from_cc_path_and_kind(ar, &path, CCKind::SystemCC)
        } else {
            // assume it's a system cc
            CC::try_from_cc_path_and_kind(ar, &path, CCKind::SystemCC)
        }
    }

    pub fn try_from_path(cc: &str) -> anyhow::Result<Self> {
        let path = PathBuf::from(cc);
        let name = path.file_name().and_then(OsStr::to_str).context(
            "Invalid compiler path: path must point to a file with valid UTF-8 filename",
        )?;
        let replaced_ar = |s: &str| name.replace(s, "ar");
        if name.ends_with("cl") {
            CC::try_from_cc_path_and_kind("lib.exe", &path, CCKind::Msvc)
        } else if name.ends_with("gcc") {
            CC::try_from_cc_path_and_kind(&replaced_ar("gcc"), &path, CCKind::Gcc)
        } else if name.ends_with("clang") {
            CC::try_from_cc_path_and_kind(&replaced_ar("clang"), &path, CCKind::Clang)
        } else if name.ends_with("tcc") {
            CC::try_from_cc_path_and_kind("", &path, CCKind::Tcc)
        } else if name.ends_with("cc") {
            CC::try_from_cc_path_and_kind(&replaced_ar("cc"), &path, CCKind::SystemCC)
        } else {
            // assume it's a system cc
            CC::try_from_cc_path_and_kind("ar", &path, CCKind::SystemCC)
        }
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

    pub fn is_libmoonbitrun_o_available(&self) -> bool {
        // If users set MOON_CC, we believe they know what they are doing
        // And we conservatively disable libmoonbitrun.o
        CAN_USE_MOONBITRUN && !self.is_msvc() && !self.is_env_override
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

    CC::try_from_cc_path_and_kind("ar", &cc_path, cc_kind)
        .context("failed to find ar")
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputType {
    Object,     // .o or .obj
    SharedLib,  // .so or .dll or .dylib
    Executable, // .exe or no extension
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    Speed,
    Size,
    Debug,
    None,
}

#[derive(Clone, PartialEq, Eq, Builder)]
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

#[derive(Clone, PartialEq, Eq, Builder)]
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

#[derive(Clone, PartialEq, Eq, Builder)]
#[builder(setter(into))]
pub struct ArchiverConfig {
    #[builder(default = false)]
    pub archive_moonbitrun: bool,
}

/// Resolve the C compiler to use from global state
pub fn resolve_cc(cc: CC, user_cc: Option<CC>) -> CC {
    ENV_CC.clone().unwrap_or_else(|| user_cc.unwrap_or(cc))
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
    let resolved_cc = resolve_cc(cc, user_cc);
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
    if cc.is_gcc_like() {
        if let Some(dyn_lib_path) = config.link_shared_runtime.as_ref() {
            buf.push(format!("-L{}", dyn_lib_path.as_ref().display()));
        }
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
        if cc.is_full_featured_gcc_like() {
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
    let resolved_cc = resolve_cc(cc, user_cc);
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
fn add_cc_output_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig, dest: &str) {
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

fn add_cc_include_and_lib_paths(cc: &CC, buf: &mut Vec<String>, config: &CCConfig, ipath: &str, lpath: &str) {
    if cc.is_msvc() {
        buf.push(format!("/I{ipath}"));
    } else if cc.is_tcc() {
        buf.push(format!("-I{ipath}"));
        // Only add -L when not building object files (i.e., when linking)
        if config.output_ty != OutputType::Object {
            buf.push(format!("-L{lpath}"));
        }
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
    if config.debug_info && cc.is_gcc_like() {
        buf.push("-g".to_string());
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
    if cc.is_full_featured_gcc_like() && config.output_ty != OutputType::Object {
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
    dest_dir: &str,
    dest: &str,
) -> Vec<String>
where
    S: AsRef<str>,
{
    let resolved_cc = resolve_cc(cc, user_cc);
    let paths = CompilerPaths::from_moon_dirs();
    make_cc_command_pure(
        resolved_cc,
        config,
        user_cc_flags,
        src,
        dest_dir,
        dest,
        &paths,
    )
}

pub fn make_cc_command_pure<S>(
    cc: CC,
    config: CCConfig,
    user_cc_flags: &[S],
    src: impl IntoIterator<Item = impl Into<String>>,
    dest_dir: &str,
    dest: &str,
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
    add_cc_include_and_lib_paths(&cc, &mut buf, &config, &paths.include_path, &paths.lib_path);
    add_cc_intermediate_dir_flags(&cc, &mut buf, &config, dest_dir);
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

    #[test]
    fn test_tcc_no_l_flag_for_object_output() {
        // Create a TCC compiler
        let cc = CC::new(CCKind::Tcc, "tcc".to_string(), ARKind::TccAr, "tcc".to_string());
        
        // Test object file compilation - should not include -L flag
        let config = CCConfigBuilder::default()
            .output_ty(OutputType::Object)
            .link_moonbitrun(false)
            .define_use_shared_runtime_macro(false)
            .build()
            .unwrap();
        
        let paths = CompilerPaths {
            include_path: "/test/include".to_string(),
            lib_path: "/test/lib".to_string(),
        };
        
        let cmd = make_cc_command_pure(
            cc.clone(),
            config,
            &[] as &[&str],
            vec!["test.c"],
            "/test/dest",
            "/test/dest/test.o",
            &paths,
        );
        
        // Should contain -I but not -L for object files
        assert!(cmd.contains(&"-I/test/include".to_string()));
        assert!(!cmd.contains(&"-L/test/lib".to_string()));
    }

    #[test]
    fn test_tcc_with_l_flag_for_executable_output() {
        // Create a TCC compiler
        let cc = CC::new(CCKind::Tcc, "tcc".to_string(), ARKind::TccAr, "tcc".to_string());
        
        // Test executable compilation - should include -L flag
        let config = CCConfigBuilder::default()
            .output_ty(OutputType::Executable)
            .link_moonbitrun(false)
            .define_use_shared_runtime_macro(false)
            .build()
            .unwrap();
        
        let paths = CompilerPaths {
            include_path: "/test/include".to_string(),
            lib_path: "/test/lib".to_string(),
        };
        
        let cmd = make_cc_command_pure(
            cc.clone(),
            config,
            &[] as &[&str],
            vec!["test.c"],
            "/test/dest",
            "/test/dest/test",
            &paths,
        );
        
        // Should contain both -I and -L for executable
        assert!(cmd.contains(&"-I/test/include".to_string()));
        assert!(cmd.contains(&"-L/test/lib".to_string()));
    }

    #[test]
    fn test_gcc_no_l_flag_for_any_output() {
        // Create a GCC compiler
        let cc = CC::new(CCKind::Gcc, "gcc".to_string(), ARKind::GnuAr, "ar".to_string());
        
        // Test object file compilation
        let config = CCConfigBuilder::default()
            .output_ty(OutputType::Object)
            .link_moonbitrun(false)
            .define_use_shared_runtime_macro(false)
            .build()
            .unwrap();
        
        let paths = CompilerPaths {
            include_path: "/test/include".to_string(),
            lib_path: "/test/lib".to_string(),
        };
        
        let cmd = make_cc_command_pure(
            cc.clone(),
            config,
            &[] as &[&str],
            vec!["test.c"],
            "/test/dest",
            "/test/dest/test.o",
            &paths,
        );
        
        // Should contain -I but never -L for GCC-like compilers in this context
        assert!(cmd.contains(&"-I/test/include".to_string()));
        assert!(!cmd.contains(&"-L/test/lib".to_string()));
    }

    #[test]
    fn test_integration_l_flag_fix() {
        // Create a TCC compiler instance
        let cc = CC::new(CCKind::Tcc, "tcc".to_string(), ARKind::TccAr, "tcc".to_string());
        
        let paths = CompilerPaths {
            include_path: "/tmp/include".to_string(),
            lib_path: "/tmp/lib".to_string(),
        };

        // Test 1: Object file compilation (should NOT include -L flag)
        let object_config = CCConfigBuilder::default()
            .output_ty(OutputType::Object)
            .link_moonbitrun(false)
            .define_use_shared_runtime_macro(false)
            .build()
            .unwrap();
        
        let object_cmd = make_cc_command_pure(
            cc.clone(),
            object_config,
            &[] as &[&str],
            vec!["test.c"],
            "/tmp/dest",
            "/tmp/dest/test.o",
            &paths,
        );
        
        // Check that object compilation doesn't include -L flag but does include -I
        let has_l_flag = object_cmd.iter().any(|arg| arg.starts_with("-L"));
        let has_i_flag = object_cmd.iter().any(|arg| arg.starts_with("-I"));
        let has_c_flag = object_cmd.contains(&"-c".to_string());
        
        println!("Object command: {}", object_cmd.join(" "));
        
        assert!(has_i_flag, "Object compilation should include -I flag");
        assert!(!has_l_flag, "Object compilation should NOT include -L flag");
        assert!(has_c_flag, "Object compilation should include -c flag");
        
        // Test 2: Executable compilation (should include -L flag)
        let exe_config = CCConfigBuilder::default()
            .output_ty(OutputType::Executable)
            .link_moonbitrun(false)
            .define_use_shared_runtime_macro(false)
            .build()
            .unwrap();
        
        let exe_cmd = make_cc_command_pure(
            cc.clone(),
            exe_config,
            &[] as &[&str],
            vec!["test.c"],
            "/tmp/dest",
            "/tmp/dest/test",
            &paths,
        );
        
        let has_l_flag_exe = exe_cmd.iter().any(|arg| arg.starts_with("-L"));
        let has_i_flag_exe = exe_cmd.iter().any(|arg| arg.starts_with("-I"));
        let has_c_flag_exe = exe_cmd.contains(&"-c".to_string());
        
        println!("Executable command: {}", exe_cmd.join(" "));
        
        assert!(has_i_flag_exe, "Executable compilation should include -I flag");
        assert!(has_l_flag_exe, "Executable compilation should include -L flag");
        assert!(!has_c_flag_exe, "Executable compilation should NOT include -c flag");
    }
}
