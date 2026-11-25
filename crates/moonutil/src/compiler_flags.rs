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

use crate::moon_dir::MOON_DIRS;
use anyhow::Context;
use std::{
    env,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
    sync::{LazyLock, OnceLock},
};

mod gcc_like;
mod msvc;
mod tcc;

#[cfg(all(test, target_os = "macos"))]
use gcc_like::resolve_apple_libtool_path;
use msvc::default_librarian;
#[cfg(all(test, windows))]
use msvc::windows_msvc_host_target_triple;
pub use msvc::{
    WINDOWS_MSVC_C_STANDARD_FLAG, WINDOWS_MSVC_DEFAULT_LIBS, WINDOWS_MSVC_STATIC_RUNTIME_FLAG,
    resolve_windows_msvc_toolchain,
};
#[cfg(windows)]
use msvc::{discovered_windows_msvc_toolchain, resolve_requested_msvc_toolchain};

const ENV_MOON_CC: &str = "MOON_CC";
const ENV_MOON_CC_WRAPPER: &str = "MOON_CC_WRAPPER";
const ENV_MOON_AR: &str = "MOON_AR";
const ENV_MOONBIT_ALLOCATOR: &str = "MOONBIT_ALLOCATOR";
#[derive(Copy, Clone, Debug)]
pub enum CCKind {
    Msvc,     // cl.exe
    ClangCl,  // clang-cl.exe: Clang semantics with a cl-compatible CLI
    SystemCC, // cc
    Gcc,      // gcc
    Clang,    // clang
    Tcc,      // tcc
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ARKind {
    MsvcLib, // lib.exe or llvm-lib
    AppleLibtool,
    GnuAr,  // ar
    LlvmAr, // llvm-ar
    TccAr,  // tcc -ar
}

#[derive(Clone, Debug)]
// TODO: Make CC resolved compiler facts only once native toolchain resolution
// owns probing and archiver selection; constructors still perform I/O here.
pub struct CC {
    pub cc_kind: CCKind,
    pub cc_path: String,
    pub ar_kind: ARKind,
    pub ar_path: String,
    pub target_triple: Option<String>,
    pub is_env_override: bool, // Whether the cc is set by env MOON_CC
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
    msvc_environment: Option<MsvcEnvironment>,
    /// A resolved launcher for C compilation only. Compiler identity, probes,
    /// linking, and archiving continue to use the underlying tools.
    cc_wrapper: Option<PathBuf>,
}

/// Environment variables required by a discovered MSVC toolchain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsvcEnvironment {
    pub command_env: Vec<(String, String)>,
}

impl MsvcEnvironment {
    pub fn command_env(&self) -> &[(String, String)] {
        &self.command_env
    }
}

/// MSVC CRT policy shared by runtime, C stubs, and final linking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsvcCrtPolicy {
    StaticMt,
}

/// ABI family carried by a native compiler/linker selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeAbiFamily {
    Msvc,
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativeAllocator {
    #[default]
    Default,
    Mimalloc,
    System,
}

impl NativeAllocator {
    pub fn from_env() -> anyhow::Result<Self> {
        match env::var(ENV_MOONBIT_ALLOCATOR) {
            Ok(value) => Self::parse(&value),
            Err(env::VarError::NotPresent) => Ok(Self::Default),
            Err(env::VarError::NotUnicode(value)) => anyhow::bail!(
                "{ENV_MOONBIT_ALLOCATOR} must be valid Unicode, got `{}`",
                value.to_string_lossy()
            ),
        }
    }

    fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "mimalloc" => Ok(Self::Mimalloc),
            "system" => Ok(Self::System),
            _ => anyhow::bail!(
                "invalid {ENV_MOONBIT_ALLOCATOR} value `{value}`; expected `mimalloc` or `system`"
            ),
        }
    }

    /// The `MOONBIT_ALLOCATOR` macro value for every translation unit that
    /// includes `moonbit.h`: the shipped runtime sources and the C emitted by
    /// `moonc link-core`. Both must agree, otherwise the program inlines a
    /// different allocator than the runtime it links against.
    pub fn moonbit_allocator_macro(self, cc: &CC, link_moonbitrun: bool) -> &'static str {
        match self {
            Self::Default if link_moonbitrun && self.should_link_moonbitrun(cc) => {
                "MOONBIT_ALLOCATOR_MIMALLOC"
            }
            Self::Default => "MOONBIT_ALLOCATOR_SYSTEM",
            Self::Mimalloc => "MOONBIT_ALLOCATOR_MIMALLOC",
            Self::System => "MOONBIT_ALLOCATOR_SYSTEM",
        }
    }

    pub fn validate_for_cc(self, cc: &CC) -> anyhow::Result<()> {
        if self == Self::Mimalloc && !cc.can_link_libmoonbitrun_o() {
            anyhow::bail!(
                "{ENV_MOONBIT_ALLOCATOR}=mimalloc is unavailable with native compiler `{}` on this platform",
                cc.cc_path
            );
        }
        Ok(())
    }

    fn should_link_moonbitrun(self, cc: &CC) -> bool {
        match self {
            Self::Default => cc.is_libmoonbitrun_o_available(),
            Self::Mimalloc => cc.can_link_libmoonbitrun_o(),
            Self::System => false,
        }
    }
}

impl Toolchain {
    pub fn from_env_override(cc: CC) -> Self {
        Self {
            cc,
            source: ToolchainSource::EnvOverride,
            msvc_environment: None,
            cc_wrapper: None,
        }
    }

