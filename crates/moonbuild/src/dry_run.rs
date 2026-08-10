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

use n2::densemap::Index;
use n2::graph::{BuildId, FileId, Graph};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    sync::LazyLock,
};

pub use moonutil::path_normalizer::PathNormalizer;

const ENV_VAR: &str = "MOON_TEST_DUMP_BUILD_GRAPH";
static DRY_RUN_TEST_OUTPUT: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var(ENV_VAR).ok());

/// Write build commands from a State.
pub fn write_build_commands(
    output: &mut dyn Write,
    graph: &Graph,
    default: &[FileId],
    logical_commands: &BTreeMap<PathBuf, Vec<String>>,
    source_dir: &Path,
    target_dir: &Path,
) -> std::io::Result<()> {
    let _ = target_dir; // TODO
    let replacer = PathNormalizer::new(source_dir);

    if !default.is_empty() {
        let mut sorted_default = default.to_vec();
        sorted_default.sort_by_key(|a| a.index());
        let builds: Vec<BuildId> = stable_toposort_graph(graph, &sorted_default);
        for b in builds.iter() {
            let build = &graph.builds[*b];
            if let Some(cmdline) = build.cmdline.as_ref() {
                let logical_args = logical_args_for_build(graph, build, logical_commands);
                let command = command_for_display(cmdline, logical_args);
                writeln!(output, "{}", replacer.normalize_command(&command))?;
            }
            if let Some(cwd) = build.cwd.as_deref().map(Path::new) {
                let resolved_cwd = if cwd.is_absolute() {
                    cwd.to_path_buf()
                } else {
                    source_dir.join(cwd)
                };
                writeln!(
                    output,
                    "  cwd: {}",
                    replacer.normalize_context_path(&resolved_cwd)
                )?;
            }
            if !build.env.is_empty() {
                writeln!(output, "  env:")?;
                for line in normalized_env_lines(&build.env, &replacer) {
                    writeln!(output, "    {line}")?;
                }
            }
        }
    }

    try_debug_dump_build_graph_to_file(graph, default, logical_commands, source_dir);
    Ok(())
}

fn logical_args_for_build<'a>(
    graph: &Graph,
    build: &n2::graph::Build,
    logical_commands: &'a BTreeMap<PathBuf, Vec<String>>,
) -> Option<&'a [String]> {
    build.outs.ids.iter().find_map(|id| {
        let file = graph.files.by_id.lookup(*id)?;
        logical_commands
            .get(Path::new(&file.name))
            .map(Vec::as_slice)
    })
}

fn command_for_display<'a>(
    execution_command: &'a str,
    logical_args: Option<&[String]>,
) -> Cow<'a, str> {
    logical_args.map_or(Cow::Borrowed(execution_command), |args| {
        Cow::Owned(moonutil::shlex::join_native(
            args.iter().map(String::as_str),
        ))
    })
}

fn normalized_env_lines(env: &[(String, String)], replacer: &PathNormalizer) -> Vec<String> {
    env.iter()
        .map(|(key, value)| format!("{key}={}", replacer.normalize_command_arg(value)))
        .collect()
}

#[derive(Debug)]
struct BuildGraphDump {
    nodes: Vec<BuildNode>,
}

impl BuildGraphDump {
    fn dump_to(&self, out: impl Write) -> anyhow::Result<()> {
        let mut writer = std::io::BufWriter::new(out);
        for node in &self.nodes {
            serde_json::to_writer(&mut writer, node)?;
            writeln!(&mut writer)?;
        }
        Ok(())
    }
}

#[derive(Debug, serde::Serialize)]
struct BuildNode {
    command: Option<String>,
    inputs: Vec<String>,
    outputs: Vec<String>,
}

fn debug_dump_build_graph(
    graph: &n2::graph::Graph,
    input_files: &[FileId],
    logical_commands: &BTreeMap<PathBuf, Vec<String>>,
    source_dir: &Path,
) -> BuildGraphDump {
    let replacer = PathNormalizer::new(source_dir);

    let accessible_nodes = dfs_for_accessible_nodes(graph, input_files);
    generate_from_nodes(graph, accessible_nodes, logical_commands, &replacer)
}

