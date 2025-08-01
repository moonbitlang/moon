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

use std::collections::HashMap;

use log::{debug, trace, warn};
use moonutil::mooncakes::{result::ResolvedEnv, ModuleId};

use super::model::{DepEdge, DepRelationship, SolveError};
use crate::{
    discover::DiscoverResult,
    model::{PackageId, TargetKind},
    pkg_name::format_package_fqn,
};

type RevMap = HashMap<String, (ModuleId, PackageId)>;

pub fn solve_only(
    modules: &ResolvedEnv,
    packages: &DiscoverResult,
) -> Result<DepRelationship, SolveError> {
    debug!(
        "Building dependency resolution structures for {} packages",
        packages.package_count()
    );
    let mut res = DepRelationship::default();

    // To convert the Package FQNs within `moon.pkg.json` into actual resolved
    // package instances, we will need to construct a reversed mapping from
    // FQNs we will see in the import list to that of actual packages in scope.
    //
    // `moonc` currently cannot disambiguate between packages of the same FQN.
    // Thus, this mapping needs to be constructed globally. (And thankfully we
    // don't have module-level dependency renaming just yet, or we will need to
    // do this twice.)
    //
    // This reverse search map only contains the minimal data to find the
    // package and determine if it can be imported.
    debug!("Building reverse package FQN mapping");
    let mut rev_map = HashMap::new();
    for (pid, pkg_val) in packages.all_packages() {
        let mid = pkg_val.module;
        let m_name = modules.mod_name_from_id(mid);
        let fqn = format_package_fqn(m_name.name(), pkg_val.fqn.package());

        trace!(
            "Mapping package FQN '{}' to pid={:?}, mid={:?}",
            fqn,
            pid,
            mid
        );
        let insert_result = rev_map.insert(fqn.clone(), (mid, pid));

        // If we already have a same name in the map, this is an error and
        // should abort the solving procedure.
        if let Some((_, existing_pid)) = insert_result {
            warn!(
                "Duplicate package FQN '{}' found: existing={:?}, new={:?}",
                fqn, existing_pid, pid
            );
            return Err(SolveError::DuplicatedPackageFQN {
                first: packages.fqn(existing_pid).into(),
                second: packages.fqn(pid).into(),
            });
        }
    }
    debug!("Built reverse mapping");

    debug!("Processing packages for dependency resolution");
    for (mid, _) in modules.all_modules_and_id() {
        let Some(pkgs) = packages.packages_for_module(mid) else {
            trace!("No packages found for module {:?}", mid);
            continue;
        };

        trace!("Processing packages for module {:?}", mid);
        for &pid in pkgs.values() {
            solve_one_package(&mut res, modules, packages, &rev_map, mid, pid)?;
        }
    }
    debug!("Processed packages");

    debug!(
        "Dependency resolution completed with {} nodes and {} edges",
        res.dep_graph.node_count(),
        res.dep_graph.edge_count()
    );
    Ok(res)
}

