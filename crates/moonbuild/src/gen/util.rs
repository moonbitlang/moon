// Copyright 2024 International Digital Economy Academy
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
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use anyhow::bail;
use moonutil::graph::get_example_cycle;
use moonutil::module::ModuleDB;
use moonutil::package::Package;
#[allow(unused)]
use std::collections::HashSet;

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
                .map(|n| m.packages[n.index()].full_name())
                .collect::<Vec<_>>();
            bail!("cyclic dependency detected: {:?}", cycle);
        }
    };
    Ok(topo)
}

pub fn topo_from_node(m: &ModuleDB, pkg: &Package) -> anyhow::Result<Vec<String>> {
    let pkg_full_name = pkg.full_name();
    let mut stk: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();

    fn dfs(
        m: &ModuleDB,
        pkg_full_name: &String,
        stk: &mut Vec<String>,
        visited: &mut HashSet<String>,
    ) -> anyhow::Result<()> {
        visited.insert(pkg_full_name.clone());

        for neighbor in m.packages[pkg_full_name].imports.iter() {
            let neighbor_full_name = neighbor.path.make_full_path();
            if !visited.contains(&neighbor_full_name) {
                dfs(m, &neighbor_full_name, stk, visited)?;
            }
        }

        stk.push(pkg_full_name.clone());
        Ok(())
    }

    dfs(m, &pkg_full_name, &mut stk, &mut visited)?;
    Ok(stk)
}

pub fn nodes_to_names(m: &ModuleDB, nodes: &[usize]) -> Vec<String> {
    nodes
        .iter()
        .map(|index| m.packages[*index].full_name())
        .collect::<Vec<_>>()
}

pub fn nodes_to_cores(m: &ModuleDB, nodes: &[String]) -> Vec<String> {
    nodes
        .iter()
        .map(|index| m.packages[index].artifact.with_extension("core"))
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
}

pub fn nodes_to_pkg_sources(m: &ModuleDB, nodes: &[String]) -> Vec<(String, String)> {
    nodes
        .iter()
        .map(|index| {
            let pkg = &m.packages[index];
            let package_source_dir: String = if pkg.rel.components.is_empty() {
                m.source_dir.display().to_string()
            } else {
                m.source_dir
                    .join(pkg.rel.fs_full_name())
                    .display()
                    .to_string()
            };
            (pkg.full_name(), package_source_dir)
        })
        .collect::<Vec<_>>()
}

pub fn graph_to_dot(m: &ModuleDB) {
    println!("{:?}", Dot::with_config(&m.graph, &[Config::EdgeNoLabel]));
}
