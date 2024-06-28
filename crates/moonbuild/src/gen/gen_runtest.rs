use anyhow::{bail, Ok};
use colored::Colorize;
use moonutil::common::gen::GeneratedTestDriver;
use moonutil::module::ModuleDB;
use moonutil::package::Package;

use super::cmd_builder::CommandBuilder;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use moonutil::common::{MoonbuildOpt, MooncOpt, MOON_PKG_JSON};
use n2::graph::{self as n2graph, Build, BuildIns, BuildOuts, FileLoc};
use n2::load::State;
use n2::smallmap::SmallMap;

use super::mdb::Alias;

#[derive(Debug)]
pub struct RuntestDepItem {
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
pub struct RuntestLinkDepItem {
    pub out: String,
    pub core_deps: Vec<String>, // need add parent's core files recursively
    pub package_full_name: String,
    pub package_sources: Vec<(String, String)>, // (pkgname, source_dir)
    pub is_main: bool,
    pub link: Option<moonutil::package::Link>,
}

#[derive(Debug)]
pub struct N2RuntestInput<'a> {
    pub build_items: Vec<RuntestDepItem>,
    pub link_items: Vec<RuntestLinkDepItem>, // entry points
    pub driver_files: Vec<&'a Path>,
    pub files_contain_test_block: Vec<&'a Path>,
}

