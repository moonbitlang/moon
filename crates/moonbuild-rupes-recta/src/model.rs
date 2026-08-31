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

use moonutil::{
    compiler_flags::NativeAllocator,
    resolution::{ModuleId, ResolvedEnv},
    target::TargetBackend,
};

use crate::discover::DiscoverResult;

slotmap::new_key_type! {
    /// An unique identifier pointing to a package currently discovered from imported modules.
    pub struct PackageId;
}

/// The selected backend and its backend-specific compile configuration.
///
/// This is the single source of truth after the user-visible target backend is
/// resolved. Keeping backend-specific options inside the matching variant
/// prevents invalid combinations such as a native implementation mode on a
/// Wasm build.
#[derive(Clone, Debug)]
pub enum BackendConfig {
    Wasm {
        use_wat: bool,
        wasi_link: bool,
    },
    WasmGc {
        use_wat: bool,
    },
    Js,
    Native {
        mode: NativeBackendMode,
        allocator: NativeAllocator,
    },
    Llvm {
        allocator: NativeAllocator,
    },
}

pub const ENV_MOONBIT_NEW_NATIVE: &str = "MOONBIT_NEW_NATIVE";

/// Concrete native object-code backend selected under the `native` surface target.
///
/// `TargetBackend::Native` remains the user-visible native backend. This type
/// only describes the experimental direct object-code lowering used behind it.
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum NativeTarget {
    Aarch64AppleDarwin,
    X86_64UnknownLinuxGnu,
    X86_64PcWindowsMsvc,
}

/// The native implementation selected under the user-visible `native` backend.
#[derive(Clone, Debug)]
pub enum NativeBackendMode {
    /// Legacy generated-C native path.
    GeneratedC,
    /// Experimental direct object-code native path.
    DirectObject(DirectNativeMode),
}

/// Concrete direct object-code native implementation.
#[derive(Clone, Debug)]
pub enum DirectNativeMode {
    Target(NativeTarget),
}

impl NativeTarget {
    pub fn from_env_for_host() -> Option<Self> {
        let env_value = std::env::var(ENV_MOONBIT_NEW_NATIVE).ok();
        Self::from_host_with_new_native_env(
            std::env::consts::ARCH,
            std::env::consts::OS,
            env_value.as_deref(),
        )
    }

    fn from_host_with_new_native_env(
        arch: &str,
        os: &str,
        env_value: Option<&str>,
    ) -> Option<Self> {
        let target = Self::from_host(arch, os)?;
        let enabled = match target {
            Self::Aarch64AppleDarwin | Self::X86_64UnknownLinuxGnu => env_value != Some("0"),
            Self::X86_64PcWindowsMsvc => env_value == Some("1"),
        };
        enabled.then_some(target)
    }

    pub fn from_host(arch: &str, os: &str) -> Option<Self> {
        match (arch, os) {
            ("aarch64", "macos") => Some(Self::Aarch64AppleDarwin),
            ("x86_64", "linux") => Some(Self::X86_64UnknownLinuxGnu),
            ("x86_64", "windows") => Some(Self::X86_64PcWindowsMsvc),
            _ => None,
        }
    }

    pub fn moonc_target_flag(self) -> &'static str {
        match self {
            Self::Aarch64AppleDarwin => "aarch64-apple-darwin",
            Self::X86_64UnknownLinuxGnu => "x86_64-unknown-linux-gnu",
            Self::X86_64PcWindowsMsvc => "x86_64-pc-windows-msvc",
        }
    }
}

impl NativeBackendMode {
    pub fn direct_target(&self) -> Option<NativeTarget> {
        self.direct_native_mode().map(DirectNativeMode::target)
    }

    pub fn direct_native_mode(&self) -> Option<&DirectNativeMode> {
        match self {
            Self::DirectObject(mode) => Some(mode),
            Self::GeneratedC => None,
        }
    }
}

impl BackendConfig {
    pub fn target_backend(&self) -> TargetBackend {
        match self {
            Self::Wasm { .. } => TargetBackend::Wasm,
            Self::WasmGc { .. } => TargetBackend::WasmGC,
            Self::Js => TargetBackend::Js,
            Self::Native { .. } => TargetBackend::Native,
            Self::Llvm { .. } => TargetBackend::LLVM,
        }
    }

