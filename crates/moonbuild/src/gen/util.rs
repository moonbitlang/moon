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

use anyhow::bail;
use moonutil::module::ModuleDB;
use moonutil::package::{NativeLinkConfig, Package};
use moonutil::{graph::get_example_cycle, package::Link};
use std::fmt::Write;

#[allow(unused)]
use std::collections::{HashMap, HashSet};

use petgraph::dot::{Config, Dot};

pub fn toposort(m: &ModuleDB) -> anyhow::Result<Vec<String>> {
    let topo = match petgraph::algo::toposort(&m.graph, None) {
        Ok(nodes) => {
            let nodes_rev = nodes.iter().rev().map(|x| x.index()).collect::<Vec<_>>();

            super::util::nodes_to_names(m, &nodes_rev)
        }
        Err(cycle) => {
            let cycle = get_example_cycle(&m.graph, cycle.node_id());
            let cycle = cycle
                .into_iter()
                .map(|n| m.get_package_by_index(n.index()).full_name())
                .collect::<Vec<_>>();
            let cycle_str = cycle.join(" -> ");
            bail!("cyclic dependency detected: {}", cycle_str);
        }
    };
    Ok(topo)
}

/// Performs a topological sort (DFS) to get package dependencies in the correct order,
/// returning package names as strings.
///
/// This is used by the build command. For test commands that need package references,
/// see `topo_from_node_with_tests` below.
pub fn topo_from_node(m: &ModuleDB, pkg: &Package) -> anyhow::Result<Vec<String>> {
    topo_from_node_impl(m, pkg, false, false)
}

/// Internal implementation of topological sort that can optionally include test imports.
///
/// This function handles virtual packages by:
/// 1. Tracking virtual-to-implementation mappings via the `overrides` field
/// 2. Resolving virtual packages to their implementations before recursion
/// 3. Including transitive dependencies of implementations
fn topo_from_node_impl(
    m: &ModuleDB,
    pkg: &Package,
    with_wbtest_import: bool,
    with_test_import: bool,
) -> anyhow::Result<Vec<String>> {
    let pkg_full_name = pkg.full_name();
    let mut stk: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    /* mapping from virtual package to its implementation */
    let mut virtual_impl: HashMap<String, String> = HashMap::new();

    fn dfs(
        m: &ModuleDB,
        pkg_full_name: &str,
        stk: &mut Vec<String>,
        visited: &mut HashSet<String>,
        virtual_impl: &mut HashMap<String, String>,
        with_wbtest_import: bool,
        with_test_import: bool,
    ) -> anyhow::Result<()> {
        visited.insert(pkg_full_name.to_string());

        let pkg = m.get_package_by_name(pkg_full_name);
        // record the virtual package and its implementation
        if let Some(overrides) = pkg.overrides.as_ref() {
            for implement in overrides.iter() {
                let implement_pkg = m.get_package_by_name(implement);
                let virtual_pkg = implement_pkg.implement.as_ref().unwrap().clone();
                if let Some(impl_pkg) = virtual_impl.get(&virtual_pkg) {
                    if *impl_pkg == implement_pkg.full_name() {
                        continue;
                    } else {
                        bail!(
                            "Virtual package {} has multiple implementations: {} and {}",
                            virtual_pkg,
                            impl_pkg,
                            implement_pkg.full_name()
                        );
                    }
                } else {
                    virtual_impl.insert(virtual_pkg, implement_pkg.full_name());
                }
            }
        }

        // Collect neighbor package names (with virtual replacements) and sort lexicographically
        let imports_iter = pkg
            .imports
            .iter()
            .chain(if with_wbtest_import {
                pkg.wbtest_imports.iter()
            } else {
                [].iter()
            })
            .chain(if with_test_import {
                pkg.test_imports.iter()
            } else {
                [].iter()
            });

        let mut neighbor_names: Vec<String> = Vec::new();
        for neighbor in imports_iter {
            let neighbor_full_name = neighbor.path.make_full_path();
            let neighbor_pkg = m.get_package_by_name(&neighbor_full_name);
            let neighbor_no_virtual = if let Some(virtual_info) = &neighbor_pkg.virtual_pkg {
                // if neighbor is a virtual package, we should find its implementation
                if let Some(impl_pkg) = virtual_impl.get(&neighbor_full_name) {
                    impl_pkg.to_string()
                } else if virtual_info.has_default {
                    neighbor_full_name
                } else {
                    bail!(
                        "Virtual package {} has no implementation",
                        neighbor_full_name
                    );
                }
            } else {
                neighbor_full_name
            };
            neighbor_names.push(neighbor_no_virtual);
        }

        // Deterministic order: lexicographic by fully-qualified name, deduplicated
        neighbor_names.sort();
        neighbor_names.dedup();

        // Match RR traversal: process the lexicographically smallest neighbor first.
        // Here with recursion, visiting in ascending order achieves the same effect.
        for neighbor_no_virtual in neighbor_names.into_iter() {
            if !visited.contains(&neighbor_no_virtual) {
                dfs(
                    m,
                    &neighbor_no_virtual,
                    stk,
                    visited,
                    virtual_impl,
                    false,
                    false,
                )?;
            }
        }

        stk.push(pkg_full_name.to_string());
        Ok(())
    }

    dfs(
        m,
        &pkg_full_name,
        &mut stk,
        &mut visited,
        &mut virtual_impl,
        with_wbtest_import,
        with_test_import,
    )?;
    Ok(stk)
}

