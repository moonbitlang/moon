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

use moonutil::module::ModuleDB;

use moonutil::common::{MoonbuildOpt, MooncOpt};

use n2::graph::{self as n2graph, Build, BuildIns, BuildOuts, FileId, FileLoc};
use n2::load::State;
use n2::smallmap::SmallMap;
use std::rc::Rc;

use crate::gen::cmd_builder::CommandBuilder;
use crate::gen::n2_errors::{N2Error, N2ErrorKind};

pub fn load_moon_proj(
    m: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    let n2_input = super::fmt::gen_fmt(m, moonc_opt, moonbuild_opt)?;
    let state = if moonbuild_opt.fmt_opt.as_ref().unwrap().check {
        super::fmt::gen_n2_fmt_check_state(&n2_input, moonc_opt, moonbuild_opt)?
    } else {
        super::fmt::gen_n2_fmt_state(&n2_input, moonc_opt, moonbuild_opt)?
    };
    Ok(state)
}

#[derive(Debug)]
pub struct FmtItem {
    pub input: String,
    pub output: String,
    pub phony_out: String,
}

#[derive(Debug)]
pub struct N2FmtInput {
    pub items: Vec<FmtItem>,
}

pub fn gen_fmt(
    m: &ModuleDB,
    _moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<N2FmtInput> {
    let mut items = vec![];
    for (_, pkg) in m.get_all_packages().iter() {
        if pkg.is_third_party {
            continue;
        }
        for (f, _) in pkg
            .files
            .iter()
            .chain(pkg.wbtest_files.iter())
            .chain(pkg.test_files.iter())
        {
            let item = FmtItem {
                input: f.display().to_string(),
                output: moonbuild_opt
                    .target_dir
                    .join(PathBuf::from_iter(pkg.rel.components.iter()))
                    .join(f.file_name().unwrap().to_str().unwrap())
                    .display()
                    .to_string(),
                phony_out: moonbuild_opt
                    .target_dir
                    .join(PathBuf::from_iter(pkg.rel.components.iter()))
                    .join(f.file_name().unwrap().to_str().unwrap())
                    .display()
                    .to_string(),
            };
            items.push(item);
        }
    }
    Ok(N2FmtInput { items })
}

fn gen_inplace_fmt_command(
    graph: &mut n2graph::Graph,
    item: &FmtItem,
    moonbuild_opt: &MoonbuildOpt,
) -> (Build, FileId) {
    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("format")),
        line: 0,
    };

    let input_ids = vec![graph.files.id_from_canonical(item.input.clone())];

    let output_id = graph.files.id_from_canonical(item.phony_out.clone());
    let output_ids = vec![output_id];

    let ins = BuildIns {
        ids: input_ids,
        explicit: 1,
        implicit: 0,
        order_only: 0,
    };

    let outs = BuildOuts {
        ids: output_ids,
        explicit: 1,
    };

    let mut build = Build::new(loc, ins, outs);

    let command = CommandBuilder::new("moonfmt")
        .arg(&item.input)
        .arg("-w")
        .arg("-o")
        .arg(&item.phony_out)
        .args(&moonbuild_opt.fmt_opt.as_ref().unwrap().extra_args)
        .arg_with_cond(
            moonbuild_opt.fmt_opt.as_ref().unwrap().block_style,
            "-block-style",
        )
        .build();
    build.cmdline = Some(command);
    build.desc = Some(format!("moonfmt {}", item.input));
    (build, output_id)
}

pub fn gen_inplace_format_action(graph: &mut n2graph::Graph, ids: &[FileId]) -> (Build, FileId) {
    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("format")),
        line: 0,
    };

    let phony_out = graph
        .files
        .id_from_canonical("__inplace_format".to_string());

    let ins = BuildIns {
        ids: ids.into(),
        explicit: ids.len(),
        implicit: 0,
        order_only: 0,
    };

    let outs = BuildOuts {
        ids: vec![phony_out],
        explicit: 1,
    };

    (Build::new(loc, ins, outs), phony_out)
}

