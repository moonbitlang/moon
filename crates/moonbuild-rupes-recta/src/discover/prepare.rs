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

//! Build-specific interpretation and additional filesystem inputs. Formatting
//! stops at the shared scan and does not call this module.

use super::{DiscoverError, DiscoveredPackage, resolve_prebuild_path, scan::PackageInput};
use crate::special_cases::{CORE_MODULE_TUPLE, add_prelude_as_import_for_core};
use indexmap::IndexSet;
use moonutil::{
    constants::{
        MBTI_USER_WRITTEN, MOON_MOD, MOON_MOD_JSON, MOON_PKG, MOON_PKG_JSON, PackageSourceFileKind,
        is_ignored_directory_name, package_source_file_kind,
    },
    manifest::ManifestFormat,
    package::MoonPkg,
    resolution::{ModuleId, ModuleSourceKind},
    target::TargetBackend,
};
use relative_path::RelativePath;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[tracing::instrument(level = "debug", skip_all, fields(package = %input.fqn))]
pub(super) fn prepare_package(
    mid: ModuleId,
    input: PackageInput,
    module_supported_targets: &IndexSet<TargetBackend>,
) -> Result<DiscoveredPackage, DiscoverError> {
    let PackageInput {
        root_path,
        fqn,
        manifest_format,
        manifest,
        supported_targets_decl,
        files,
    } = input;
    let abs = root_path.as_path();
    let m = fqn.module();
    let pkg_is_stdlib = matches!(m.source(), ModuleSourceKind::Stdlib(_));
    let pkg_json = if *m.name() == CORE_MODULE_TUPLE {
        add_prelude_as_import_for_core(manifest)
    } else {
        manifest
    };
    let mut effective_supported_targets = pkg_json.supported_targets.clone();
    effective_supported_targets.retain(|target| module_supported_targets.contains(target));
    let mut source_files = Vec::new();
    let mut mbt_md_files = Vec::new();
    let mut mbtp_files = Vec::new();
    for path in files {
        let filename = path
            .file_name()
            .expect("scanned file has a name")
            .to_string_lossy();
        if filename.ends_with(".mbl") || filename.ends_with(".mby") {
            let declared = pkg_json.pre_build.iter().flatten().any(|rule| {
                rule.input()
                    .iter()
                    .chain(rule.output().iter())
                    .any(|declared| resolve_prebuild_path(abs, declared) == path)
            });
            if !declared {
                return Err(DiscoverError::UnsupportedLegacyGeneratedSource { file: path });
            }
        }
        match package_source_file_kind(&filename) {
            Some(PackageSourceFileKind::Mbt) => source_files.push(path),
            Some(PackageSourceFileKind::MbtMd) => mbt_md_files.push(path),
            Some(PackageSourceFileKind::Mbtp) => mbtp_files.push(path),
            None => (),
        }
    }

    // Read C stubs from package json
    let mut c_stubs = Vec::new();
    if let Some(stub_list) = &pkg_json.native_stub {
        for stub in stub_list {
            let (stub_path, escapes_package) = if cfg!(windows) {
                // Native joining handles both slash styles and dot components
                // against the canonical verbatim package root. Validate the
                // resolved location using the same semantics.
                let path = abs.join(stub);
                let escapes_package = !path.starts_with(abs);
                (path, escapes_package)
            } else {
                let rel_path = RelativePath::new(stub).normalize();
                (rel_path.to_path(abs), rel_path.starts_with(".."))
            };
            if escapes_package {
                return Err(DiscoverError::InvalidStubPath {
                    module: m.clone(),
                    package: fqn.package().clone(),
                    path: stub.clone(),
                    msg: "Path descends into parent directory",
                });
            }
            c_stubs.push(stub_path);
        }
    };
    let c_stub_headers = if c_stubs.is_empty() {
        Vec::new()
    } else {
        discover_c_stub_headers(abs).map_err(|inner| DiscoverError::CantListPackageDir {
            module: m.clone(),
            package: fqn.package().clone(),
            path: abs.to_owned(),
            inner,
        })?
    };

    let virtual_mbti_files = discover_virtual_mbti_files(&pkg_json, abs);
    let manifest_path = abs.join(match manifest_format {
        ManifestFormat::New => MOON_PKG,
        ManifestFormat::Legacy => MOON_PKG_JSON,
    });
    Ok(DiscoveredPackage {
        root_path,
        module: mid,
        fqn,
        single_file_source_kind: None,
        manifest_path: Some(manifest_path),
        raw: Box::new(pkg_json),
        supported_targets_decl,
        effective_supported_targets,
        source_files,
        mbt_md_files,
        mbtp_files,
        c_stub_files: c_stubs,
        c_stub_header_files: c_stub_headers,
        virtual_mbti_files,
        is_stdlib: pkg_is_stdlib,
    })
}

pub(super) fn discover_c_stub_headers(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut headers = Vec::new();
    let mut entries = WalkDir::new(root).sort_by_file_name().into_iter();
    while let Some(entry) = entries.next() {
        let entry = entry?;
        if entry.depth() != 0 && entry.file_type().is_dir() {
            if is_ignored_directory_name(entry.file_name()) {
                entries.skip_current_dir();
                continue;
            }

            let path = entry.path();
            if [MOON_MOD, MOON_MOD_JSON, MOON_PKG, MOON_PKG_JSON]
                .iter()
                .any(|manifest| path.join(manifest).exists())
            {
                entries.skip_current_dir();
                continue;
            }
        }

        if (entry.file_type().is_file() || entry.file_type().is_symlink())
            && entry.path().extension().is_some_and(|extension| {
                matches!(extension.to_str(), Some("h" | "hh" | "hpp" | "hxx"))
            })
        {
            headers.push(entry.into_path());
        }
    }
    headers.sort();
    Ok(headers)
}

pub(super) fn discover_virtual_mbti_files(pkg_json: &MoonPkg, abs: &Path) -> Vec<PathBuf> {
    if pkg_json.virtual_pkg.is_some() {
        [abs.join(MBTI_USER_WRITTEN)]
            .into_iter()
            .filter(|path| path.exists())
            .collect()
    } else {
        Vec::new()
    }
}
