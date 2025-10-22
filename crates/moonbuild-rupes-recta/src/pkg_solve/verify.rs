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

//! Verify the validity of the package dependency graph.

use std::collections::{HashMap, HashSet, hash_map::Entry};

use indexmap::IndexSet;
use log::debug;
use petgraph::visit::IntoNodeIdentifiers;

use crate::{
    discover::DiscoverResult,
    model::{BuildTarget, PackageId},
    pkg_solve::{
        DepEdge,
        model::{DepRelationship, SolveError},
    },
};

use super::model::MultipleError;

/// Verify that this package dependency graph is valid.
///
/// This function checks the following:
/// - No loops (except test imports, which don't currently have a workaround)
/// - Aliases are unique within one package
pub fn verify(dep: &DepRelationship, packages: &DiscoverResult) -> Result<(), SolveError> {
    debug!("Verifying package dependency graph integrity");

    let mut errs = vec![];

    match verify_no_loop(dep) {
        Ok(()) => {}
        Err(e) => errs.push(e),
    }
    debug!("Done loop verification");

    verify_no_duplicated_alias(dep, packages, &mut errs);
    debug!("Done uniqueness verification");
    verify_no_forbidden_internal_imports(dep, packages, &mut errs);
    debug!("Done internal import verification");

    debug!("Package dependency graph verification completed successfully");
    if errs.is_empty() {
        Ok(())
    } else {
        Err(SolveError::Multiple(MultipleError(errs)))
    }
}

/// Verify there's no loops within the dependency graph. If there's any import
/// loop, return an error.
fn verify_no_loop(dep: &DepRelationship) -> Result<(), SolveError> {
    // An indexed current-visiting path, for finding loops
    let mut path = IndexSet::new();
    // Work stack.
    let mut stack = Vec::new();
    // The visited list
    let mut vis = HashSet::new();
    let graph = &dep.dep_graph;

    // Loop through all nodes for starting
    for starting_node in graph.node_identifiers() {
        if vis.contains(&starting_node) {
            continue;
        }
        assert!(stack.is_empty(), "DFS starting with non-empty stack");

        // Begin DFS
        stack.push(WorkStackItem::new(starting_node));

        while let Some(it) = stack.pop() {
            if it.pop {
                assert_eq!(path.pop(), Some(it.node), "DFS path mismatch");
                continue;
            }
            let node = it.node;

            // Check for loops
            if path.contains(&node) {
                // We found a loop, return an error
                // TODO: handle white box testing should not cause loop
                let loop_path: Vec<_> = path.into_iter().collect();
                return Err(SolveError::ImportLoop { loop_path });
            }
            // Set visibility
            vis.insert(node);
            // Add to the path
            path.insert(node);

            // Visit the node, which is currently no-op here
            // (we may want to do something with the node in the future)
            stack.push(WorkStackItem::pop(node));

            // Push outgoing edges
            for target in graph.neighbors(node) {
                // If the target is not visited, push it to the stack
                if !vis.contains(&target) {
                    stack.push(WorkStackItem::new(target));
                }
            }
        }
    }

    Ok(())
}

struct WorkStackItem {
    node: BuildTarget,
    pop: bool,
}

impl WorkStackItem {
    /// A new item is pushed onto the stack
    fn new(node: BuildTarget) -> Self {
        Self { node, pop: false }
    }
    /// The item has been visited, the next visit should pop it from path
    fn pop(node: BuildTarget) -> Self {
        Self { node, pop: true }
    }
}

/// Verify that there's no duplicated alias for each build node within the graph.
fn verify_no_duplicated_alias(
    dep: &DepRelationship,
    packages: &DiscoverResult,
    errs: &mut Vec<SolveError>,
) {
    for node in dep.dep_graph.node_identifiers() {
        // The alias map for the current node
        let mut map: HashMap<&arcstr::Substr, (BuildTarget, &DepEdge)> = HashMap::new();

        for (from, _, edge) in dep
            .dep_graph
            .edges_directed(node, petgraph::Direction::Outgoing)
        {
            match map.entry(&edge.short_alias) {
                Entry::Occupied(e) => {
                    let (first_from, first_edge) = e.get();

                    errs.push(SolveError::ConflictingImportAlias {
                        alias: edge.short_alias.to_string(),
                        package_node: node,
                        package_fqn: packages.fqn(node.package).into(),
                        first_import_node: *first_from,
                        first_import: packages.fqn(first_from.package).into(),
                        first_import_kind: first_edge.kind,
                        second_import_node: from,
                        second_import: packages.fqn(from.package).into(),
                        second_import_kind: edge.kind,
                    });
                }

                Entry::Vacant(vacant_entry) => {
                    vacant_entry.insert((from, edge));
                }
            }
        }
    }
}

/// Verify no forbidden internal imports between package pairs.
///
/// TODO: Need to re-asses if this is better reported here or in solver
fn verify_no_forbidden_internal_imports(
    dep: &DepRelationship,
    packages: &DiscoverResult,
    errs: &mut Vec<SolveError>,
) {
    // De-duplicate by (importer package, dependency package) so we don't spam
    // errors for different target kinds of the same package pair.
    let mut seen: HashSet<(PackageId, PackageId)> = HashSet::new();

    for node in dep.dep_graph.node_identifiers() {
        for (from, to, _edge) in dep
            .dep_graph
            .edges_directed(node, petgraph::Direction::Outgoing)
        {
            let pair = (from.package, to.package);
            if !seen.insert(pair) {
                continue;
            }

            let importer_pkg = packages.get_package(from.package);
            let dependency_pkg = packages.get_package(to.package);

            let same_module = importer_pkg.module == dependency_pkg.module;
            let importer_path = importer_pkg.fqn.package();
            let dependency_path = dependency_pkg.fqn.package();

            if !importer_path.can_import(dependency_path, same_module) {
                errs.push(SolveError::InternalImportForbidden {
                    importer_node: from,
                    importer: packages.fqn(from.package).into(),
                    dependency_node: to,
                    dependency: packages.fqn(to.package).into(),
                });
            }
        }
    }
}
