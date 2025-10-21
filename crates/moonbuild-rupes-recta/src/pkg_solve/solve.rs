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
    discover::{DiscoverResult, DiscoveredPackage},
    model::{PackageId, TargetKind},
    pkg_name::format_package_fqn,
    pkg_solve::model::VirtualUser,
};

type RevMap = HashMap<String, (ModuleId, PackageId)>;

/// A grouped environment for resolving dependencies.
struct ResolveEnv<'a> {
    modules: &'a ResolvedEnv,
    packages: &'a DiscoverResult,
    rev_map: &'a RevMap,
    res: DepRelationship,
}

pub fn solve_only(
    modules: &ResolvedEnv,
    packages: &DiscoverResult,
) -> Result<DepRelationship, SolveError> {
    debug!(
        "Building dependency resolution structures for {} packages",
        packages.package_count()
    );

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

    let mut env = ResolveEnv {
        modules,
        packages,
        rev_map: &rev_map,
        res: DepRelationship::default(),
    };

    debug!("Processing packages for dependency resolution");
    for (mid, _) in modules.all_modules_and_id() {
        let Some(pkgs) = packages.packages_for_module(mid) else {
            trace!("No packages found for module {:?}", mid);
            continue;
        };

        for &pid in pkgs.values() {
            solve_one_package_virtual_impl(&mut env, mid, pid)?;
        }
    }

    for (mid, _) in modules.all_modules_and_id() {
        let Some(pkgs) = packages.packages_for_module(mid) else {
            trace!("No packages found for module {:?}", mid);
            continue;
        };

        trace!("Processing packages for module {:?}", mid);
        for &pid in pkgs.values() {
            solve_one_package(&mut env, mid, pid)?;
        }
    }
    debug!("Processed packages");

    let res = env.res;

    debug!(
        "Dependency resolution completed with {} nodes and {} edges",
        res.dep_graph.node_count(),
        res.dep_graph.edge_count()
    );
    Ok(res)
}

/// Solve the virtual package implementation (and only this field) for a given package.
///
/// MAINTAINERS: This part is split into a separate pass because the main
/// resolving path of one package may depend on the virtual implementation
/// information of other packages. Thus, we need to ensure all virtual
/// implementations are resolved before we start the main solving pass.
fn solve_one_package_virtual_impl(
    env: &mut ResolveEnv<'_>,
    mid: ModuleId,
    pid: PackageId,
) -> Result<(), SolveError> {
    let pkg_data = env.packages.get_package(pid);
    trace!(
        "Solving virtual package implementations for package {:?} in module {:?}: {}",
        pid,
        mid,
        pkg_data.fqn.package()
    );

    let v_impl = pkg_data.raw.implement.as_deref();
    if let Some(v_impl) = v_impl {
        let (impl_pid, impl_data) = resolve_import_raw(env, mid, pid, v_impl)?;

        if !impl_data.is_virtual() {
            return Err(SolveError::ImplementTargetNotVirtual {
                package: pkg_data.fqn.clone().into(),
                implements: impl_data.fqn.clone().into(),
            });
        }
        env.res.virt_impl.insert(pid, impl_pid);
    }

    Ok(())
}

/// Solve related dependency information for one package.
fn solve_one_package(
    env: &mut ResolveEnv,
    mid: ModuleId,
    pid: PackageId,
) -> Result<(), SolveError> {
    let pkg_data = env.packages.get_package(pid);
    trace!(
        "Solving package {:?} in module {:?}: {}",
        pid,
        mid,
        pkg_data.fqn.package()
    );

    let mut resolve = |import, kind| {
        let resolved = resolve_import(env, mid, pid, import)?;
        add_dep_edges_for_import(env, pid, resolved, kind);
        Ok(())
    };

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

    // Black box tests also add the source package as an import
    insert_black_box_dep(env, pid, pkg_data);

    // TODO: Add heuristic to not generate white box test targets for external packages

    let virtual_info = resolve_virtual_usages(env, pid, pkg_data)?;
    if let Some(vu) = virtual_info {
        env.res.virtual_users.insert(pid, vu);
    }

    trace!("Completed solving package {:?}", pid);
    Ok(())
}

