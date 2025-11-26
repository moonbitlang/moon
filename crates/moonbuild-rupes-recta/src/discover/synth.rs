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
//! The synthetic package's `MoonPkg` imports all discovered packages (excluding the
//! std abort) so that the single-file can reference any package symbols without
//! declaring per-package deps in the front matter.
//!
//! This mirrors the legacy path in `moon/src/cli/test.rs:get_module_for_single_file`
//! where it programmatically enumerates imports for the synthetic single-file package.

use std::collections::HashSet;
use std::path::Path;

use moonutil::common::TargetBackend;
use moonutil::mooncakes::result::ResolvedEnv;
use moonutil::package::{Import, MoonPkg, MoonPkgFormatter};

use crate::discover::{DiscoverResult, DiscoveredPackage};
use crate::model::PackageId;
use crate::pkg_name::{PackageFQN, PackagePath};

/// Build and insert a synthetic single-file package into the discovery result,
/// returning the newly created package ID.
///
/// The synthetic package will be named `single` and will import all discovered
/// packages (excluding the std abort) so that the single file can reference any
/// package symbols without declaring per-package deps in the front matter.
pub fn build_synth_single_file_package(
    file: &Path,
    env: &ResolvedEnv,
    discovered: &mut DiscoverResult,
) -> PackageId {
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

    // Build import-all list (excluding std abort)
    let abort_pkg = discovered.abort_pkg();
    let mut imports = Vec::new();
    for (pid, pkg) in discovered.all_packages() {
        if Some(pid) == abort_pkg {
            continue;
        }
        if pkg.fqn.has_internal_segment() {
            continue;
        }
        let fqn_str = pkg.fqn.to_string();
        imports.push(Import::Simple(fqn_str));
    }

    // Construct MoonPkg for synthetic package
    let mut supported = HashSet::new();
    for &b in TargetBackend::all() {
        supported.insert(b);
    }
    let moon_pkg = MoonPkg {
        name: None,
        is_main: false,
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
    };

    // Insert and return the new package ID
    discovered.add_package(mid, pkg_path, synth_pkg)
}