    pub fn from_path_probe(cc: CC) -> Self {
        Self {
            cc,
            source: ToolchainSource::PathProbe,
            msvc_environment: None,
            cc_wrapper: None,
        }
    }

    pub fn from_package_override(cc: CC) -> Self {
        Self {
            cc,
            source: ToolchainSource::PackageOverride,
            msvc_environment: None,
            cc_wrapper: None,
        }
    }

    pub fn cc(&self) -> &CC {
        &self.cc
    }

    pub fn source(&self) -> ToolchainSource {
        self.source
    }

    pub fn abi_family(&self) -> NativeAbiFamily {
        if self.cc.uses_msvc_cli() || self.cc.targets_msvc() {
            NativeAbiFamily::Msvc
        } else {
            NativeAbiFamily::Other
        }
    }

    /// Whether the toolchain uses the MSVC ABI, independently of its CLI syntax.
    pub fn uses_msvc_abi(&self) -> bool {
        self.abi_family() == NativeAbiFamily::Msvc
    }

    /// Whether the compiler accepts the cl-compatible CLI, including clang-cl.
    pub fn uses_msvc_cli(&self) -> bool {
        self.cc.uses_msvc_cli()
    }

    pub fn msvc_crt_policy(&self) -> Option<MsvcCrtPolicy> {
        self.uses_msvc_abi().then_some(MsvcCrtPolicy::StaticMt)
    }

    pub fn msvc_crt_compiler_flag(&self) -> Option<&'static str> {
        match self.msvc_crt_policy()? {
            MsvcCrtPolicy::StaticMt if self.uses_msvc_cli() => {
                Some(WINDOWS_MSVC_STATIC_RUNTIME_FLAG)
            }
            MsvcCrtPolicy::StaticMt => Some("-fms-runtime-lib=static"),
        }
    }

    pub fn with_msvc_environment(mut self, environment: MsvcEnvironment) -> Self {
        self.msvc_environment = Some(environment);
        self
    }

    pub fn msvc_environment(&self) -> Option<&MsvcEnvironment> {
        self.msvc_environment.as_ref()
    }

    pub fn cc_command_path(&self) -> String {
        self.cc.cc_path.clone()
    }

    pub fn cc_wrapper(&self) -> Option<&Path> {
        self.cc_wrapper.as_deref()
    }

    /// Attach an already resolved executable without changing compiler selection.
    pub fn with_cc_wrapper(mut self, wrapper: PathBuf) -> Self {
        self.cc_wrapper = Some(wrapper);
        self
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

/// Normalize OS spellings reported by compiler drivers, including versioned
/// components such as `darwin25.6.0` and the MinGW target `mingw32`.
fn target_os_name(component: &str) -> &str {
    match component.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.') {
        "darwin" | "macosx" => "macos",
        "mingw" | "cygwin" => "windows",
        os => os,
    }
}

fn target_os_components(triple: &str) -> impl Iterator<Item = &str> {
    triple.split('-').skip(1).map(target_os_name)
}

/// Whether a target triple names the given host architecture and operating
/// system, as reported by `std::env::consts::{ARCH, OS}`, with the default
/// ABI of that host.
fn triple_matches_host(triple: &str, host_arch: &str, host_os: &str) -> bool {
    let triple = triple.to_ascii_lowercase();
    let mut components = triple.split('-');
    let arch = match components.next().unwrap_or_default() {
        "arm64" | "arm64e" => "aarch64",
        "amd64" => "x86_64",
        "i386" | "i486" | "i586" | "i686" => "x86",
        arch => arch,
    };
    if arch != host_arch {
        return false;
    }
    if !components
        .by_ref()
        .any(|part| target_os_name(part) == host_os)
    {
        return false;
    }
    // The component after the OS names the ABI, as in `gnu`, `gnux32` or
    // `musl`. Host-only objects use glibc on Linux and no named ABI on other
    // Unix hosts. On Windows they are not used, and the MSVC toolchain
    // policy selects the ABI, so the ABI is not compared there.
    match (host_os, components.next()) {
        ("windows", _) | (_, None) => true,
        ("linux", Some(abi)) => abi == "gnu",
        (_, Some(_)) => false,
    }
}

#[cfg(windows)]
fn is_path_like_tool(tool: &str) -> bool {
    let path = Path::new(tool);
    let has_parent = path
        .parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty());
    path.is_absolute() || has_parent
}

fn resolve_native_toolchain_executables_with(
    mut toolchain: Toolchain,
    resolve_executable: impl Fn(&str) -> anyhow::Result<PathBuf>,
) -> anyhow::Result<Toolchain> {
    let compiler = toolchain.cc.cc_path.clone();
    toolchain.cc.cc_path = resolve_executable(&compiler)
        .with_context(|| format!("failed to resolve native compiler executable `{compiler}`"))?
        .display()
        .to_string();

    if toolchain.cc.is_tcc() {
        tcc::resolve_archiver_path(&mut toolchain.cc);
        return Ok(toolchain);
    }

    let archiver = toolchain.cc.ar_path.clone();
    toolchain.cc.ar_path = resolve_executable(&archiver)
        .with_context(|| format!("failed to resolve native archiver executable `{archiver}`"))?
        .display()
        .to_string();
    Ok(toolchain)
}

