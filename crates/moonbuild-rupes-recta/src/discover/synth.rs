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

//! Synthetic single-file project discovery
//!
//! Implements the "import everything from resolved modules" behavior for single-file
//! scenarios by synthesizing a package `single` under the local single-file module.
//!
//! The synthetic package's `MoonPkg` either:
//! - Uses package imports declared in single-file front matter, or
//! - Imports all discovered packages (excluding internal-only packages).
//!
//! This mirrors the legacy path in `moon/src/cli/test.rs:get_module_for_single_file`
//! where it programmatically enumerates imports for the synthetic single-file package.

use std::path::Path;

use indexmap::IndexSet;
use moonutil::common::TargetBackend;
use moonutil::mooncakes::result::ResolvedEnv;
use moonutil::package::{Import, MoonPkg, MoonPkgFormatter};

use crate::discover::{DiscoverError, DiscoverResult, DiscoveredPackage};
use crate::model::PackageId;
use crate::pkg_name::{PackageFQN, PackagePath};

/// Build and insert a synthetic single-file package into the discovery result,
/// returning the newly created package ID.
///
/// The synthetic package will be named `single` and will either:
/// - Use explicit front matter package imports, or
/// - Import all discovered packages (excluding internal-only packages).
///
/// If `run_mode` is true, the package will be marked as a main package.
pub fn build_synth_single_file_package(
    file: &Path,
    env: &ResolvedEnv,
    discovered: &mut DiscoverResult,
    run_mode: bool,
    front_matter_imports: Option<Vec<Import>>,
) -> Result<PackageId, DiscoverError> {
    // Expect exactly one local module for single-file synth
    let &[mid] = env.input_module_ids() else {
        panic!("No multiple main modules are supported")
    };

    // Resolve ModuleSource for the ModuleId
    let module_src = env
        .all_modules_and_id()
        .find(|(id, _)| *id == mid)
        .map(|(_, src)| src.clone())
        .expect("Cannot find module source for single-file module");

    // Synthetic package path: "single"
    let pkg_path = PackagePath::new("single").expect("synthetic package path should be valid");

    let imports = if let Some(imports) = front_matter_imports {
        for import in &imports {
            let path = import.get_path();
            if discovered.get_package_id_by_name(path).is_none() {
                return Err(DiscoverError::SingleFileImportNotFound {
                    import: path.to_string(),
                });
            }
        }
        imports
    } else {
        // Build import-all list (excluding std abort)
        let mut imports = Vec::new();
        for (_pid, pkg) in discovered.all_packages(false) {
            if pkg.fqn.has_internal_segment() {
                continue;
            }
            let fqn_str = pkg.fqn.to_string();

            // Check if this is an immut package that would conflict with mutable counterpart
            // e.g., moonbitlang/core/immut/array conflicts with moonbitlang/core/array
            let custom_alias = get_immut_alias(&pkg.fqn);
            if let Some(alias) = custom_alias {
                imports.push(Import::Alias {
                    path: fqn_str,
                    alias: Some(alias),
                    sub_package: false,
                });
            } else {
                imports.push(Import::Simple(fqn_str));
            }
        }
        imports
    };

    // Construct MoonPkg for synthetic package
    let mut supported = IndexSet::new();
    for &b in TargetBackend::all() {
        supported.insert(b);
    }
    let moon_pkg = MoonPkg {
        name: None,
        is_main: run_mode,
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
        alert_list: None,
        targets: None,
        pre_build: None,
        bin_name: None,
        bin_target: None,
        supported_targets: supported,
        native_stub: None,
        virtual_pkg: None,
        implement: None,
        overrides: None,
        max_concurrent_tests: None,
        regex_backend: None,
    };

    // Assign file to appropriate list
    let file_path = dunce::canonicalize(file).expect("Failed to canonicalize single-file input");
    let files = vec![file_path.clone()];
    let (source_files, mbt_md_files) = if file_path.extension().is_some_and(|x| x == "md") {
        (Vec::new(), files)
    } else {
        (files, Vec::new())
    };

    // Build DiscoveredPackage for synthetic package
    let synth_pkg = DiscoveredPackage {
        root_path: file_path
            .parent()
            .expect("single file must have a parent directory")
            .to_path_buf(),
        module: mid,
        fqn: PackageFQN::new(module_src, pkg_path.clone()),
        is_single_file: true,
        raw: Box::new(moon_pkg),
        source_files,
        mbt_lex_files: Vec::new(),
        mbt_yacc_files: Vec::new(),
        mbt_md_files,
        c_stub_files: Vec::new(),
        virtual_mbti: None,
        is_stdlib: false,
    };

    // Insert and return the new package ID
    discovered.add_package(mid, pkg_path, synth_pkg)
}

/// Returns a custom alias for packages under `moonbitlang/core/immut/*` to avoid
/// conflicts with their mutable counterparts (e.g., `moonbitlang/core/array`).
///
/// For example:
/// - `moonbitlang/core/immut/array` -> `immut/array`
/// - `moonbitlang/core/immut/hashmap` -> `immut/hashmap`
///
/// HACK: This is a temporary workaround for alias conflicts in single-file mode.
/// A proper solution should systematically handle alias conflicts, potentially by:
/// - Implementing general alias conflict resolution during package solving
/// - Allowing users to specify package-level imports in front matter
/// - Parsing source files to detect which `@package` references are actually used
fn get_immut_alias(fqn: &PackageFQN) -> Option<String> {
    const IMMUT_PREFIX: &str = "immut/";

    let pkg_path = fqn.package().as_str();

    // Check if package path starts with "immut/"
    if !pkg_path.starts_with(IMMUT_PREFIX) {
        return None;
    }

    // Only apply to moonbitlang/core module
    let module_name = fqn.module().name();
    if module_name.username != "moonbitlang" || module_name.unqual != "core" {
        return None;
    }

    // Return alias like "immut/array" for "moonbitlang/core/immut/array"
    Some(pkg_path.to_string())
}