    pub fn direct_native_target(&self) -> Option<NativeTarget> {
        match self {
            Self::Native { mode, .. } => mode.direct_target(),
            Self::Wasm { .. } | Self::WasmGc { .. } | Self::Js | Self::Llvm { .. } => None,
        }
    }

    pub fn native_allocator(&self) -> Option<NativeAllocator> {
        match self {
            Self::Native { allocator, .. } | Self::Llvm { allocator } => Some(*allocator),
            Self::Wasm { .. } | Self::WasmGc { .. } | Self::Js => None,
        }
    }

    pub fn wasi_link(&self) -> bool {
        matches!(
            self,
            Self::Wasm {
                wasi_link: true,
                ..
            }
        )
    }
}

impl DirectNativeMode {
    pub fn target(&self) -> NativeTarget {
        match self {
            Self::Target(target) => *target,
        }
    }
}

/// The kind of build target within a package.
///
/// This determines what files are included and/or the arguments passed to the
/// compiler.
#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum TargetKind {
    /// The library or executable itself represented by the package.
    Source,
    /// Whitebox tests are tests that are built *alongside* the source code,
    /// having access to its internal symbols.
    WhiteboxTest,
    /// Blackbox tests are tests that depend on the package as an external
    /// module, only having access to its public interface.
    BlackboxTest,

    /// Inline tests are tests that are embedded within the source files
    /// themselves. They are similar to whitebox tests, but lack their own
    /// import list.
    InlineTest,
    /// This is the subpackage designed originally for breaking cycles in
    /// `moonbitlang/core`. It's expected to be used sparingly.
    SubPackage,
}

impl TargetKind {
    pub fn is_test(self) -> bool {
        matches!(
            self,
            TargetKind::WhiteboxTest | TargetKind::BlackboxTest | TargetKind::InlineTest
        )
    }

    pub fn all_tests() -> &'static [TargetKind] {
        &[
            TargetKind::WhiteboxTest,
            TargetKind::BlackboxTest,
            TargetKind::InlineTest,
        ]
    }
}

/// Represents a single compile target that may be separately checked, built,
/// linked, etc.
#[derive(Clone, PartialEq, Eq, Hash, Copy, PartialOrd, Ord)]
pub struct BuildTarget {
    pub package: PackageId,
    pub kind: TargetKind,
    // MAINTAINERS: You might want to add a target-backend field here, if
    // packages no longer share the same target backend. That should be an ID
    // into a global list or something similar.
}

impl std::fmt::Debug for BuildTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}@{:?}", self.package, self.kind)
    }
}

impl PackageId {
    pub fn build_target(self, kind: TargetKind) -> BuildTarget {
        BuildTarget {
            package: self,
            kind,
        }
    }
}

/// A node in the build dependency graph, containing a build target and the
/// corresponding action that should be performed on that target.
///
/// Note: You may recognize that some nodes are keyed by [`BuildTarget`] while
/// others are keyed by just [`PackageId`] or even [`ModuleId`]. This is because
/// some backend artifacts, such as C stubs, are shared by every target within
/// the package/module and do not need to be duplicated for each target.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum BuildPlanNode {
    /// Check the given build target
    Check(BuildTarget),
    /// Emit proof artifacts (`.mlw` + proof-aware `.mi`) without invoking Why3.
    EmitProof(BuildTarget),
    /// Prove the given build target.
    Prove(BuildTarget),

    /// Build the `.core` file from `.mbt` sources for the given target.
    BuildCore(BuildTarget),

    /// Build the i-th C file in the C stub list.
    BuildCStub(PackageId, u32), // change into global artifact list if we need non-package ones

    /// Archive all C stubs for the given package.
    ArchiveOrLinkCStubs(PackageId),

    /// Link the `.core` file into an executable or library for the given target.
    LinkCore(BuildTarget),

    /// The final runnable artifact for a target.
    ///
    /// This action exists only for native and LLVM targets, where the output
    /// from `LinkCore` is a C file or object file that needs further compilation
    /// and linking. Wasm and JavaScript `LinkCore` actions provide the final
    /// executable artifact directly.
    MakeExecutable(BuildTarget),

    /// Generate the macOS dSYM bundle for a linked executable.
    GenerateDsym(BuildTarget),

    /// Generate test driver and metadata for the given test target.
    GenerateTestInfo(BuildTarget),

    /// Generate the package boundary used to run JavaScript tests under Node.
    GenerateNodeTestPackageConfig(PackageId),

    /// Generate the `.mbti` interface file for the given target's package.
    /// This does not promote the `.mbti` into the source directory.
    GenerateMbti(BuildTarget),

    /// Bundle all non-virtual packages in the given module. This produces a
    /// `.core` file containing all packages.
    ///
    /// This is only used in the standard library `moonbitlang/core` currently.
    Bundle(ModuleId),

    /// Build the i-th runtime C translation unit.
    BuildRuntimeObject(u32),

    /// Archive the runtime objects into the native runtime library.
    BuildRuntimeLib,

    /// Build the virtual package's `.mbti` interface file to get an `.mi` file.
    BuildVirtual(PackageId),

    /// Docs build for a single selected module.
    ///
    /// The legacy layout does not have a separate folder for different kinds
    /// of docs, and the behavior is dictated by `packages.json`, so we can't
    /// do much better for now.
    BuildDocs(ModuleId),
}

