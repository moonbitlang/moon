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
use std::collections::{HashSet, VecDeque};

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
        let builds: Vec<BuildId> = bfs_graph(&state.graph, &sorted_default);
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

fn bfs_graph(graph: &Graph, sorted_default: &[FileId]) -> Vec<BuildId> {
    let mut bids: Vec<BuildId> = Vec::new();
    for &target in sorted_default.iter() {
        let mut fid_queue: VecDeque<FileId> = VecDeque::from([target]);
        let mut fid_set: HashSet<FileId> = HashSet::from([target]);
        while !fid_queue.is_empty() {
            let file_id = fid_queue.pop_front().unwrap();
            fid_set.remove(&file_id);
            if let Some(bid) = graph.file(file_id).input {
                bids.push(bid);
                let build = &graph.builds[bid];
                for &fid in build.explicit_ins().iter().rev() {
                    if fid_set.insert(fid) {
                        fid_queue.push_back(fid);
                    }
                }
            }
        }
    }
    dedupe(bids)
}

fn dedupe(inputs: Vec<BuildId>) -> Vec<BuildId> {
    let mut seen: HashSet<BuildId> = HashSet::new();
    let mut bids: Vec<BuildId> = Vec::new();
    for &bid in inputs.iter().rev() {
        if seen.insert(bid) {
            bids.push(bid);
        }
    }
    bids
}
