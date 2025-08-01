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

use std::path::{Path, PathBuf};

use moonutil::{common::TargetBackend, mooncakes::ModuleSource};

use crate::{
    discover::DiscoverResult,
    model::{BuildTarget, TargetKind},
    pkg_name::PackageFQN,
};

/// The extension of the intermediate representation emitted by the Build action
const CORE_EXTENSION: &str = ".core";
/// The extension of the package public interface file emitted by Check and Build
const MI_EXTENSION: &str = ".mi";

/// Target folder layout that matches the legacy (pre-beta) behavior
pub struct LegacyLayout {
    /// The base target directory, usually `<project-root>/target`
    target_base_dir: PathBuf,
    /// The name of the main module, so that packages from the main module will
    /// not be put into nested directories.
    main_module: Option<ModuleSource>,
}

const LEGACY_NON_MAIN_MODULE_DIR: &str = ".mooncakes";

impl LegacyLayout {
    /// Creates a new legacy layout instance.
    pub fn new(target_base_dir: PathBuf, main_module: Option<ModuleSource>) -> Self {
        Self {
            target_base_dir,
            main_module,
        }
    }

    /// Returns the directory the given package resides in.
    ///
    /// For modules determined as the "main module", this path is
    /// `target/<backend>/<...package>/`. Otherwise, it's
    /// `target/<backend>/.mooncakes/<...module>/<...package>`.
    pub fn package_dir(&self, pkg: &PackageFQN, backend: TargetBackend) -> PathBuf {
        let mut dir = self.target_base_dir.clone();
        push_backend(&mut dir, backend);

        if self.main_module.as_ref().is_some_and(|m| pkg.module() == m) {
            // no nested directory for the working module
        } else {
            dir.push(LEGACY_NON_MAIN_MODULE_DIR);
            dir.extend(pkg.module().name().segments());
        }
        dir.extend(pkg.package().segments());

        dir
    }

    fn pkg_core_basename(&self, pkg: &PackageFQN, kind: TargetKind) -> String {
        format!(
            "{}{}{}",
            pkg.short_alias(),
            build_kind_suffix(kind),
            CORE_EXTENSION
        )
    }

    pub fn core_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(self.pkg_core_basename(pkg_fqn, target.kind));
        base_dir
    }

    fn pkg_mi_basename(&self, pkg: &PackageFQN, kind: TargetKind) -> String {
        format!(
            "{}{}{}",
            pkg.short_alias(),
            build_kind_suffix(kind),
            MI_EXTENSION
        )
    }

    pub fn mi_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(self.pkg_mi_basename(pkg_fqn, target.kind));
        base_dir
    }

    pub fn linked_core_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
        os: &str,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(self.pkg_linked_core_artifact_basename(pkg_fqn, backend, os));
        base_dir
    }

    #[allow(unused)]
    pub fn executable_of_build_target(
        &self,
        pkg_list: &DiscoverResult,
        target: &BuildTarget,
        backend: TargetBackend,
        os: &str,
        legacy_behavior: bool,
    ) -> PathBuf {
        let pkg_fqn = &pkg_list.get_package(target.package).fqn;
        let mut base_dir = self.package_dir(pkg_fqn, backend);
        base_dir.push(self.pkg_executable_artifact_basename(pkg_fqn, backend, os, legacy_behavior));
        base_dir
    }

    #[allow(unused)]
    fn pkg_linked_core_artifact_basename(
        &self,
        pkg: &PackageFQN,
        backend: TargetBackend,
        os: &str, // FIXME: is using string a good idea?
    ) -> String {
        format!(
            "{}{}",
            pkg.short_alias(),
            linked_core_artifact_ext(backend, os)
        )
    }

    fn pkg_executable_artifact_basename(
        &self,
        pkg: &PackageFQN,
        backend: TargetBackend,
        os: &str, // FIXME: is using string a good idea?
        legacy_behavior: bool,
    ) -> String {
        format!(
            "{}{}",
            pkg.short_alias(),
            make_executable_artifact_ext(backend, os, legacy_behavior)
        )
    }
}

fn push_backend(path: &mut PathBuf, backend: TargetBackend) {
    path.push(backend.to_dir_name())
}

fn build_kind_suffix(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Source => "",
        TargetKind::WhiteboxTest => "_whitebox_test",
        TargetKind::BlackboxTest => "_blackbox_test",
        TargetKind::InlineTest => "_inline_test",
        TargetKind::SubPackage => "_sub",
    }
}

fn linked_core_artifact_ext(backend: TargetBackend, os: &str) -> &'static str {
    match backend {
        TargetBackend::Wasm | TargetBackend::WasmGC => ".wasm",
        TargetBackend::Js => ".js",
        TargetBackend::Native => ".c",
        TargetBackend::LLVM => object_file_ext(os),
    }
}

fn make_executable_artifact_ext(
    backend: TargetBackend,
    os: &str,
    legacy_behavior: bool,
) -> &'static str {
    match backend {
        TargetBackend::Wasm | TargetBackend::WasmGC => ".wasm",
        TargetBackend::Js => ".js",
        TargetBackend::Native | TargetBackend::LLVM => executable_ext(os, legacy_behavior),
    }
}

/// The extension for executables. The legacy behavior forces everything into an `.exe`.
fn executable_ext(os: &str, legacy_behavior: bool) -> &'static str {
    if legacy_behavior {
        ".exe"
    } else {
        match os {
            "windows" => ".exe",
            "linux" | "macos" => "",
            _ => panic!("Unsupported OS {os}"),
        }
    }
}

/// Returns the file extension for static libraries on the given OS
#[allow(unused)]
fn static_library_ext(os: &str) -> &'static str {
    match os {
        "windows" => ".lib",
        "linux" | "macos" => ".a",
        _ => panic!("Unsupported OS {os}"),
    }
}

/// Returns the file extension for dynamic libraries on the given OS
#[allow(unused)]
fn dynamic_library_ext(os: &str) -> &'static str {
    match os {
        "windows" => ".dll",
        "linux" => ".so",
        "macos" => ".dylib",
        _ => panic!("Unsupported OS {os}"),
    }
}

/// Returns the file extension for object files on the given OS
fn object_file_ext(os: &str) -> &'static str {
    match os {
        "windows" => ".obj",
        "linux" | "macos" => ".o",
        _ => panic!("Unsupported OS {os}"),
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

/// Returns the path to core.core for the given backend.
pub fn core_core_path(core_root: &Path, backend: TargetBackend) -> PathBuf {
    let mut path = core_bundle_path(core_root, backend);
    path.push("core.core");
    path
}