impl BuildPlanNode {
    /// Extract the target from a BuildPlanNode, if it has one
    pub fn extract_target(&self) -> Option<BuildTarget> {
        match *self {
            BuildPlanNode::Check(target)
            | BuildPlanNode::EmitProof(target)
            | BuildPlanNode::Prove(target)
            | BuildPlanNode::BuildCore(target)
            | BuildPlanNode::LinkCore(target)
            | BuildPlanNode::MakeExecutable(target)
            | BuildPlanNode::GenerateDsym(target)
            | BuildPlanNode::GenerateTestInfo(target)
            | BuildPlanNode::GenerateMbti(target) => Some(target),
            BuildPlanNode::BuildCStub(_, _)
            | BuildPlanNode::BuildRuntimeObject(_)
            | BuildPlanNode::ArchiveOrLinkCStubs(_)
            | BuildPlanNode::GenerateNodeTestPackageConfig(_)
            | BuildPlanNode::Bundle(_)
            | BuildPlanNode::BuildRuntimeLib
            | BuildPlanNode::BuildDocs(_)
            | BuildPlanNode::BuildVirtual(_) => None,
        }
    }

    /// Return a human-readable description for this build plan node, resolving
    /// PackageId/ModuleId to names and rendering its backend and target kind as
    /// one qualifier.
    pub(crate) fn human_desc(
        &self,
        env: &ResolvedEnv,
        packages: &DiscoverResult,
        backend: &str,
    ) -> String {
        let file_basename = |path: &std::path::Path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| path.display().to_string())
        };

        let qualifier = |kind: TargetKind| match kind {
            TargetKind::Source => format!(" ({backend})"),
            TargetKind::WhiteboxTest => format!(" ({backend}, whitebox test)"),
            TargetKind::BlackboxTest => format!(" ({backend}, blackbox test)"),
            TargetKind::InlineTest => format!(" ({backend}, inline test)"),
            TargetKind::SubPackage => format!(" ({backend}, subpackage)"),
        };

        let description = match self {
            BuildPlanNode::Check(build_target) => {
                let fqn = packages.fqn(build_target.package);
                format!("check {}{}", fqn, qualifier(build_target.kind))
            }
            BuildPlanNode::EmitProof(build_target) => {
                let fqn = packages.fqn(build_target.package);
                format!("emit proof {}{}", fqn, qualifier(build_target.kind))
            }
            BuildPlanNode::Prove(build_target) => {
                let fqn = packages.fqn(build_target.package);
                format!("prove {}{}", fqn, qualifier(build_target.kind))
            }
            BuildPlanNode::BuildCore(build_target) => {
                let fqn = packages.fqn(build_target.package);
                format!("build {}{}", fqn, qualifier(build_target.kind))
            }
            BuildPlanNode::BuildCStub(package_id, index) => {
                let pkg = packages.get_package(*package_id);
                let file = file_basename(pkg.c_stub_files[*index as usize].as_path());
                format!("build c stub {} {}", packages.fqn(*package_id), file)
            }
            BuildPlanNode::ArchiveOrLinkCStubs(package_id) => {
                format!("archive c stubs {}", packages.fqn(*package_id))
            }
            BuildPlanNode::LinkCore(build_target) => {
                let fqn = packages.fqn(build_target.package);
                format!("link {}{}", fqn, qualifier(build_target.kind))
            }
            BuildPlanNode::MakeExecutable(build_target) => {
                let fqn = packages.fqn(build_target.package);
                format!("make executable {}{}", fqn, qualifier(build_target.kind))
            }
            BuildPlanNode::GenerateDsym(build_target) => {
                let fqn = packages.fqn(build_target.package);
                format!("generate dSYM {}{}", fqn, qualifier(build_target.kind))
            }
            BuildPlanNode::GenerateTestInfo(build_target) => {
                let fqn = packages.fqn(build_target.package);
                format!(
                    "generate test driver for {}{}",
                    fqn,
                    qualifier(build_target.kind)
                )
            }
            BuildPlanNode::GenerateNodeTestPackageConfig(package_id) => {
                format!(
                    "generate Node test package config for {}",
                    packages.fqn(*package_id)
                )
            }
            BuildPlanNode::GenerateMbti(build_target) => {
                let fqn = packages.fqn(build_target.package);
                format!("generate mbti for {}{}", fqn, qualifier(build_target.kind))
            }
            BuildPlanNode::Bundle(module_id) => {
                let module_src = env.module_source(*module_id);
                format!(
                    "bundle module {}@{}",
                    module_src.name(),
                    module_src.version()
                )
            }
            BuildPlanNode::BuildRuntimeObject(index) => {
                format!("build runtime object {index}")
            }
            BuildPlanNode::BuildRuntimeLib => "build runtime library".to_string(),
            BuildPlanNode::BuildVirtual(package_id) => {
                format!("build virtual {}", packages.fqn(*package_id))
            }
            BuildPlanNode::BuildDocs(module_id) => {
                let src = env.module_source(*module_id);
                format!("build docs {}", src)
            }
        };

        if self.extract_target().is_some() {
            description
        } else {
            format!("{description} ({backend})")
        }
    }

    /// Return a concise, human-readable identifier resolving PackageId/ModuleId to names.
    /// Single-line and stable; suitable for filenames/labels (e.g. n2 fileloc).
    pub fn string_id(&self, env: &ResolvedEnv, packages: &DiscoverResult) -> String {
        match self {
            BuildPlanNode::Check(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@Check", fqn, t.kind)
            }
            BuildPlanNode::EmitProof(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@EmitProof", fqn, t.kind)
            }
            BuildPlanNode::Prove(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@Prove", fqn, t.kind)
            }
            BuildPlanNode::BuildCore(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@BuildCore", fqn, t.kind)
            }
            BuildPlanNode::BuildCStub(pkg, idx) => {
                let fqn = packages.fqn(*pkg);
                format!("{}@BuildCStub_{}", fqn, idx)
            }
            BuildPlanNode::ArchiveOrLinkCStubs(pkg) => {
                let fqn = packages.fqn(*pkg);
                format!("{}@ArchiveCStubs", fqn)
            }
            BuildPlanNode::LinkCore(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@LinkCore", fqn, t.kind)
            }
            BuildPlanNode::MakeExecutable(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@MakeExecutable", fqn, t.kind)
            }
            BuildPlanNode::GenerateDsym(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@GenerateDsym", fqn, t.kind)
            }
            BuildPlanNode::GenerateTestInfo(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@GenerateTestInfo", fqn, t.kind)
            }
            BuildPlanNode::GenerateNodeTestPackageConfig(pkg) => {
                let fqn = packages.fqn(*pkg);
                format!("{}@GenerateNodeTestPackageConfig", fqn)
            }
            BuildPlanNode::GenerateMbti(t) => {
                let fqn = packages.fqn(t.package);
                format!("{}@{:?}@GenerateMbti", fqn, t.kind)
            }
            BuildPlanNode::Bundle(mid) => {
                let src = env.module_source(*mid);
                format!("{}@Bundle", src)
            }
            BuildPlanNode::BuildRuntimeObject(index) => {
                format!("BuildRuntimeObject_{index}")
            }
            BuildPlanNode::BuildRuntimeLib => "BuildRuntimeLib".to_string(),
            BuildPlanNode::BuildVirtual(pkg) => {
                let fqn = packages.fqn(*pkg);
                format!("{}@BuildVirtual", fqn)
            }
            BuildPlanNode::BuildDocs(module_id) => {
                let src = env.module_source(*module_id);
                format!("{}@BuildDocs", src)
            }
        }
    }
}

