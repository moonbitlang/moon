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
    env,
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
    let accessible_nodes = dfs_for_accessible_nodes(graph, input_files);
    generate_from_nodes(graph, accessible_nodes, source_dir)
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
    source_dir: &Path,
) -> BuildGraphDump {
    let normalizer = PathNormalizer::new(source_dir);
    let mut nodes = vec![];
    for node in accessible_nodes {
        let node = graph.builds.lookup(node).expect("Unknown build in graph");
        let command = node
            .cmdline
            .as_ref()
            .map(|cmd| normalizer.normalize_command(cmd));
        let inputs = node
            .ins
            .ids
            .iter()
            .map(|&id| {
                let file = graph.files.by_id.lookup(id).expect("Unknown node in graph");
                normalizer.normalize_path(&file.name)
            })
            .collect::<Vec<_>>();
        let outputs = node
            .outs
            .ids
            .iter()
            .map(|&id| {
                let file = graph.files.by_id.lookup(id).expect("Unknown node in graph");
                normalizer.normalize_path(&file.name)
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

struct PathNormalizer {
    original: PathBuf,
    original_str: String,
    original_alt: String,
    canonical: Option<PathBuf>,
    canonical_str: Option<String>,
    canonical_alt: Option<String>,
    moon_bin_str: Option<String>,
    moon_bin_alt: Option<String>,
}

impl PathNormalizer {
    fn new(source_dir: &Path) -> Self {
        let original = source_dir.to_path_buf();
        let original_str = original.to_string_lossy().to_string();
        let original_alt = original_str.replace('\\', "/");
        let canonical = std::fs::canonicalize(&original).ok();
        let canonical_str = canonical.as_ref().map(|p| p.to_string_lossy().to_string());
        let canonical_alt = canonical_str.as_ref().map(|s| s.replace('\\', "/"));
        let moon_bin = env::current_exe().ok();
        let moon_bin_str = moon_bin.as_ref().map(|p| p.to_string_lossy().to_string());
        let moon_bin_alt = moon_bin_str.as_ref().map(|s| s.replace('\\', "/"));
        PathNormalizer {
            original,
            original_str,
            original_alt,
            canonical,
            canonical_str,
            canonical_alt,
            moon_bin_str,
            moon_bin_alt,
        }
    }

    fn normalize_command(&self, command: &str) -> String {
        let mut normalized = command.to_owned();
        for key in self.replacement_keys() {
            if key.is_empty() {
                continue;
            }
            normalized = normalized.replace(key, ".");
        }
        for key in self.moon_keys() {
            if key.is_empty() {
                continue;
            }
            normalized = normalized.replace(key, "moon");
        }
        normalized
    }

    fn normalize_path(&self, path: &str) -> String {
        let path_obj = Path::new(path);
        if let Some(canonical) = &self.canonical
            && let Ok(stripped) = path_obj.strip_prefix(canonical)
        {
            return Self::relative_from_path(stripped);
        }
        if let Ok(stripped) = path_obj.strip_prefix(&self.original) {
            return Self::relative_from_path(stripped);
        }
        for key in self.replacement_keys() {
            if let Some(rest) = path.strip_prefix(key) {
                return Self::relative_from_str(rest);
            }
        }
        path.replace('\\', "/")
    }

    fn replacement_keys(&self) -> impl Iterator<Item = &str> {
        [
            self.canonical_str.as_deref(),
            self.canonical_alt.as_deref(),
            Some(self.original_str.as_str()),
            Some(self.original_alt.as_str()),
        ]
        .into_iter()
        .flatten()
    }

    fn moon_keys(&self) -> impl Iterator<Item = &str> {
        [self.moon_bin_str.as_deref(), self.moon_bin_alt.as_deref()]
            .into_iter()
            .flatten()
    }

    fn relative_from_path(stripped: &Path) -> String {
        if stripped.as_os_str().is_empty() {
            ".".to_owned()
        } else {
            let normalized = stripped.to_string_lossy().replace('\\', "/");
            format!("./{}", normalized)
        }
    }

    fn relative_from_str(rest: &str) -> String {
        let trimmed = rest.trim_start_matches(['/', '\\']);
        if trimmed.is_empty() {
            ".".to_owned()
        } else {
            format!("./{}", trimmed.replace('\\', "/"))
        }
    }
}
