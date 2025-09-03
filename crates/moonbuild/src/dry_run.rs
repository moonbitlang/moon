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

use moonutil::module::ModuleDB;
use n2::densemap::Index;
use n2::graph::{BuildId, FileId, Graph};
use std::collections::{HashMap, HashSet};

use moonutil::common::{MoonbuildOpt, MooncOpt, RunMode, TargetBackend};

pub fn print_commands(
    module: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<i32> {
    let moonc_opt = &MooncOpt {
        render: false,
        ..moonc_opt.clone()
    };

    let (source_dir, target_dir) = (&moonbuild_opt.source_dir, &moonbuild_opt.target_dir);

    let in_same_dir = target_dir.starts_with(source_dir);
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
    if !state.default.is_empty() {
        let mut sorted_default = state.default.clone();
        sorted_default.sort_by_key(|a| a.index());
        let builds: Vec<BuildId> = stable_toposort_graph(&state.graph, &sorted_default);
        for b in builds.iter() {
            let build = &state.graph.builds[*b];
            if let Some(cmdline) = &build.cmdline {
                if in_same_dir {
                    // TODO: this replace is not safe
                    println!(
                        "{}",
                        cmdline.replace(&source_dir.display().to_string(), ".")
                    );
                } else {
                    println!("{cmdline}");
                }
            }
        }
        if mode == RunMode::Run {
            for fid in sorted_default.iter() {
                let mut watfile = state.graph.file(*fid).name.clone();
                let cmd = match moonc_opt.link_opt.target_backend {
                    TargetBackend::Wasm | TargetBackend::WasmGC => "moonrun ",
                    TargetBackend::Js => "node ",
                    TargetBackend::Native | TargetBackend::LLVM => {
                        // stub.o would be default for native and llvm, skip them
                        if !watfile.ends_with(".exe") {
                            continue;
                        }
                        ""
                    }
                };
                if in_same_dir {
                    watfile = watfile.replacen(&source_dir.display().to_string(), ".", 1);
                }

                let mut moonrun_command = format!("{cmd}{watfile}");
                if !moonbuild_opt.args.is_empty() {
                    moonrun_command =
                        format!("{moonrun_command} -- {}", moonbuild_opt.args.join(" "));
                }

                println!("{moonrun_command}");
            }
        }
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
