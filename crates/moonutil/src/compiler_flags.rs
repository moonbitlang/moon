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
    ffi::OsStr,
    path::{Path, PathBuf},
};

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum CCKind {
    Msvc,
    SystemCC,
    Gcc,
    Clang,
    Tcc,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ARKind {
    MsvcLib,
    GnuAr,
    LlvmAr,
    TccAr,
}

#[derive(Clone, PartialEq, Eq)]
pub struct CC {
    pub cc_kind: CCKind,
    pub cc_path: String,
    pub ar_kind: ARKind,
    pub ar_path: String,
}

impl Default for CC {
    fn default() -> Self {
        NATIVE_CC.clone()
    }
}

#[cfg(target_os = "linux")]
const CAN_USE_MOONBITRUN: bool = true;
#[cfg(target_os = "macos")]
const CAN_USE_MOONBITRUN: bool = true;
#[cfg(windows)]
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
        }
    }

    pub fn try_from_cc_path_and_kind(
        ar_name: &str,
        cc_path: &Path,
        cc_kind: CCKind,
    ) -> anyhow::Result<Self> {
        let cc_dir = cc_path
            .parent()
            .expect("cc_path should have a parent directory");
        let (ar_kind, ar_path) = match cc_kind {
            CCKind::Msvc => {
                let ar = cc_dir.join(ar_name);
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

    pub fn try_from_path(cc: &str) -> anyhow::Result<Self> {
        let path = PathBuf::from(cc);
        let name = path.file_name().and_then(OsStr::to_str).unwrap();
        let replaced_ar = |s: &str| name.replace(s, "ar");
        if name.contains("cl") {
            CC::try_from_cc_path_and_kind("lib.exe", &path, CCKind::Msvc)
        } else if name.contains("gcc") {
            CC::try_from_cc_path_and_kind(&replaced_ar("gcc"), &path, CCKind::Gcc)
        } else if name.contains("clang") {
            CC::try_from_cc_path_and_kind(&replaced_ar("clang"), &path, CCKind::Clang)
        } else if name.contains("cc") {
            CC::try_from_cc_path_and_kind(&replaced_ar("cc"), &path, CCKind::SystemCC)
        } else if name.contains("tcc") {
            CC::try_from_cc_path_and_kind("", &path, CCKind::Tcc)
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
}

// change all struct construction to CC::new
pub static NATIVE_CC: std::sync::LazyLock<CC> = std::sync::LazyLock::new(|| {
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputType {
    Object,
    SharedLib,
    Executable,
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
    pub debug_info: bool,
    // Some compilers, like TCC, may not be able to handle the system header
    // In this case, we need to disable the system header used in the runtime
    pub link_moonbitrun: bool,
    #[builder(default = false)]
    pub no_sys_header: bool,
    #[builder(default = OutputType::Object)]
    pub output_ty: OutputType,
    #[builder(default = OptLevel::Speed)]
    pub opt_level: OptLevel,
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
    let cc = user_cc.unwrap_or(cc);
    let mut buf = vec![cc.ar_path.clone()];

    if cc.is_msvc() {
        buf.push("/nologo".to_string());
        buf.push(format!("/Out:{}", dest));
    } else if cc.is_tcc() {
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

    if CAN_USE_MOONBITRUN && config.archive_moonbitrun && !cc.is_msvc() {
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

    buf.extend(src.iter().map(|s| s.as_ref().to_string()));

    buf
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
    let cc = user_cc.unwrap_or(cc);
    let mut buf = vec![cc.cc_path.clone()];
    let lpath = &MOON_DIRS.moon_lib_path.display().to_string();
    // if user_link_flags is set, we only set necessary flags
    // that are tightly coupled with the paths and output types
    // as user cannot easily specify them in the configuration file
    let has_user_flags = !user_link_flags.is_empty();

    // Output file
    if cc.is_msvc() {
        match config.output_ty {
            OutputType::SharedLib | OutputType::Executable => {
                buf.push(format!("/Fe{}", dest));
            }
            _ => panic!("Linker only supports shared lib, executable and static lib"),
        }
    } else if cc.is_gcc_like() {
        buf.push("-o".to_string());
        buf.push(dest.to_string());
    }

    // Library paths
    if cc.is_gcc_like() {
        buf.push(format!("-L{}", lpath));
        if let Some(dyn_lib_path) = config.link_shared_runtime.as_ref() {
            buf.push(format!("-L{}", dyn_lib_path.as_ref().display()));
        }
    };

    // MSVC may throw intermediate files into current directory
    // Explicitly set the output directory of these files
    if cc.is_msvc() {
        buf.push(format!("/Fo{}\\", dest_dir));
    }

    // Build shared library
    if config.output_ty == OutputType::SharedLib && !has_user_flags {
        if cc.is_msvc() {
            buf.push("/LD".to_string());
        } else if cc.is_gcc_like() {
            buf.push("-shared".to_string());
            buf.push("-fPIC".to_string());
        }
    }

    // Misc options
    if cc.is_msvc() && !has_user_flags {
        buf.push("/nologo".to_string());
    }

    if CAN_USE_MOONBITRUN && config.link_moonbitrun && !cc.is_msvc() {
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

    buf.extend(src.iter().map(|s| s.as_ref().to_string()));

    // Link against some common libraries
    if cc.is_gcc_like() {
        if cc.is_full_featured_gcc_like() {
            buf.push("-lm".to_string());
        }
        if config.link_shared_runtime.is_some() {
            buf.push("-lruntime".to_string());
        }
    }

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
        buf.push(format!("/LIBPATH:{}", lpath));
    }

    buf.extend(user_link_flags.iter().map(|s| s.as_ref().to_string()));

    buf
}

pub fn make_cc_command<S>(
    cc: CC,
    user_cc: Option<CC>,
    config: CCConfig,
    user_cc_flags: &[S],
    src: &[S],
    dest_dir: &str,
    dest: &str,
) -> Vec<String>
where
    S: AsRef<str>,
{
    let cc = user_cc.unwrap_or(cc);
    let mut buf = vec![cc.cc_path.clone()];
    let ipath = &MOON_DIRS.moon_include_path.display().to_string();
    let lpath = &MOON_DIRS.moon_lib_path.display().to_string();

    // if user_cc_flags is set, we only set necessary flags
    // that are tightly coupled with the paths and output types
    // as user cannot easily specify them in the configuration file
    let has_user_flags = !user_cc_flags.is_empty();

    // Output file
    if cc.is_msvc() {
        match config.output_ty {
            OutputType::Object => {
                buf.push(format!("/Fo{}", dest));
            }
            OutputType::SharedLib | OutputType::Executable => {
                buf.push(format!("/Fe{}", dest));
            }
        }
    } else if cc.is_gcc_like() {
        buf.push("-o".to_string());
        buf.push(dest.to_string());
    }

    // Include and lib paths
    if cc.is_msvc() {
        buf.push(format!("/I{}", ipath));
    } else if cc.is_gcc_like() {
        buf.push(format!("-I{}", ipath));
        buf.push(format!("-L{}", lpath));
    };

    // MSVC may throw intermediate files into current directory
    // Explicitly set the output directory of these files
    if cc.is_msvc() && config.output_ty != OutputType::Object {
        buf.push(format!("/Fo{}\\", dest_dir));
    }

    // Generate debug info
    if config.debug_info && cc.is_gcc_like() {
        buf.push("-g".to_string());
    }

    // Build shared library
    if config.output_ty == OutputType::SharedLib {
        if cc.is_msvc() {
            buf.push("/LD".to_string());
        } else if cc.is_gcc_like() {
            buf.push("-shared".to_string());
            buf.push("-fPIC".to_string());
        }
    }

    // Compile without linking
    if config.output_ty == OutputType::Object {
        if cc.is_msvc() {
            buf.push("/c".to_string());
        } else if cc.is_gcc_like() {
            buf.push("-c".to_string());
        }
    }

    // Misc options
    if cc.is_msvc() && !has_user_flags {
        buf.push("/utf-8".to_string());
        buf.push("/wd4819".to_string());
        buf.push("/nologo".to_string());
    } else if cc.is_full_featured_gcc_like() && !has_user_flags {
        buf.push("-fwrapv".to_string());
        buf.push("-fno-strict-aliasing".to_string());
    }

    // Optimization level
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

    if cc.is_tcc() && !has_user_flags {
        if config.no_sys_header {
            buf.push("-DMOONBIT_NATIVE_NO_SYS_HEADER".to_string());
        } else {
            eprintln!(
                "{}: Use tcc without set MOONBIT_NATIVE_NO_SYS_HEADER.",
                "Warning".yellow().bold(),
            );
        }
    }

    // always set this even if user_cc_flags is set
    // user cannot easily know when we use shared runtime
    if config.define_use_shared_runtime_macro {
        if cc.is_msvc() {
            buf.push("/DMOONBIT_USE_SHARED_RUNTIME".to_string());
        } else if cc.is_gcc_like() {
            buf.push("-DMOONBIT_USE_SHARED_RUNTIME".to_string());
        }
    }

    if config.output_ty != OutputType::Object
        && CAN_USE_MOONBITRUN
        && config.link_moonbitrun
        && !cc.is_msvc()
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
    buf.extend(src.iter().map(|s| s.as_ref().to_string()));

    // Link against some common libraries
    if cc.is_full_featured_gcc_like() && config.output_ty != OutputType::Object {
        buf.push("-lm".to_string());
    }

    buf.extend(user_cc_flags.iter().map(|s| s.as_ref().to_string()));

    // MSVC specific linker flags
    if cc.is_msvc() && config.output_ty != OutputType::Object {
        buf.push("/link".to_string());
        buf.push(format!("/LIBPATH:{}", lpath));
    }

    buf
}
