use moonutil::common::gen::ModuleDB;
use n2::densemap::Index;
use n2::graph::{BuildId, FileId, Graph};
use std::collections::VecDeque;

use moonutil::common::{MoonbuildOpt, MooncOpt, RunMode, TargetBackend};

pub fn print_commands(
    module: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<i32> {
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
                let watfile = &state.graph.file(*fid).name;
                let cmd = match moonc_opt.link_opt.target_backend {
                    TargetBackend::Wasm => "moonrun",
                    TargetBackend::WasmGC => "moonrun",
                    TargetBackend::Js => "node",
                };
                if in_same_dir {
                    println!(
                        "{} {}",
                        cmd,
                        watfile.replace(&source_dir.display().to_string(), ".")
                    );
                } else {
                    println!("{} {}", cmd, watfile);
                }
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