/// Finalizes the effective native toolchain after source selection and overrides.
/// Toolchains returned across this boundary always contain concrete executable paths.
fn resolve_native_toolchain_executables(toolchain: Toolchain) -> anyhow::Result<Toolchain> {
    let mut toolchain = resolve_native_toolchain_executables_with(toolchain, |tool| {
        crate::toolchain::resolve_executable(tool)
    })?;
    toolchain.cc_wrapper = ENV_CC_WRAPPER
        .as_ref()
        .map_err(|error| anyhow::anyhow!("{error:#}"))?
        .clone();
    Ok(toolchain)
}

// Resolve only when a native toolchain is actually needed. Lightweight commands
// such as `moon check` must not require an installed compiler wrapper.
static ENV_CC_WRAPPER: std::sync::LazyLock<anyhow::Result<Option<PathBuf>>> =
    std::sync::LazyLock::new(|| {
        let wrapper = match env::var(ENV_MOON_CC_WRAPPER) {
            Ok(wrapper) if !wrapper.is_empty() => wrapper,
            Ok(_) | Err(env::VarError::NotPresent) => return Ok(None),
            Err(error) => return Err(error).context(ENV_MOON_CC_WRAPPER),
        };
        crate::toolchain::resolve_executable(&wrapper)
            .with_context(|| {
                format!(
                    "failed to resolve C compiler wrapper `{wrapper}` from {ENV_MOON_CC_WRAPPER}"
                )
            })
            .map(Some)
    });

impl CC {
    pub fn cc_name(&self) -> &'static str {
        match self.cc_kind {
            CCKind::Msvc => "cl.exe",
            CCKind::ClangCl => "clang-cl.exe",
            CCKind::SystemCC => "cc",
            CCKind::Gcc => "gcc",
            CCKind::Clang => "clang",
            CCKind::Tcc => "tcc",
        }
    }

    pub fn ar_name(&self) -> &'static str {
        match self.ar_kind {
            ARKind::MsvcLib => "lib.exe",
            ARKind::AppleLibtool => "libtool",
            ARKind::GnuAr => "ar",
            ARKind::LlvmAr => "llvm-ar",
            ARKind::TccAr => "tcc",
        }
    }

    /// Whether invoking this archiver can leave members from an existing archive
    /// that are absent from the current input list.
    pub fn archiver_updates_existing_archive(&self) -> bool {
        matches!(self.ar_kind, ARKind::GnuAr | ARKind::LlvmAr | ARKind::TccAr)
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

    fn compiler_kind(path: &Path) -> anyhow::Result<CCKind> {
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .context("Invalid compiler path: path must point to a file with valid UTF-8 filename")?
            .to_ascii_lowercase();
        let stem = Self::strip_exe_suffix(&name);
        Ok(if stem.ends_with("clang-cl") {
            CCKind::ClangCl
        } else if stem.ends_with("cl") {
            CCKind::Msvc
        } else if stem.ends_with("gcc") {
            CCKind::Gcc
        } else if stem.ends_with("clang") {
            CCKind::Clang
        } else if stem.ends_with("tcc") {
            CCKind::Tcc
        } else {
            CCKind::SystemCC
        })
    }

    fn strip_exe_suffix(name: &str) -> &str {
        name.strip_suffix(".exe").unwrap_or(name)
    }

    fn probe_target_triple(cc_path: &Path, cc_kind: CCKind) -> std::io::Result<Option<String>> {
        if matches!(cc_kind, CCKind::Msvc | CCKind::ClangCl) {
            return Ok(None);
        }
        let output = Command::new(cc_path).arg("-dumpmachine").output()?;
        if !output.status.success() {
            return Ok(None);
        }
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_ascii_lowercase))
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

    pub fn try_from_cc_path_and_kind(
        ar_name: &str,
        cc_path: &Path,
        cc_kind: CCKind,
    ) -> anyhow::Result<Self> {
        let target = Self::probe_target_triple(cc_path, cc_kind).ok().flatten();
        Ok(Self::with_archiver(ar_name, cc_path, cc_kind, target))
    }

    fn with_archiver(
        ar_name: &str,
        cc_path: &Path,
        cc_kind: CCKind,
        target_triple: Option<String>,
    ) -> Self {
        let (ar_kind, ar_path) = match cc_kind {
            CCKind::Msvc | CCKind::ClangCl => {
                (ARKind::MsvcLib, default_librarian(cc_path, cc_kind))
            }
            CCKind::SystemCC | CCKind::Gcc | CCKind::Clang => (
                CC::classify_gcc_like_archiver(ar_name, target_triple.as_deref()),
                CC::resolve_tool_path(cc_path, ar_name),
            ),
            CCKind::Tcc => tcc::default_archiver(cc_path),
        };
        CC::new(
            cc_kind,
            cc_path.display().to_string(),
            ar_kind,
            ar_path,
            target_triple,
        )
    }

    fn from_driver(cc: &str, kind: CCKind, target: Option<String>, ar: Option<&str>) -> Self {
        let path = Path::new(cc);
        if let Some(ar) = ar {
            return Self::with_archiver(ar, path, kind, target);
        }
        let fallback_ar =
            |suffix| Self::replace_compiler_suffix(cc, suffix, "ar").unwrap_or_else(|| "ar".into());
        let ar = match kind {
            CCKind::Clang => Self::probe_existing_prog_name(path, "ar")
                .or_else(|| Self::probe_existing_prog_name(path, "llvm-ar"))
                .unwrap_or_else(|| fallback_ar("clang")),
            CCKind::Gcc => fallback_ar("gcc"),
            CCKind::SystemCC => fallback_ar("cc"),
            CCKind::Msvc | CCKind::ClangCl | CCKind::Tcc => String::new(),
        };
        Self::with_archiver(&ar, path, kind, target).with_default_platform_archiver()
    }

    pub fn try_from_path_with_ar(cc: &str, ar: &str) -> anyhow::Result<Self> {
        let path = Path::new(cc);
        Self::try_from_cc_path_and_kind(ar, path, Self::compiler_kind(path)?)
    }

    pub fn try_from_path(cc: &str) -> anyhow::Result<Self> {
        let path = Path::new(cc);
        let kind = Self::compiler_kind(path)?;
        Ok(Self::from_driver(
            cc,
            kind,
            Self::probe_target_triple(path, kind).ok().flatten(),
            None,
        ))
    }

    fn try_from_detected_path(cc_path: &Path, cc_kind: CCKind) -> anyhow::Result<Self> {
        if matches!(cc_kind, CCKind::Clang)
            && let Some(cc) = cc_path.to_str()
        {
            return CC::try_from_path(cc);
        }

        let ar_name = match cc_kind {
            CCKind::Msvc | CCKind::ClangCl => "lib.exe",
            CCKind::Tcc => "",
            CCKind::SystemCC | CCKind::Gcc | CCKind::Clang => "ar",
        };
        Self::try_from_cc_path_and_kind(ar_name, cc_path, cc_kind)
            .map(Self::with_default_platform_archiver)
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

    /// Whether the compiler accepts the cl-compatible CLI, including clang-cl.
    pub fn uses_msvc_cli(&self) -> bool {
        matches!(self.cc_kind, CCKind::Msvc | CCKind::ClangCl)
    }

    pub fn is_tcc(&self) -> bool {
        matches!(self.cc_kind, CCKind::Tcc)
    }

    /// Discover the SDK library directory only when constructing a TCC command.
    /// Other toolchains must not trigger the macOS SDK probe.
    pub fn macos_sdk_library_path(&self) -> Option<&'static Path> {
        self.is_tcc().then(tcc::macos_sdk_library_path).flatten()
    }

    /// Whether the probed target triple identifies the MSVC ABI.
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

    /// Whether the probed triple names this OS, independently of its CPU and ABI.
    pub fn targets_os(&self, os: &str) -> bool {
        self.target_triple
            .as_deref()
            .is_some_and(|triple| target_os_components(triple).any(|component| component == os))
    }

    /// Whether the compiler builds for the machine and ABI that run Moon.
    ///
    /// The prebuilt objects in the MoonBit lib directory (`libmoonbitrun.o`,
    /// the simdutf objects and `libbacktrace.a`) are built for that machine,
    /// so a cross compiler must not link them. A compiler whose target triple
    /// could not be probed is assumed to target the host.
    pub fn targets_host(&self) -> bool {
        self.target_triple
            .as_deref()
            .is_none_or(|triple| triple_matches_host(triple, env::consts::ARCH, env::consts::OS))
    }

    pub fn should_link_libm(&self) -> bool {
        self.is_full_featured_gcc_like() && !self.targets_msvc()
    }

    pub fn can_use_simdutf(&self) -> bool {
        CAN_USE_SIMDUTF
            && !self.is_tcc()
            && !self.uses_msvc_cli()
            && !self.targets_msvc()
            && self.targets_host()
    }

    pub fn can_link_libmoonbitrun_o(&self) -> bool {
        CAN_USE_MOONBITRUN && !self.uses_msvc_cli() && !self.is_tcc() && self.targets_host()
    }

    pub fn is_libmoonbitrun_o_available(&self) -> bool {
        // If users set MOON_CC, we believe they know what they are doing
        // And we conservatively disable libmoonbitrun.o
        self.can_link_libmoonbitrun_o() && !self.is_env_override
    }
}

