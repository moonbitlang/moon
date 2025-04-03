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

use std::path::PathBuf;
use std::rc::Rc;

use moonutil::common::{
    MoonbuildOpt, PrePostBuild, DEP_PATH, MOD_DIR, MOONCAKE_BIN, MOON_BIN_DIR, PKG_DIR,
};
use moonutil::module::ModuleDB;
use moonutil::package::StringOrArray;
use n2::graph::{self as n2graph, Build, BuildIns, BuildOuts, FileId, FileLoc};
use n2::load::State;
use n2::smallmap::SmallMap;

use crate::gen::n2_errors::{N2Error, N2ErrorKind};

pub fn load_moon_x_build(
    moonbuild_opt: &MoonbuildOpt,
    module: &ModuleDB,
    build_type: &PrePostBuild,
) -> anyhow::Result<Option<State>> {
    let mut graph = n2graph::Graph::default();
    let mut defaults: Vec<FileId> = vec![];

    for (_, pkg) in module.get_all_packages().iter() {
        if pkg.is_third_party {
            continue;
        }
        let xbuild = match build_type {
            PrePostBuild::PreBuild => &pkg.pre_build,
            PrePostBuild::PostBuild => &pkg.post_build,
        };
        if let Some(generate) = xbuild {
            for rule in generate {
                let cwd = &pkg.root_path;
                let input = &rule.input;
                let output = &rule.output;
                let command = &rule.command;
                let inputs = match input {
                    StringOrArray::String(s) => {
                        vec![cwd.join(s)]
                    }
                    StringOrArray::Array(arr) => {
                        arr.iter().map(|s| cwd.join(s)).collect::<Vec<_>>()
                    }
                };
                let inputs = inputs
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>();

                let inputs_ids = inputs
                    .iter()
                    .map(|f| graph.files.id_from_canonical(f.into()))
                    .collect::<Vec<_>>();

                let outputs = match output {
                    StringOrArray::String(s) => vec![cwd.join(s)],
                    StringOrArray::Array(arr) => {
                        arr.iter().map(|s| cwd.join(s)).collect::<Vec<_>>()
                    }
                };
                let outputs = outputs
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>();
                let outputs_ids = outputs
                    .iter()
                    .map(|f| graph.files.id_from_canonical(f.into()))
                    .collect::<Vec<_>>();

                let ins = BuildIns {
                    explicit: inputs_ids.len(),
                    ids: inputs_ids,
                    implicit: 0,
                    order_only: 0,
                };
                for o in outputs_ids.iter() {
                    defaults.push(*o);
                }
                let outs = BuildOuts {
                    explicit: outputs_ids.len(),
                    ids: outputs_ids,
                };

                let loc = FileLoc {
                    filename: Rc::new(PathBuf::from(build_type.name())),
                    line: 0,
                };

                let mut build = Build::new(loc, ins, outs);
                let moon_bin = std::env::current_exe()?;

                let command = if command.starts_with(":embed") {
                    command
                        .replacen(":embed", &format!("{} tool embed", moon_bin.display()), 1)
                        .to_string()
                } else {
                    command.to_string()
                }
                .replace(
                    MOONCAKE_BIN,
                    &moonbuild_opt
                        .source_dir
                        .join(DEP_PATH)
                        .join(MOON_BIN_DIR)
                        .display()
                        .to_string(),
                )
                .replace(MOD_DIR, &moonbuild_opt.source_dir.display().to_string())
                .replace(PKG_DIR, &cwd.display().to_string());

                #[cfg(target_os = "windows")]
                let command = {
                    let maybe_ps1 = command.trim_start().split(" ").next().unwrap();
                    let ps1_path = moonbuild_opt
                        .source_dir
                        .join(maybe_ps1)
                        .with_extension("ps1");
                    if ps1_path.exists() {
                        let ps1_path = dunce::canonicalize(ps1_path).unwrap();
                        format!("powershell {}", ps1_path.display())
                    } else {
                        command
                    }
                };

                let command = command
                    .replace("$input", &inputs.join(" "))
                    .replace("$output", &outputs.join(" "));
                build.cmdline = Some(command.clone());
                graph.add_build(build).unwrap();
            }
        }
    }

    if defaults.is_empty() {
        return Ok(None);
    }

    let mut hashed = n2graph::Hashes::default();
    let common = moonbuild_opt.raw_target_dir.join("common");

    let n2_db_path = common.join(build_type.dbname());
    let db = n2::db::open(&n2_db_path, &mut graph, &mut hashed).map_err(|e| N2Error {
        source: N2ErrorKind::DBOpenError(e),
    })?;
    Ok(Some(State {
        graph,
        db,
        hashes: hashed,
        default: defaults,
        pools: SmallMap::default(),
    }))
}