/// Supported operating systems for artifact generation
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OperatingSystem {
    Windows,
    Linux,
    MacOS,
    /// No operating system (e.g., WASM/JS targets)
    None,
}

impl std::fmt::Display for OperatingSystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            OperatingSystem::Windows => "windows",
            OperatingSystem::Linux => "linux",
            OperatingSystem::MacOS => "macos",
            OperatingSystem::None => "none",
        };
        write!(f, "{}", s)
    }
}

impl std::str::FromStr for OperatingSystem {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "windows" => Ok(OperatingSystem::Windows),
            "linux" => Ok(OperatingSystem::Linux),
            "macos" => Ok(OperatingSystem::MacOS),
            "none" => Ok(OperatingSystem::None),
            _ => Err(format!("Unsupported OS: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DirectNativeMode, NativeTarget};

    #[test]
    fn native_target_selection_is_host_specific() {
        assert_eq!(
            NativeTarget::from_host("aarch64", "macos"),
            Some(NativeTarget::Aarch64AppleDarwin)
        );
        assert_eq!(NativeTarget::from_host("x86_64", "macos"), None);
        assert_eq!(
            NativeTarget::from_host("x86_64", "linux"),
            Some(NativeTarget::X86_64UnknownLinuxGnu)
        );
        assert_eq!(
            NativeTarget::from_host("x86_64", "windows"),
            Some(NativeTarget::X86_64PcWindowsMsvc)
        );
        assert_eq!(NativeTarget::from_host("aarch64", "linux"), None);
    }

    #[test]
    fn new_native_defaults_on_for_apple_silicon_macos_and_x86_64_linux() {
        assert_eq!(
            NativeTarget::from_host_with_new_native_env("aarch64", "macos", None),
            Some(NativeTarget::Aarch64AppleDarwin)
        );
        assert_eq!(
            NativeTarget::from_host_with_new_native_env("aarch64", "macos", Some("1")),
            Some(NativeTarget::Aarch64AppleDarwin)
        );
        assert_eq!(
            NativeTarget::from_host_with_new_native_env("aarch64", "macos", Some("0")),
            None
        );
        assert_eq!(
            NativeTarget::from_host_with_new_native_env("aarch64", "macos", Some("other")),
            Some(NativeTarget::Aarch64AppleDarwin)
        );

        assert_eq!(
            NativeTarget::from_host_with_new_native_env("x86_64", "linux", None),
            Some(NativeTarget::X86_64UnknownLinuxGnu)
        );
        assert_eq!(
            NativeTarget::from_host_with_new_native_env("x86_64", "linux", Some("1")),
            Some(NativeTarget::X86_64UnknownLinuxGnu)
        );
        assert_eq!(
            NativeTarget::from_host_with_new_native_env("x86_64", "linux", Some("0")),
            None
        );
        assert_eq!(
            NativeTarget::from_host_with_new_native_env("x86_64", "linux", Some("other")),
            Some(NativeTarget::X86_64UnknownLinuxGnu)
        );
        assert_eq!(
            NativeTarget::from_host_with_new_native_env("x86_64", "windows", None),
            None
        );
        assert_eq!(
            NativeTarget::from_host_with_new_native_env("x86_64", "windows", Some("1")),
            Some(NativeTarget::X86_64PcWindowsMsvc)
        );
        assert_eq!(
            NativeTarget::from_host_with_new_native_env("x86_64", "windows", Some("other")),
            None
        );
        assert_eq!(
            NativeTarget::from_host_with_new_native_env("x86_64", "macos", Some("1")),
            None
        );
    }

    #[test]
    fn direct_native_mode_carries_only_target_choice() {
        let mode = DirectNativeMode::Target(NativeTarget::X86_64PcWindowsMsvc);

        assert_eq!(mode.target(), NativeTarget::X86_64PcWindowsMsvc);
    }
}
