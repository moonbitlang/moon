//! Build artifact path calculation and relevant information

use std::path::PathBuf;

use moonutil::{common::TargetBackend, mooncakes::ModuleSource};

use crate::{model::TargetKind, pkg_name::PackageFQN};

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
    main_module: ModuleSource,
}

const LEGACY_NON_MAIN_MODULE_DIR: &str = ".mooncakes";

impl LegacyLayout {
    /// Creates a new legacy layout instance.
    pub fn new(target_base_dir: PathBuf, main_module: ModuleSource) -> Self {
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

        if pkg.module() == &self.main_module {
            dir.extend(pkg.package().segments());
        } else {
            dir.push(LEGACY_NON_MAIN_MODULE_DIR);
            dir.extend(pkg.module().name().segments());
        }
        dir.extend(pkg.package().segments());

        dir
    }

    pub fn pkg_core_basename(&self, pkg: &PackageFQN, kind: TargetKind) -> String {
        format!(
            "{}{}{}",
            pkg.short_alias(),
            build_kind_suffix(kind),
            CORE_EXTENSION
        )
    }

    pub fn pkg_mi_basename(&self, pkg: &PackageFQN, kind: TargetKind) -> String {
        format!(
            "{}{}{}",
            pkg.short_alias(),
            build_kind_suffix(kind),
            MI_EXTENSION
        )
    }

    pub fn pkg_linked_core_artifact_basename(
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

    pub fn pkg_executable_artifact_basename(
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