fn solve_one_package(
    res: &mut DepRelationship,
    modules: &ResolvedEnv,
    packages: &DiscoverResult,
    rev_map: &RevMap,
    mid: ModuleId,
    pid: PackageId,
) -> Result<(), SolveError> {
    let pkg_data = packages.get_package(pid);
    trace!(
        "Solving package {:?} in module {:?}: {}",
        pid,
        mid,
        pkg_data.fqn.package()
    );

    let mut resolve =
        |import, kind| resolve_import(res, modules, packages, rev_map, mid, pid, import, kind);

    // Gotcha: This part adds import edges based on different fields of the
    // package declaration, i.e. given each import list (regular imports,
    // whitebox test imports, etc.), which packages should use this import list.
    // This is a transpose of what we usually do (given a package kind, import
    // from the given fields).
    //
    // The reason for this is mainly the efficiency. Adding the same import into
    // multiple targets reduces redundant calculation about the alias,

    // regular imports
    trace!("Processing regular imports");
    for import in &pkg_data.raw.imports {
        resolve(import, TargetKind::Source)?;
    }
    // white box tests
    trace!("Processing whitebox test imports");
    for import in &pkg_data.raw.wbtest_imports {
        resolve(import, TargetKind::WhiteboxTest)?;
    }
    // black box tests
    trace!("Processing blackbox test imports");
    for import in &pkg_data.raw.test_imports {
        resolve(import, TargetKind::BlackboxTest)?;
    }
    // subpackage
    if let Some(sub) = &pkg_data.raw.sub_package {
        trace!("Processing subpackage imports");
        for import in &sub.import {
            resolve(import, TargetKind::SubPackage)?;
        }
    }
    // TODO: Add heuristic to not generate white box test targets for external packages

    trace!("Completed solving package {:?}", pid);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn resolve_import(
    res: &mut DepRelationship,
    modules: &ResolvedEnv,
    packages: &DiscoverResult,
    rev_map: &RevMap,
    mid: ModuleId,
    pid: PackageId,
    import: &moonutil::package::Import,
    import_source_kind: TargetKind,
) -> Result<(), SolveError> {
    let import_source = import.get_path();
    trace!(
        "Resolving import '{}' for package {:?} with kind {:?}",
        import_source,
        pid,
        import_source_kind
    );

    // Try to resolve this import
    let Some((import_mid, import_pid)) = rev_map.get(import_source) else {
        debug!(
            "Import '{}' not found in reverse mapping for package {:?}",
            import_source, pid
        );
        return Err(SolveError::ImportNotFound {
            import: import_source.to_owned(),
            package_fqn: packages.fqn(pid).into(),
        });
    };

    trace!(
        "Import '{}' resolved to module {:?}, package {:?}",
        import_source,
        import_mid,
        import_pid
    );

    // Check if the import actually belongs to the current module's import
    let imported = packages.get_package(*import_pid);
    if *import_mid != mid && modules.graph().edge_weight(mid, *import_mid).is_none() {
        debug!(
            "Import '{}' module {:?} not imported by current module {:?}",
            import_source, import_mid, mid
        );
        return Err(SolveError::ImportNotImportedByModule {
            import: imported.fqn.clone().into(),
            module: modules.mod_name_from_id(mid).clone(),
            pkg: packages.get_package(pid).fqn.package().clone(),
        });
    }

    // Okay, now let's add this package to our package's import in deps
    // TODO: the import alias determination part is a mess, will need to refactor later
    //     Currently this part is just for making the whole thing work.
    let short_alias = match import {
        moonutil::package::Import::Simple(_) => imported.fqn.short_alias(),
        moonutil::package::Import::Alias { alias, .. } => alias
            .as_deref()
            .unwrap_or_else(|| imported.fqn.short_alias()),
    };
    let is_import_target_subpackage = match import {
        moonutil::package::Import::Simple(_) => false,
        moonutil::package::Import::Alias { sub_package, .. } => *sub_package,
    };

    trace!(
        "Import alias determined as '{}', is_subpackage={}",
        short_alias,
        is_import_target_subpackage
    );

    // Insert edges
    let targets = dep_edge_source_from_targets(import_source_kind);
    trace!(
        "Adding dependency edges for import '{}' ({:?})",
        import_source,
        targets
    );
    for package_target in targets {
        let import_kind = if is_import_target_subpackage {
            TargetKind::SubPackage
        } else {
            TargetKind::Source
        };

        let dependency = import_pid.build_target(import_kind);
        let package = pid.build_target(*package_target);

        trace!(
            "Adding edge: {:?} -> {:?} (short alias: '{}')",
            package,
            dependency,
            short_alias
        );

        res.dep_graph.add_edge(
            package,
            dependency,
            DepEdge {
                short_alias: short_alias.into(),
                kind: import_source_kind,
            },
        );
    }

    trace!("Successfully resolved import '{}'", import_source);
    Ok(())
}

/// Get the source nodes that will need to be added, depending on the import
/// field kind. See body of [`solve_one_package`] for more info.
///
/// We're reusing the [`TargetKind`] enum here, which might not be a good idea.
/// Specifically, Inline Tests don't have their own import. Maybe we should use
/// a separate enum to represent this.
fn dep_edge_source_from_targets(kind: TargetKind) -> &'static [TargetKind] {
    match kind {
        TargetKind::Source => &[
            TargetKind::Source,
            TargetKind::WhiteboxTest,
            TargetKind::BlackboxTest,
        ],
        TargetKind::InlineTest => panic!("Inline tests don't have their separate import list."),
        TargetKind::WhiteboxTest => &[TargetKind::WhiteboxTest],
        TargetKind::BlackboxTest => &[TargetKind::BlackboxTest],
        TargetKind::SubPackage => &[TargetKind::SubPackage],
    }
}
