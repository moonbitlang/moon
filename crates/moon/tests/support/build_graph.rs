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

//! Capture and compare build graphs from Moon commands.

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    io::{BufRead, Write},
};

use colored::Colorize;
use similar::DiffTag;

const ENV_VAR: &str = "MOON_TEST_DUMP_BUILD_GRAPH";

const ALGORITHM: similar::Algorithm = similar::Algorithm::Patience;

/// The JSONL graph snapshot emitted by Moon's dry-run renderer.
#[derive(Debug)]
struct BuildGraphDump {
    nodes: Vec<BuildNode>,
}

impl BuildGraphDump {
    /// Dump the build graph dump to the given output, in JSONL format
    fn dump_to(&self, out: impl Write) -> anyhow::Result<()> {
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
    fn read_from(input: impl std::io::Read) -> anyhow::Result<BuildGraphDump> {
        let reader = std::io::BufReader::new(input);
        let mut nodes = vec![];
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let node: BuildNode = serde_json::from_str(&line)?;
            nodes.push(node);
        }
        Ok(BuildGraphDump { nodes })
    }
}

/// The node in the build graph dump
#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct BuildNode {
    command: Option<String>,
    inputs: Vec<String>,
    outputs: Vec<String>,
}

/// Trait for various snapshot types, since we have both [`expect_test::Expect`]
/// and [`expect_test::ExpectFile`] to handle.
pub(crate) trait IExpect {
    /// The data of the snapshot
    fn data(&self) -> Cow<'_, str>;
    /// If we feed new data, can we update the snapshot?
    fn can_update(&self) -> bool;
    /// Update the snapshot with new data
    fn update(self, new_data: &str);
}

impl IExpect for expect_test::Expect {
    fn data(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.data())
    }

    fn can_update(&self) -> bool {
        expect_test_update()
    }

    fn update(self, new_data: &str) {
        self.assert_eq(new_data);
    }
}

impl IExpect for expect_test::ExpectFile {
    fn data(&self) -> Cow<'_, str> {
        Cow::Owned(self.data())
    }

    fn can_update(&self) -> bool {
        expect_test_update()
    }

    fn update(self, new_data: &str) {
        self.assert_eq(new_data);
    }
}

fn expect_test_update() -> bool {
    std::env::var("UPDATE_EXPECT").is_ok_and(|x| x == "1")
}

/// Run a successful Moon command and compare its graph with the snapshot.
/// The command must include the appropriate dry-run arguments. Graph capture
/// and temporary-file cleanup are owned here; the returned assertion supports
/// additional checks on the command's stdout and stderr.
#[track_caller]
pub(crate) fn assert(
    command: snapbox::cmd::Command,
    expected: impl IExpect,
) -> snapbox::cmd::OutputAssert {
    assert_with_replacements(command, expected, |_| {})
}

/// Capture and compare a graph with test-specific normalization of its paths
/// and commands. The expected snapshot is left unchanged unless UPDATE_EXPECT
/// enables snapshot updates.
#[track_caller]
pub(crate) fn assert_with_replacements(
    command: snapbox::cmd::Command,
    expected: impl IExpect,
    transform: impl Fn(&mut String),
) -> snapbox::cmd::OutputAssert {
    // Keep the path alive through capture and comparison without holding a file
    // open while the child process writes it, including on Windows.
    let graph_dir = tempfile::tempdir().expect("Failed to create graph capture directory");
    let graph_path = graph_dir.path().join("graph.jsonl");
    let output = command.env(ENV_VAR, &graph_path).assert().success();
    let actual_file =
        std::fs::File::open(&graph_path).expect("Failed to open actual graph output file");
    let mut actual_graph =
        BuildGraphDump::read_from(actual_file).expect("Failed to read actual graph");
    transform_graph(&mut actual_graph, |s| {
        normalize_current_moon_binary(s);
        transform(s);
    });

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
            return output;
        }
    };

    let mut out = String::new();
    let differ = compare_graphs_inner(&actual_graph, &expected_graph, &mut out);

    if expected.can_update() {
        if differ {
            println!("Graph snapshot differs, updating...\n{out}");
        }
        let mut actual_graph_s = Vec::<u8>::new();
        actual_graph
            .dump_to(&mut actual_graph_s)
            .expect("Failed to write actual graph");
        let actual_graph_str =
            String::from_utf8(actual_graph_s).expect("Graph dump is not valid UTF-8");
        expected.update(&actual_graph_str);
    } else if differ {
        panic!("Graph snapshot differs:\n{out}");
    }
    output
}