/// A global driver supplies one target for MoonBit sources and for native tool actions.
pub struct GlobalCompiler {
    path: String,
    kind: CCKind,
    target_triple: Option<String>,
    cc: OnceLock<CC>,
}

impl GlobalCompiler {
    /// The OS of a reported target triple, for the supported native platforms.
    pub fn target_os(&self) -> Option<&str> {
        target_os_components(self.target_triple.as_deref()?)
            .find(|os| matches!(*os, "windows" | "linux" | "macos"))
    }

    /// Discover the archiver only when planning a native tool action.
    pub fn cc(&self) -> &CC {
        self.cc.get_or_init(|| CC {
            is_env_override: true,
            ..CC::from_driver(
                &self.path,
                self.kind,
                self.target_triple.clone(),
                env::var(ENV_MOON_AR).ok().as_deref(),
            )
        })
    }
}

pub fn global_compiler() -> anyhow::Result<Option<&'static GlobalCompiler>> {
    static COMPILER: LazyLock<anyhow::Result<Option<GlobalCompiler>>> = LazyLock::new(|| {
        let path = match env::var(ENV_MOON_CC) {
            Ok(path) => path,
            Err(env::VarError::NotPresent) => return Ok(None),
            Err(err) => return Err(err).context(ENV_MOON_CC),
        };
        let kind = CC::compiler_kind(Path::new(&path)).context(ENV_MOON_CC)?;
        // Bare cl is discovered through Visual Studio when a native action needs it.
        let target_triple = if cfg!(windows)
            && (path.eq_ignore_ascii_case("cl") || path.eq_ignore_ascii_case("cl.exe"))
        {
            None
        } else {
            let executable = crate::toolchain::resolve_executable(&path).with_context(|| {
                format!("failed to resolve native compiler executable `{path}` from {ENV_MOON_CC}")
            })?;
            CC::probe_target_triple(&executable, kind).with_context(|| {
                format!("failed to probe native compiler executable `{path}` from {ENV_MOON_CC}")
            })?
        };
        Ok(Some(GlobalCompiler {
            path,
            kind,
            target_triple,
            cc: OnceLock::new(),
        }))
    });
    COMPILER
        .as_ref()
        .map(Option::as_ref)
        .map_err(|err| anyhow::anyhow!("{err:#}"))
}

