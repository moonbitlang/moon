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

//! Build artifact path calculation and relevant information

use std::{
    ffi::OsStr,
    fmt::Display,
    path::{Path, PathBuf},
};

use derive_builder::Builder;
use moonutil::{
    common::{MBTI_GENERATED, RunMode, TargetBackend},
    cond_expr::OptLevel,
    mooncakes::{ModuleName, ModuleSource},
};

use crate::{
    discover::DiscoverResult,
    model::{BuildTarget, OperatingSystem, PackageId, TargetKind},
    pkg_name::PackageFQN,
};

/// The extension of the intermediate representation emitted by the Build action
const CORE_EXTENSION: &str = ".core";
/// The extension of the package public interface file emitted by Check and Build
const MI_EXTENSION: &str = ".mi";

/// Target folder layout that matches the legacy (pre-beta) behavior
#[derive(Builder)]
pub struct LegacyLayout {
    /// The base target directory, usually `<project-root>/target`
    target_base_dir: PathBuf,
    /// The name of the main module, so that packages from the main module will
    /// not be put into nested directories.
    main_module: Option<ModuleSource>,

    /// The directory of the standard library
    stdlib_dir: Option<PathBuf>,

    /// The optimization level, debug or release
    opt_level: OptLevel,
    /// The operation done
    run_mode: RunMode,
}

const LEGACY_NON_MAIN_MODULE_DIR: &str = ".mooncakes";

/// A common structure for generating artifact basenames of packages.
///
/// We need to disambiguate between different kinds of output, so each artifact
/// will have a different suffix.
///
/// Note that this is different from [`super::compiler::CompiledPackageName`],
/// which represents the full package name passed to the compiler.
#[derive(Clone, Debug)]
struct PackageArtifactName<'a> {
    pub fqn: &'a PackageFQN,
    pub kind: TargetKind,
}

fn artifact(fqn: &'_ PackageFQN, kind: TargetKind) -> PackageArtifactName<'_> {
    PackageArtifactName { fqn, kind }
}

impl Display for PackageArtifactName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}",
            self.fqn.short_alias(),
            build_kind_suffix(self.kind)
        )
    }
}

impl LegacyLayout {
    /// Returns the directory the given package resides in.
    ///
    /// For modules determined as the "main module", this path is
    /// `target/<backend>[/<opt_level>/build]/<...package>/`. Otherwise, it's
    /// `target/<backend>[/<opt_level>/build]/.mooncakes/<...module>/<...package>`.
    pub fn package_dir(&self, pkg: &PackageFQN, backend: TargetBackend) -> PathBuf {
        let mut dir = self.target_base_dir.clone();
        push_backend(&mut dir, backend);

        match self.opt_level {
            OptLevel::Release => dir.push("release"),
            OptLevel::Debug => dir.push("debug"),
        }
        dir.push(self.run_mode.to_dir_name());

        if self.main_module.as_ref().is_some_and(|m| pkg.module() == m) {
            // no nested directory for the working module
        } else {
            dir.push(LEGACY_NON_MAIN_MODULE_DIR);
            dir.extend(pkg.module().name().segments());
        }
        dir.extend(pkg.package().segments());

        dir
    }

    fn push_package_dir_no_backend(&self, dir: &mut PathBuf, pkg: &PackageFQN) {
        if self.main_module.as_ref().is_some_and(|m| pkg.module() == m) {
            // no nested directory for the working module
        } else {
            dir.push(LEGACY_NON_MAIN_MODULE_DIR);
            dir.extend(pkg.module().name().segments());
        }
        dir.extend(pkg.package().segments());
    }

