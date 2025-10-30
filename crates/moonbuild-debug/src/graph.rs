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

//! Test utilities

use std::{
    collections::HashSet,
    io::{BufRead, Write},
};

use n2::graph::{BuildId, FileId};

/// The in-memory format for dumping a `n2` build graph
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct BuildGraphDump {
    pub nodes: Vec<BuildNode>,
}

impl BuildGraphDump {
    /// Dump the build graph dump to the given output, in JSONL format
    pub fn dump_to(&self, out: impl Write) -> anyhow::Result<()> {
        let mut writer = std::io::BufWriter::new(out);
        for node in &self.nodes {
            serde_json::to_writer(&mut writer, node)?;
            writeln!(&mut writer)?;
        }
        Ok(())
    }

    /// Read the build graph dump from the given input, in JSONL format.
    ///
    /// This will deplete the input.
    pub fn read_from(input: impl std::io::Read) -> anyhow::Result<BuildGraphDump> {
        let reader = std::io::BufReader::new(input);
        let mut nodes = vec![];
        for line in reader.lines() {
            let line = line?;
            let node: BuildNode = serde_json::from_str(&line)?;
            nodes.push(node);
        }
        Ok(BuildGraphDump { nodes })
    }
}

/// The node in the build graph dump
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct BuildNode {
    pub command: Option<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// Dump the `n2` build graph for debugging purposes
pub fn debug_dump_build_graph(graph: &n2::graph::Graph, input_files: &[FileId]) -> BuildGraphDump {
    let accessible_nodes = dfs_for_accessible_nodes(graph, input_files);
    generate_from_nodes(graph, accessible_nodes)
}

fn dfs_for_accessible_nodes(graph: &n2::graph::Graph, start_files: &[FileId]) -> Vec<BuildId> {
    let mut stack = Vec::<FileId>::new();
    stack.extend_from_slice(start_files);
    let mut visited_builds = HashSet::new();
    let mut accessible_builds = vec![];

    while let Some(fid) = stack.pop() {
        let file = graph
            .files
            .by_id
            .lookup(fid)
            .expect("Unknown file in graph");
        if let Some(bid) = file.input
            && visited_builds.insert(bid)
        {
            let build = graph.builds.lookup(bid).expect("Unknown build in graph");
            accessible_builds.push(bid);
            for &in_fid in &build.ins.ids {
                stack.push(in_fid);
            }
        }
    }

    accessible_builds
}

fn generate_from_nodes(
    graph: &n2::graph::Graph,
    accessible_nodes: impl IntoIterator<Item = BuildId>,
) -> BuildGraphDump {
    let mut nodes = vec![];
    for node in accessible_nodes {
        let node = graph.builds.lookup(node).expect("Unknown build in graph");
        let command = node.cmdline.clone();
        let inputs = node
            .ins
            .ids
            .iter()
            .map(|&id| {
                graph
                    .files
                    .by_id
                    .lookup(id)
                    .expect("Unknown node in graph")
                    .name
                    .clone()
            })
            .collect::<Vec<_>>();
        let outputs = node
            .outs
            .ids
            .iter()
            .map(|&id| {
                graph
                    .files
                    .by_id
                    .lookup(id)
                    .expect("Unknown node in graph")
                    .name
                    .clone()
            })
            .collect::<Vec<_>>();
        nodes.push(BuildNode {
            command,
            inputs,
            outputs,
        });
    }
    BuildGraphDump { nodes }
}
