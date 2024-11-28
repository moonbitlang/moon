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

use moonutil::common::{get_desc_name, CheckOpt, MoonbuildOpt, MooncOpt, MOON_PKG_JSON};
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
    pub warn_list: Option<String>,
    pub alert_list: Option<String>,
    pub is_main: bool,
    pub patch_file: Option<PathBuf>,
    pub no_mi: bool,
    pub is_whitebox_test: bool,
    pub is_blackbox_test: bool,
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

    let backend_filtered = moonutil::common::backend_filter(
        &pkg.files,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );
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
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: pkg.is_main,
        is_whitebox_test: false,
        is_blackbox_test: false,
        patch_file: pkg.patch_file.as_ref().and_then(|p| {
            let file_stem = p.file_stem().unwrap().to_str().unwrap();
            (!file_stem.ends_with("_wbtest") && !file_stem.ends_with("_test")).then_some(p.clone())
        }),
        no_mi: pkg.no_mi,
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

    let backend_filtered = moonutil::common::backend_filter(
        &files_and_con,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );
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
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: pkg.is_main,
        is_whitebox_test: true,
        is_blackbox_test: false,
        patch_file: pkg.patch_file.as_ref().and_then(|p| {
            p.file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with("_wbtest")
                .then_some(p.clone())
        }),
        no_mi: pkg.no_mi,
    })
}

fn pkg_with_test_to_check_item(
    source_dir: &Path,
    packages: &IndexMap<String, Package>,
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<CheckDepItem> {
    let self_in_test_import = pkg.test_imports.iter().any(|import| {
        import.path.make_full_path() == format!("{}/{}", pkg.root.full_name(), pkg.rel.full_name())
    });

    if !self_in_test_import
        && pkg
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
            pkg.last_name(), pkg.root_path.join(MOON_PKG_JSON).display()
        );
    }

    let out = pkg
        .artifact
        .with_file_name(format!("{}.blackbox_test.mi", pkg.last_name()));

    let backend_filtered = moonutil::common::backend_filter(
        &pkg.test_files,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );
    let mbt_deps: Vec<String> = backend_filtered
        .iter()
        .map(|f| f.display().to_string())
        .collect::<Vec<_>>();

    let mut mi_deps = vec![];

    // add cur pkg as .mi dependency if it's not set in test_imports
    if !self_in_test_import {
        mi_deps.push(MiAlias {
            name: pkg
                .artifact
                .with_file_name(format!("{}.mi", pkg.last_name()))
                .display()
                .to_string(),
            alias: pkg.last_name().into(),
        });
    }

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
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: pkg.is_main,
        is_whitebox_test: false,
        is_blackbox_test: true,
        patch_file: pkg.patch_file.as_ref().and_then(|p| {
            p.file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with("_test")
                .then_some(p.clone())
        }),
        no_mi: pkg.no_mi,
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

    // if pkg is specified, check that pkg and it's deps; if no pkg specified, check all pkgs
    let pkgs_to_check = if let Some(CheckOpt {
        package_path: Some(pkg_path),
        ..
    }) = moonbuild_opt.check_opt.as_ref()
    {
        &m.get_filtered_packages_and_its_deps_by_pkgpath(&moonbuild_opt.source_dir.join(pkg_path))
    } else {
        m.get_all_packages()
    };

    for (_, pkg) in pkgs_to_check {
        let item = pkg_to_check_item(&pkg.root_path, pkgs_to_check, pkg, moonc_opt)?;
        dep_items.push(item);

        if !pkg.wbtest_files.is_empty() {
            let item =
                pkg_with_wbtest_to_check_item(&pkg.root_path, pkgs_to_check, pkg, moonc_opt)?;
            dep_items.push(item);
        }
        if !pkg.test_files.is_empty() {
            let item = pkg_with_test_to_check_item(&pkg.root_path, pkgs_to_check, pkg, moonc_opt)?;
            dep_items.push(item);
        }
    }

    // dbg!(&dep_items);
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

    let command = CommandBuilder::new("moonc")
        .arg("check")
        .arg_with_cond(item.patch_file.is_some(), "-patch-file")
        .lazy_args_with_cond(item.patch_file.is_some(), || {
            vec![item.patch_file.as_ref().unwrap().display().to_string()]
        })
        .arg_with_cond(item.no_mi, "-no-mi")
        .args_with_cond(moonc_opt.render, vec!["-error-format", "json"])
        .args_with_cond(
            moonc_opt.build_opt.deny_warn,
            // the default strategy for warn and alert is +a-31-32 and +all-raise-throw-unsafe+deprecated
            // we replace + with @ to tell moonc treat warning as error
            [
                "-w",
                "@a-31-32",
                "-alert",
                "@all-raise-throw-unsafe+deprecated",
            ],
        )
        .args(&item.mbt_deps)
        .lazy_args_with_cond(item.warn_list.is_some(), || {
            vec!["-w".to_string(), item.warn_list.clone().unwrap()]
        })
        .lazy_args_with_cond(item.alert_list.is_some(), || {
            vec!["-alert".to_string(), item.alert_list.clone().unwrap()]
        })
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
        .arg_with_cond(item.is_whitebox_test, "-whitebox-test")
        .arg_with_cond(item.is_blackbox_test, "-blackbox-test")
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
