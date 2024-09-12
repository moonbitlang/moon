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

use super::cmd_builder::CommandBuilder;
use super::n2_errors::{N2Error, N2ErrorKind};
use crate::gen::MiAlias;
use anyhow::bail;
use indexmap::map::IndexMap;
use moonutil::module::ModuleDB;
use moonutil::package::Package;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use moonutil::common::{get_desc_name, MoonbuildOpt, MooncOpt, MOON_PKG_JSON};
use n2::graph::{self as n2graph, Build, BuildIns, BuildOuts, FileLoc};
use n2::load::State;
use n2::smallmap::SmallMap;

#[derive(Debug)]
pub struct CheckDepItem {
    pub mi_out: String,
    pub mbt_deps: Vec<String>,
    pub mi_deps: Vec<MiAlias>,
    pub package_full_name: String,
    pub package_source_dir: String,
    pub is_main: bool,
}

#[derive(Debug)]
pub struct N2CheckInput {
    pub dep_items: Vec<CheckDepItem>,
}

fn pkg_to_check_item(
    source_dir: &Path,
    packages: &IndexMap<String, Package>,
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<CheckDepItem> {
    let out = pkg.artifact.with_extension("mi");

    let backend_filtered = moonutil::common::backend_filter(&pkg.files, moonc_opt);
    let mbt_deps: Vec<String> = backend_filtered
        .iter()
        .map(|f| f.display().to_string())
        .collect::<Vec<_>>();

    let mut mi_deps = vec![];

    for dep in pkg.imports.iter() {
        let full_import_name = dep.path.make_full_path();
        if !packages.contains_key(&full_import_name) {
            bail!(
                "{}: the imported package `{}` could not be located.",
                source_dir
                    .join(pkg.rel.fs_full_name())
                    .join(MOON_PKG_JSON)
                    .display(),
                full_import_name,
            );
        }
        let cur_pkg = &packages[&full_import_name];
        let d = cur_pkg.artifact.with_extension("mi");
        let alias = dep.alias.clone().unwrap_or(cur_pkg.last_name().into());
        mi_deps.push(MiAlias {
            name: d.display().to_string(),
            alias,
        });
    }

    let package_full_name = pkg.full_name();
    let package_source_dir: String = pkg.root_path.to_string_lossy().into_owned();

    Ok(CheckDepItem {
        mi_out: out.display().to_string(),
        mbt_deps,
        mi_deps,
        package_full_name,
        package_source_dir,
        is_main: pkg.is_main,
    })
}

fn pkg_with_wbtest_to_check_item(
    source_dir: &Path,
    packages: &IndexMap<String, Package>,
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<CheckDepItem> {
    let out = pkg
        .artifact
        .with_file_name(format!("{}.whitebox_test.mi", pkg.last_name()));

    let mut files_and_con = IndexMap::new();
    files_and_con.extend(
        pkg.files
            .iter()
            .chain(pkg.wbtest_files.iter())
            .map(|(p, c)| (p.clone(), c.clone())),
    );

    let backend_filtered = moonutil::common::backend_filter(&files_and_con, moonc_opt);
    let mbt_deps: Vec<String> = backend_filtered
        .iter()
        .map(|f| f.display().to_string())
        .collect::<Vec<_>>();

    let mut mi_deps = vec![];

    for dep in pkg.imports.iter().chain(pkg.wbtest_imports.iter()) {
        let full_import_name = dep.path.make_full_path();
        if !packages.contains_key(&full_import_name) {
            bail!(
                "{}: the imported package `{}` could not be located.",
                source_dir
                    .join(pkg.rel.fs_full_name())
                    .join(MOON_PKG_JSON)
                    .display(),
                full_import_name,
            );
        }
        let cur_pkg = &packages[&full_import_name];
        let d = cur_pkg.artifact.with_extension("mi");
        let alias = dep.alias.clone().unwrap_or(cur_pkg.last_name().into());
        mi_deps.push(MiAlias {
            name: d.display().to_string(),
            alias,
        });
    }

    let package_full_name = pkg.full_name();
    let package_source_dir: String = pkg.root_path.to_string_lossy().into_owned();

    Ok(CheckDepItem {
        mi_out: out.display().to_string(),
        mbt_deps,
        mi_deps,
        package_full_name,
        package_source_dir,
        is_main: pkg.is_main,
    })
}

fn pkg_with_test_to_check_item(
    source_dir: &Path,
    packages: &IndexMap<String, Package>,
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<CheckDepItem> {
    if pkg
        .test_imports
        .iter()
        .chain(pkg.imports.iter())
        .any(|import| {
            import
                .alias
                .as_ref()
                .map_or(false, |alias| alias.eq(pkg.last_name()))
        })
    {
        bail!(
            "Duplicate alias `{}` at \"{}\". \
            \"test-import\" will automatically add \"import\" and current pkg as dependency so you don't need to add it manually. \
            If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias.",
            pkg.last_name(), pkg.rel.components.join("/") + "/" + MOON_PKG_JSON
        );
    }

    let out = pkg
        .artifact
        .with_file_name(format!("{}.blackbox_test.mi", pkg.last_name()));

    let backend_filtered = moonutil::common::backend_filter(&pkg.test_files, moonc_opt);
    let mbt_deps: Vec<String> = backend_filtered
        .iter()
        .map(|f| f.display().to_string())
        .collect::<Vec<_>>();

    // add cur pkg as .mi dependency
    let mut mi_deps = vec![MiAlias {
        name: pkg
            .artifact
            .with_file_name(format!("{}.mi", pkg.last_name()))
            .display()
            .to_string(),
        alias: pkg.last_name().into(),
    }];

    for dep in pkg.imports.iter().chain(pkg.test_imports.iter()) {
        let full_import_name = dep.path.make_full_path();
        if !packages.contains_key(&full_import_name) {
            bail!(
                "{}: the imported package `{}` could not be located.",
                source_dir
                    .join(pkg.rel.fs_full_name())
                    .join(MOON_PKG_JSON)
                    .display(),
                full_import_name,
            );
        }
        let cur_pkg = &packages[&full_import_name];
        let d = cur_pkg.artifact.with_extension("mi");
        let alias = dep.alias.clone().unwrap_or(cur_pkg.last_name().into());
        mi_deps.push(MiAlias {
            name: d.display().to_string(),
            alias,
        });
    }

    // this is used for `-pkg` flag in `moonc check`, shouldn't be `pkg.full_name()` since we aren't check that package, otherwise we might encounter an error like "4015] Error: Type StructName has no method method_name"(however, StructName does has method method_name).
    // actually, `-pkg` flag is not necessary for blackbox test, but we still keep it for consistency
    let package_full_name = pkg.full_name() + "_blackbox_test";
    let package_source_dir: String = pkg.root_path.to_string_lossy().into_owned();

    Ok(CheckDepItem {
        mi_out: out.display().to_string(),
        mbt_deps,
        mi_deps,
        package_full_name,
        package_source_dir,
        is_main: pkg.is_main,
    })
}

pub fn gen_check(
    m: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<N2CheckInput> {
    let _ = moonc_opt;
    let _ = moonbuild_opt;
    let mut dep_items = vec![];
    for (_, pkg) in m.packages.iter() {
        let item = pkg_to_check_item(&pkg.root_path, &m.packages, pkg, moonc_opt)?;
        dep_items.push(item);
        if !pkg.wbtest_files.is_empty() {
            let item = pkg_with_wbtest_to_check_item(&pkg.root_path, &m.packages, pkg, moonc_opt)?;
            dep_items.push(item);
        }
        if !pkg.test_files.is_empty() {
            let item = pkg_with_test_to_check_item(&pkg.root_path, &m.packages, pkg, moonc_opt)?;
            dep_items.push(item);
        }
    }
    Ok(N2CheckInput { dep_items })
}

pub fn gen_check_command(
    graph: &mut n2graph::Graph,
    item: &CheckDepItem,
    moonc_opt: &MooncOpt,
) -> Build {
    let mi_output_id = graph.files.id_from_canonical(item.mi_out.clone());
    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("check")),
        line: 0,
    };

    let mut inputs = item.mbt_deps.clone();
    inputs.extend(item.mi_deps.iter().map(|a| a.name.clone()));

    let input_ids = inputs
        .into_iter()
        .map(|f| graph.files.id_from_canonical(f))
        .collect::<Vec<_>>();

    let len = input_ids.len();

    let ins = BuildIns {
        ids: input_ids,
        explicit: len,
        implicit: 0,
        order_only: 0,
    };

    let mi_files_with_alias: Vec<String> = item
        .mi_deps
        .iter()
        .map(|a| format!("{}:{}", a.name, a.alias))
        .collect();

    let outs = BuildOuts {
        ids: vec![mi_output_id],
        explicit: 1,
    };

    let mut build = Build::new(loc, ins, outs);

    let cur_pkg_warn_list = match moonc_opt.build_opt.warn_lists.get(&item.package_full_name) {
        Some(Some(warn_list)) => warn_list,
        _ => "",
    };
    let cur_pkg_alert_list = match moonc_opt.build_opt.alert_lists.get(&item.package_full_name) {
        Some(Some(alert_list)) => alert_list,
        _ => "",
    };

    let command = CommandBuilder::new("moonc")
        .arg("check")
        .args_with_cond(moonc_opt.render, vec!["-error-format", "json"])
        .args_with_cond(
            moonc_opt.build_opt.deny_warn,
            // the default strategy for warn and alert is +a and +all-raise-throw-unsafe+deprecated
            // we replace + with @ to tell moonc treat warning as error
            ["-w", "@a", "-alert", "@all-raise-throw-unsafe+deprecated"],
        )
        .args(&item.mbt_deps)
        .args_with_cond(!cur_pkg_warn_list.is_empty(), ["-w", cur_pkg_warn_list])
        .args_with_cond(
            !cur_pkg_alert_list.is_empty(),
            ["-alert", cur_pkg_alert_list],
        )
        .arg("-o")
        .arg(&item.mi_out)
        .arg("-pkg")
        .arg(&item.package_full_name)
        .arg_with_cond(item.is_main, "-is-main")
        .args_with_cond(
            !moonc_opt.nostd,
            [
                "-std-path",
                moonutil::moon_dir::core_bundle(moonc_opt.link_opt.target_backend)
                    .to_str()
                    .unwrap(),
            ],
        )
        .args_with_prefix_separator(mi_files_with_alias, "-i")
        .arg("-pkg-sources")
        .arg(&format!(
            "{}:{}",
            &item.package_full_name, &item.package_source_dir
        ))
        .args(["-target", moonc_opt.build_opt.target_backend.to_flag()])
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build.desc = Some(format!(
        "check: {}",
        get_desc_name(&item.package_full_name, &item.mi_out)
    ));
    build
}

pub fn gen_n2_check_state(
    input: &N2CheckInput,
    target_dir: &Path,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    let _ = moonbuild_opt;
    let mut graph = n2graph::Graph::default();

    for item in input.dep_items.iter() {
        let build = gen_check_command(&mut graph, item, moonc_opt);
        graph.add_build(build)?;
    }

    let mut hashes = n2graph::Hashes::default();
    let n2_db_path = &target_dir.join("check.moon_db");
    let db = n2::db::open(n2_db_path, &mut graph, &mut hashes).map_err(|e| N2Error {
        source: N2ErrorKind::DBOpenError(e),
    })?;

    let default = graph.get_start_nodes();

    Ok(State {
        graph,
        db,
        hashes,
        default,
        pools: SmallMap::default(),
    })
}
