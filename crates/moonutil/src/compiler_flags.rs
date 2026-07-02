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
    ffi::{OsStr, OsString},
    fmt,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum NativeDriverSource {
    EnvOverride,
    PathProbe,
    PackageOverride,
}

/// Compiler/linker executable and archiver chosen for one native action.
#[derive(Clone, Debug)]
struct NativeCommandDriver {
    cc: CC,
    source: NativeDriverSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsvcEnvironment {
    pub cl_exe: PathBuf,
    pub env_pairs: Vec<(OsString, OsString)>,
    pub include_paths: Vec<PathBuf>,
    pub lib_paths: Vec<PathBuf>,
}

/// MSVC CRT policy shared by runtime, C stubs, and final linking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsvcCrtPolicy {
    StaticMt,
}

impl MsvcCrtPolicy {
    pub fn compiler_flag(self) -> &'static str {
        match self {
            Self::StaticMt => WINDOWS_MSVC_STATIC_RUNTIME_FLAG,
        }
    }
}

/// ABI family carried by a native compiler/linker selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeAbiFamily {
    Msvc,
    WindowsGnu,
    AppleDarwin,
    UnixLike,
    Other,
}

impl fmt::Display for NativeAbiFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Msvc => f.write_str("msvc"),
            Self::WindowsGnu => f.write_str("windows-gnu"),
            Self::AppleDarwin => f.write_str("apple-darwin"),
            Self::UnixLike => f.write_str("unix-like"),
            Self::Other => f.write_str("other"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeBuildContract {
    abi_family: NativeAbiFamily,
    msvc_crt_policy: Option<MsvcCrtPolicy>,
    msvc_environment: Option<MsvcEnvironment>,
}

/// Action-scoped driver paired with the native build contract it satisfies.
#[derive(Clone, Debug)]
pub struct NativeToolchain {
    driver: NativeCommandDriver,
    contract: NativeBuildContract,
}

impl NativeCommandDriver {
    fn from_env_override(cc: CC) -> Self {
        Self {
            cc,
            source: NativeDriverSource::EnvOverride,
        }
    }

    fn from_path_probe(cc: CC) -> Self {
        Self {
            cc,
            source: NativeDriverSource::PathProbe,
        }
    }

    fn from_package_override(cc: CC) -> Self {
        Self {
            cc,
            source: NativeDriverSource::PackageOverride,
        }
    }

    fn cc(&self) -> &CC {
        &self.cc
    }

    fn abi_family(&self) -> NativeAbiFamily {
        if self.cc.is_msvc() {
            return NativeAbiFamily::Msvc;
        }

        let Some(target) = self.cc.target_triple.as_deref() else {
            return NativeAbiFamily::Other;
        };

        if target.contains("msvc") {
            NativeAbiFamily::Msvc
        } else if target.contains("windows-gnu")
            || target.contains("windows-gnullvm")
            || target.contains("w64")
        {
            NativeAbiFamily::WindowsGnu
        } else if target.contains("apple-darwin") {
            NativeAbiFamily::AppleDarwin
        } else if target.contains("linux") || target.contains("freebsd") {
            NativeAbiFamily::UnixLike
        } else {
            NativeAbiFamily::Other
        }
    }

    fn uses_msvc_driver(&self) -> bool {
        self.cc.is_msvc()
    }

    fn msvc_crt_policy(&self) -> Option<MsvcCrtPolicy> {
        self.uses_msvc_driver().then_some(MsvcCrtPolicy::StaticMt)
    }

    fn with_package_override(&self, package_cc: Option<&CC>) -> Self {
        match (self.source, package_cc) {
            (NativeDriverSource::EnvOverride, Some(_)) => {
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
            (NativeDriverSource::EnvOverride, None) | (_, None) => self.clone(),
            (_, Some(package_cc)) => Self::from_package_override(package_cc.clone()),
        }
    }
}

impl NativeBuildContract {
    fn from_driver(driver: &NativeCommandDriver) -> Self {
        Self {
            abi_family: driver.abi_family(),
            msvc_crt_policy: driver.msvc_crt_policy(),
            msvc_environment: None,
        }
    }

    pub fn abi_family(&self) -> NativeAbiFamily {
        self.abi_family
    }

    pub fn uses_msvc_abi(&self) -> bool {
        self.abi_family == NativeAbiFamily::Msvc
    }

    pub fn uses_msvc_link_library_names(&self) -> bool {
        self.uses_msvc_abi()
    }

    pub fn msvc_crt_policy(&self) -> Option<MsvcCrtPolicy> {
        self.msvc_crt_policy
    }

    pub fn msvc_environment(&self) -> Option<&MsvcEnvironment> {
        self.msvc_environment.as_ref()
    }

    pub fn with_msvc_environment(mut self, environment: MsvcEnvironment) -> Self {
        self.msvc_environment = Some(environment);
        self
    }
}

impl NativeToolchain {
    fn from_driver(driver: NativeCommandDriver) -> Self {
        let contract = NativeBuildContract::from_driver(&driver);
        Self { driver, contract }
    }

    pub fn from_env_override(cc: CC) -> Self {
        Self::from_driver(NativeCommandDriver::from_env_override(cc))
    }

    pub fn from_path_probe(cc: CC) -> Self {
        Self::from_driver(NativeCommandDriver::from_path_probe(cc))
    }

    pub fn contract(&self) -> &NativeBuildContract {
        &self.contract
    }

    fn driver(&self) -> &NativeCommandDriver {
        &self.driver
    }

    pub fn cc(&self) -> &CC {
        self.driver.cc()
    }

    pub fn abi_family(&self) -> NativeAbiFamily {
        self.contract.abi_family()
    }

    pub fn uses_msvc_abi(&self) -> bool {
        self.contract.uses_msvc_abi()
    }

    pub fn uses_msvc_driver(&self) -> bool {
        self.driver.uses_msvc_driver()
    }

    pub fn uses_msvc_link_library_names(&self) -> bool {
        self.contract.uses_msvc_link_library_names()
    }

    pub fn msvc_crt_policy(&self) -> Option<MsvcCrtPolicy> {
        self.contract.msvc_crt_policy()
    }

    pub fn msvc_environment(&self) -> Option<&MsvcEnvironment> {
        self.contract.msvc_environment()
    }

    pub fn with_msvc_environment(mut self, environment: MsvcEnvironment) -> Self {
        self.contract = self.contract.with_msvc_environment(environment);
        self
    }

    pub fn with_package_override(&self, package_cc: Option<&CC>) -> Self {
        let driver = self.driver.with_package_override(package_cc);
        Self {
            driver,
            contract: self.contract.clone(),
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

// The shipped simdutf objects are not wired up for Windows yet: the native
// backend still needs a no-stdlib runtime build there, plus an /MT vs /MD call.
const CAN_USE_SIMDUTF: bool = cfg!(any(target_os = "linux", target_os = "macos"));

pub const WINDOWS_MSVC_DEFAULT_LIBS: &[&str] = &[
    "libcmt.lib",
    "oldnames.lib",
    "kernel32.lib",
    "shell32.lib",
    "user32.lib",
    "dbghelp.lib",
    "uuid.lib",
];
pub const WINDOWS_MSVC_STATIC_RUNTIME_FLAG: &str = "/MT";

static WINDOWS_MSVC_ENVIRONMENT: OnceLock<Option<MsvcEnvironment>> = OnceLock::new();

#[cfg(any(windows, test))]
fn get_env_value<'a>(env: &'a [(OsString, OsString)], name: &str) -> Option<&'a OsStr> {
    env.iter()
        .find(|(key, _)| key.to_string_lossy().eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_os_str())
}

#[cfg(any(windows, test))]
fn msvc_environment_from_env_pairs(
    cl_exe: PathBuf,
    env: &[(OsString, OsString)],
) -> Option<MsvcEnvironment> {
    let include = get_env_value(env, "INCLUDE")?;
    let lib = get_env_value(env, "LIB")?;
    let include_paths = env::split_paths(include)
        .filter(|path| !path.as_os_str().is_empty())
        .collect::<Vec<_>>();
    let lib_paths = env::split_paths(lib)
        .filter(|path| !path.as_os_str().is_empty())
        .collect::<Vec<_>>();
    Some(MsvcEnvironment {
        cl_exe,
        env_pairs: env.to_vec(),
        include_paths,
        lib_paths,
    })
}

#[cfg(any(windows, test))]
fn msvc_environment_from_tool_env_or_current_env(
    cl_exe: PathBuf,
    tool_env: &[(OsString, OsString)],
    current_env: &[(OsString, OsString)],
) -> Option<MsvcEnvironment> {
    msvc_environment_from_env_pairs(cl_exe.clone(), tool_env)
        .or_else(|| msvc_environment_from_env_pairs(cl_exe, current_env))
}

#[cfg(windows)]
fn find_windows_msvc_environment() -> Option<MsvcEnvironment> {
    let tool = find_msvc_tools::find_tool("x86_64-pc-windows-msvc", "cl.exe")?;
    let env = tool.env().into_iter().cloned().collect::<Vec<_>>();
    let current_env = env::vars_os().collect::<Vec<_>>();
    msvc_environment_from_tool_env_or_current_env(tool.path().to_path_buf(), &env, &current_env)
}

#[cfg(not(windows))]
fn find_windows_msvc_environment() -> Option<MsvcEnvironment> {
    None
}

pub fn resolve_windows_msvc_environment() -> anyhow::Result<MsvcEnvironment> {
    if !cfg!(windows) {
        anyhow::bail!("Windows MSVC environment resolution is only supported on Windows");
    }

    WINDOWS_MSVC_ENVIRONMENT
        .get_or_init(find_windows_msvc_environment)
        .clone()
        .with_context(
            || "Windows native backend requires MSVC Build Tools with C++ tools and Windows SDK",
        )
}

fn attach_msvc_environment(toolchain: NativeToolchain) -> anyhow::Result<NativeToolchain> {
    if !toolchain.cc().is_msvc() {
        return Ok(toolchain);
    }

    let environment = resolve_windows_msvc_environment()?;
    let cl_exe = PathBuf::from(&toolchain.cc().cc_path);
    Ok(toolchain.with_msvc_environment(MsvcEnvironment {
        cl_exe,
        env_pairs: environment.env_pairs,
        include_paths: environment.include_paths,
        lib_paths: environment.lib_paths,
    }))
}

fn attach_msvc_environment_if_available(toolchain: NativeToolchain) -> NativeToolchain {
    if !toolchain.cc().is_msvc() {
        return toolchain;
    }
    match resolve_windows_msvc_environment() {
        Ok(environment) => {
            let cl_exe = PathBuf::from(&toolchain.cc().cc_path);
            toolchain.with_msvc_environment(MsvcEnvironment {
                cl_exe,
                env_pairs: environment.env_pairs,
                include_paths: environment.include_paths,
                lib_paths: environment.lib_paths,
            })
        }
        Err(_) => toolchain,
    }
}

pub fn resolve_windows_msvc_toolchain() -> anyhow::Result<NativeToolchain> {
    let environment = resolve_windows_msvc_environment()?;
    let cl_exe = environment.cl_exe.display().to_string();
    let cc = CC::try_from_path(&cl_exe)
        .with_context(|| format!("failed to resolve MSVC compiler at {cl_exe}"))?;
    Ok(NativeToolchain::from_path_probe(cc).with_msvc_environment(environment))
}

fn ensure_windows_msvc_compatible(cc: &CC) -> anyhow::Result<()> {
    if cc.is_msvc() {
        Ok(())
    } else {
        anyhow::bail!(
            "MSVC ABI native builds require a cl-compatible compiler driver such as cl.exe or clang-cl.exe; found {}",
            cc.cc_path
        )
    }
}

pub fn ensure_supported_native_toolchain_contract(
    contract: &NativeBuildContract,
) -> anyhow::Result<()> {
    if contract.uses_msvc_abi() {
        if contract.msvc_crt_policy().is_none() {
            anyhow::bail!("MSVC ABI native builds require a known CRT policy")
        }
        if contract.msvc_environment().is_none() {
            anyhow::bail!("MSVC ABI native builds require a resolved MSVC toolchain environment")
        }
    }
    Ok(())
}

fn validate_native_driver_for_contract(
    contract: &NativeBuildContract,
    driver: &NativeCommandDriver,
) -> anyhow::Result<()> {
    if driver.abi_family() != contract.abi_family() {
        anyhow::bail!(
            "native toolchain ABI mismatch: selected build uses {} ABI, but package override uses {} ABI via {}",
            contract.abi_family(),
            driver.abi_family(),
            driver.cc().cc_path
        );
    }

    if driver.msvc_crt_policy() != contract.msvc_crt_policy() {
        anyhow::bail!(
            "native toolchain CRT mismatch: selected build uses {:?}, but package override uses {:?} via {}",
            contract.msvc_crt_policy(),
            driver.msvc_crt_policy(),
            driver.cc().cc_path
        );
    }

    if contract.uses_msvc_abi() {
        ensure_windows_msvc_compatible(driver.cc())?;
    }

    Ok(())
}

pub fn native_toolchain_with_package_override(
    selected: &NativeToolchain,
    package_cc: Option<&CC>,
) -> anyhow::Result<NativeToolchain> {
    let toolchain = selected.with_package_override(package_cc);
    validate_native_driver_for_contract(toolchain.contract(), toolchain.driver())?;
    ensure_supported_native_toolchain_contract(toolchain.contract())?;
    Ok(toolchain)
}

pub fn windows_msvc_native_toolchain() -> anyhow::Result<NativeToolchain> {
    if let Some(env_cc) = ENV_CC.as_ref().filter(|cc| cc.is_msvc()) {
        return attach_msvc_environment(NativeToolchain::from_env_override(env_cc.clone()));
    }

    resolve_windows_msvc_toolchain()
}

pub fn has_incompatible_windows_msvc_env_override() -> bool {
    ENV_CC.as_ref().is_some_and(|cc| !cc.is_msvc())
}

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

    pub fn targets_apple_darwin(&self) -> bool {
        self.target_triple
            .as_deref()
            .is_some_and(|target| target.contains("apple-darwin"))
    }

    pub fn should_link_libm(&self) -> bool {
        self.is_full_featured_gcc_like() && !self.targets_msvc()
    }

    pub fn can_use_simdutf(&self) -> bool {
        CAN_USE_SIMDUTF && !self.is_tcc()
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
        try_internal_tcc()
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

fn detect_path_candidate(cc_path: &Path, cc_kind: CCKind) -> anyhow::Result<CC> {
    CC::try_from_detected_path(cc_path, cc_kind)
        .with_context(|| format!("failed to use C compiler at {}", cc_path.display()))
}

fn detect_system_cc() -> anyhow::Result<CC> {
    let mut errors = Vec::new();
    for (name, kind) in [
        ("cl", CCKind::Msvc),
        ("cc", CCKind::SystemCC),
        ("gcc", CCKind::Gcc),
        ("clang", CCKind::Clang),
    ] {
        let Ok(cc_path) = which::which(name) else {
            continue;
        };
        match detect_path_candidate(&cc_path, kind) {
            Ok(cc) => return Ok(cc),
            Err(e) => errors.push(format!("{e:#}")),
        }
    }

    if errors.is_empty() {
        anyhow::bail!("no system C compiler found; tried cl, cc, gcc, clang")
    }
    anyhow::bail!(
        "failed to resolve system C compiler candidates: {}",
        errors.join("; ")
    )
}

fn detect_internal_tcc() -> anyhow::Result<CC> {
    let cc_path = which::which(&MOON_DIRS.internal_tcc_path).with_context(|| {
        format!(
            "internal tcc not found at {}",
            MOON_DIRS.internal_tcc_path.display()
        )
    })?;
    detect_path_candidate(&cc_path, CCKind::Tcc)
}

static DETECTED_SYSTEM_CC: std::sync::LazyLock<anyhow::Result<CC>> =
    std::sync::LazyLock::new(detect_system_cc);
static DETECTED_INTERNAL_TCC: std::sync::LazyLock<anyhow::Result<CC>> =
    std::sync::LazyLock::new(detect_internal_tcc);

fn cached_cc(result: &std::sync::LazyLock<anyhow::Result<CC>>) -> anyhow::Result<CC> {
    result
        .as_ref()
        .cloned()
        .map_err(|e| anyhow::anyhow!("{e:#}"))
}

pub fn try_system_cc() -> anyhow::Result<CC> {
    cached_cc(&DETECTED_SYSTEM_CC)
}

pub fn try_internal_tcc() -> anyhow::Result<CC> {
    cached_cc(&DETECTED_INTERNAL_TCC)
}

pub fn has_cc_env_override() -> bool {
    env::var_os(ENV_MOON_CC).is_some()
}

pub fn default_native_toolchain(
    internal_tcc_fallback: Option<&CC>,
) -> anyhow::Result<NativeToolchain> {
    if let Some(env_cc) = ENV_CC.as_ref() {
        return Ok(attach_msvc_environment_if_available(
            NativeToolchain::from_env_override(env_cc.clone()),
        ));
    }

    if cfg!(windows)
        && let Ok(toolchain) = resolve_windows_msvc_toolchain()
    {
        return Ok(toolchain);
    }

    match try_system_cc() {
        Ok(cc) => Ok(attach_msvc_environment_if_available(
            NativeToolchain::from_path_probe(cc),
        )),
        Err(err) => match internal_tcc_fallback {
            Some(internal_tcc) => Ok(NativeToolchain::from_path_probe(internal_tcc.clone())),
            None => Err(err),
        },
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
    // Link libbacktrace.a from the configured MoonBit lib path if it exists.
    pub link_libbacktrace: bool,
    // Define MOONBIT_NATIVE_NO_SYS_HEADER
    // Usually used with TCC
    // TCC may not be able to handle the system header
    pub no_sys_header: bool,
    #[builder(default = OutputType::Object)]
    pub output_ty: OutputType,
    #[builder(default = OptLevel::Speed)]
    pub opt_level: OptLevel,
    #[builder(default = false)]
    // Define MOONBIT_ALLOW_STACKTRACE
    pub allow_stacktrace: bool,
    #[builder(default = false)]
    // Define __TINYC__
    pub define_tinyc_macro: bool,
    #[builder(default = false)]
    // Preserve frame pointers for backtrace walkers.
    pub preserve_frame_pointer: bool,
    // Define MOONBIT_USE_SHARED_RUNTIME
    // It's non-op on Linux and MacOS
    // But on Windows, it will mark runtime function declarations
    // with extra __declspec(dllimport)
    // This is needed to use the shared runtime
    pub define_use_shared_runtime_macro: bool,
    #[builder(default = false)]
    // Define MOONBIT_USE_SIMDUTF.
    pub use_simdutf: bool,
}

#[derive(Clone, Builder)]
#[builder(setter(into))]
pub struct LinkerConfig<P: AsRef<Path>> {
    #[builder(default = false)]
    pub link_moonbitrun: bool,
    #[builder(default = false)]
    // Link libbacktrace.a from the configured MoonBit lib path if it exists.
    pub link_libbacktrace: bool,
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

    pub fn simdutf_object_paths(&self) -> Option<[PathBuf; 2]> {
        let moonbit_simdutf = Path::new(&self.lib_path).join("moonbit_simdutf.o");
        let simdutf = Path::new(&self.lib_path).join("simdutf.o");
        (moonbit_simdutf.exists() && simdutf.exists()).then_some([moonbit_simdutf, simdutf])
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
fn add_archiver_moonbitrun_with_warnings(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &ArchiverConfig,
    paths: &CompilerPaths,
) {
    if cc.is_libmoonbitrun_o_available() && config.archive_moonbitrun {
        if cc.is_tcc() {
            eprintln!(
                "{}: Cannot archive libmoonbitrun.o when using tcc",
                "Warning".yellow().bold(),
            );
        } else {
            buf.push(
                Path::new(&paths.lib_path)
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
    let paths = CompilerPaths::from_moon_dirs();
    make_archiver_command_resolved(resolved_cc, config, src, dest, &paths)
}

pub fn make_archiver_command_resolved<S>(
    cc: CC,
    config: ArchiverConfig,
    src: &[S],
    dest: &str,
    paths: &CompilerPaths,
) -> Vec<String>
where
    S: AsRef<str>,
{
    let mut buf = vec![cc.ar_path.clone()];

    add_archiver_flags(&cc, &mut buf, dest);
    add_archiver_moonbitrun_with_warnings(&cc, &mut buf, &config, paths);
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
        add_tcc_macos_sdk_library_path(buf);
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
fn add_linker_msvc_specific_flags(cc: &CC, buf: &mut Vec<String>) {
    if cc.is_msvc() {
        buf.push("/nologo".to_string());
    }
}

// Linker compiler-specific handling for moonbitrun
fn add_linker_moonbitrun_with_warnings(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &LinkerConfig<impl AsRef<Path>>,
    lpath: &str,
) {
    if config.link_moonbitrun && cc.is_libmoonbitrun_o_available() {
        if cc.is_tcc() {
            eprintln!(
                "{}: Cannot link libmoonbitrun.o when using tcc",
                "Warning".yellow().bold(),
            );
        } else {
            buf.push(
                Path::new(lpath)
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
    make_linker_command_resolved(
        resolved_cc,
        config,
        user_link_flags,
        src,
        dest_dir,
        dest,
        lib_path,
    )
}

pub fn make_linker_command_resolved<S, P>(
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
    add_linker_output_flags(&cc, &mut buf, &config, dest);
    add_linker_library_paths(&cc, &mut buf, &config, lpath);
    add_linker_intermediate_dir_flags(&cc, &mut buf, dest_dir);
    add_linker_shared_lib_flags(&cc, &mut buf, &config);

    // Linker compiler-specific flags
    add_linker_msvc_specific_flags(&cc, &mut buf);

    add_linker_moonbitrun_with_warnings(&cc, &mut buf, &config, lpath);

    buf.extend(src.iter().map(|s| s.as_ref().to_string()));

    add_linker_common_libraries(&cc, &mut buf, &config);
    add_linker_msvc_runtime(&cc, &mut buf, &config, lpath);

    buf.extend(user_link_flags.iter().map(|s| s.as_ref().to_string()));
    if config.link_libbacktrace {
        let libbacktrace_path = Path::new(lpath).join("libbacktrace.a");
        if libbacktrace_path.exists() {
            buf.push(libbacktrace_path.display().to_string());
        }
    }

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

#[cfg(target_os = "macos")]
fn resolve_macos_sdk_lib_path() -> Option<PathBuf> {
    let output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let sdk_root = String::from_utf8_lossy(&output.stdout);
    let sdk_root = sdk_root.lines().next()?.trim();
    if sdk_root.is_empty() {
        return None;
    }

    let sdk_lib_path = Path::new(sdk_root).join("usr").join("lib");
    sdk_lib_path.is_dir().then_some(sdk_lib_path)
}

#[cfg(target_os = "macos")]
static MACOS_SDK_LIB_PATH: std::sync::LazyLock<Option<PathBuf>> =
    std::sync::LazyLock::new(resolve_macos_sdk_lib_path);

#[cfg(target_os = "macos")]
fn add_tcc_macos_sdk_library_path(buf: &mut Vec<String>) {
    if let Some(sdk_lib_path) = MACOS_SDK_LIB_PATH.as_ref() {
        buf.push(format!("-L{}", sdk_lib_path.display()));
    }
}

#[cfg(not(target_os = "macos"))]
fn add_tcc_macos_sdk_library_path(_buf: &mut Vec<String>) {}

fn add_cc_include_and_lib_paths(cc: &CC, buf: &mut Vec<String>, ipath: &str, lpath: &str) {
    if cc.is_msvc() {
        buf.push(format!("/I{ipath}"));
    } else if cc.is_tcc() {
        buf.push(format!("-I{ipath}"));
        add_tcc_macos_sdk_library_path(buf);
        buf.push(format!("-L{lpath}"));
    } else if cc.is_gcc_like() {
        buf.push(format!("-I{ipath}"));
    }
}

fn add_cc_msvc_environment_include_paths(
    toolchain: Option<&NativeToolchain>,
    buf: &mut Vec<String>,
) {
    let Some(environment) = toolchain.and_then(NativeToolchain::msvc_environment) else {
        return;
    };
    buf.extend(
        environment
            .include_paths
            .iter()
            .map(|path| format!("/I{}", path.display())),
    );
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
    }
    buf.push("/nologo".to_string());
}

fn add_cc_msvc_runtime_flags(cc: &CC, toolchain: Option<&NativeToolchain>, buf: &mut Vec<String>) {
    if cc.is_msvc()
        && let Some(crt) = toolchain.and_then(NativeToolchain::msvc_crt_policy)
    {
        buf.push(crt.compiler_flag().to_string());
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

fn add_cc_build_system_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if cc.is_msvc() {
        if config.allow_stacktrace {
            buf.push("/DMOONBIT_ALLOW_STACKTRACE".to_string());
        }
        if config.define_tinyc_macro {
            buf.push("/D__TINYC__".to_string());
        }
    } else if cc.is_gcc_like() {
        if config.allow_stacktrace {
            buf.push("-DMOONBIT_ALLOW_STACKTRACE".to_string());
        }
        if config.define_tinyc_macro {
            buf.push("-D__TINYC__".to_string());
        }
    }

    if config.preserve_frame_pointer && cc.is_full_featured_gcc_like() {
        buf.push("-fno-omit-frame-pointer".to_string());
    }
}

fn add_cc_shared_runtime_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    // always set this even if user cc flags are set
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

fn add_cc_simdutf_flags(cc: &CC, buf: &mut Vec<String>, config: &CCConfig) {
    if !config.use_simdutf {
        return;
    }

    if cc.is_msvc() {
        buf.push("/DMOONBIT_USE_SIMDUTF".to_string());
    } else if cc.is_gcc_like() {
        buf.push("-DMOONBIT_USE_SIMDUTF".to_string());
    }
}

// CC compiler-specific handling for moonbitrun
fn add_cc_moonbitrun_with_warnings(
    cc: &CC,
    buf: &mut Vec<String>,
    config: &CCConfig,
    paths: &CompilerPaths,
) {
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
                Path::new(&paths.lib_path)
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
    cc_flags: &[S],
    src: impl IntoIterator<Item = impl Into<String>>,
    intermediate_dir: &str,
    dest: &str,
) -> Vec<String>
where
    S: AsRef<str>,
{
    let resolved_cc = resolve_cc(&cc, user_cc.as_ref());
    let paths = CompilerPaths::from_moon_dirs();
    make_cc_command_resolved(
        resolved_cc,
        config,
        cc_flags,
        src,
        intermediate_dir,
        Some(dest),
        &paths,
    )
}

/// Build a C compiler command after the caller has already selected the exact
/// compiler and MoonBit include/lib paths.
///
/// Use this when there are no extra link-only flags. It still may produce a
/// link command when `config.output_ty` is `Executable` or `SharedLib`; the
/// name only means there is no separate `user_link_flags` input.
pub fn make_cc_command_resolved<S>(
    cc: CC,
    config: CCConfig,
    cc_flags: &[S],
    src: impl IntoIterator<Item = impl Into<String>>,
    intermediate_dir: &str,
    dest: Option<&str>,
    paths: &CompilerPaths,
) -> Vec<String>
where
    S: AsRef<str>,
{
    make_cc_command_resolved_with_link_flags(
        cc,
        config,
        cc_flags,
        &[] as &[&str],
        src,
        intermediate_dir,
        dest,
        paths,
    )
}

/// Build a C compiler-driver command with separate compile and link flag inputs.
///
/// `resolved` means this function does not consult `MOON_CC`, package CC
/// overrides, or global `MOON_HOME` paths. Callers pass the effective `CC` and
/// `CompilerPaths` decided by build planning/lowering.
///
/// `cc_flags` are compile-driver flags. Non-empty C flags are treated as user
/// overrides and suppress default optimization flags. `user_link_flags` are
/// appended only for the link step and do not suppress compiler defaults.
#[allow(clippy::too_many_arguments)]
pub fn make_cc_command_resolved_with_link_flags<S, L>(
    cc: CC,
    config: CCConfig,
    cc_flags: &[S],
    user_link_flags: &[L],
    src: impl IntoIterator<Item = impl Into<String>>,
    intermediate_dir: &str,
    dest: Option<&str>,
    paths: &CompilerPaths,
) -> Vec<String>
where
    S: AsRef<str>,
    L: AsRef<str>,
{
    make_cc_command_resolved_with_link_flags_and_toolchain(
        None,
        cc,
        config,
        cc_flags,
        user_link_flags,
        src,
        intermediate_dir,
        dest,
        paths,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn make_cc_command_resolved_for_toolchain<S, L>(
    toolchain: &NativeToolchain,
    config: CCConfig,
    cc_flags: &[S],
    user_link_flags: &[L],
    src: impl IntoIterator<Item = impl Into<String>>,
    intermediate_dir: &str,
    dest: Option<&str>,
    paths: &CompilerPaths,
) -> Vec<String>
where
    S: AsRef<str>,
    L: AsRef<str>,
{
    make_cc_command_resolved_with_link_flags_and_toolchain(
        Some(toolchain),
        toolchain.cc().clone(),
        config,
        cc_flags,
        user_link_flags,
        src,
        intermediate_dir,
        dest,
        paths,
    )
}

#[allow(clippy::too_many_arguments)]
fn make_cc_command_resolved_with_link_flags_and_toolchain<S, L>(
    toolchain: Option<&NativeToolchain>,
    cc: CC,
    config: CCConfig,
    cc_flags: &[S],
    user_link_flags: &[L],
    src: impl IntoIterator<Item = impl Into<String>>,
    intermediate_dir: &str,
    dest: Option<&str>,
    paths: &CompilerPaths,
) -> Vec<String>
where
    S: AsRef<str>,
    L: AsRef<str>,
{
    let mut buf = vec![cc.cc_path.clone()];

    // If user C flags are set, we only set necessary flags
    // that are tightly coupled with the paths and output types
    // as user cannot easily specify them in the configuration file
    // Link-only flags should not affect compiler defaults.
    let has_user_flags = !cc_flags.is_empty();

    add_cc_output_flags(&cc, &mut buf, &config, dest);
    add_cc_include_and_lib_paths(&cc, &mut buf, &paths.include_path, &paths.lib_path);
    add_cc_msvc_environment_include_paths(toolchain, &mut buf);
    add_cc_intermediate_dir_flags(&cc, &mut buf, &config, intermediate_dir);
    add_cc_debug_flags(&cc, &mut buf, &config);
    add_cc_shared_lib_flags(&cc, &mut buf, &config);
    add_cc_compile_only_flags(&cc, &mut buf, &config);

    // Compiler-specific flags
    add_cc_msvc_specific_flags(&cc, &mut buf, has_user_flags);
    add_cc_gcc_like_specific_flags(&cc, &mut buf);
    add_cc_tcc_specific_flags(&cc, &mut buf, &config);

    add_cc_optimization_flags(&cc, &mut buf, &config, has_user_flags);
    add_cc_build_system_flags(&cc, &mut buf, &config);
    add_cc_shared_runtime_flags(&cc, &mut buf, &config);
    add_cc_simdutf_flags(&cc, &mut buf, &config);
    add_cc_moonbitrun_with_warnings(&cc, &mut buf, &config, paths);

    buf.extend(src.into_iter().map(|s| s.into()));

    add_cc_common_libraries(&cc, &mut buf, &config);
    buf.extend(cc_flags.iter().map(|s| s.as_ref().to_string()));
    add_cc_msvc_runtime_flags(&cc, toolchain, &mut buf);
    buf.extend(user_link_flags.iter().map(|s| s.as_ref().to_string()));
    if config.link_libbacktrace && config.output_ty != OutputType::Object {
        let libbacktrace_path = Path::new(&paths.lib_path).join("libbacktrace.a");
        if libbacktrace_path.exists() {
            buf.push(libbacktrace_path.display().to_string());
        }
    }
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
            link_libbacktrace: false,
            no_sys_header: false,
            output_ty: OutputType::Executable,
            opt_level: OptLevel::Speed,
            allow_stacktrace: false,
            define_tinyc_macro: false,
            preserve_frame_pointer: false,
            define_use_shared_runtime_macro: false,
            use_simdutf: false,
        }
    }

    #[test]
    fn detects_apple_darwin_target_triple() {
        assert!(fake_cc(CCKind::Clang, Some("arm64-apple-darwin25.5.0")).targets_apple_darwin());
        assert!(fake_cc(CCKind::Gcc, Some("aarch64-apple-darwin")).targets_apple_darwin());
        assert!(!fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu")).targets_apple_darwin());
        assert!(!fake_cc(CCKind::Gcc, None).targets_apple_darwin());
    }

    #[test]
    fn detects_msvc_target_triple() {
        assert!(fake_cc(CCKind::Clang, Some("x86_64-pc-windows-msvc")).targets_msvc());
        assert!(fake_cc(CCKind::Gcc, Some("aarch64-pc-windows-msvc")).targets_msvc());
        assert!(!fake_cc(CCKind::Gcc, Some("x86_64-w64-windows-gnu")).targets_msvc());
        assert!(!fake_cc(CCKind::Gcc, None).targets_msvc());
    }

    #[test]
    fn classifies_native_abi_family_from_target_triple() {
        assert_eq!(
            NativeToolchain::from_path_probe(fake_cc(CCKind::Msvc, None)).abi_family(),
            NativeAbiFamily::Msvc
        );
        assert_eq!(
            NativeToolchain::from_path_probe(fake_cc(
                CCKind::Clang,
                Some("x86_64-pc-windows-msvc")
            ))
            .abi_family(),
            NativeAbiFamily::Msvc
        );
        assert_eq!(
            NativeToolchain::from_path_probe(fake_cc(CCKind::Gcc, Some("x86_64-w64-windows-gnu")))
                .abi_family(),
            NativeAbiFamily::WindowsGnu
        );
        assert_eq!(
            NativeToolchain::from_path_probe(fake_cc(
                CCKind::Clang,
                Some("arm64-apple-darwin25.5.0")
            ))
            .abi_family(),
            NativeAbiFamily::AppleDarwin
        );
        assert_eq!(
            NativeToolchain::from_path_probe(fake_cc(
                CCKind::Gcc,
                Some("x86_64-unknown-linux-gnu")
            ))
            .abi_family(),
            NativeAbiFamily::UnixLike
        );
    }

    #[test]
    fn windows_msvc_compatibility_rejects_gnu_toolchains() {
        assert!(ensure_windows_msvc_compatible(&fake_cc(CCKind::Msvc, None)).is_ok());
        assert!(
            ensure_windows_msvc_compatible(&fake_cc(CCKind::Clang, Some("x86_64-pc-windows-msvc")))
                .is_err()
        );
        assert!(
            ensure_windows_msvc_compatible(&fake_cc(CCKind::Gcc, Some("x86_64-w64-windows-gnu")))
                .is_err()
        );
    }

    #[test]
    fn toolchain_detects_msvc_abi_and_crt_policy() {
        let mut cc = fake_cc(CCKind::Msvc, None);
        cc.cc_path = "cl.exe".to_string();
        cc.ar_kind = ARKind::MsvcLib;
        cc.ar_path = "lib.exe".to_string();

        let toolchain = NativeToolchain::from_path_probe(cc);

        assert_eq!(toolchain.abi_family(), NativeAbiFamily::Msvc);
        assert!(toolchain.uses_msvc_abi());
        assert!(toolchain.uses_msvc_driver());
        assert_eq!(toolchain.msvc_crt_policy(), Some(MsvcCrtPolicy::StaticMt));
        assert_eq!(
            toolchain
                .msvc_crt_policy()
                .expect("MSVC toolchain has CRT policy")
                .compiler_flag(),
            WINDOWS_MSVC_STATIC_RUNTIME_FLAG
        );
    }

    #[test]
    fn toolchain_tracks_msvc_abi_without_cl_driver_crt_policy() {
        let toolchain = NativeToolchain::from_path_probe(fake_cc(
            CCKind::Clang,
            Some("x86_64-pc-windows-msvc"),
        ));

        assert_eq!(toolchain.abi_family(), NativeAbiFamily::Msvc);
        assert!(toolchain.uses_msvc_abi());
        assert!(!toolchain.uses_msvc_driver());
        assert!(toolchain.uses_msvc_link_library_names());
        assert_eq!(toolchain.msvc_crt_policy(), None);
    }

    #[test]
    fn msvc_contract_requires_resolved_toolchain_environment() {
        let mut cc = fake_cc(CCKind::Msvc, None);
        cc.cc_path = "cl.exe".to_string();
        cc.ar_kind = ARKind::MsvcLib;
        cc.ar_path = "lib.exe".to_string();
        let toolchain = NativeToolchain::from_path_probe(cc);

        let err = ensure_supported_native_toolchain_contract(toolchain.contract())
            .expect_err("MSVC ABI contract without environment should be rejected");

        assert!(err.to_string().contains("MSVC toolchain environment"));
    }

    #[test]
    fn windows_msvc_package_override_preserves_env_override_precedence() {
        let mut env_cc = fake_cc(CCKind::Msvc, None);
        env_cc.cc_path = "env-cl.exe".to_string();
        env_cc.ar_kind = ARKind::MsvcLib;
        env_cc.ar_path = "env-lib.exe".to_string();
        env_cc.is_env_override = true;
        let resolved =
            NativeToolchain::from_env_override(env_cc).with_msvc_environment(MsvcEnvironment {
                cl_exe: PathBuf::from("env-cl.exe"),
                env_pairs: vec![(OsString::from("PATH"), OsString::from("env/bin"))],
                include_paths: vec![PathBuf::from("env/include")],
                lib_paths: vec![PathBuf::from("env/lib")],
            });
        let mut package_cc = fake_cc(CCKind::Clang, Some("x86_64-pc-windows-msvc"));
        package_cc.cc_path = "clang.exe".to_string();

        let toolchain = native_toolchain_with_package_override(&resolved, Some(&package_cc))
            .expect("MOON_CC-style source should ignore package cc");

        assert_eq!(toolchain.cc().cc_path, "env-cl.exe");
        assert_eq!(
            toolchain
                .msvc_environment()
                .expect("env toolchain keeps MSVC environment")
                .lib_paths,
            vec![PathBuf::from("env/lib")]
        );
        assert_eq!(
            toolchain
                .msvc_environment()
                .expect("env toolchain keeps MSVC environment")
                .env_pairs,
            vec![(OsString::from("PATH"), OsString::from("env/bin"))]
        );
    }

    #[test]
    fn native_toolchain_contract_rejects_mixed_msvc_and_windows_gnu_abi() {
        let mut selected_cc = fake_cc(CCKind::Msvc, None);
        selected_cc.cc_path = "cl.exe".to_string();
        selected_cc.ar_kind = ARKind::MsvcLib;
        selected_cc.ar_path = "lib.exe".to_string();
        let selected =
            NativeToolchain::from_path_probe(selected_cc).with_msvc_environment(MsvcEnvironment {
                cl_exe: PathBuf::from("cl.exe"),
                env_pairs: vec![(OsString::from("PATH"), OsString::from("msvc/bin"))],
                include_paths: vec![PathBuf::from("msvc/include")],
                lib_paths: vec![PathBuf::from("msvc/lib")],
            });
        let mut package_cc = fake_cc(CCKind::Gcc, Some("x86_64-w64-windows-gnu"));
        package_cc.cc_path = "x86_64-w64-mingw32-gcc".to_string();

        let err = native_toolchain_with_package_override(&selected, Some(&package_cc))
            .expect_err("Windows-GNU package override must not enter an MSVC link contract");

        assert!(err.to_string().contains("ABI mismatch"));
    }

    #[test]
    fn native_toolchain_contract_rejects_plain_clang_msvc_driver_for_now() {
        let mut selected_cc = fake_cc(CCKind::Msvc, None);
        selected_cc.cc_path = "cl.exe".to_string();
        selected_cc.ar_kind = ARKind::MsvcLib;
        selected_cc.ar_path = "lib.exe".to_string();
        let selected = NativeToolchain::from_path_probe(selected_cc);
        let mut package_cc = fake_cc(CCKind::Clang, Some("x86_64-pc-windows-msvc"));
        package_cc.cc_path = "clang.exe".to_string();

        let err = native_toolchain_with_package_override(&selected, Some(&package_cc))
            .expect_err("plain clang MSVC target has a different CRT driver contract today");

        assert!(err.to_string().contains("CRT mismatch"));
    }

    #[test]
    fn native_toolchain_contract_reuses_msvc_environment_for_clang_cl_override() {
        let mut selected_cc = fake_cc(CCKind::Msvc, None);
        selected_cc.cc_path = "cl.exe".to_string();
        selected_cc.ar_kind = ARKind::MsvcLib;
        selected_cc.ar_path = "lib.exe".to_string();
        let selected =
            NativeToolchain::from_path_probe(selected_cc).with_msvc_environment(MsvcEnvironment {
                cl_exe: PathBuf::from("cl.exe"),
                env_pairs: vec![(OsString::from("PATH"), OsString::from("msvc/bin"))],
                include_paths: vec![PathBuf::from("msvc/include")],
                lib_paths: vec![PathBuf::from("msvc/lib")],
            });
        let mut package_cc = fake_cc(CCKind::Msvc, None);
        package_cc.cc_path = "clang-cl.exe".to_string();
        package_cc.ar_kind = ARKind::MsvcLib;
        package_cc.ar_path = "lib.exe".to_string();

        let toolchain = native_toolchain_with_package_override(&selected, Some(&package_cc))
            .expect("clang-cl should be compatible with the selected MSVC contract");

        assert_eq!(toolchain.cc().cc_path, "clang-cl.exe");
        assert_eq!(
            toolchain
                .msvc_environment()
                .expect("compatible package override reuses MSVC env")
                .env_pairs,
            vec![(OsString::from("PATH"), OsString::from("msvc/bin"))]
        );
        assert_eq!(
            toolchain
                .msvc_environment()
                .expect("compatible package override reuses MSVC env")
                .cl_exe,
            PathBuf::from("cl.exe")
        );
    }

    #[test]
    fn msvc_toolchain_adds_mt_to_cc_command() {
        let mut cc = fake_cc(CCKind::Msvc, None);
        cc.cc_path = "cl.exe".to_string();
        cc.ar_kind = ARKind::MsvcLib;
        cc.ar_path = "lib.exe".to_string();
        let toolchain = NativeToolchain::from_path_probe(cc);
        let paths = CompilerPaths {
            include_path: "moon/include".to_string(),
            lib_path: "moon/lib".to_string(),
        };

        let command = make_cc_command_resolved_for_toolchain(
            &toolchain,
            CCConfigBuilder::default()
                .no_sys_header(true)
                .link_moonbitrun(false)
                .output_ty(OutputType::Object)
                .define_use_shared_runtime_macro(false)
                .build()
                .expect("MSVC command config should build"),
            &["/MD"],
            &[] as &[&str],
            ["stub.c".to_string()],
            "pkg",
            Some("stub.obj"),
            &paths,
        );

        let md_position = command
            .iter()
            .position(|arg| arg == "/MD")
            .expect("test command should include user runtime flag");
        let mt_position = command
            .iter()
            .position(|arg| arg == WINDOWS_MSVC_STATIC_RUNTIME_FLAG)
            .expect("test command should force static runtime flag");
        assert!(mt_position > md_position);
    }

    #[test]
    fn msvc_environment_uses_find_msvc_tools_env_pairs_case_insensitively() {
        let include_paths = vec![PathBuf::from("crt/include"), PathBuf::from("sdk/include")];
        let lib_paths = vec![PathBuf::from("crt/lib"), PathBuf::from("sdk/lib")];
        let env = vec![
            (OsString::from("Path"), OsString::from("C:\\Windows")),
            (
                OsString::from("include"),
                env::join_paths(&include_paths).unwrap(),
            ),
            (OsString::from("LIB"), env::join_paths(&lib_paths).unwrap()),
        ];
        let environment = msvc_environment_from_env_pairs(PathBuf::from("cl.exe"), &env);

        let environment = environment.expect("find-msvc-tools env pairs should parse");
        assert_eq!(environment.cl_exe, PathBuf::from("cl.exe"));
        assert_eq!(environment.include_paths, include_paths);
        assert_eq!(environment.lib_paths, lib_paths);
    }

    #[test]
    fn msvc_environment_ignores_empty_include_and_lib_paths() {
        #[cfg(windows)]
        const PATH_LIST_SEPARATOR: &str = ";";
        #[cfg(not(windows))]
        const PATH_LIST_SEPARATOR: &str = ":";

        let include_paths = vec![PathBuf::from("crt/include"), PathBuf::from("sdk/include")];
        let lib_paths = vec![PathBuf::from("crt/lib"), PathBuf::from("sdk/lib")];
        let mut include = env::join_paths(&include_paths).unwrap();
        include.push(PATH_LIST_SEPARATOR);
        let mut lib = env::join_paths(&lib_paths).unwrap();
        lib.push(PATH_LIST_SEPARATOR);
        let env = vec![
            (OsString::from("INCLUDE"), include),
            (OsString::from("LIB"), lib),
        ];

        let environment = msvc_environment_from_env_pairs(PathBuf::from("cl.exe"), &env)
            .expect("MSVC environment with trailing separators should parse");

        assert_eq!(environment.include_paths, include_paths);
        assert_eq!(environment.lib_paths, lib_paths);
    }

    #[test]
    fn msvc_environment_falls_back_to_current_env_when_tool_env_is_empty() {
        let include_paths = vec![PathBuf::from("current/include")];
        let lib_paths = vec![PathBuf::from("current/lib")];
        let tool_env = vec![];
        let current_env = vec![
            (
                OsString::from("INCLUDE"),
                env::join_paths(&include_paths).unwrap(),
            ),
            (OsString::from("LIB"), env::join_paths(&lib_paths).unwrap()),
        ];

        let environment = msvc_environment_from_tool_env_or_current_env(
            PathBuf::from("cl.exe"),
            &tool_env,
            &current_env,
        )
        .expect("current MSVC environment should parse when find-msvc-tools env is empty");

        assert_eq!(environment.cl_exe, PathBuf::from("cl.exe"));
        assert_eq!(environment.include_paths, include_paths);
        assert_eq!(environment.lib_paths, lib_paths);
    }

    #[test]
    fn tcc_compile_driver_keeps_configured_toolchain_lib_path() {
        let paths = CompilerPaths {
            include_path: "include".to_string(),
            lib_path: "custom-lib".to_string(),
        };
        let mut config = executable_cc_config();
        config.no_sys_header = true;

        let command = make_cc_command_resolved_with_link_flags(
            fake_cc(CCKind::Tcc, None),
            config,
            &[] as &[&str],
            &[] as &[&str],
            ["main.c"],
            "build/main",
            Some("build/main/main"),
            &paths,
        );

        assert!(command.iter().any(|flag| flag == "-Lcustom-lib"));
    }

    #[test]
    fn tcc_linker_keeps_configured_toolchain_lib_path() {
        let command = make_linker_command_resolved(
            fake_cc(CCKind::Tcc, None),
            LinkerConfig::<&Path> {
                link_moonbitrun: false,
                link_libbacktrace: false,
                output_ty: OutputType::Executable,
                link_shared_runtime: None,
            },
            &[] as &[&str],
            &["main.o"],
            "build/main",
            "build/main/main",
            "custom-lib",
        );

        assert!(command.iter().any(|flag| flag == "-Lcustom-lib"));
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn resolved_archiver_uses_configured_lib_path_for_available_moonbitrun() {
        let paths = CompilerPaths {
            include_path: "include".to_string(),
            lib_path: "custom-lib".to_string(),
        };

        let command = make_archiver_command_resolved(
            fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu")),
            ArchiverConfig {
                archive_moonbitrun: true,
            },
            &["stub.o"],
            "libstub.a",
            &paths,
        );

        let libmoonbitrun_arg = Path::new("custom-lib")
            .join("libmoonbitrun.o")
            .display()
            .to_string();
        assert!(command.iter().any(|arg| arg == &libmoonbitrun_arg));
    }

    #[test]
    fn clang_msvc_target_does_not_link_libm() {
        let cc = fake_cc(CCKind::Clang, Some("x86_64-pc-windows-msvc"));

        let mut cc_flags = vec![];
        add_cc_common_libraries(&cc, &mut cc_flags, &executable_cc_config());
        assert!(!cc_flags.iter().any(|f| f == "-lm"));

        let linker_config = LinkerConfig::<&Path> {
            link_moonbitrun: false,
            link_libbacktrace: false,
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
            link_libbacktrace: false,
            output_ty: OutputType::Executable,
            link_shared_runtime: None,
        };
        let mut linker_flags = vec![];
        add_linker_common_libraries(&cc, &mut linker_flags, &linker_config);
        assert!(linker_flags.iter().any(|f| f == "-lm"));
    }

    #[test]
    fn simdutf_requires_supported_host_and_non_tcc_compiler() {
        assert_eq!(
            fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu")).can_use_simdutf(),
            CAN_USE_SIMDUTF
        );
        assert!(!fake_cc(CCKind::Tcc, Some("x86_64-unknown-linux-gnu")).can_use_simdutf());
    }

    #[test]
    fn link_flags_do_not_disable_default_optimization_flags() {
        let paths = CompilerPaths {
            include_path: "include".to_string(),
            lib_path: "lib".to_string(),
        };

        let command = make_cc_command_resolved_with_link_flags(
            fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu")),
            executable_cc_config(),
            &[] as &[&str],
            &["-lcustom"],
            ["main.c"],
            "build/main",
            Some("build/main/main"),
            &paths,
        );

        assert!(command.iter().any(|flag| flag == "-O2"));
        assert!(command.iter().any(|flag| flag == "-lcustom"));
    }

    #[test]
    fn msvc_compile_flags_keep_nologo() {
        let paths = CompilerPaths {
            include_path: "include".to_string(),
            lib_path: "lib".to_string(),
        };

        let command = make_cc_command_resolved_with_link_flags(
            fake_cc(CCKind::Msvc, None),
            executable_cc_config(),
            &["/O2"],
            &[] as &[&str],
            ["main.c"],
            "build/main",
            Some("build/main/main.exe"),
            &paths,
        );

        assert!(command.iter().any(|flag| flag == "/nologo"));
        assert!(command.iter().any(|flag| flag == "/O2"));
        assert!(!command.iter().any(|flag| flag == "/utf-8"));
        assert!(!command.iter().any(|flag| flag == "/wd4819"));
    }

    #[test]
    fn msvc_link_flags_keep_nologo() {
        let command = make_linker_command_resolved(
            fake_cc(CCKind::Msvc, None),
            LinkerConfig::<&Path> {
                link_moonbitrun: false,
                link_libbacktrace: false,
                output_ty: OutputType::Executable,
                link_shared_runtime: None,
            },
            &["/DEBUG"],
            &["main.obj"],
            "build",
            "build/main.exe",
            "lib",
        );

        assert!(command.iter().any(|flag| flag == "/nologo"));
        assert!(command.iter().any(|flag| flag == "/DEBUG"));
    }

    #[test]
    fn build_system_flags_keep_default_optimization_flags() {
        let paths = CompilerPaths {
            include_path: "include".to_string(),
            lib_path: "lib".to_string(),
        };
        let mut config = executable_cc_config();
        config.allow_stacktrace = true;

        let command = make_cc_command_resolved_with_link_flags(
            fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu")),
            config,
            &[] as &[&str],
            &[] as &[&str],
            ["runtime.c"],
            "build",
            Some("build/runtime.o"),
            &paths,
        );

        assert!(command.iter().any(|flag| flag == "-O2"));
        assert!(
            command
                .iter()
                .any(|flag| flag == "-DMOONBIT_ALLOW_STACKTRACE")
        );
    }

    #[test]
    fn configured_libbacktrace_does_not_disable_default_optimization_flags() {
        let temp_dir = std::env::temp_dir().join(format!(
            "moonutil-libbacktrace-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after Unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp lib dir");
        let libbacktrace_path = temp_dir.join("libbacktrace.a");
        std::fs::write(&libbacktrace_path, b"").expect("create temp libbacktrace.a");

        let paths = CompilerPaths {
            include_path: "include".to_string(),
            lib_path: temp_dir.display().to_string(),
        };
        let mut config = executable_cc_config();
        config.link_libbacktrace = true;

        let command = make_cc_command_resolved_with_link_flags(
            fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu")),
            config,
            &[] as &[&str],
            &["-lcustom"],
            ["main.c"],
            "build/main",
            Some("build/main/main"),
            &paths,
        );

        assert!(command.iter().any(|flag| flag == "-O2"));
        let user_link_flag = command
            .iter()
            .position(|flag| flag == "-lcustom")
            .expect("user link flag should be present");
        let libbacktrace_arg = libbacktrace_path.display().to_string();
        let libbacktrace_flag = command
            .iter()
            .position(|flag| flag == &libbacktrace_arg)
            .expect("libbacktrace should be present");
        assert!(user_link_flag < libbacktrace_flag);

        std::fs::remove_dir_all(temp_dir).expect("remove temp lib dir");
    }

    #[test]
    fn configured_simdutf_keeps_default_optimization_flags() {
        let paths = CompilerPaths {
            include_path: "include".to_string(),
            lib_path: "lib".to_string(),
        };
        let mut config = executable_cc_config();
        config.use_simdutf = true;

        let command = make_cc_command_resolved_with_link_flags(
            fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu")),
            config,
            &[] as &[&str],
            &[] as &[&str],
            ["runtime.c"],
            "build",
            Some("build/runtime.o"),
            &paths,
        );

        assert!(command.iter().any(|flag| flag == "-O2"));
        assert!(command.iter().any(|flag| flag == "-DMOONBIT_USE_SIMDUTF"));
    }

    #[test]
    fn simdutf_objects_require_both_toolchain_objects() {
        let temp_dir = std::env::temp_dir().join(format!(
            "moonutil-simdutf-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after Unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp lib dir");
        let paths = CompilerPaths {
            include_path: "include".to_string(),
            lib_path: temp_dir.display().to_string(),
        };

        std::fs::write(temp_dir.join("moonbit_simdutf.o"), b"").expect("create adapter object");
        assert!(paths.simdutf_object_paths().is_none());

        let simdutf_path = temp_dir.join("simdutf.o");
        std::fs::write(&simdutf_path, b"").expect("create simdutf object");
        assert_eq!(
            paths.simdutf_object_paths(),
            Some([temp_dir.join("moonbit_simdutf.o"), simdutf_path])
        );

        std::fs::remove_dir_all(temp_dir).expect("remove temp lib dir");
    }

    #[test]
    fn compile_flags_still_disable_default_optimization_flags() {
        let paths = CompilerPaths {
            include_path: "include".to_string(),
            lib_path: "lib".to_string(),
        };

        let command = make_cc_command_resolved_with_link_flags(
            fake_cc(CCKind::Gcc, Some("x86_64-unknown-linux-gnu")),
            executable_cc_config(),
            &["-O3"],
            &[] as &[&str],
            ["main.c"],
            "build/main",
            Some("build/main/main"),
            &paths,
        );

        assert!(!command.iter().any(|flag| flag == "-O2"));
        assert!(command.iter().any(|flag| flag == "-O3"));
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
