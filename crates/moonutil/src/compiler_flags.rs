use crate::moon_dir::MOON_DIRS;
use anyhow::Context;
use colored::Colorize;
use derive_builder::Builder;
use std::ffi::OsStr;

#[derive(Clone, PartialEq, Eq)]
pub enum CCKind {
    Msvc,
    SystemCC,
    Gcc,
    Clang,
    Tcc,
}

#[derive(Clone, PartialEq, Eq)]
pub struct CC {
    pub kind: CCKind,
    pub path: String,
}

impl CC {
    fn new(kind: CCKind, path: String) -> Self {
        CC { kind, path }
    }

    pub fn try_from_path(path: &str) -> anyhow::Result<Self> {
        let path = which::which(path).context(format!("{} not found", path))?;
        match path.file_name().and_then(OsStr::to_str) {
            Some("cl") => Ok(CC::new(CCKind::Msvc, path.display().to_string())),
            Some("gcc") => Ok(CC::new(CCKind::Gcc, path.display().to_string())),
            Some("clang") => Ok(CC::new(CCKind::Clang, path.display().to_string())),
            Some("cc") => Ok(CC::new(CCKind::SystemCC, "cc".to_string())),
            Some("tcc") => Ok(CC::new(CCKind::Tcc, path.display().to_string())),
            // assume all other names are system cc, which also should be
            Some(_) => Ok(CC::new(CCKind::SystemCC, path.display().to_string())),
            None => Err(anyhow::anyhow!("{} not found", path.display())),
        }
    }

    pub fn default() -> Self {
        NATIVE_CC.clone()
    }

    pub fn is_gcc_like(&self) -> bool {
        matches!(
            self.kind,
            CCKind::SystemCC | CCKind::Gcc | CCKind::Clang | CCKind::Tcc
        )
    }

    pub fn is_msvc(&self) -> bool {
        matches!(self.kind, CCKind::Msvc)
    }

    pub fn is_tcc(&self) -> bool {
        matches!(self.kind, CCKind::Tcc)
    }
}

// change all struct construction to CC::new
pub static NATIVE_CC: std::sync::LazyLock<CC> = std::sync::LazyLock::new(|| {
    use CCKind::*;
    if let Ok(cc) = which::which("cl") {
        CC::new(Msvc, cc.display().to_string())
    } else if let Ok(_cc) = which::which("cc") {
        CC::new(SystemCC, "cc".to_string())
    } else if let Ok(cc) = which::which("gcc") {
        CC::new(Gcc, cc.display().to_string())
    } else if let Ok(cc) = which::which("clang") {
        CC::new(Clang, cc.display().to_string())
    } else {
        let cc = which::which(&MOON_DIRS.internal_tcc_path)
            .context("internal tcc not found")
            .unwrap();
        CC::new(Tcc, cc.display().to_string())
    }
});

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputType {
    Object,
    SharedLib,
    StaticLib,
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
    #[builder(default = false)]
    pub link_moonbitrun: bool,
    #[builder(default = false)]
    pub no_sys_header: bool,
    #[builder(default = OutputType::Object)]
    pub output_ty: OutputType,
    #[builder(default = OptLevel::Speed)]
    pub opt_level: OptLevel,
}

#[derive(Clone, PartialEq, Eq, Builder)]
#[builder(setter(into))]
pub struct LinkConfig {
    #[builder(default = false)]
    pub link_moonbitrun: bool,
    #[builder(default = OutputType::Executable)]
    pub output_ty: OutputType,
}

