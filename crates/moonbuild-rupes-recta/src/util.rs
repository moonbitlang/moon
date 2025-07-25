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

//! A number of random utilities useful for debugging the project

use crate::build_plan::BuildPlan;
use crate::discover::DiscoverResult;
use crate::pkg_solve::DepRelationship;
use moonutil::mooncakes::result::ResolvedEnv;
use petgraph::Direction;
use std::io::{self, Write};

/// Print a resolved environment as a DOT graph
pub fn print_resolved_env_dot(env: &ResolvedEnv, writer: &mut dyn Write) -> io::Result<()> {
    writeln!(writer, "digraph ResolvedEnv {{")?;
    // Nodes: use ModuleId debug as ID and full source name as label
    for (id, src) in env.all_modules_and_id() {
        let node_id = format!("{:?}", id);
        let label = src.to_string();
        writeln!(writer, "    \"{}\" [label=\"{}\"];", node_id, label)?;
    }
    // Edges: dependencies with module IDs and dependency key labels
    for (from, _) in env.all_modules_and_id() {
        for (to, key) in env.deps_keyed(from) {
            let from_id = format!("{:?}", from);
            let to_id = format!("{:?}", to);
            writeln!(
                writer,
                "    \"{}\" -> \"{}\" [label=\"{}\"];",
                from_id, to_id, key
            )?;
        }
    }
    writeln!(writer, "}}")?;
    Ok(())
}

/// Print a dependency relationship of build targets as a DOT graph, resolving package IDs to full names
pub fn print_dep_relationship_dot(
    dep: &DepRelationship,
    packages: &DiscoverResult,
    writer: &mut dyn Write,
) -> io::Result<()> {
    writeln!(writer, "digraph DepRelationship {{")?;
    // Nodes: use BuildTarget debug as ID, label with full package FQN and kind
    for target in dep.dep_graph.nodes() {
        let id = format!("{:?}@{:?}", target.package, target.kind);
        let fqn = packages.fqn(target.package);
        let label = format!("{}@{:?}", fqn, target.kind);
        writeln!(writer, "    \"{}\" [label=\"{}\"];", id, label)?;
    }
    // Edges: use node IDs with alias label
    for from in dep.dep_graph.nodes() {
        for to in dep.dep_graph.neighbors_directed(from, Direction::Outgoing) {
            if let Some(edge) = dep.dep_graph.edge_weight(from, to) {
                let id_from = format!("{:?}@{:?}", from.package, from.kind);
                let id_to = format!("{:?}@{:?}", to.package, to.kind);
                writeln!(
                    writer,
                    "    \"{}\" -> \"{}\" [label=\"{}\"];",
                    id_from, id_to, edge.short_alias
                )?;
            }
        }
    }
    writeln!(writer, "}}")?;
    Ok(())
}

/// Print a build plan as a DOT graph, showing build nodes and their dependencies
pub fn print_build_plan_dot(
    build_plan: &BuildPlan,
    packages: &DiscoverResult,
    writer: &mut dyn Write,
) -> io::Result<()> {
    writeln!(writer, "digraph BuildPlan {{")?;
    writeln!(writer, "    rankdir=TB;")?;
    writeln!(writer, "    node [shape=box];")?;

    // Nodes: use BuildPlanNode debug as ID, label with package FQN, target kind, and action
    for node in build_plan.all_nodes() {
        let node_id = format!(
            "{:?}@{:?}@{:?}",
            node.target.package, node.target.kind, node.action
        );
        let fqn = packages.fqn(node.target.package);
        let label = format!("{}\\n{:?}\\n{:?}", fqn, node.target.kind, node.action);

        // Color nodes based on action type
        let color = match node.action {
            crate::model::TargetAction::Check => "lightblue",
            crate::model::TargetAction::Build => "lightgreen",
            crate::model::TargetAction::BuildCStubs => "lightyellow",
            crate::model::TargetAction::LinkCore => "lightcoral",
            crate::model::TargetAction::MakeExecutable => "lightpink",
            crate::model::TargetAction::GenerateTestInfo => "lightgray",
        };

        writeln!(
            writer,
            "    \"{}\" [label=\"{}\" fillcolor=\"{}\" style=\"filled\"];",
            node_id, label, color
        )?;
    }

    // Edges: dependencies between build plan nodes
    for node in build_plan.all_nodes() {
        for dep in build_plan.dependency_nodes(node) {
            let node_id = format!(
                "{:?}@{:?}@{:?}",
                node.target.package, node.target.kind, node.action
            );
            let dep_id = format!(
                "{:?}@{:?}@{:?}",
                dep.target.package, dep.target.kind, dep.action
            );
            writeln!(writer, "    \"{}\" -> \"{}\";\n", node_id, dep_id)?;
        }
    }

    writeln!(writer, "}}")?;
    Ok(())
}