/// Add the dependency from black box test to source package.
///
/// The dependency edge will be created with the default short alias of the
/// source package. If this duplicates with any existing alias, print a warning
/// and replace the duplicated one's alias with its full name.
fn insert_black_box_dep(env: &mut ResolveEnv<'_>, pid: PackageId, pkg_data: &DiscoveredPackage) {
    let short_alias = pkg_data.fqn.short_alias();
    let mut violating = None;

    // Check for violation
    //
    // FIXME: Should this live here or in `verify.rs`?
    // But `verify.rs` should be immutable, which means we can't do the
    // replacement immediately when we find a violation.
    for (f, t, edge) in env.res.dep_graph.edges_directed(
        pid.build_target(TargetKind::BlackboxTest),
        petgraph::Direction::Outgoing,
    ) {
        if t == pid.build_target(TargetKind::Source) {
            // If the edge points to the source package, we don't need to do
            // anything -- the edge is already inserted, nothing more to check.
            return;
        } else if edge.short_alias == short_alias {
            // Otherwise, if the edge has the same short alias, we have a violation.
            violating = Some((f, t, edge));
            break;
        }
    }

    // Print about the violation and replace
    //
    // Note: If there are multiple violations, we only handle one of them.
    // This is because multiple packages with the same short alias is already
    // an error, so resolving it doesn't make much sense (and it fixes/hides the
    // error instead).
    if let Some((f, t, edge)) = violating {
        let violating_pkg = env.packages.get_package(t.package);
        warn_about_test_import(pkg_data, violating_pkg);
        // replace the existing one's alias with its full name
        let new_alias = violating_pkg.fqn.to_string();
        trace!(
            "Replacing existing alias '{}' with '{}' for package {:?}",
            edge.short_alias,
            new_alias,
            t.package
        );
        env.res.dep_graph.add_edge(
            f,
            t,
            DepEdge {
                short_alias: new_alias,
                kind: edge.kind,
            },
        );
    }

    // Finally, add the edge from black box test to source package
    env.res.dep_graph.add_edge(
        pid.build_target(TargetKind::BlackboxTest),
        pid.build_target(TargetKind::Source),
        DepEdge {
            short_alias: short_alias.to_string(),
            kind: TargetKind::BlackboxTest,
        },
    );
}

fn warn_about_test_import(pkg: &DiscoveredPackage, violating: &DiscoveredPackage) {
    warn!(
        "Duplicate alias `{}` at \"{}\". \
        \"test-import\" will automatically add \"import\" and current \
        package as dependency so you don't need to add it manually. \
        If you're test-importing a dependency with the same default \
        alias as your current package, considering give it a different \
        alias than the current package. \
        Violating import: `{}`",
        pkg.fqn.short_alias(),
        pkg.config_path().display(),
        violating.fqn
    );
}

/// Grouped necessary information about an import
struct ResolvedImport<'a> {
    package_id: PackageId,
    target_is_subpackage: bool,
    short_alias: &'a str,
}

/// Resolve one import item for a given package.
#[allow(clippy::too_many_arguments)]
fn resolve_import<'a>(
    env: &mut ResolveEnv<'a>,
    mid: ModuleId,
    pid: PackageId,
    import: &'a moonutil::package::Import,
) -> Result<ResolvedImport<'a>, SolveError> {
    let import_source = import.get_path();

    let (import_pid, imported) = resolve_import_raw(env, mid, pid, import_source)?;

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

    Ok(ResolvedImport {
        package_id: import_pid,
        target_is_subpackage: is_import_target_subpackage,
        short_alias,
    })
}