    pub fn core_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        // Special case: `abort` lives in core
        if let Some(abort) = pkg_list.abort_pkg()
            && abort == target.package
        {
            if target.kind == TargetKind::Source {
                return abort_core_path(
                    self.stdlib_dir
                        .as_ref()
                        .expect("Standard library should be present"),
                    backend,
                );
            } else {
                panic!("Cannot import `.mi` for moonbitlang/core/abort");
            }
        }

        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(format!(
            "{}{}",
            artifact(pkg_fqn, target.kind),
            CORE_EXTENSION
        ));
        base_dir
    }

    pub fn mi_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        // Special case: `abort` lives in core
        if let Some(abort) = pkg_list.abort_pkg()
            && abort == target.package
        {
            if target.kind == TargetKind::Source {
                return abort_mi_path(
                    self.stdlib_dir
                        .as_ref()
                        .expect("Standard library should be present"),
                    backend,
                );
            } else {
                panic!("Cannot import `.mi` for moonbitlang/core/abort");
            }
        }

        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(format!(
            "{}{}",
            artifact(pkg_fqn, target.kind),
            MI_EXTENSION
        ));
        base_dir
    }

    pub fn linked_core_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
        os: OperatingSystem,
        wasm_use_wat: bool,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(format!(
            "{}{}",
            artifact(pkg_fqn, target.kind),
            linked_core_artifact_ext(backend, os, wasm_use_wat)
        ));
        base_dir
    }

    #[allow(unused)]
    pub fn executable_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
        os: OperatingSystem,
        legacy_behavior: bool,
        wasm_use_wat: bool,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(format!(
            "{}{}",
            artifact(pkg_fqn, target.kind),
            make_executable_artifact_ext(backend, os, legacy_behavior, wasm_use_wat),
        ));
        base_dir
    }

    pub fn generated_test_driver(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(format!(
            "{}__generated_driver_for{}.mbt",
            pkg_fqn.short_alias(),
            build_kind_suffix(target.kind)
        ));
        base_dir
    }

    pub fn generated_test_driver_metadata(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(format!(
            "__{}_test_info.json",
            build_kind_suffix(target.kind)
        ));
        base_dir
    }

    pub fn bundle_result_path(&self, backend: TargetBackend, module: &ModuleName) -> PathBuf {
        let mut result = self.target_base_dir.clone();
        push_backend(&mut result, backend);
        result.push(format!("{}.core", module.last_segment()));
        result
    }

    pub fn runtime_output_path(&self, backend: TargetBackend, os: OperatingSystem) -> PathBuf {
        let mut result = self.target_base_dir.clone();
        push_backend(&mut result, backend);
        result.push(format!("runtime{}", object_file_ext(os)));
        result
    }

    /// The *artifact* of the format operation.
    ///
    /// At the time of writing, it should only be used as a stamp file to
    /// indicate that formatting has been done. However, due to the way
    /// `moonfmt` works, it actually produces a formatted copy of the input file
    /// at this path.
    pub fn format_artifact_path(&self, pkg: &PackageFQN, filename: &OsStr) -> PathBuf {
        let mut result = self.package_dir(pkg, TargetBackend::WasmGC);
        result.push(filename);
        result
    }

    pub fn mi_of_pkg_without_backend(&self, pkg: &PackageFQN) -> PathBuf {
        let mut base_dir = PathBuf::new();
        self.push_package_dir_no_backend(&mut base_dir, pkg);
        base_dir.push(format!("{}{}", pkg.short_alias(), MI_EXTENSION));
        base_dir
    }

    pub fn generated_mbti_path(&self, pkg_source: &Path) -> PathBuf {
        pkg_source.join(MBTI_GENERATED)
    }

    /// Returns the path for a C stub object file.
    ///
    /// Format: `target/{backend}/{opt_level}/build/{package_path}/{stub_name}.o`
    pub fn c_stub_object_path(
        &self,
        pkg_list: &DiscoverResult,
        package: PackageId,
        stub_name: &OsStr,
        backend: TargetBackend,
        os: OperatingSystem,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        let mut stub_name = stub_name.to_os_string();
        stub_name.push(object_file_ext(os));
        base_dir.push(stub_name);
        base_dir
    }

    /// Returns the path for a C stub static library archive.
    ///
    /// Format: `target/{backend}/{opt_level}/build/{package_path}/lib{package_name}.a`
    pub fn c_stub_archive_path(
        &self,
        pkg_list: &DiscoverResult,
        package: PackageId,
        backend: TargetBackend,
        os: OperatingSystem,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(format!(
            "lib{}{}",
            pkg_fqn.short_alias(),
            static_library_ext(os)
        ));
        base_dir
    }

    /// Returns the directory for outputting documentation.
    ///
    /// Format: `target/doc`
    pub fn doc_dir(&self) -> PathBuf {
        let mut dir = self.target_base_dir.clone();
        dir.push("doc");
        dir
    }

    /// Returns the path of `package.json`, the metadata file to be read by
    /// IDE plugins and other tools.
    pub fn packages_json_path(&self) -> PathBuf {
        let mut path = self.target_base_dir.clone();
        path.push("packages.json");
        path
    }
}

