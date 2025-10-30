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

//! Utilities for testing with build graphs

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::Path,
};

use moonbuild_debug::graph::{BuildGraphDump, BuildNode};

/// Trait for various snapshot types, since we have both [`expect_test::Expect`]
/// and [`expect_test::ExpectFile`] to handle.
trait IExpect {
    /// The data of the snapshot
    fn data(&self) -> &str;
    /// If we feed new data, can we update the snapshot?
    fn can_update(&self) -> bool;
    /// Update the snapshot with new data
    fn update(self, new_data: &str);
}

impl IExpect for expect_test::Expect {
    fn data(&self) -> &str {
        self.data()
    }

    fn can_update(&self) -> bool {
        std::env::var("UPDATE_EXPECT").is_ok_and(|x| x == "1")
    }

    fn update(self, new_data: &str) {
        self.assert_eq(new_data);
    }
}

pub fn compare_graphs(actual: &Path, expected: impl IExpect) {
    let actual_file = std::fs::File::open(actual).expect("Failed to open actual graph output file");
    let actual_graph = BuildGraphDump::read_from(actual_file).expect("Failed to read actual graph");

    let Ok(expected_graph) = BuildGraphDump::read_from(expected.data().as_bytes()) else {
        if !expected.can_update() {
            panic!("Expected graph snapshot is invalid and cannot be updated");
        } else {
            // We can't parse the expected input, we have no other way than updating
            println!("Unable to parse expected graph, trying to update the snapshot instead...");
            let mut actual_graph_s = Vec::<u8>::new();
            actual_graph.dump_to(&mut actual_graph_s).unwrap();
            let actual_graph_str = String::from_utf8(actual_graph_s).unwrap();
            expected.update(&actual_graph_str);
            return;
        }
    };

    match compare_graphs_inner(&actual_graph, &expected_graph) {
        Ok(()) => {}
        Err(diff) => {
            if expected.can_update() {
                println!("Graph snapshot differs, updating...\n{diff}");
                let mut actual_graph_s = Vec::<u8>::new();
                actual_graph
                    .dump_to(&mut actual_graph_s)
                    .expect("Failed to write actual graph");
                let actual_graph_str =
                    String::from_utf8(actual_graph_s).expect("Graph dump is not valid UTF-8");
                expected.update(&actual_graph_str);
            } else {
                panic!("Build graphs differ:\n{diff}");
            }
        }
    }
}

