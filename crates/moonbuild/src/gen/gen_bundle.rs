use anyhow::bail;
use indexmap::IndexMap;
use moonutil::module::ModuleDB;
use moonutil::package::Package;
use n2::graph::{self as n2graph, Build, BuildIns, BuildOuts, FileLoc};
use n2::load::State;
use n2::smallmap::SmallMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use moonutil::common::{MoonbuildOpt, MooncOpt, MOON_PKG_JSON};

use super::cmd_builder::CommandBuilder;
use super::mdb::MiAlias;

#[derive(Debug)]
pub struct BundleDepItem {
    pub mi_out: String,
    pub core_out: String,
    pub mbt_deps: Vec<String>,
    pub mi_deps: Vec<MiAlias>,
    pub package_full_name: String,
    pub package_source_dir: String,
    pub is_main: bool,
}

#[derive(Debug)]
pub struct N2BundleAll {
    pub order: Vec<String>,
    pub name: String,
}

#[derive(Debug)]
pub struct N2BundleInput {
    pub bundle_items: Vec<BundleDepItem>,
    pub bundle_order: N2BundleAll,
}

pub fn gen_bundle(
    m: &ModuleDB,
    moonc_opt: &MooncOpt,
    _moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<N2BundleInput> {
    let mut dep_items = vec![];
    for (_, pkg) in m.packages.iter() {
        let item = pkg_to_bundle_item(&m.source_dir, &m.packages, pkg, moonc_opt)?;
        dep_items.push(item);
    }

    let nodes = super::util::toposort(m)?;
    let mut order = vec![];
    for node in nodes.iter() {
        let p = &m.packages[node];
        order.push(
            p.artifact
                .with_extension("core")
                .to_str()
                .unwrap()
                .to_string(),
        );
    }

    Ok(N2BundleInput {
        bundle_items: dep_items,
        bundle_order: N2BundleAll {
            order,
            name: m.name.split('/').last().unwrap_or("bundle").to_string(),
        },
    })
}

pub fn pkg_to_bundle_item(
    source_dir: &Path,
    packages: &IndexMap<String, Package>,
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<BundleDepItem> {
    let core_out = pkg.artifact.with_extension("core");
    let mi_out = pkg.artifact.with_extension("mi");

    let backend_filtered =
        moonutil::common::backend_filter(&pkg.files, moonc_opt.link_opt.target_backend);
    let mbt_deps = backend_filtered
        .iter()
        .filter(|f| {
            !f.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with("_test.mbt")
        })
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
    let package_source_dir: String = if pkg.rel.components.is_empty() {
        source_dir.display().to_string()
    } else {
        source_dir
            .join(pkg.rel.fs_full_name())
            .to_str()
            .unwrap()
            .into()
    };
    Ok(BundleDepItem {
        mi_out: mi_out.display().to_string(),
        core_out: core_out.display().to_string(),
        mbt_deps,
        mi_deps,
        package_full_name,
        package_source_dir,
        is_main: pkg.is_main,
    })
}

pub fn gen_build_command(
    graph: &mut n2graph::Graph,
    item: &BundleDepItem,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> Build {
    let _ = moonbuild_opt;
    let core_output_id = graph.files.id_from_canonical(item.core_out.clone());
    let mi_output_id = graph.files.id_from_canonical(item.mi_out.clone());

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("build")),
        line: 0,
    };

    let mut inputs = item.mbt_deps.clone();
    inputs.extend(item.mi_deps.iter().map(|a| a.name.clone()));
    let input_ids = inputs
        .iter()
        .map(|f| graph.files.id_from_canonical(f.clone()))
        .collect::<Vec<_>>();

    let mi_files_with_alias: Vec<String> = item
        .mi_deps
        .iter()
        .map(|a| format!("{}:{}", a.name, a.alias))
        .collect();

    let len = input_ids.len();

    let ins = BuildIns {
        ids: input_ids,
        explicit: len,
        implicit: 0,
        order_only: 0,
    };

    let outs = BuildOuts {
        ids: vec![core_output_id, mi_output_id],
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
        .arg("build-package")
        .args_with_cond(moonc_opt.render, vec!["-error-format", "json"])
        .args(&item.mbt_deps)
        .args_with_cond(!cur_pkg_warn_list.is_empty(), ["-w", cur_pkg_warn_list])
        .args_with_cond(
            !cur_pkg_alert_list.is_empty(),
            ["-alert", cur_pkg_alert_list],
        )
        .arg("-o")
        .arg(&item.core_out)
        .arg("-pkg")
        .arg(&item.package_full_name)
        .arg_with_cond(item.is_main, "-is-main")
        .args_with_prefix_separator(mi_files_with_alias, "-i")
        .arg("-pkg-sources")
        .arg(&format!(
            "{}:{}",
            &item.package_full_name, &item.package_source_dir
        ))
        .args(["-target", moonc_opt.build_opt.target_backend.to_flag()])
        .arg_with_cond(moonc_opt.build_opt.debug_flag, "-g")
        .arg_with_cond(moonc_opt.link_opt.source_map, "-source-map")
        .args(moonc_opt.extra_build_opt.iter())
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build
}

fn gen_bundle_all(
    graph: &mut n2graph::Graph,
    bundle_all: &N2BundleAll,
    target_dir: &Path,
    _moonc_opt: &MooncOpt,
) -> Build {
    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("bundle")),
        line: 0,
    };

    let out = target_dir.join(&bundle_all.name).with_extension("core");
    let core_output_id = graph.files.id_from_canonical(out.display().to_string());

    let input_ids = bundle_all
        .order
        .iter()
        .map(|f| graph.files.id_from_canonical(f.clone()))
        .collect::<Vec<_>>();

    let len = input_ids.len();

    let ins = BuildIns {
        ids: input_ids,
        explicit: len,
        implicit: 0,
        order_only: 0,
    };

    let outs = BuildOuts {
        ids: vec![core_output_id],
        explicit: 1,
    };

    let mut build = Build::new(loc, ins, outs);

    let command = CommandBuilder::new("moonc")
        .arg("bundle-core")
        .args(bundle_all.order.iter())
        .arg("-o")
        .arg(out.to_str().unwrap())
        .build();

    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build
}

pub fn gen_n2_bundle_state(
    input: &N2BundleInput,
    target_dir: &Path,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    let mut graph = n2graph::Graph::default();

    for item in input.bundle_items.iter() {
        let build = gen_build_command(&mut graph, item, moonc_opt, moonbuild_opt);
        graph.add_build(build)?;
    }

    {
        let build = gen_bundle_all(&mut graph, &input.bundle_order, target_dir, moonc_opt);
        graph.add_build(build)?;
    }

    let default = graph.get_start_nodes();

    let mut hashes = n2graph::Hashes::default();
    let db = n2::db::open(&target_dir.join("build.moon_db"), &mut graph, &mut hashes)?;

    Ok(State {
        graph,
        db,
        hashes,
        default,
        pools: SmallMap::default(),
    })
}