pub fn gen_package_core(
    m: &ModuleDB,
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<RuntestDepItem> {
    let core_out = pkg.artifact.with_extension("core");
    let mi_out = pkg.artifact.with_extension("mi");

    let backend_filtered: Vec<PathBuf> =
        moonutil::common::backend_filter(&pkg.files, moonc_opt.link_opt.target_backend);
    let mbt_deps = backend_filtered
        .iter()
        .map(|f| f.display().to_string())
        .collect();

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
    let package_source_dir: String = m
        .source_dir
        .join(pkg.rel.fs_full_name())
        .display()
        .to_string();

    Ok(RuntestDepItem {
        core_out: core_out.display().to_string(),
        mi_out: mi_out.display().to_string(),
        mbt_deps,
        mi_deps,
        package_full_name,
        package_source_dir,
        is_main: false,
        is_third_party: pkg.is_third_party,
    })
}

pub fn gen_package_internal_test(
    m: &ModuleDB,
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<RuntestDepItem> {
    let pkgname = pkg.artifact.file_stem().unwrap().to_str().unwrap();
    let core_out = pkg
        .artifact
        .with_file_name(format!("{}.internal_test.core", pkgname));
    let mi_out = pkg
        .artifact
        .with_file_name(format!("{}.internal_test.mi", pkgname));

    let backend_filtered =
        moonutil::common::backend_filter(&pkg.files, moonc_opt.link_opt.target_backend);
    let mut mbt_deps: Vec<String> = backend_filtered
        .iter()
        .map(|f| f.display().to_string())
        .collect();

    for item in pkg.generated_test_drivers.iter() {
        if let GeneratedTestDriver::InternalTest(path) = item {
            mbt_deps.push(path.display().to_string());
        }
    }

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
    let package_source_dir: String = m
        .source_dir
        .join(pkg.rel.fs_full_name())
        .display()
        .to_string();

    Ok(RuntestDepItem {
        core_out: core_out.display().to_string(),
        mi_out: mi_out.display().to_string(),
        mbt_deps,
        mi_deps,
        package_full_name,
        package_source_dir,
        is_main: true,
        is_third_party: pkg.is_third_party,
    })
}

pub fn gen_package_underscore_test(
    m: &ModuleDB,
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<RuntestDepItem> {
    let pkgname = pkg.artifact.file_stem().unwrap().to_str().unwrap();
    let core_out = pkg
        .artifact
        .with_file_name(format!("{}.underscore_test.core", pkgname));
    let mi_out = pkg
        .artifact
        .with_file_name(format!("{}.underscore_test.mi", pkgname));

    let backend_filtered =
        moonutil::common::backend_filter(&pkg.files, moonc_opt.link_opt.target_backend);
    let mut mbt_deps: Vec<String> = backend_filtered
        .iter()
        .chain(pkg.test_files.iter())
        .map(|f| f.display().to_string())
        .collect();

    for item in pkg.generated_test_drivers.iter() {
        if let GeneratedTestDriver::UnderscoreTest(path) = item {
            mbt_deps.push(path.display().to_string());
        }
    }

    let mut mi_deps = vec![];
    for dep in pkg.imports.iter().chain(pkg.test_imports.iter()) {
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
    let package_source_dir: String = m
        .source_dir
        .join(pkg.rel.fs_full_name())
        .display()
        .to_string();

    Ok(RuntestDepItem {
        core_out: core_out.display().to_string(),
        mi_out: mi_out.display().to_string(),
        mbt_deps,
        mi_deps,
        package_full_name,
        package_source_dir,
        is_main: true,
        is_third_party: pkg.is_third_party,
    })
}

fn get_pkg_topo_order<'a>(m: &'a ModuleDB, leaf: &Package) -> Vec<&'a Package> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut pkg_topo_order: Vec<&Package> = vec![];
    fn dfs<'a>(
        m: &'a ModuleDB,
        pkg_topo_order: &mut Vec<&'a Package>,
        visited: &mut HashSet<String>,
        cur_pkg_full_name: &String,
        top: bool,
    ) {
        if visited.contains(cur_pkg_full_name) {
            return;
        }
        visited.insert(cur_pkg_full_name.clone());
        let cur_pkg = &m.packages[cur_pkg_full_name];
        if top {
            for dep in cur_pkg.imports.iter().chain(cur_pkg.test_imports.iter()) {
                dfs(
                    m,
                    pkg_topo_order,
                    visited,
                    &dep.path.make_full_path(),
                    false,
                );
            }
        } else {
            for dep in cur_pkg.imports.iter() {
                dfs(
                    m,
                    pkg_topo_order,
                    visited,
                    &dep.path.make_full_path(),
                    false,
                );
            }
        }
        pkg_topo_order.push(cur_pkg);
    }
    dfs(
        m,
        &mut pkg_topo_order,
        &mut visited,
        &leaf.full_name(),
        true,
    );
    pkg_topo_order
}

fn get_package_sources(m: &ModuleDB, pkg_topo_order: &[&Package]) -> Vec<(String, String)> {
    let mut package_sources = vec![];
    for cur_pkg in pkg_topo_order {
        let package_source_dir: String = if cur_pkg.rel.components.is_empty() {
            m.source_dir.display().to_string()
        } else {
            m.source_dir
                .join(cur_pkg.rel.fs_full_name())
                .display()
                .to_string()
        };
        package_sources.push((cur_pkg.full_name(), package_source_dir));
    }
    package_sources
}

pub fn gen_link_internal_test(
    m: &ModuleDB,
    pkg: &Package,
    _moonc_opt: &MooncOpt,
) -> anyhow::Result<RuntestLinkDepItem> {
    let out = pkg
        .artifact
        .with_file_name(format!("{}.internal_test.wat", pkg.last_name()));

    let pkg_topo_order: Vec<&Package> = get_pkg_topo_order(m, pkg);

    let mut core_deps = vec![];
    for cur_pkg in pkg_topo_order.iter() {
        let d = if cur_pkg.full_name() == pkg.full_name() {
            cur_pkg
                .artifact
                .with_file_name(format!("{}.internal_test.core", cur_pkg.last_name()))
        } else {
            cur_pkg.artifact.with_extension("core")
        };
        core_deps.push(d.display().to_string());
    }
    let package_sources = get_package_sources(m, &pkg_topo_order);

    let package_full_name = pkg.full_name();

    Ok(RuntestLinkDepItem {
        out: out.display().to_string(),
        core_deps,
        package_full_name,
        package_sources,
        is_main: true,
        link: pkg.link.clone(),
    })
}

pub fn gen_link_underscore_test(
    m: &ModuleDB,
    pkg: &Package,
    _moonc_opt: &MooncOpt,
) -> anyhow::Result<RuntestLinkDepItem> {
    let out = pkg
        .artifact
        .with_file_name(format!("{}.underscore_test.wat", pkg.last_name()));

    let pkg_topo_order: Vec<&Package> = get_pkg_topo_order(m, pkg);

    let mut core_deps = vec![];
    for cur_pkg in pkg_topo_order.iter() {
        let d = if cur_pkg.full_name() == pkg.full_name() {
            cur_pkg
                .artifact
                .with_file_name(format!("{}.underscore_test.core", cur_pkg.last_name()))
        } else {
            cur_pkg.artifact.with_extension("core")
        };
        core_deps.push(d.display().to_string());
    }

    let package_sources = get_package_sources(m, &pkg_topo_order);

    let package_full_name = pkg.full_name();

    Ok(RuntestLinkDepItem {
        out: out.display().to_string(),
        core_deps,
        package_full_name,
        package_sources,
        is_main: true,
        link: pkg.link.clone(),
    })
}

pub fn contain_mbt_test_file(pkg: &Package, moonc_opt: &MooncOpt) -> bool {
    let backend_filtered =
        moonutil::common::backend_filter(&pkg.files, moonc_opt.link_opt.target_backend);
    backend_filtered.iter().any(|f| {
        let filename = f.file_name().unwrap().to_str().unwrap().to_string();
        filename.ends_with("_test.mbt")
    })
}

pub fn gen_runtest<'a>(
    m: &'a ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<N2RuntestInput<'a>> {
    let mut build_items = vec![];
    let mut link_items = vec![];
    let mut driver_files = vec![];
    let mut files_contain_test_block = vec![];

    let filter_pkg = moonbuild_opt
        .test_opt
        .as_ref()
        .and_then(|f| f.filter_package.as_ref());

    for (pkgname, pkg) in m.packages.iter() {
        if pkg.is_main {
            continue;
        }

        build_items.push(gen_package_core(m, pkg, moonc_opt)?);

        if pkg.is_third_party {
            continue;
        }

        files_contain_test_block.extend(pkg.files_contain_test_block.iter().map(|it| it.as_path()));

        if !pkg.test_files.is_empty() {
            for item in pkg.generated_test_drivers.iter() {
                match item {
                    GeneratedTestDriver::InternalTest(it) => {
                        build_items.push(gen_package_internal_test(m, pkg, moonc_opt)?);
                        link_items.push(gen_link_internal_test(m, pkg, moonc_opt)?);
                        driver_files.push(it.as_path());
                    }
                    GeneratedTestDriver::UnderscoreTest(it) => {
                        build_items.push(gen_package_underscore_test(m, pkg, moonc_opt)?);
                        link_items.push(gen_link_underscore_test(m, pkg, moonc_opt)?);
                        driver_files.push(it.as_path());
                    }
                }
            }
        } else if filter_pkg.is_none() || filter_pkg.unwrap().contains(Path::new(pkgname)) {
            build_items.push(gen_package_internal_test(m, pkg, moonc_opt)?);
            link_items.push(gen_link_internal_test(m, pkg, moonc_opt)?);

            for item in pkg.generated_test_drivers.iter() {
                if let GeneratedTestDriver::InternalTest(it) = item {
                    driver_files.push(it.as_path());
                }
            }
        }
    }

    Ok(N2RuntestInput {
        build_items,
        link_items,
        driver_files,
        files_contain_test_block,
    })
}

pub fn gen_runtest_build_command(
    graph: &mut n2graph::Graph,
    item: &RuntestDepItem,
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
        .arg_with_cond(enable_coverage, "-enable-coverage")
        .arg_with_cond(self_coverage, "-coverage-package-override=@self")
        .args(moonc_opt.extra_build_opt.iter())
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build
}

pub fn gen_runtest_link_command(
    graph: &mut n2graph::Graph,
    item: &RuntestLinkDepItem,
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
        .arg("-test-mode") // always passing -test-mode to allow recover from panic
        .lazy_args_with_cond(export_memory_name.is_some(), || {
            vec![
                "-export-memory-name".to_string(),
                export_memory_name.unwrap().to_string(),
            ]
        })
        .lazy_args_with_cond(link_flags.is_some(), || link_flags.unwrap())
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
        .args(moonc_opt.extra_link_opt.iter())
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    (build, artifact_id)
}

pub fn gen_n2_runtest_state(
    input: &N2RuntestInput,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    let _ = moonbuild_opt;
    let mut graph = n2graph::Graph::default();
    let mut default = vec![];

    log::debug!("input: {:#?}", input);

    let gen_generate_test_driver_command =
        gen_generate_test_driver_command(&mut graph, input, moonbuild_opt);
    graph.add_build(gen_generate_test_driver_command)?;

    for item in input.build_items.iter() {
        let build = gen_runtest_build_command(&mut graph, item, moonc_opt);
        graph.add_build(build)?;
    }
    for item in input.link_items.iter() {
        let (build, fid) = gen_runtest_link_command(&mut graph, item, moonc_opt);
        default.push(fid);
        graph.add_build(build)?;
    }

    if default.is_empty() {
        eprintln!("{}: no test entry found", "Warning".yellow().bold());
        std::process::exit(0);
    }

    let mut hashes = n2graph::Hashes::default();
    let db = n2::db::open(
        &moonbuild_opt.target_dir.join("build.moon_db"),
        &mut graph,
        &mut hashes,
    )?;

    Ok(State {
        graph,
        db,
        hashes,
        default,
        pools: SmallMap::default(),
    })
}

fn gen_generate_test_driver_command(
    graph: &mut n2graph::Graph,
    n2_run_test_input: &N2RuntestInput,
    moonbuild_opt: &MoonbuildOpt,
) -> Build {
    let (driver_files, files_contain_test_block) = (
        &n2_run_test_input.driver_files,
        &n2_run_test_input.files_contain_test_block,
    );

    let ins = BuildIns {
        ids: files_contain_test_block
            .iter()
            .map(|f| graph.files.id_from_canonical(f.display().to_string()))
            .collect(),
        explicit: files_contain_test_block.len(),
        implicit: 0,
        order_only: 0,
    };
    let outs = BuildOuts {
        explicit: driver_files.len(),
        ids: driver_files
            .iter()
            .map(|f| graph.files.id_from_canonical(f.display().to_string()))
            .collect(),
    };

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("build")),
        line: 0,
    };

    let mut build = Build::new(loc, ins, outs);

    let test_filter_command = moonbuild_opt
        .test_opt
        .as_ref()
        .map_or(vec![], |t| t.to_command());
    let command = CommandBuilder::new(
        &std::env::current_exe()
            .map_or_else(|_| "moon".into(), |x| x.to_string_lossy().into_owned()),
    )
    .arg("generate-test-driver")
    .arg("--source-dir")
    .arg(&moonbuild_opt.source_dir.display().to_string())
    .arg("--target-dir")
    .arg(&moonbuild_opt.target_dir.display().to_string())
    .args_with_cond(!test_filter_command.is_empty(), &test_filter_command)
    .arg_with_cond(moonbuild_opt.sort_input, "--sort-input")
    .build();

    build.cmdline = Some(command);
    build
}
