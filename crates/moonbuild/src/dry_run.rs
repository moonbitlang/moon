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

use moonbuild_debug::graph::try_debug_dump_build_graph_to_file;
use moonutil::module::ModuleDB;
use n2::densemap::Index;
use n2::graph::{BuildId, FileId, Graph};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use std::path::Path;

use moonutil::common::{MoonbuildOpt, MooncOpt, RunMode, TargetBackend};
use n2::load::State;

/// Print build commands from a State
pub fn print_build_commands(
    graph: &Graph,
    default: &[FileId],
    source_dir: &Path,
    target_dir: &Path,
) {
    let in_same_dir = target_dir.starts_with(source_dir);

    if !default.is_empty() {
        let mut sorted_default = default.to_vec();
        sorted_default.sort_by_key(|a| a.index());
        let builds: Vec<BuildId> = stable_toposort_graph(graph, &sorted_default);
        for b in builds.iter() {
            let build = &graph.builds[*b];
            if let Some(cmdline) = &build.cmdline {
                let res = replace_path(source_dir, in_same_dir, cmdline);
                println!("{}", res);
            }
        }
    }

    try_debug_dump_build_graph_to_file(graph, default, source_dir);
}

pub fn replace_path<'a>(source_dir: &Path, in_same_dir: bool, cmdline: &'a str) -> Cow<'a, str> {
    if in_same_dir {
        // TODO: this replace is not safe
        cmdline
            .replace(&source_dir.display().to_string(), ".")
            .into()
    } else {
        Cow::Borrowed(cmdline)
    }
}

/// Print run commands from a State
pub fn print_run_commands(
    state: &State,
    target_backend: TargetBackend,
    source_dir: &Path,
    target_dir: &Path,
    args: &[String],
) {
    let in_same_dir = target_dir.starts_with(source_dir);

    if !state.default.is_empty() {
        // FIXME: This sorts the default targets twice. Should not affect the perf much though.
        let mut sorted_default = state.default.clone();
        sorted_default.sort_by_key(|a| a.index());

        for fid in sorted_default.iter() {
            let mut watfile = state.graph.file(*fid).name.clone();
            let cmd = match target_backend {
                TargetBackend::Wasm | TargetBackend::WasmGC => {
                    Some(moonutil::BINARIES.moonrun.clone())
                }
                TargetBackend::Js => Some(moonutil::BINARIES.node_or_default()),
                TargetBackend::Native | TargetBackend::LLVM => {
                    // stub.o would be default for native and llvm, skip them
                    if !watfile.ends_with(".exe") {
                        continue;
                    }
                    None
                }
            };
            if in_same_dir {
                watfile = watfile.replacen(&source_dir.display().to_string(), ".", 1);
            }

            let mut moonrun_command = if let Some(cmd) = cmd {
                let cmd = cmd.display();
                format!("{cmd} {watfile} --")
            } else {
                watfile
            };
            if !args.is_empty() {
                moonrun_command = format!("{moonrun_command} {}", args.join(" "));
            }

            println!("{moonrun_command}");
        }
    }
}

pub fn print_commands(
    module: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<i32> {
    let moonc_opt = &MooncOpt {
        json_diagnostics: false,
        ..moonc_opt.clone()
    };

    let (source_dir, target_dir) = (&moonbuild_opt.source_dir, &moonbuild_opt.target_dir);
    let mode = moonbuild_opt.run_mode;

    let state = match mode {
        RunMode::Build | RunMode::Run => {
            crate::build::load_moon_proj(module, moonc_opt, moonbuild_opt)?
        }
        RunMode::Check => crate::check::normal::load_moon_proj(module, moonc_opt, moonbuild_opt)?,
        RunMode::Test | RunMode::Bench => {
            crate::runtest::load_moon_proj(module, moonc_opt, moonbuild_opt)?
        }
        RunMode::Bundle => crate::bundle::load_moon_proj(module, moonc_opt, moonbuild_opt)?,
        RunMode::Format => crate::fmt::load_moon_proj(module, moonc_opt, moonbuild_opt)?,
    };
    log::debug!("{:#?}", state);

    print_build_commands(&state.graph, &state.default, source_dir, target_dir);

    if mode == RunMode::Run {
        print_run_commands(
            &state,
            moonc_opt.link_opt.target_backend,
            source_dir,
            target_dir,
            &moonbuild_opt.args,
        );
    }

    Ok(0)
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
    let mut key_cache = HashMap::new();
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
