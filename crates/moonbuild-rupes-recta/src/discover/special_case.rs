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

use crate::special_cases::CORE_MODULE_TUPLE;
use moonutil::mooncakes::{DirSyncResult, result::ResolvedEnv};
use relative_path::RelativePath;
use tracing::{info, instrument, warn};

use crate::{
    discover::{DiscoverError, DiscoverResult, discover_one_package},
    pkg_name::PackagePath,
};

/// Inject `moonbitlang/core/abort` to the package graph, so that user packages
/// can override it.
#[instrument(skip_all)]
pub fn inject_std_abort(
    env: &ResolvedEnv,
    dirs: &DirSyncResult,
    res: &mut DiscoverResult,
) -> Result<(), DiscoverError> {
    // Don't inject if there's no injected standard library
    let Some(stdlib) = env.stdlib() else {
        info!("No standard library injected, skipping abort injection");
        return Ok(());
    };

    let source = env.mod_name_from_id(stdlib);
    let path = dirs.get(stdlib).expect("stdlib directory non-existent");

    // Hardcoded package name and path: `abort`
    let abort_path = path.join("abort");
    let mut pkg = discover_one_package(
        stdlib,
        source,
        &abort_path,
        RelativePath::new("abort"),
        true,
        true,
    )?;

    // I know you have imports, but no, you don't.
    //
    // The imports of `abort` is fully encompassed in the bundled `moonbitlang/core`
    pkg.raw.imports.clear();
    pkg.raw.test_imports.clear();
    pkg.raw.wbtest_imports.clear();

    let abort_rel_pkg = PackagePath::new("abort").expect("abort is a valid name");

    let id = res.add_package(stdlib, abort_rel_pkg, pkg)?;
    res.set_abort_pkg(id);

    Ok(())
}

/// Inject `moonbitlang/core/coverage` contents into `moonbitlang/core/builtin`
/// so builtin is effectively augmented with coverage sources during discovery.
///
/// This mirrors legacy behavior that bundled coverage into builtin, ensuring
/// downstream compilation/linking sees coverage alongside builtin without
/// additional per-target import wiring.
#[instrument(skip_all)]
pub fn inject_core_coverage_into_builtin(
    env: &ResolvedEnv,
    res: &mut DiscoverResult,
) -> Result<(), DiscoverError> {
    // Only proceed if we have a stdlib module locally
    let Some(&stdlib) = env
        .input_module_ids()
        .iter()
        .find(|id| *env.mod_name_from_id(**id).name() == CORE_MODULE_TUPLE)
    else {
        info!(
            "No standard library injected and no local core module found, skipping coverage->builtin injection"
        );
        return Ok(());
    };

    // Resolve coverage and builtin package ids within stdlib module
    let Some(map) = res.packages_for_module(stdlib) else {
        // No packages for stdlib; nothing to do
        return Ok(());
    };

    let builtin_path = PackagePath::new("builtin").expect("builtin is a valid package path");
    let coverage_path = PackagePath::new("coverage").expect("coverage is a valid package path");

    let Some(&builtin_id) = map.get(&builtin_path) else {
        warn!("No builtin package found in core module, skipping coverage->builtin injection");
        return Ok(());
    };
    let Some(&coverage_id) = map.get(&coverage_path) else {
        warn!("No coverage package found in core module, skipping coverage->builtin injection");
        return Ok(());
    };

    // Clone coverage source files into builtin
    let coverage_files = res.get_package(coverage_id).source_files.clone();
    let builtin = res.get_package_mut(builtin_id);

    // Merge .mbt source files
    builtin.source_files.extend(coverage_files);
    builtin.source_files.sort();

    Ok(())
}