fn detect_path_candidate(cc_path: &Path, cc_kind: CCKind) -> anyhow::Result<CC> {
    CC::try_from_detected_path(cc_path, cc_kind)
        .with_context(|| format!("failed to use C compiler at {}", cc_path.display()))
}

fn detect_cc_on_path() -> anyhow::Result<CC> {
    // MSVC discovery separately resolves the target architecture and command environment.
    let candidates = [
        ("cc", CCKind::SystemCC),
        ("gcc", CCKind::Gcc),
        ("clang", CCKind::Clang),
    ];
    let mut errors = Vec::new();
    for (name, kind) in candidates {
        let Ok(cc_path) = which::which(name) else {
            continue;
        };
        match detect_path_candidate(&cc_path, kind) {
            Ok(cc) => return Ok(cc),
            Err(e) => errors.push(format!("{e:#}")),
        }
    }

    if errors.is_empty() {
        let names = candidates.map(|(name, _)| name).join(", ");
        anyhow::bail!("no system C compiler found; tried {names}")
    }
    anyhow::bail!(
        "failed to resolve system C compiler candidates: {}",
        errors.join("; ")
    )
}

static DETECTED_CC_ON_PATH: std::sync::LazyLock<anyhow::Result<CC>> =
    std::sync::LazyLock::new(detect_cc_on_path);

fn cached_cc(result: &std::sync::LazyLock<anyhow::Result<CC>>) -> anyhow::Result<CC> {
    result
        .as_ref()
        .cloned()
        .map_err(|e| anyhow::anyhow!("{e:#}"))
}

pub fn try_cc_on_path() -> anyhow::Result<CC> {
    cached_cc(&DETECTED_CC_ON_PATH)
}

fn detected_default_native_toolchain() -> anyhow::Result<Toolchain> {
    #[cfg(windows)]
    {
        if let Ok(toolchain) = discovered_windows_msvc_toolchain() {
            return Ok(toolchain);
        }
    }

    try_cc_on_path().map(Toolchain::from_path_probe)
}

/// Resolve the winning explicit override, or discover a toolchain when none applies.
pub fn effective_native_toolchain(
    env_cc: Option<&CC>,
    package_cc: Option<&str>,
) -> anyhow::Result<Toolchain> {
    let (cc, source) = if let Some(cc) = env_cc {
        (cc.clone(), ToolchainSource::EnvOverride)
    } else if let Some(package_cc) = package_cc {
        (
            CC::try_from_path(package_cc)?,
            ToolchainSource::PackageOverride,
        )
    } else {
        return resolve_native_toolchain_executables(detected_default_native_toolchain()?);
    };

    let toolchain = match cc {
        #[cfg(windows)]
        cc if cc.uses_msvc_cli() && !is_path_like_tool(&cc.cc_path) => {
            resolve_requested_msvc_toolchain(cc, source)?
        }
        cc => Toolchain {
            cc,
            source,
            msvc_environment: None,
            cc_wrapper: None,
        },
    };
    resolve_native_toolchain_executables(toolchain)
}