/// Performs a topological sort with test imports, returning package names.
///
/// Used by test commands when package names are needed.
pub fn topo_from_node_with_tests(
    m: &ModuleDB,
    pkg: &Package,
    with_wbtest_import: bool,
    with_test_import: bool,
) -> anyhow::Result<Vec<String>> {
    topo_from_node_impl(m, pkg, with_wbtest_import, with_test_import)
}

pub fn nodes_to_names(m: &ModuleDB, nodes: &[usize]) -> Vec<String> {
    nodes
        .iter()
        .map(|index| m.get_package_by_index(*index).full_name())
        .collect::<Vec<_>>()
}

pub fn nodes_to_cores(m: &ModuleDB, nodes: &[String]) -> Vec<String> {
    nodes
        .iter()
        .map(|name| m.get_package_by_name(name).artifact.with_extension("core"))
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
}

pub fn nodes_to_pkg_sources(m: &ModuleDB, nodes: &[String]) -> Vec<(String, String)> {
    nodes
        .iter()
        .map(|name| {
            let pkg = &m.get_package_by_name(name);
            (pkg.full_name(), pkg.root_path.display().to_string())
        })
        .collect::<Vec<_>>()
}

pub fn graph_to_dot(m: &ModuleDB) {
    println!("{:?}", Dot::with_config(&m.graph, &[Config::EdgeNoLabel]));
}

pub fn self_in_test_import(pkg: &Package) -> bool {
    // for package in the same level of mod, like "Yoorkin/prettyprinter", pkg.rel.full_name() is_empty
    // current_pkg_full_path should be "Yoorkin/prettyprinter" instead of "Yoorkin/prettyprinter/"
    let current_pkg_full_path = if pkg.rel.full_name().is_empty() {
        pkg.root.full_name()
    } else {
        format!("{}/{}", pkg.root.full_name(), pkg.rel.full_name())
    };

    pkg.test_imports
        .iter()
        .any(|import| import.path.make_full_path() == current_pkg_full_path)
}

pub fn calc_link_args(m: &ModuleDB, pkg: &Package) -> Link {
    let mut link = pkg.link.clone().unwrap_or_default();
    // Add native link flags
    for (_name, pkg) in m
        .get_filtered_packages_and_its_deps_by_pkgname(&pkg.full_name())
        .expect("Package not in DB")
    {
        if has_link_flags(&pkg) {
            let link_native = link.native.get_or_insert(NativeLinkConfig::default());
            let link_flags = link_native.cc_link_flags.get_or_insert(Default::default());
            let new_link_flags = fmt_link_flags(&pkg);
            link_flags.push_str(&new_link_flags);
        }
    }
    link
}

fn has_link_flags(pkg: &Package) -> bool {
    pkg.link_flags.is_some() || !pkg.link_search_paths.is_empty() || !pkg.link_libs.is_empty()
}

fn fmt_link_flags(pkg: &Package) -> String {
    let mut out_str = String::new();
    if let Some(flags) = &pkg.link_flags {
        out_str.push(' ');
        out_str.push_str(flags);
    }
    for link_search_path in &pkg.link_search_paths {
        #[cfg(not(windows))]
        write!(out_str, " -L{link_search_path}").unwrap();
        #[cfg(windows)]
        write!(out_str, " /LIBPATH:{}", link_search_path).unwrap();
    }
    for link_lib in &pkg.link_libs {
        #[cfg(not(windows))]
        write!(out_str, " -l{link_lib}").unwrap();
        #[cfg(windows)]
        write!(out_str, " {}.lib", link_lib).unwrap();
    }
    out_str
}
