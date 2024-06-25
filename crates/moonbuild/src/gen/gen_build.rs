use anyhow::{bail, Ok};
use moonutil::common::gen::{ModuleDB, Package};

use super::cmd_builder::CommandBuilder;
use super::mdb::Alias;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use moonutil::common::{JsFormat, MoonbuildOpt, MooncOpt, MOON_PKG_JSON};
use n2::graph::{self as n2graph, Build, BuildIns, BuildOuts, FileLoc};
use n2::load::State;
use n2::smallmap::SmallMap;

#[derive(Debug)]
pub struct BuildDepItem {
    pub core_out: String,
    pub mi_out: String,
    pub mbt_deps: Vec<String>,
    pub mi_deps: Vec<Alias>, // do not need add parent's mi files
    pub package_full_name: String,
    pub package_source_dir: String,
    pub is_main: bool,
    pub is_third_party: bool,
}

#[derive(Debug)]
pub struct LinkDepItem {
    pub out: String,
    pub core_deps: Vec<String>, // need add parent's core files recursively
    pub package_full_name: String,
    pub package_sources: Vec<(String, String)>, // (pkgname, source_dir)

    pub link: Option<moonutil::common::Link>,
}

#[derive(Debug)]
pub struct N2BuildInput {
    pub build_items: Vec<BuildDepItem>,
    pub link_items: Vec<LinkDepItem>, // entry points
}

pub fn gen_build_build_item(
    m: &ModuleDB,
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<BuildDepItem> {
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
        if !m.packages.contains_key(&full_import_name) {
            bail!(
                "{}: the imported package `{}` could not be located.",
                m.source_dir
                    .join(pkg.rel.fs_full_name())
                    .join(MOON_PKG_JSON)
                    .display(),
                full_import_name,
            );
        }
        let cur_pkg = &m.packages[&full_import_name];
        let d = cur_pkg.artifact.with_extension("mi");
        let alias = dep.alias.clone().unwrap_or(cur_pkg.last_name().into());
        mi_deps.push(Alias {
            name: d.display().to_string(),
            alias,
        });
    }

    let package_full_name = pkg.full_name();
    let package_source_dir: String = pkg.root_path.to_string_lossy().into_owned();

    Ok(BuildDepItem {
        core_out: core_out.display().to_string(),
        mi_out: mi_out.display().to_string(),
        mbt_deps,
        mi_deps,
        package_full_name,
        package_source_dir,
        is_main: pkg.is_main,
        is_third_party: pkg.is_third_party,
    })
}

pub fn gen_build_link_item(
    m: &ModuleDB,
    pkg: &Package,
    _moonc_opt: &MooncOpt,
) -> anyhow::Result<LinkDepItem> {
    let out = pkg.artifact.with_extension("wat"); // TODO: extension is determined by build option
    let package_full_name = pkg.full_name();

    let tp = super::util::topo_from_node(m, pkg)?;
    let core_deps = super::util::nodes_to_cores(m, &tp);
    let package_sources = super::util::nodes_to_pkg_sources(m, &tp);

    Ok(LinkDepItem {
        out: out.display().to_string(),
        core_deps,
        package_sources,
        package_full_name,
        link: pkg.link.clone(),
    })
}

pub fn gen_build(
    m: &ModuleDB,
    moonc_opt: &MooncOpt,
    _moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<N2BuildInput> {
    let mut build_items = vec![];
    let mut link_items = vec![];
    for (i, (_, pkg)) in m.packages.iter().enumerate() {
        let is_main = m.entries.contains(&i);

        if is_main {
            // entry also need build
            build_items.push(gen_build_build_item(m, pkg, moonc_opt)?);
            // link need add *.core files recursively
            link_items.push(gen_build_link_item(m, pkg, moonc_opt)?);
            continue;
        }

        if pkg.need_link {
            build_items.push(gen_build_build_item(m, pkg, moonc_opt)?);
            link_items.push(gen_build_link_item(m, pkg, moonc_opt)?);
            continue;
        }

        {
            build_items.push(gen_build_build_item(m, pkg, moonc_opt)?);
        }
    }
    Ok(N2BuildInput {
        build_items,
        link_items,
    })
}

pub fn gen_build_command(
    graph: &mut n2graph::Graph,
    item: &BuildDepItem,
    moonc_opt: &MooncOpt,
) -> Build {
    let core_output_id = graph.files.id_from_canonical(item.core_out.clone());
    let mi_output_id = graph.files.id_from_canonical(item.mi_out.clone());

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("build")),
        line: 0,
    };

    let mut inputs = item.mbt_deps.clone();
    inputs.extend(item.mi_deps.iter().map(|a| a.name.clone()));
    let input_ids = inputs
        .into_iter()
        .map(|f| graph.files.id_from_canonical(f))
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

    // WORKAROUND: do not test coverage on coverage library itself, because of cyclic dependency
    let enable_coverage = moonc_opt.build_opt.enable_coverage
        && !super::is_skip_coverage_lib(&item.package_full_name)
        && !item.is_third_party;
    // WORKAROUND: lang core/builtin and core/coverage should be able to cover themselves
    let self_coverage = enable_coverage && super::is_self_coverage_lib(&item.package_full_name);

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
        .arg(&item.core_out)
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
        .arg_with_cond(moonc_opt.build_opt.debug_flag, "-g")
        .arg_with_cond(moonc_opt.link_opt.source_map, "-source-map")
        .arg_with_cond(enable_coverage, "-enable-coverage")
        .arg_with_cond(self_coverage, "-coverage-package-override=@self")
        .args(moonc_opt.extra_build_opt.iter())
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build
}