fn resolve_import_raw<'a>(
    env: &mut ResolveEnv<'a>,
    mid: ModuleId,
    pid: PackageId,
    import_source: &str,
) -> Result<(PackageId, &'a DiscoveredPackage), SolveError> {
    trace!("Resolving import '{}' for package {:?}", import_source, pid);

    let Some((import_mid, import_pid)) = env.rev_map.get(import_source) else {
        debug!(
            "Import '{}' not found in reverse mapping for package {:?}",
            import_source, pid
        );
        return Err(SolveError::ImportNotFound {
            import: import_source.to_owned(),
            package_fqn: env.packages.fqn(pid).into(),
        });
    };
    trace!(
        "Import '{}' resolved to module {:?}, package {:?}",
        import_source,
        import_mid,
        import_pid
    );
    let imported = env.packages.get_package(*import_pid);
    if *import_mid != mid && env.modules.graph().edge_weight(mid, *import_mid).is_none() {
        warn!(
            "Import {} exists in global environment, but its containing module is not imported by {}, \
            thus cannot be imported by its package '{}'. \
            This will become an error in the future.",
            imported.fqn,
            env.modules.mod_name_from_id(mid).name(),
            env.packages.get_package(pid).fqn.package()
        );
    }
    Ok((*import_pid, imported))
}

/// Insert dependency edges for one resolved import.
fn add_dep_edges_for_import(
    env: &mut ResolveEnv,
    pid: PackageId,
    import: ResolvedImport,
    import_source_kind: TargetKind,
) {
    // Insert edges
    let targets = dep_edge_source_from_targets(import_source_kind);
    trace!(
        "Adding dependency edges for import '{:?}' ({:?})",
        import.package_id,
        targets
    );
    for package_target in targets {
        let import_kind = if import.target_is_subpackage {
            TargetKind::SubPackage
        } else {
            TargetKind::Source
        };

        let dependency = import.package_id.build_target(import_kind);
        let package = pid.build_target(*package_target);

        trace!(
            "Adding edge: {:?} -> {:?} (short alias: '{}')",
            package,
            dependency,
            import.short_alias
        );

        env.res.dep_graph.add_edge(
            package,
            dependency,
            DepEdge {
                short_alias: import.short_alias.into(),
                kind: import_source_kind,
            },
        );
    }
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
            TargetKind::InlineTest,
            TargetKind::WhiteboxTest,
            TargetKind::BlackboxTest,
        ],
        TargetKind::InlineTest => panic!("Inline tests don't have their separate import list."),
        TargetKind::WhiteboxTest => &[TargetKind::WhiteboxTest],
        TargetKind::BlackboxTest => &[TargetKind::BlackboxTest],
        TargetKind::SubPackage => &[TargetKind::SubPackage],
    }
}

/// Resolve the virtual package usages for a specific package, and returns the
/// side table to insert of needed.
fn resolve_virtual_usages(
    env: &mut ResolveEnv,
    pid: PackageId,
    pkg: &DiscoveredPackage,
) -> Result<Option<VirtualUser>, SolveError> {
    // For each override, check its implementation
    let mut v_user: Option<VirtualUser> = None;
    for over in pkg.raw.overrides.iter().flatten() {
        let (over_pid, over_pkg) = resolve_import_raw(env, pkg.module, pid, over)?;

        // Check if it's implementing a virtual package
        let Some(&over_target) = env.res.virt_impl.get(over_pid) else {
            return Err(SolveError::OverrideNotImplementor {
                package: pkg.fqn.clone().into(),
                virtual_override: over_pkg.fqn.clone().into(),
            });
        };

        // Insert this override into user graph
        let user = v_user.get_or_insert_with(Default::default);
        if let Some(existing) = user.overrides.insert(over_target, over_pid) {
            return Err(SolveError::VirtualOverrideConflict {
                package: pkg.fqn.clone().into(),
                virtual_pkg: env.packages.fqn(over_target).into(),
                first_override: env.packages.fqn(existing).into(),
                second_override: over_pkg.fqn.clone().into(),
            });
        }
    }

    Ok(v_user)
}