// Struct to hold path configuration for commands
#[derive(Clone, Debug)]
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

    /// Select the allocator support object using the toolchain's host/ABI policy.
    pub fn moonbitrun_object(&self, cc: &CC, allocator: NativeAllocator) -> Option<PathBuf> {
        allocator
            .should_link_moonbitrun(cc)
            .then(|| Path::new(&self.lib_path).join("libmoonbitrun.o"))
    }

    /// Observe the optional host-only backtrace archive when a link requests it.
    pub fn libbacktrace_path(&self, cc: &CC) -> Option<PathBuf> {
        if !cc.targets_host() {
            return None;
        }
        let path = Path::new(&self.lib_path).join("libbacktrace.a");
        path.exists().then_some(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn native_toolchain_fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native-toolchain")
    }

    fn test_executable_name(name: &str) -> String {
        if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        }
    }

    /// A target triple that names the machine running the tests.
    fn host_triple() -> String {
        let os = match env::consts::OS {
            "macos" => "apple-darwin",
            os => os,
        };
        format!("{}-unknown-{os}", env::consts::ARCH)
    }

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

    #[test]
    fn archive_membership_fingerprint_is_limited_to_update_style_archivers() {
        let mut cc = fake_cc(CCKind::SystemCC, None);

        for ar_kind in [ARKind::GnuAr, ARKind::LlvmAr, ARKind::TccAr] {
            cc.ar_kind = ar_kind;
            assert!(cc.archiver_updates_existing_archive());
        }
        for ar_kind in [ARKind::AppleLibtool, ARKind::MsvcLib] {
            cc.ar_kind = ar_kind;
            assert!(!cc.archiver_updates_existing_archive());
        }
    }

    #[test]
    fn native_allocator_parses_environment_values() {
        assert_eq!(
            NativeAllocator::parse("mimalloc").unwrap(),
            NativeAllocator::Mimalloc
        );
        assert_eq!(
            NativeAllocator::parse("system").unwrap(),
            NativeAllocator::System
        );
        assert!(NativeAllocator::parse("malloc").is_err());
    }

    #[test]
    fn explicit_mimalloc_rejects_toolchains_without_support_object() {
        let msvc = fake_cc(CCKind::Msvc, Some("x86_64-pc-windows-msvc"));
        let tcc = fake_cc(CCKind::Tcc, None);

        assert!(NativeAllocator::Mimalloc.validate_for_cc(&msvc).is_err());
        assert!(NativeAllocator::Mimalloc.validate_for_cc(&tcc).is_err());
        assert!(NativeAllocator::System.validate_for_cc(&msvc).is_ok());
        assert!(NativeAllocator::Default.validate_for_cc(&tcc).is_ok());
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
    fn explicit_archiver_names_select_the_matching_cli() {
        for target in [
            None,
            Some("x86_64-pc-windows-msvc"),
            Some("aarch64-pc-windows-msvc"),
        ] {
            for archiver in ["lib", "LIB.ExE", "llvm-lib", "C:/LLVM/bin/LLVM-LIB.exe"] {
                assert_eq!(
                    CC::classify_gcc_like_archiver(archiver, target),
                    ARKind::MsvcLib,
                    "archiver: {archiver}, target: {target:?}"
                );
            }
        }
        assert_eq!(
            CC::classify_gcc_like_archiver(
                "/Applications/Xcode.app/usr/bin/libtool",
                Some("arm64-apple-darwin")
            ),
            ARKind::AppleLibtool
        );
        assert_eq!(
            CC::classify_gcc_like_archiver("llvm-ar", Some("x86_64-unknown-linux-gnu")),
            ARKind::LlvmAr
        );
        assert_eq!(
            CC::classify_gcc_like_archiver("ar", Some("x86_64-unknown-linux-gnu")),
            ARKind::GnuAr
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_msvc_host_target_triple_matches_supported_64_bit_arch() {
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            windows_msvc_host_target_triple().as_deref(),
            Some("x86_64-pc-windows-msvc")
        );
        #[cfg(target_arch = "aarch64")]
        assert_eq!(
            windows_msvc_host_target_triple().as_deref(),
            Some("aarch64-pc-windows-msvc")
        );
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        assert_eq!(windows_msvc_host_target_triple(), None);
    }

    #[test]
    fn toolchain_detects_msvc_abi_and_crt_policy() {
        let mut cc = fake_cc(CCKind::Msvc, None);
        cc.cc_path = "cl.exe".to_string();
        cc.ar_kind = ARKind::MsvcLib;
        cc.ar_path = "lib.exe".to_string();

        let toolchain = Toolchain::from_path_probe(cc);

        assert_eq!(toolchain.abi_family(), NativeAbiFamily::Msvc);
        assert!(toolchain.uses_msvc_abi());
        assert!(toolchain.uses_msvc_cli());
        assert_eq!(toolchain.msvc_crt_policy(), Some(MsvcCrtPolicy::StaticMt));
        assert_eq!(
            toolchain.msvc_crt_compiler_flag(),
            Some(WINDOWS_MSVC_STATIC_RUNTIME_FLAG)
        );
    }

    #[test]
    fn msvc_abi_uses_static_crt_with_plain_clang() {
        let toolchain =
            Toolchain::from_path_probe(fake_cc(CCKind::Clang, Some("x86_64-pc-windows-msvc")));

        assert_eq!(toolchain.abi_family(), NativeAbiFamily::Msvc);
        assert!(toolchain.uses_msvc_abi());
        assert!(!toolchain.uses_msvc_cli());
        assert_eq!(
            toolchain.msvc_crt_compiler_flag(),
            Some("-fms-runtime-lib=static")
        );
    }

    #[test]
    fn compiler_wrapper_does_not_override_toolchain_selection() {
        let toolchain = Toolchain::from_path_probe(fake_cc(CCKind::Clang, None));
        let wrapped = toolchain
            .clone()
            .with_cc_wrapper(PathBuf::from("/tools/ccache"));
        assert_eq!(wrapped.source(), toolchain.source());
        assert_eq!(wrapped.cc().cc_path, toolchain.cc().cc_path);
        assert_eq!(wrapped.cc().ar_path, toolchain.cc().ar_path);
        assert_eq!(
            wrapped.cc().is_libmoonbitrun_o_available(),
            toolchain.cc().is_libmoonbitrun_o_available()
        );
    }

    #[test]
    fn bare_package_override_resolves_compiler_and_archiver_from_search_path() {
        let fixture = native_toolchain_fixture();
        let bin = fixture.join("bin");
        let compiler_name = test_executable_name("fake-gcc");
        let archiver_name = test_executable_name("fake-ar");
        let compiler = bin.join(&compiler_name);
        let archiver = bin.join(&archiver_name);

        let mut cc = fake_cc(CCKind::Gcc, None);
        cc.cc_path = compiler_name;
        cc.ar_path = archiver_name;
        let toolchain = resolve_native_toolchain_executables_with(
            Toolchain::from_package_override(cc),
            |tool| {
                which::which_in(tool, Some(bin.as_os_str()), std::env::current_dir()?)
                    .map_err(anyhow::Error::from)
            },
        )
        .expect("resolve bare package override from test search path");

        assert_eq!(Path::new(&toolchain.cc().cc_path), compiler);
        assert_eq!(Path::new(&toolchain.cc().ar_path), archiver);
        assert!(Path::new(&toolchain.cc().cc_path).is_absolute());
        assert!(Path::new(&toolchain.cc().ar_path).is_absolute());
    }

    #[test]
    fn missing_native_compiler_reports_its_role_during_resolution() {
        let bin = native_toolchain_fixture().join("bin");
        let archiver = bin.join(test_executable_name("fake-ar"));

        let mut cc = fake_cc(CCKind::Gcc, None);
        cc.cc_path = bin
            .join(test_executable_name("missing-gcc"))
            .display()
            .to_string();
        cc.ar_path = archiver.display().to_string();
        let err = resolve_native_toolchain_executables(Toolchain::from_package_override(cc))
            .expect_err("missing compiler should fail toolchain resolution");

        assert!(
            format!("{err:#}").contains("native compiler executable"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn missing_native_archiver_reports_its_role_during_resolution() {
        let bin = native_toolchain_fixture().join("bin");
        let compiler = bin.join(test_executable_name("fake-gcc"));

        let mut cc = fake_cc(CCKind::Gcc, None);
        cc.cc_path = compiler.display().to_string();
        cc.ar_path = bin
            .join(test_executable_name("missing-ar"))
            .display()
            .to_string();
        let err = resolve_native_toolchain_executables(Toolchain::from_package_override(cc))
            .expect_err("missing archiver should fail toolchain resolution");

        assert!(
            format!("{err:#}").contains("native archiver executable"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn tcc_uses_one_resolved_executable_for_compiler_and_archiver() {
        let tcc = native_toolchain_fixture()
            .join("bin")
            .join(test_executable_name("fake-tcc"));

        let mut cc = fake_cc(CCKind::Tcc, None);
        cc.cc_path = tcc.display().to_string();
        cc.ar_kind = ARKind::TccAr;
        cc.ar_path = "unresolved-tcc-archiver".to_string();
        let toolchain = resolve_native_toolchain_executables(Toolchain::from_package_override(cc))
            .expect("resolve TCC toolchain");

        assert_eq!(toolchain.cc().cc_path, toolchain.cc().ar_path);
        assert_eq!(Path::new(&toolchain.cc().cc_path), tcc);
        assert!(Path::new(&toolchain.cc().cc_path).is_absolute());
    }

    #[test]
    fn compiler_selection_applies_precedence_before_parsing_package_override() {
        for compiler in [
            native_toolchain_fixture().join("custom/bin/cl.exe"),
            native_toolchain_fixture()
                .join("bin")
                .join(test_executable_name("fake-gcc")),
        ] {
            let env = CC {
                is_env_override: true,
                ..CC::try_from_path(compiler.to_str().unwrap()).unwrap()
            };
            let toolchain = effective_native_toolchain(Some(&env), Some("")).unwrap();
            assert_eq!(Path::new(&toolchain.cc().cc_path), compiler);
            assert_eq!(toolchain.source(), ToolchainSource::EnvOverride);
        }
    }

    #[test]
    fn path_like_msvc_request_does_not_require_discovery() {
        let fixture = native_toolchain_fixture();
        let compiler = fixture.join("custom/bin/cl.exe");
        let archiver = fixture.join("custom/bin/lib.exe");
        // This also runs outside Windows, where MSVC discovery always errors.
        let toolchain = effective_native_toolchain(None, compiler.to_str()).unwrap();
        assert_eq!(Path::new(&toolchain.cc().cc_path), compiler);
        assert_eq!(Path::new(&toolchain.cc().ar_path), archiver);
        assert_eq!(toolchain.msvc_environment(), None);
        assert_eq!(toolchain.source(), ToolchainSource::PackageOverride);
    }

    #[cfg(windows)]
    #[test]
    fn bare_msvc_requests_resolve_the_requested_compiler() {
        for name in ["cl", "cl.exe", "clang-cl", "clang-cl.exe"] {
            // SDK discovery can find cl outside PATH; only clang-cl is optional.
            if name.starts_with("clang-cl") && which::which(name).is_err() {
                assert!(env::var_os("CI").is_none(), "{name} missing in CI");
                eprintln!("skipping compiler selection case: {name} is unavailable");
                continue;
            }
            let toolchain = effective_native_toolchain(None, Some(name))
                .expect("resolve requested compiler and SDK");
            assert_eq!(
                Path::new(&toolchain.cc().cc_path).file_stem().unwrap(),
                Path::new(name).file_stem().unwrap()
            );
            assert!(Path::new(&toolchain.cc().cc_path).is_absolute());
            assert!(Path::new(&toolchain.cc().ar_path).is_absolute());
            assert!(toolchain.msvc_environment().is_some());
        }
        let err = effective_native_toolchain(None, Some("moon-missing-clang-cl")).unwrap_err();
        assert!(format!("{err:#}").contains("native compiler executable `moon-missing-clang-cl`"));
    }

    #[test]
    fn windows_msvc_compiler_keeps_archiver_in_the_same_tool_directory() {
        let cc = CC::try_from_path_with_ar("custom/x64/cl.exe", "custom/arm/lib.exe")
            .expect("parse MSVC compiler with independent archiver override");

        assert_eq!(cc.cc_path, "custom/x64/cl.exe");
        assert_eq!(normalize_path_separators(&cc.ar_path), "custom/x64/lib.exe");
    }

    #[test]
    fn clang_cl_uses_sibling_llvm_lib_when_lib_is_absent() {
        let dir = tempfile::tempdir().expect("create temporary tool directory");
        let clang_cl = dir.path().join("clang-cl.exe");
        let llvm_lib = dir.path().join("llvm-lib.exe");
        std::fs::write(&clang_cl, []).expect("create fake clang-cl");
        std::fs::write(&llvm_lib, []).expect("create fake llvm-lib");

        let cc = CC::try_from_path(&clang_cl.display().to_string()).expect("parse clang-cl");

        assert_eq!(cc.ar_kind, ARKind::MsvcLib);
        assert_eq!(Path::new(&cc.ar_path), llvm_lib);
    }

    #[test]
    fn simdutf_requires_supported_host_and_non_tcc_compiler() {
        assert_eq!(
            fake_cc(CCKind::Gcc, Some(&host_triple())).can_use_simdutf(),
            CAN_USE_SIMDUTF
        );
        assert!(!fake_cc(CCKind::Tcc, Some("x86_64-unknown-linux-gnu")).can_use_simdutf());
        assert!(!fake_cc(CCKind::Msvc, None).can_use_simdutf());
        assert!(!fake_cc(CCKind::Clang, Some("x86_64-pc-windows-msvc")).can_use_simdutf());
    }

    #[test]
    fn compiler_target_os_is_independent_of_arch_and_abi() {
        for triple in [
            "x86_64-w64-mingw32",
            "aarch64-pc-windows-msvc",
            "i686-w64-windows-gnu",
            "x86_64-pc-cygwin",
        ] {
            assert!(fake_cc(CCKind::Gcc, Some(triple)).targets_os("windows"));
        }
        let darwin = fake_cc(CCKind::Clang, Some("arm64-apple-darwin25.6.0"));
        assert!(darwin.targets_os("macos"));
        assert!(!darwin.targets_os("windows"));
        assert!(!fake_cc(CCKind::Gcc, None).targets_os("windows"));
    }

    #[test]
    fn target_triple_matches_host_arch_and_os() {
        for triple in [
            "arm64-apple-darwin25.6.0",
            "aarch64-apple-darwin",
            "aarch64-unknown-macos",
            "arm64-apple-macosx14.0.0",
        ] {
            assert!(triple_matches_host(triple, "aarch64", "macos"), "{triple}");
        }
        for triple in [
            "x86_64-linux-gnu",
            "x86_64-unknown-linux-gnu",
            "amd64-pc-linux-gnu",
            "x86_64-redhat-linux",
        ] {
            assert!(triple_matches_host(triple, "x86_64", "linux"), "{triple}");
        }
        assert!(triple_matches_host(
            "x86_64-w64-mingw32",
            "x86_64",
            "windows"
        ));
        assert!(triple_matches_host(
            "x86_64-pc-windows-msvc",
            "x86_64",
            "windows"
        ));

        assert!(!triple_matches_host(
            "x86_64-apple-darwin",
            "aarch64",
            "macos"
        ));
        assert!(!triple_matches_host(
            "x86_64-unknown-linux-musl",
            "aarch64",
            "macos"
        ));
        assert!(!triple_matches_host(
            "aarch64-linux-musl",
            "x86_64",
            "linux"
        ));
        assert!(!triple_matches_host(
            "x86_64-w64-windows-gnu",
            "x86_64",
            "linux"
        ));
        assert!(!triple_matches_host(
            "arm64-apple-ios17.0",
            "aarch64",
            "macos"
        ));
        assert!(!triple_matches_host(
            "aarch64-macos-none",
            "aarch64",
            "macos"
        ));
        for triple in [
            "x86_64-unknown-linux-gnux32",
            "x86_64-unknown-linux-musl",
            "x86_64-linux-android",
        ] {
            assert!(!triple_matches_host(triple, "x86_64", "linux"), "{triple}");
        }
    }

    /// A Linux target triple whose architecture differs from the test host.
    fn foreign_triple() -> &'static str {
        if env::consts::ARCH == "x86_64" {
            "aarch64-unknown-linux-musl"
        } else {
            "x86_64-unknown-linux-musl"
        }
    }

    #[test]
    fn cross_compiler_does_not_use_host_prebuilt_objects() {
        let cc = fake_cc(CCKind::Gcc, Some(foreign_triple()));
        assert!(!cc.targets_host());
        assert!(!cc.can_use_simdutf());
        assert!(!cc.can_link_libmoonbitrun_o());
        assert!(!cc.is_libmoonbitrun_o_available());
        assert!(NativeAllocator::Mimalloc.validate_for_cc(&cc).is_err());

        assert!(fake_cc(CCKind::Gcc, Some(&host_triple())).targets_host());
        assert!(fake_cc(CCKind::Gcc, None).targets_host());
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
    fn try_from_path_recognizes_clang_exe() {
        let cc = CC::try_from_path("C:/llvm/bin/clang.exe").expect("parse clang.exe");
        assert!(matches!(cc.cc_kind, CCKind::Clang));
    }

    #[test]
    fn try_from_path_recognizes_clang_cl_exe() {
        let cc = CC::try_from_path("C:/llvm/bin/clang-cl.exe").expect("parse clang-cl.exe");
        assert!(matches!(cc.cc_kind, CCKind::ClangCl));
        assert!(cc.uses_msvc_cli());
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
        if cc.targets_msvc() {
            assert_eq!(cc.ar_kind, ARKind::MsvcLib);
            assert_eq!(
                cc.ar_path,
                CC::probe_existing_prog_name(&clang_path, "llvm-lib")
                    .expect("resolve Clang-reported llvm-lib")
            );
            return;
        }
        #[cfg(target_os = "macos")]
        if cc.targets_apple_darwin() {
            assert_eq!(cc.ar_kind, ARKind::AppleLibtool);
            assert_eq!(
                Path::new(&cc.ar_path),
                resolve_apple_libtool_path().expect("resolve Apple libtool")
            );
            return;
        }
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