fn push_backend(path: &mut PathBuf, backend: TargetBackend) {
    path.push(backend.to_dir_name())
}

fn build_kind_suffix(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Source => "",
        TargetKind::WhiteboxTest => ".whitebox_test",
        TargetKind::BlackboxTest => ".blackbox_test",
        TargetKind::InlineTest => ".inline_test",
        TargetKind::SubPackage => "_sub",
    }
}

fn linked_core_artifact_ext(
    backend: TargetBackend,
    os: OperatingSystem,
    wasm_use_wat: bool, // TODO: centralize knobs
) -> &'static str {
    match backend {
        TargetBackend::Wasm | TargetBackend::WasmGC if wasm_use_wat => ".wat",
        TargetBackend::Wasm | TargetBackend::WasmGC => ".wasm",
        TargetBackend::Js => ".js",
        TargetBackend::Native => ".c",
        TargetBackend::LLVM => object_file_ext(os),
    }
}

fn make_executable_artifact_ext(
    backend: TargetBackend,
    os: OperatingSystem,
    legacy_behavior: bool,
    wasm_use_wat: bool,
) -> &'static str {
    match backend {
        TargetBackend::Wasm | TargetBackend::WasmGC if wasm_use_wat => ".wat",
        TargetBackend::Wasm | TargetBackend::WasmGC => ".wasm",
        TargetBackend::Js => ".js",
        TargetBackend::Native | TargetBackend::LLVM => executable_ext(os, legacy_behavior),
    }
}

/// The extension for executables. The legacy behavior forces everything into an `.exe`.
fn executable_ext(os: OperatingSystem, legacy_behavior: bool) -> &'static str {
    if legacy_behavior {
        ".exe"
    } else {
        match os {
            OperatingSystem::Windows => ".exe",
            OperatingSystem::Linux | OperatingSystem::MacOS => "",
            OperatingSystem::None => panic!("No executable extension for no-OS targets"),
        }
    }
}

/// Returns the file extension for static libraries on the given OS
fn static_library_ext(os: OperatingSystem) -> &'static str {
    match os {
        OperatingSystem::Windows => ".lib",
        OperatingSystem::Linux | OperatingSystem::MacOS => ".a",
        OperatingSystem::None => panic!("No static library extension for no-OS targets"),
    }
}

/// Returns the file extension for dynamic libraries on the given OS
#[allow(unused)]
fn dynamic_library_ext(os: OperatingSystem) -> &'static str {
    match os {
        OperatingSystem::Windows => ".dll",
        OperatingSystem::Linux => ".so",
        OperatingSystem::MacOS => ".dylib",
        OperatingSystem::None => panic!("No dynamic library extension for no-OS targets"),
    }
}

/// Returns the file extension for object files on the given OS
fn object_file_ext(os: OperatingSystem) -> &'static str {
    match os {
        OperatingSystem::Windows => ".obj",
        OperatingSystem::Linux | OperatingSystem::MacOS => ".o",
        OperatingSystem::None => panic!("No object file extension for no-OS targets"),
    }
}

/// Get the bundled core bundle path for the given backend.
///
/// This is a recreation of [`moonutil::moon_dir::core`], which we hope will be
/// removed in the future.
pub fn core_bundle_path(core_root: &Path, backend: TargetBackend) -> PathBuf {
    let mut path = PathBuf::from(core_root);
    path.push("target");
    path.push(backend.to_dir_name());
    path.push("release");
    path.push("bundle");
    path
}

/// Returns the path to abort.core for the given backend.
pub fn abort_core_path(core_root: &Path, backend: TargetBackend) -> PathBuf {
    let mut path = core_bundle_path(core_root, backend);
    path.push("abort");
    path.push("abort.core");
    path
}

pub fn abort_mi_path(core_root: &Path, backend: TargetBackend) -> PathBuf {
    let mut path = core_bundle_path(core_root, backend);
    path.push("abort");
    path.push("abort.mi");
    path
}

/// Returns the path to core.core for the given backend.
pub fn core_core_path(core_root: &Path, backend: TargetBackend) -> PathBuf {
    let mut path = core_bundle_path(core_root, backend);
    path.push("core.core");
    path
}
