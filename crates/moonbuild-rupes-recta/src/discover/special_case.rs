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

use moonutil::mooncakes::{result::ResolvedEnv, DirSyncResult};
use relative_path::RelativePath;
use tracing::instrument;

use crate::{
    discover::{discover_one_package, DiscoverError, DiscoverResult},
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
    )?;

    // I know you have imports, but no, you don't.
    //
    // The imports of `abort` is fully encompassed in the bundled `moonbitlang/core`
    pkg.raw.imports.clear();
    pkg.raw.test_imports.clear();
    pkg.raw.wbtest_imports.clear();

    let abort_rel_pkg = PackagePath::new("abort").expect("abort is a valid name");

    let id = res.add_package(stdlib, abort_rel_pkg, pkg);
    res.abort_pkg = Some(id);

    Ok(())
}