fn compare_graphs_inner(actual: &BuildGraphDump, expected: &BuildGraphDump) -> Result<(), String> {
    let (actual_nodes, actual_map) =
        build_node_index(actual).map_err(|e| format!("actual graph: {e}"))?;
    let (expected_nodes, expected_map) =
        build_node_index(expected).map_err(|e| format!("expected graph: {e}"))?;

    let actual_outputs: BTreeSet<String> = actual_map.keys().map(|s| (*s).to_owned()).collect();
    let expected_outputs: BTreeSet<String> = expected_map.keys().map(|s| (*s).to_owned()).collect();

    if actual_outputs != expected_outputs {
        let mut diff = String::new();
        let only_in_actual: Vec<_> = actual_outputs
            .difference(&expected_outputs)
            .map(|s| s.as_str())
            .collect();
        let only_in_expected: Vec<_> = expected_outputs
            .difference(&actual_outputs)
            .map(|s| s.as_str())
            .collect();

        if !only_in_actual.is_empty() {
            writeln!(&mut diff, "Outputs only in actual graph:").unwrap();
            for output in only_in_actual {
                writeln!(&mut diff, "  {output}").unwrap();
            }
        }

        if !only_in_expected.is_empty() {
            writeln!(&mut diff, "Outputs only in expected graph:").unwrap();
            for output in only_in_expected {
                writeln!(&mut diff, "  {output}").unwrap();
            }
        }

        return Err(diff);
    }

    let mut diffs = String::new();
    let mut compared_pairs = BTreeSet::new();

    for output in &actual_outputs {
        let Some(&actual_idx) = actual_map.get(output.as_str()) else {
            return Err(format!("missing output `{}` in actual map", output));
        };
        let Some(&expected_idx) = expected_map.get(output.as_str()) else {
            return Err(format!("missing output `{}` in expected map", output));
        };

        if !compared_pairs.insert((actual_idx, expected_idx)) {
            continue;
        }

        let actual_node = &actual_nodes[actual_idx];
        let expected_node = &expected_nodes[expected_idx];

        if !equals_str_slices(&actual_node.outputs, &expected_node.outputs) {
            writeln!(
                &mut diffs,
                "Outputs differ for nodes producing `{}`:\n  actual: [{}]\n  expected: [{}]",
                output,
                actual_node.outputs.join(", "),
                expected_node.outputs.join(", ")
            )
            .unwrap();
        }

        if actual_node.command_canonical != expected_node.command_canonical {
            writeln!(
                &mut diffs,
                "Command differs for node producing [{}]:\n  actual: {}\n  expected: {}",
                actual_node.outputs.join(", "),
                format_command(
                    actual_node.command_canonical.as_deref(),
                    actual_node.node.command.as_deref()
                ),
                format_command(
                    expected_node.command_canonical.as_deref(),
                    expected_node.node.command.as_deref()
                )
            )
            .unwrap();
        }

        if !equals_str_slices(&actual_node.inputs, &expected_node.inputs) {
            writeln!(
                &mut diffs,
                "Inputs differ for node producing [{}]:\n  actual: [{}]\n  expected: [{}]",
                actual_node.outputs.join(", "),
                join_strs(&actual_node.inputs),
                join_strs(&expected_node.inputs)
            )
            .unwrap();
        }
    }

    if diffs.is_empty() { Ok(()) } else { Err(diffs) }
}

struct NodeView<'a> {
    node: &'a BuildNode,
    outputs: Vec<&'a str>,
    inputs: Vec<&'a str>,
    command_canonical: Option<String>,
}

fn build_node_index<'a>(
    graph: &'a BuildGraphDump,
) -> Result<(Vec<NodeView<'a>>, BTreeMap<&'a str, usize>), String> {
    let mut nodes = Vec::with_capacity(graph.nodes.len());
    let mut output_map = BTreeMap::new();

    for (idx, node) in graph.nodes.iter().enumerate() {
        if node.outputs.is_empty() {
            return Err("node has no outputs".to_owned());
        }

        let mut outputs: Vec<&str> = node.outputs.iter().map(|s| s.as_str()).collect();
        outputs.sort();

        for output in &outputs {
            if output_map.insert(*output, idx).is_some() {
                return Err(format!("multiple nodes produce `{output}`"));
            }
        }

        let mut inputs: Vec<&str> = node.inputs.iter().map(|s| s.as_str()).collect();
        inputs.sort();

        let command_canonical = match &node.command {
            Some(cmd) => {
                let parts = shlex::split(cmd)
                    .ok_or_else(|| format!("failed to shlex-split command `{cmd}`"))?;
                Some(
                    shlex::try_join(parts.iter().map(|part| part.as_str()))
                        .map_err(|_| "failed to re-join command".to_owned())?,
                )
            }
            None => None,
        };

        nodes.push(NodeView {
            node,
            outputs,
            inputs,
            command_canonical,
        });
    }

    Ok((nodes, output_map))
}

fn equals_str_slices(left: &[&str], right: &[&str]) -> bool {
    left.len() == right.len() && left.iter().zip(right.iter()).all(|(l, r)| l == r)
}

fn join_strs(items: &[&str]) -> String {
    if items.is_empty() {
        "<none>".to_owned()
    } else {
        items.join(", ")
    }
}

fn format_command(command: Option<&str>, original: Option<&str>) -> String {
    match (command, original) {
        (Some(canonical), Some(raw)) if canonical == raw => canonical.to_owned(),
        (Some(canonical), Some(raw)) => format!("{canonical} (raw: {raw})"),
        (Some(canonical), None) => canonical.to_owned(),
        (None, Some(raw)) => raw.to_owned(),
        (None, None) => "<no command>".to_owned(),
    }
}