fn normalize_current_moon_binary(s: &mut String) {
    let moon = crate::util::moon_bin();
    let moon = moon.to_string_lossy();
    *s = s.replace(moon.as_ref(), "$MOON_HOME/bin/moon");
    if let Some(moon_without_exe) = moon.strip_suffix(".exe") {
        *s = s.replace(moon_without_exe, "$MOON_HOME/bin/moon");
    }
    let moon_with_forward_slashes = moon.replace('\\', "/");
    *s = s.replace(moon_with_forward_slashes.as_str(), "$MOON_HOME/bin/moon");
    if let Some(moon_without_exe) = moon_with_forward_slashes.strip_suffix(".exe") {
        *s = s.replace(moon_without_exe, "$MOON_HOME/bin/moon");
    }
}

fn transform_graph(graph: &mut BuildGraphDump, transform: impl Fn(&mut String)) {
    for n in graph.nodes.iter_mut() {
        if let Some(cmd) = &mut n.command {
            transform(cmd)
        }
        for in_file in n.inputs.iter_mut() {
            transform(in_file)
        }
        for out_file in n.outputs.iter_mut() {
            transform(out_file)
        }
    }
}

/// Compares two build graphs and returns true if they differ.
/// The message describing the differences is written to `out`.
fn compare_graphs_inner(
    actual: &BuildGraphDump,
    expected: &BuildGraphDump,
    mut out: impl std::fmt::Write,
) -> bool {
    let (actual_nodes, actual_map) = build_node_index(actual)
        .unwrap_or_else(|e| panic!("Failed to build index for actual graph: {e}"));
    let (expected_nodes, expected_map) = build_node_index(expected)
        .unwrap_or_else(|e| panic!("Failed to build index for expected graph: {e}"));

    let actual_outputs: BTreeSet<String> = actual_map.keys().map(|s| (*s).to_owned()).collect();
    let expected_outputs: BTreeSet<String> = expected_map.keys().map(|s| (*s).to_owned()).collect();

    let mut any_diff_found = false;

    if actual_outputs != expected_outputs {
        let only_in_actual: Vec<_> = actual_outputs
            .difference(&expected_outputs)
            .map(|s| s.as_str())
            .collect();
        let only_in_expected: Vec<_> = expected_outputs
            .difference(&actual_outputs)
            .map(|s| s.as_str())
            .collect();

        if !only_in_actual.is_empty() {
            writeln!(out, "Outputs only in actual graph:").unwrap();
            for output in &only_in_actual {
                writeln!(out, "  {output}").unwrap();
            }
            any_diff_found = true;
        }

        if !only_in_expected.is_empty() {
            writeln!(out, "Outputs only in expected graph:").unwrap();
            for output in &only_in_expected {
                writeln!(out, "  {output}").unwrap();
            }
            any_diff_found = true;
        }
    }

    let mut compared_pairs = BTreeSet::new();

    for output in &actual_outputs {
        let Some(&actual_idx) = actual_map.get(output.as_str()) else {
            writeln!(out, "missing output `{}` in actual map", output).unwrap();
            continue;
        };
        let Some(&expected_idx) = expected_map.get(output.as_str()) else {
            writeln!(out, "missing output `{}` in expected map", output).unwrap();
            continue;
        };

        if !compared_pairs.insert((actual_idx, expected_idx)) {
            continue;
        }

        let actual_node = &actual_nodes[actual_idx];
        let expected_node = &expected_nodes[expected_idx];

        // diffs of the current node
        let mut diffs = String::new();

        let mut actual_out = actual_node.outputs.clone();
        actual_out.sort();
        let mut expected_out = expected_node.outputs.clone();
        expected_out.sort();

        let out_diff = similar::capture_diff_slices(ALGORITHM, &expected_out, &actual_out);
        if out_diff.iter().any(|op| op.tag() != DiffTag::Equal) {
            writeln!(&mut diffs, "  Outputs diff:").unwrap();
            for op in out_diff {
                for slice in op.iter_changes(&expected_out, &actual_out) {
                    match slice.tag() {
                        similar::ChangeTag::Equal => {
                            writeln!(&mut diffs, "    {}", slice.value()).unwrap()
                        }
                        similar::ChangeTag::Delete => {
                            writeln!(&mut diffs, "   {}{}", "-".red(), slice.value().red()).unwrap()
                        }
                        similar::ChangeTag::Insert => {
                            writeln!(&mut diffs, "   {}{}", "+".green(), slice.value().green())
                                .unwrap()
                        }
                    }
                }
            }
        }

        let in_diff =
            similar::capture_diff_slices(ALGORITHM, &expected_node.inputs, &actual_node.inputs);
        if in_diff.iter().any(|op| op.tag() != DiffTag::Equal) {
            writeln!(&mut diffs, "  Inputs diff:").unwrap();
            for op in in_diff {
                for slice in op.iter_changes(&expected_node.inputs, &actual_node.inputs) {
                    match slice.tag() {
                        similar::ChangeTag::Equal => {
                            writeln!(&mut diffs, "    {}", slice.value()).unwrap()
                        }
                        similar::ChangeTag::Delete => {
                            writeln!(&mut diffs, "   {}{}", "-".red(), slice.value().red()).unwrap()
                        }
                        similar::ChangeTag::Insert => {
                            writeln!(&mut diffs, "   {}{}", "+".green(), slice.value().green())
                                .unwrap()
                        }
                    }
                }
            }
        }

        if actual_node.command_canonical != expected_node.command_canonical {
            writeln!(&mut diffs, "  Command differs:").unwrap();
            let actual_cmd = actual_node
                .command_canonical
                .as_deref()
                .unwrap_or("<no command>");
            let expected_cmd = expected_node
                .command_canonical
                .as_deref()
                .unwrap_or("<no command>");
            let cmd_diff = similar::TextDiff::from_words(expected_cmd, actual_cmd);

            write!(&mut diffs, "    Old: ").unwrap();
            for change in cmd_diff.iter_all_changes() {
                match change.tag() {
                    similar::ChangeTag::Equal => {
                        write!(&mut diffs, "{}", change.value()).unwrap();
                    }
                    similar::ChangeTag::Delete => {
                        write!(&mut diffs, "{}", change.value().bright_red().underline()).unwrap();
                    }
                    similar::ChangeTag::Insert => {}
                }
            }
            writeln!(&mut diffs).unwrap();

            write!(&mut diffs, "    New: ").unwrap();
            for change in cmd_diff.iter_all_changes() {
                match change.tag() {
                    similar::ChangeTag::Equal => {
                        write!(&mut diffs, "{}", change.value()).unwrap();
                    }
                    similar::ChangeTag::Insert => {
                        write!(&mut diffs, "{}", change.value().bright_green().underline())
                            .unwrap();
                    }
                    similar::ChangeTag::Delete => {}
                }
            }
            writeln!(&mut diffs).unwrap();
        }

        if !diffs.is_empty() {
            writeln!(
                out,
                "Differences for node producing [{}]:\n{}",
                actual_node.outputs.join(", "),
                diffs
            )
            .unwrap();
            any_diff_found = true;
        }
    }

    any_diff_found
}

struct NodeView<'a> {
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
            outputs,
            inputs,
            command_canonical,
        });
    }

    Ok((nodes, output_map))
}
