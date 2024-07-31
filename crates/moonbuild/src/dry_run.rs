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
use std::collections::VecDeque;

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
        RunMode::Test => crate::runtest::load_moon_proj(module, moonc_opt, moonbuild_opt)?,
        RunMode::Bundle => crate::bundle::load_moon_proj(module, moonc_opt, moonbuild_opt)?,
        RunMode::Format => crate::fmt::load_moon_proj(module, moonc_opt, moonbuild_opt)?,
    };
    log::debug!("{:#?}", state);
    let mut builds: VecDeque<BuildId> = VecDeque::new();
    if !state.default.is_empty() {
        let mut sorted_default = state.default.clone();
        sorted_default.sort_by_key(|a| a.index());
        let mut queue: VecDeque<FileId> = VecDeque::new();
        for target in sorted_default.iter() {
            queue.push_back(*target);
            bfs_graph(&state.graph, &mut queue, &mut builds);
            queue.clear();
        }
        for b in builds.iter().rev() {
            let build = &state.graph.builds[*b];
            if let Some(cmdline) = &build.cmdline {
                if in_same_dir {
                    // TODO: this replace is not safe
                    println!(
                        "{}",
                        cmdline.replace(&source_dir.display().to_string(), ".")
                    );
                } else {
                    println!("{}", cmdline);
                }
            }
        }
        if mode == RunMode::Run {
            for fid in sorted_default.iter() {
                let mut watfile = state.graph.file(*fid).name.clone();
                let cmd = match moonc_opt.link_opt.target_backend {
                    TargetBackend::Wasm => "moonrun",
                    TargetBackend::WasmGC => "moonrun",
                    TargetBackend::Js => "node",
                };
                if in_same_dir {
                    watfile = watfile.replacen(&source_dir.display().to_string(), ".", 1);
                }

                let mut moonrun_command = format!("{cmd} {watfile}");
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

fn bfs_graph(graph: &Graph, queue: &mut VecDeque<FileId>, builds: &mut VecDeque<BuildId>) {
    while !queue.is_empty() {
        let file_id = queue.pop_front().unwrap();
        if let Some(bid) = graph.file(file_id).input {
            if !builds.contains(&bid) {
                builds.push_back(bid);
            } else {
                builds.retain(|id| &bid != id);
                builds.push_back(bid);
            }
            let build = &graph.builds[bid];
            for &fid in build.explicit_ins().iter().rev() {
                if !queue.contains(&fid) {
                    queue.push_back(fid);
                }
            }
        }
    }
}
