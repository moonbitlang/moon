//! A number of random utilities useful for debugging the project

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
