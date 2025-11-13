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
    path::{Path, PathBuf},
    sync::LazyLock,
};

use n2::graph::{BuildId, FileId};

pub const ENV_VAR: &str = "MOON_TEST_DUMP_BUILD_GRAPH";
static DRY_RUN_TEST_OUTPUT: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var(ENV_VAR).ok());

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
pub struct BuildNode {
    pub command: Option<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// Dump the `n2` build graph for debugging purposes
pub fn debug_dump_build_graph(
    graph: &n2::graph::Graph,
    input_files: &[FileId],
    source_dir: &Path,
) -> BuildGraphDump {
    let path_replace_table = moonutil::BINARIES
        .all_moon_bins()
        .iter()
        .map(|(name, path)| (path.to_string_lossy().to_string(), name.to_string()))
        .collect();
    let replacer = PathNormalizer::new(source_dir, path_replace_table);

    let accessible_nodes = dfs_for_accessible_nodes(graph, input_files);
    generate_from_nodes(graph, accessible_nodes, &replacer)
}

pub fn try_debug_dump_build_graph_to_file(
    build_graph: &n2::graph::Graph,
    default_files: &[n2::graph::FileId],
    source_dir: &Path,
) {
    let Some(out_file) = DRY_RUN_TEST_OUTPUT.as_deref() else {
        return;
    };

    let file = std::fs::File::create(out_file).expect("Failed to create dry-run dump target");
    let dump = debug_dump_build_graph(build_graph, default_files, source_dir);
    dump.dump_to(file).expect("Failed to dump to target output");
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
    replacer: &PathNormalizer,
) -> BuildGraphDump {
    let mut nodes = vec![];
    for node in accessible_nodes {
        let node = graph.builds.lookup(node).expect("Unknown build in graph");
        let command = node
            .cmdline
            .as_ref()
            .map(|cmd| replacer.normalize_command(cmd));
        let inputs = node
            .ins
            .ids
            .iter()
            .map(|&id| {
                let file = graph.files.by_id.lookup(id).expect("Unknown node in graph");
                replacer.normalize_path(&file.name)
            })
            .collect::<Vec<_>>();
        let outputs = node
            .outs
            .ids
            .iter()
            .map(|&id| {
                let file = graph.files.by_id.lookup(id).expect("Unknown node in graph");
                replacer.normalize_path(&file.name)
            })
            .collect::<Vec<_>>();
        nodes.push(BuildNode {
            command,
            inputs,
            outputs,
        });
    }

    // To ensure a stable ordering for tests
    //
    // Note: because build graphs requires outputs to be unique, it is
    // sufficient to sort by outputs only.
    nodes.sort_by(|a, b| a.outputs.cmp(&b.outputs));

    BuildGraphDump { nodes }
}

struct PathNormalizer {
    canonical: Option<PathBuf>,
    replace_table: Vec<(String, String)>,
}

impl PathNormalizer {
    fn new(source_dir: &Path, replace_table: Vec<(String, String)>) -> Self {
        let canonical = dunce::canonicalize(source_dir).ok();
        PathNormalizer {
            canonical,
            replace_table,
        }
    }

    fn normalize_command(&self, command: &str) -> String {
        let mut s = command.to_owned();

        if let Some(canonical) = &self.canonical {
            let prefix = canonical.to_string_lossy();
            let prefix_str = prefix.as_ref();
            let with_sep = format!("{prefix_str}{}", std::path::MAIN_SEPARATOR);
            s = s.replace(&with_sep, "./");
            s = s.replace(prefix_str, ".");
        }

        for (from, to) in &self.replace_table {
            s = s.replace(from, to);
        }
        s = s.replace('\\', "/");

        s
    }

    fn normalize_path(&self, path: &str) -> String {
        let path_obj = Path::new(path);
        if let Some(canonical) = &self.canonical
            && let Ok(stripped) = path_obj.strip_prefix(canonical)
        {
            return Self::relative_from_path(stripped);
        }
        path.replace('\\', "/")
    }

    fn relative_from_path(stripped: &Path) -> String {
        if stripped.as_os_str().is_empty() {
            ".".to_owned()
        } else {
            let normalized = stripped.to_string_lossy().replace('\\', "/");
            format!("./{}", normalized)
        }
    }
}