pub fn gen_link_command(
    graph: &mut n2graph::Graph,
    item: &LinkDepItem,
    moonc_opt: &MooncOpt,
) -> (Build, n2graph::FileId) {
    let artifact_output_path = PathBuf::from(&item.out)
        .with_extension(moonc_opt.link_opt.output_format.to_str())
        .display()
        .to_string();

    let artifact_id = graph.files.id_from_canonical(artifact_output_path.clone());

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("build")),
        line: 0,
    };

    let input_ids = item
        .core_deps
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
        ids: vec![artifact_id],
        explicit: 1,
    };

    let mut build = Build::new(loc, ins, outs);

    let exports = match moonc_opt.link_opt.target_backend {
        moonutil::common::TargetBackend::Wasm => item
            .link
            .as_ref()
            .and_then(|l| l.wasm.as_ref())
            .and_then(|w| w.exports.as_ref()),
        moonutil::common::TargetBackend::WasmGC => item
            .link
            .as_ref()
            .and_then(|l| l.wasm_gc.as_ref())
            .and_then(|w| w.exports.as_ref()),
        moonutil::common::TargetBackend::Js => item
            .link
            .as_ref()
            .and_then(|l| l.js.as_ref())
            .and_then(|w| w.exports.as_ref()),
    };

    let export_memory_name = match moonc_opt.link_opt.target_backend {
        moonutil::common::TargetBackend::Wasm => item
            .link
            .as_ref()
            .and_then(|l| l.wasm.as_ref())
            .and_then(|w| w.export_memory_name.as_ref())
            .map(|s| s.to_string()),
        moonutil::common::TargetBackend::WasmGC => item
            .link
            .as_ref()
            .and_then(|l| l.wasm_gc.as_ref())
            .and_then(|w| w.export_memory_name.as_ref())
            .map(|s| s.to_string()),
        moonutil::common::TargetBackend::Js => None,
    };

    let link_flags: Option<Vec<String>> = match moonc_opt.link_opt.target_backend {
        moonutil::common::TargetBackend::Wasm => item
            .link
            .as_ref()
            .and_then(|l| l.wasm.as_ref())
            .and_then(|w| w.flags.as_ref())
            .cloned(),
        moonutil::common::TargetBackend::WasmGC => item
            .link
            .as_ref()
            .and_then(|l| l.wasm_gc.as_ref())
            .and_then(|w| w.flags.as_ref())
            .cloned(),
        moonutil::common::TargetBackend::Js => None,
    };

    let command = CommandBuilder::new("moonc")
        .arg("link-core")
        .arg_with_cond(
            !moonc_opt.nostd,
            moonutil::moon_dir::core_core(moonc_opt.link_opt.target_backend)
                .to_str()
                .unwrap(),
        )
        .args(&item.core_deps)
        .arg("-main")
        .arg(&item.package_full_name)
        .arg("-o")
        .arg(&artifact_output_path)
        .args_with_prefix_separator(
            item.package_sources
                .iter()
                .map(|(pkg, src)| format!("{}:{}", pkg, src)),
            "-pkg-sources",
        )
        .args_with_cond(
            !moonc_opt.nostd,
            [
                "-pkg-sources",
                &format!("moonbitlang/core:{}", &moonutil::moon_dir::core().display()),
            ],
        )
        .args(["-target", moonc_opt.link_opt.target_backend.to_flag()])
        .arg_with_cond(moonc_opt.link_opt.debug_flag, "-g")
        .arg_with_cond(moonc_opt.link_opt.source_map, "-source-map")
        .lazy_args_with_cond(exports.is_some(), || {
            let es = exports.unwrap();
            if es.is_empty() {
                vec!["".to_string()]
            } else {
                vec![format!(
                    "-exported_functions={}",
                    exports.unwrap().join(",")
                )]
            }
        })
        .lazy_args_with_cond(export_memory_name.is_some(), || {
            vec![
                "-export-memory-name".to_string(),
                export_memory_name.unwrap().to_string(),
            ]
        })
        .lazy_args_with_cond(link_flags.is_some(), || link_flags.unwrap())
        .lazy_args_with_cond(
            moonc_opt.link_opt.target_backend == moonutil::common::TargetBackend::Js
                && item.link.is_some()
                && item.link.as_ref().unwrap().js.is_some(),
            || {
                let js = item.link.as_ref().unwrap().js.as_ref().unwrap();
                if js.format.is_some() {
                    vec![
                        "-js-format".to_string(),
                        js.format.unwrap().to_flag().to_string(),
                    ]
                } else {
                    vec![
                        "-js-format".to_string(),
                        JsFormat::default().to_flag().to_string(),
                    ]
                }
            },
        )
        .args(moonc_opt.extra_link_opt.iter())
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    (build, artifact_id)
}

pub fn gen_n2_build_state(
    input: &N2BuildInput,
    target_dir: &Path,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    let _ = moonbuild_opt;
    let mut graph = n2graph::Graph::default();
    let mut default = vec![];

    for item in input.build_items.iter() {
        let build = gen_build_command(&mut graph, item, moonc_opt);
        graph.add_build(build)?;
    }
    for item in input.link_items.iter() {
        let (build, fid) = gen_link_command(&mut graph, item, moonc_opt);
        default.push(fid);
        graph.add_build(build)?;
    }

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