pub fn gen_n2_fmt_state(
    input: &N2FmtInput,
    _moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    let mut graph = n2graph::Graph::default();
    let mut default = vec![];
    let mut builds = vec![];

    for item in input.items.iter() {
        let (build, fid) = gen_inplace_fmt_command(&mut graph, item, moonbuild_opt);
        graph.add_build(build)?;
        builds.push(fid);
    }
    let (all_inplace_format_build, inplace_format_action) =
        gen_inplace_format_action(&mut graph, &builds);

    graph.add_build(all_inplace_format_build)?;
    default.push(inplace_format_action);

    let mut hashes = n2graph::Hashes::default();
    let n2_db_path = &moonbuild_opt.target_dir.join("format.db");
    let db = n2::db::open(n2_db_path, &mut graph, &mut hashes).map_err(|e| N2Error {
        source: N2ErrorKind::DBOpenError(e),
    })?;

    Ok(State {
        graph,
        db,
        hashes,
        default,
        pools: SmallMap::default(),
    })
}

fn gen_fmt_to_command(
    graph: &mut n2graph::Graph,
    item: &FmtItem,
    moonbuild_opt: &MoonbuildOpt,
) -> (Build, FileId) {
    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("format")),
        line: 0,
    };

    let input_ids = vec![graph.files.id_from_canonical(item.input.clone())];

    let output_id = graph.files.id_from_canonical(item.output.clone());
    let output_ids = vec![output_id];

    let ins = BuildIns {
        ids: input_ids,
        explicit: 1,
        implicit: 0,
        order_only: 0,
    };

    let outs = BuildOuts {
        ids: output_ids,
        explicit: 1,
    };

    let mut build = Build::new(loc, ins, outs);

    let command = CommandBuilder::new(
        &std::env::current_exe()
            .map_or_else(|_| "moon".into(), |x| x.to_string_lossy().into_owned()),
    )
    .arg("tool")
    .arg("format-and-diff")
    .arg("--old")
    .arg(&item.input)
    .arg("--new")
    .arg(&item.output)
    .arg_with_cond(
        moonbuild_opt.fmt_opt.as_ref().unwrap().block_style,
        "--block-style",
    )
    .args(&moonbuild_opt.fmt_opt.as_ref().unwrap().extra_args)
    .build();
    build.cmdline = Some(command);
    build.desc = Some(format!("moonfmt {}", item.input));

    (build, output_id)
}

pub fn gen_format_to_action(graph: &mut n2graph::Graph, ids: &[FileId]) -> (Build, FileId) {
    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("format")),
        line: 0,
    };

    let phony_out = graph.files.id_from_canonical("__format_to".to_string());

    let ins = BuildIns {
        ids: ids.into(),
        explicit: ids.len(),
        implicit: 0,
        order_only: 0,
    };

    let outs = BuildOuts {
        ids: vec![phony_out],
        explicit: 1,
    };

    (Build::new(loc, ins, outs), phony_out)
}

pub fn gen_n2_fmt_check_state(
    input: &N2FmtInput,
    _moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    assert!(moonbuild_opt.fmt_opt.as_ref().unwrap().check);
    let mut graph = n2graph::Graph::default();
    let mut default = vec![];
    let mut builds = vec![];

    for item in input.items.iter() {
        let (bs, fs) = gen_fmt_to_command(&mut graph, item, moonbuild_opt);
        graph.add_build(bs)?;
        builds.push(fs);
    }
    let (all_format_to_build, format_to_action) = gen_format_to_action(&mut graph, &builds);
    graph.add_build(all_format_to_build)?;
    default.push(format_to_action);

    let mut hashes = n2graph::Hashes::default();
    let n2_db_path = &moonbuild_opt.target_dir.join("format.db");
    let db = n2::db::open(n2_db_path, &mut graph, &mut hashes).map_err(|e| N2Error {
        source: N2ErrorKind::DBOpenError(e),
    })?;

    Ok(State {
        graph,
        db,
        hashes,
        default,
        pools: SmallMap::default(),
    })
}
