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

//! Synthetic single-file project discovery
//!
//! Synthesizes a package `single` under the local single-file module.
//!
//! Scripts are executable; Markdown contributes blackbox tests to a library
//! package. Both formats carry their already resolved explicit imports.

use std::{path::Path, sync::Arc};

use indexmap::IndexSet;
use moonutil::package::{Import, MoonPkg, MoonPkgFormatter, SupportedTargetsDeclKind};
use moonutil::resolution::ModuleDependencyGraph;
use moonutil::target::TargetBackend;

use crate::discover::{DiscoverError, DiscoverResult, DiscoveredPackage, SingleFileSourceKind};
use crate::model::PackageId;
use crate::pkg_name::{PackageFQN, PackagePath};

/// Build and insert a synthetic single-file package into the discovery result,
/// returning the newly created package ID.
///
/// The synthetic package is named `single` and uses explicit package imports.
/// An empty import list is valid. Only `.mbtx` packages require a main function.
pub fn build_synth_single_file_package(
    file: &Path,
    source_kind: SingleFileSourceKind,
    env: &ModuleDependencyGraph,
    discovered: &mut DiscoverResult,
    imports: Vec<Import>,
) -> Result<PackageId, DiscoverError> {
    // Expect exactly one local module for single-file synth
    let &[mid] = env.input_module_ids() else {
        panic!("No multiple main modules are supported")
    };
    discovered.set_module_info(mid, Arc::clone(env.module_info(mid)));

    // Resolve ModuleSource for the ModuleId
    let module_src = env
        .all_modules_and_id()
        .find(|(id, _)| *id == mid)
        .map(|(_, src)| src.clone())
        .expect("Cannot find module source for single-file module");

    // Synthetic package path: "single"
    let pkg_path = PackagePath::new("single").expect("synthetic package path should be valid");

    for import in &imports {
        let path = import.get_path();
        if discovered.get_package_id_by_name(path).is_none() {
            return Err(DiscoverError::SingleFileImportNotFound {
                import: path.to_string(),
            });
        }
    }

    // Construct MoonPkg for synthetic package
    let mut supported = IndexSet::new();
    for &b in TargetBackend::all() {
        supported.insert(b);
    }
    let moon_pkg = MoonPkg {
        name: None,
        experimental: false,
        allow_experimental_packages: None,
        is_main: source_kind == SingleFileSourceKind::Mbtx,
        force_link: false,
        sub_package: None,
        imports,
        wbtest_imports: Vec::new(),
        test_imports: Vec::new(),
        formatter: MoonPkgFormatter {
            ignore: Default::default(),
        },
        link: None,
        warn_list: None,
        proof_enabled: false,
        use_async_io: false,
        targets: None,
        pre_build: None,
        bin_name: None,
        bin_target: None,
        supported_targets: supported.clone(),
        native_stub: None,
        virtual_pkg: None,
        implement: None,
        overrides: None,
        max_concurrent_tests: None,
        regex_backend: None,
        local_rules: None,
    };

    // Assign file to appropriate list
    // Keep the invoked filename through compiler and test-file classification.
    // Following a renamed symlink here would change the source interpretation.
    let file_path = file.to_path_buf();
    // Build DiscoveredPackage for synthetic package
    let synth_pkg = DiscoveredPackage {
        root_path: file_path
            .parent()
            .expect("single file must have a parent directory")
            .to_path_buf(),
        module: mid,
        fqn: PackageFQN::new(module_src, pkg_path.clone()),
        single_file_source_kind: Some(source_kind),
        manifest_path: None,
        raw: Box::new(moon_pkg),
        supported_targets_decl: SupportedTargetsDeclKind::Omitted,
        effective_supported_targets: supported,
        source_files: if source_kind == SingleFileSourceKind::Mbtx {
            vec![file_path.clone()]
        } else {
            Vec::new()
        },
        mbt_md_files: if source_kind == SingleFileSourceKind::MbtMd {
            vec![file_path]
        } else {
            Vec::new()
        },
        mbtp_files: Vec::new(),
        c_stub_files: Vec::new(),
        c_stub_header_files: Vec::new(),
        virtual_mbti_files: Vec::new(),
        is_stdlib: false,
    };

    // Insert and return the new package ID
    discovered.add_package(mid, pkg_path, synth_pkg)
}