// FIXME: `MOON_TEST_DUMP_BUILD_GRAPH` is integration-test infrastructure kept
// in production-facing dry-run code only so existing snapshot tests can keep
// invoking the compiled `moon` binary. Gate or relocate this once the test
// harness no longer needs the runtime hook.
fn try_debug_dump_build_graph_to_file(
    build_graph: &n2::graph::Graph,
    default_files: &[n2::graph::FileId],
    logical_commands: &BTreeMap<PathBuf, Vec<String>>,
    source_dir: &Path,
) {
    let Some(out_file) = DRY_RUN_TEST_OUTPUT.as_deref() else {
        return;
    };

    let file = std::fs::File::create(out_file).expect("Failed to create dry-run dump target");
    let dump = debug_dump_build_graph(build_graph, default_files, logical_commands, source_dir);
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
            // FIXME: This preserves the current graph dump behavior, but raw
            // `ins.ids` collapses explicit, implicit, order-only, and
            // validation/lazy inputs. Follow up by using the n2 accessor that
            // matches the intended dry-run graph snapshot semantics.
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
    logical_commands: &BTreeMap<PathBuf, Vec<String>>,
    replacer: &PathNormalizer,
) -> BuildGraphDump {
    let mut nodes = vec![];
    for node in accessible_nodes {
        let node = graph.builds.lookup(node).expect("Unknown build in graph");
        let command = node.cmdline.as_ref().map(|cmd| {
            let logical_args = logical_args_for_build(graph, node, logical_commands);
            let command = command_for_display(cmd, logical_args);
            replacer.normalize_command(&command)
        });
        let mut inputs = node
            .ins
            .ids
            .iter()
            .map(|&id| {
                let file = graph.files.by_id.lookup(id).expect("Unknown node in graph");
                replacer.normalize_path(&file.name)
            })
            .collect::<Vec<_>>();
        inputs.sort();
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

    nodes.sort_by(|a, b| a.outputs.cmp(&b.outputs));

    BuildGraphDump { nodes }
}

/// Create a filename-based sorting key cache for stable graph traversal.
///
/// The key prioritizes filename over full path to provide deterministic
/// ordering for dry-run output. This handles test sandbox path variations
/// while maintaining stable output across different environments.
///
/// Note: This is specifically for stable dry-run output in tests and CI.
/// Absolute stability across all possible edge cases is not a goal.
fn create_file_sorting_cache(graph: &Graph) -> HashMap<FileId, (String, usize)> {
    let mut key_cache = HashMap::with_capacity(graph.files.all_ids().size_hint().0);
    for id in graph.files.all_ids() {
        let name = &graph.file(id).name;
        let normalized = name.replace('\\', "/");
        let last_slash = normalized.rfind('/').map_or(0, |i| i + 1);
        key_cache.insert(id, (normalized, last_slash));
    }
    key_cache
}

/// Perform an iteration over the build graph to get the total list of build
/// commands that corresponds to the given inputs.
///
/// This function provides stable output order based on file names and
/// the build graph structure, independent of graph insertion order.
fn stable_toposort_graph(graph: &Graph, inputs: &[FileId]) -> Vec<BuildId> {
    let key_cache = create_file_sorting_cache(graph);
    let by_file_name = |k: &FileId| {
        let (name, last_slash) = &key_cache[k];
        (&name[*last_slash..], name)
    };

    // Sort input files by filename for deterministic order
    let mut input_order = Vec::new();
    input_order.extend_from_slice(inputs);
    input_order.sort_unstable_by_key(by_file_name);

    // DFS stack: (file_id, is_pop)
    let mut stack = Vec::<(FileId, bool)>::new();
    stack.extend(input_order.into_iter().map(|x| (x, false)));
    // Result
    let mut res = vec![];
    // Visited builds set
    let mut vis = HashSet::new();
    // Scratch vec for sorting input. Leave empty when unused.
    let mut sort_in_scratch = vec![];

    while let Some((fid, pop)) = stack.pop() {
        let file = graph.file(fid);
        if let Some(bid) = file.input {
            if !pop {
                if vis.insert(bid) {
                    let build = &graph.builds[bid];
                    stack.push((fid, true));

                    // Sort input files for stable traversal order
                    debug_assert!(sort_in_scratch.is_empty());
                    sort_in_scratch.extend_from_slice(build.explicit_ins());
                    sort_in_scratch.sort_unstable_by_key(by_file_name);
                    stack.extend(sort_in_scratch.iter().copied().map(|x| (x, false)));
                    sort_in_scratch.clear();
                }
            } else {
                res.push(bid);
            }
        }
    }

    res
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, io::Write, path::Path, rc::Rc};

    use n2::graph::{Build, BuildIns, BuildOuts, FileLoc, Graph};

    use super::{PathNormalizer, command_for_display, normalized_env_lines, write_build_commands};

    #[test]
    fn displays_logical_args_instead_of_the_execution_transport() {
        let logical_args = vec![
            "moonc".to_owned(),
            "build-package".to_owned(),
            "source/a.mbt".to_owned(),
        ];

        let command =
            command_for_display("moonc -rsp-file build/pkg.core.rsp", Some(&logical_args));

        assert_eq!(moonutil::shlex::split_native(&command), logical_args);
    }

    #[test]
    fn renders_build_env_with_normalized_values() {
        let source_dir = tempfile::tempdir().unwrap();
        let include = dunce::canonicalize(source_dir.path())
            .unwrap()
            .join("crt/include");
        let replacer = PathNormalizer::new(source_dir.path());

        let lines = normalized_env_lines(
            &[
                ("LIB".to_owned(), "C:\\SDK\\Lib".to_owned()),
                ("INCLUDE".to_owned(), include.to_string_lossy().into_owned()),
            ],
            &replacer,
        );

        assert_eq!(lines, ["LIB=C:/SDK/Lib", "INCLUDE=./crt/include"]);
    }

    #[test]
    fn propagates_dry_run_write_errors() {
        struct FailingWriter;

        impl Write for FailingWriter {
            fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("write failed"))
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let mut graph = Graph::default();
        let input = graph
            .files
            .id_from_canonical("/workspace/main.mbt".to_owned());
        let output = graph
            .files
            .id_from_canonical("/workspace/main.core".to_owned());
        let mut build = Build::new(
            FileLoc {
                filename: Rc::new("test".into()),
                line: 0,
            },
            BuildIns {
                ids: vec![input],
                explicit: 1,
                implicit: 0,
                order_only: 0,
            },
            BuildOuts {
                ids: vec![output],
                explicit: 1,
            },
        );
        build.cmdline = Some("moonc main.mbt -o main.core".to_owned());
        graph.add_build(build).unwrap();

        let error = write_build_commands(
            &mut FailingWriter,
            &graph,
            &[output],
            &BTreeMap::new(),
            Path::new("/workspace"),
            Path::new("/workspace/_build"),
        )
        .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::Other);
    }
}