pub fn make_linker_command<S>(
    cc: CC,
    user_cc: Option<CC>,
    config: LinkConfig,
    user_link_flags: &[S],
    src: &[S],
    dest_dir: &str,
    dest: &str,
) -> Vec<String>
where
    S: AsRef<str>,
{
    let cc = user_cc.unwrap_or(cc);
    let mut buf = vec![cc.path.clone()];
    let lpath = &MOON_DIRS.moon_lib_path.display().to_string();
    // if user_link_flags is set, we only set necessary flags
    // that are tightly coupled with the paths and output types
    // as user cannot easily specify them in the configuration file
    let has_user_flags = !user_link_flags.is_empty();

    // Library paths
    if cc.is_msvc() {
        buf.push(format!("/link /LIBPATH:{}", lpath));
    } else if cc.is_gcc_like() {
        buf.push(format!("-L{}", lpath));
    };

    // MSVC may throw intermediate files into current directory
    // Explicitly set the output directory of these files
    if cc.is_msvc() {
        buf.push(format!("/Fo{}\\", dest_dir));
    }

    // Output file
    if cc.is_msvc() {
        match config.output_ty {
            OutputType::SharedLib | OutputType::Executable | OutputType::StaticLib => {
                buf.push(format!("/Fe{}", dest));
            }
            _ => panic!("Linker only supports shared lib, executable and static lib"),
        }
    } else if cc.is_gcc_like() {
        buf.push("-o".to_string());
        buf.push(dest.to_string());
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

    // Link against some common libraries
    if cc.is_gcc_like() && !has_user_flags {
        buf.push("-lm".to_string());
    }

    // Misc options
    if cc.is_msvc() && !has_user_flags {
        buf.push("/nologo".to_string());
    }

    if config.link_moonbitrun && !cc.is_msvc() {
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
    let mut buf = vec![cc.path.clone()];
    let ipath = &MOON_DIRS.moon_include_path.display().to_string();
    let lpath = &MOON_DIRS.moon_lib_path.display().to_string();

    // if user_cc_flags is set, we only set necessary flags
    // that are tightly coupled with the paths and output types
    // as user cannot easily specify them in the configuration file
    let has_user_flags = !user_cc_flags.is_empty();

    // Include and lib paths
    if cc.is_msvc() {
        buf.push(format!("/I{}", ipath));
        buf.push(format!("/link /LIBPATH:{}", lpath));
    } else if cc.is_gcc_like() {
        buf.push(format!("-I{}", ipath));
        buf.push(format!("-L{}", lpath));
    };

    // MSVC may throw intermediate files into current directory
    // Explicitly set the output directory of these files
    if cc.is_msvc() {
        buf.push(format!("/Fo{}\\", dest_dir));
    }

    // Generate debug info
    if config.debug_info {
        if cc.is_msvc() {
            buf.push("/Zi".to_string());
        } else if cc.is_gcc_like() {
            buf.push("-g".to_string());
        }
    }

    // Output file
    if cc.is_msvc() {
        match config.output_ty {
            OutputType::Object => {
                buf.push(format!("/Fo{}", dest));
            }
            OutputType::SharedLib | OutputType::Executable | OutputType::StaticLib => {
                buf.push(format!("/Fe{}", dest));
            }
        }
    } else if cc.is_gcc_like() {
        buf.push("-o".to_string());
        buf.push(dest.to_string());
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
    } else if cc.is_gcc_like() && !has_user_flags {
        buf.push("-fwrapv".to_string());
        buf.push("-fno-strict-aliasing".to_string());
    }

    // Optimization level
    if !has_user_flags {
        match config.opt_level {
            OptLevel::Speed => {
                if cc.is_msvc() {
                    buf.push("/O2".to_string());
                } else if cc.is_gcc_like() {
                    buf.push("-O2".to_string());
                }
            }
            OptLevel::Size => {
                if cc.is_msvc() {
                    buf.push("/Os".to_string());
                } else if cc.is_gcc_like() {
                    buf.push("-Os".to_string());
                }
            }
            OptLevel::Debug => {
                if cc.is_msvc() {
                    buf.push("/Od".to_string());
                } else if cc.is_gcc_like() {
                    buf.push("-Og".to_string());
                }
            }
            OptLevel::None => {
                if cc.is_msvc() {
                    buf.push("/Od".to_string());
                } else if cc.is_gcc_like() {
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

    if config.output_ty != OutputType::Object && config.link_moonbitrun && !cc.is_msvc() {
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

    buf.extend(user_cc_flags.iter().map(|s| s.as_ref().to_string()));

    buf
}
