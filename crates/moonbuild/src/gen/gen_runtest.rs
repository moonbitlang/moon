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

use anyhow::{bail, Ok};
use colored::Colorize;
use indexmap::IndexMap;
use log::info;
use moonutil::common::{
    get_desc_name, DriverKind, GeneratedTestDriver, TargetBackend, BLACKBOX_TEST_PATCH,
    MOONBITLANG_CORE, MOONBITLANG_COVERAGE, O_EXT, WHITEBOX_TEST_PATCH,
};
use moonutil::module::ModuleDB;
use moonutil::package::Package;
use moonutil::path::{ImportPath, PathComponent};
use petgraph::graph::NodeIndex;

use super::cmd_builder::CommandBuilder;
use super::util::self_in_test_import;
use super::{is_self_coverage_lib, is_skip_coverage_lib};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use moonutil::common::{MoonbuildOpt, MooncOpt, MOON_PKG_JSON};
use n2::graph::{self as n2graph, Build, BuildIns, BuildOuts, FileLoc};
use n2::load::State;
use n2::smallmap::SmallMap;

use crate::gen::gen_build::{gen_compile_exe_command, gen_compile_stub_command};
use crate::gen::n2_errors::{N2Error, N2ErrorKind};
use crate::gen::{coverage_args, MiAlias};

#[derive(Debug)]
pub struct RuntestDepItem {
    pub core_out: String,
    pub mi_out: String,
    pub mbt_deps: Vec<String>,
    pub mi_deps: Vec<MiAlias>, // do not need add parent's mi files
    pub package_full_name: String,
    pub original_package_full_name: Option<String>,
    pub package_source_dir: String,
    pub warn_list: Option<String>,
    pub alert_list: Option<String>,
    pub is_main: bool,
    pub is_third_party: bool,
    pub is_whitebox_test: bool,
    pub is_blackbox_test: bool,
    pub no_mi: bool,
    pub patch_file: Option<PathBuf>,
}

type RuntestLinkDepItem = moonutil::package::LinkDepItem;

#[derive(Debug)]
pub struct RuntestDriverItem {
    pub driver_kind: DriverKind,
    pub package_name: String,
    pub driver_file: String,
    pub files_may_contain_test_block: Vec<String>,
    pub patch_file: Option<PathBuf>,
}

#[derive(Debug)]
pub struct N2RuntestInput {
    pub build_items: Vec<RuntestDepItem>,
    pub link_items: Vec<RuntestLinkDepItem>, // entry points
    pub test_drivers: Vec<RuntestDriverItem>,
    pub compile_stub_items: Vec<RuntestLinkDepItem>,
}

/// Automatically add coverage library import to core module if needed
pub fn add_coverage_to_core_if_needed(
    mdb: &mut ModuleDB,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<()> {
    if moonc_opt.build_opt.enable_coverage {
        // Only core module needs to add coverage library
        if mdb.name == MOONBITLANG_CORE {
            info!("Automatically adding coverage library to other packages in the core module");

            // Check if the coverage library is available
            if !mdb.contains_package(MOONBITLANG_COVERAGE) {
                log::warn!("Coverage library is not available in core module. Skipping relevant operations.");
                return Ok(());
            }

            // Add coverage library reference to each package
            for (pkg_name, pkg) in mdb.get_all_packages_mut() {
                if is_self_coverage_lib(pkg_name) || is_skip_coverage_lib(pkg_name) {
                    continue;
                }
                pkg.imports.push(moonutil::path::ImportComponent {
                    path: ImportPath {
                        module_name: MOONBITLANG_CORE.into(),
                        rel_path: PathComponent {
                            components: vec!["coverage".into()],
                        },
                        is_3rd: false,
                    },
                    alias: None,
                });
            }

            // Update dependency graph
            let coverage_lib_node = mdb
                .get_all_packages()
                .get_index_of(MOONBITLANG_COVERAGE)
                .unwrap();
            let coverage_lib_node = NodeIndex::new(coverage_lib_node);
            let node_cnt = mdb.graph.node_count();
            for i in 0..node_cnt {
                let node_ix = NodeIndex::new(i);
                let node = mdb.graph.node_weight(node_ix).unwrap();
                if is_self_coverage_lib(node) || is_skip_coverage_lib(node) {
                    continue;
                }
                mdb.graph.add_edge(node_ix, coverage_lib_node, 0);
            }
        }
    }

    Ok(())
}

pub fn gen_package_test_driver(
    g: &GeneratedTestDriver,
    pkg: &Package,
) -> anyhow::Result<RuntestDriverItem> {
    match g {
        GeneratedTestDriver::InternalTest(it) => {
            let package_name = pkg.full_name();
            let driver_file = it.display().to_string();
            let files_may_contain_test_block = pkg
                .files
                .iter()
                .map(|(f, _)| f.display().to_string())
                .collect();
            Ok(RuntestDriverItem {
                package_name,
                driver_file,
                files_may_contain_test_block,
                driver_kind: DriverKind::Internal,
                patch_file: pkg.patch_file.clone(),
            })
        }
        GeneratedTestDriver::BlackboxTest(it) => {
            let package_name = pkg.full_name();
            let driver_file = it.display().to_string();
            let mut files_may_contain_test_block: Vec<String> = pkg
                .test_files
                .iter()
                .map(|(f, _)| f.display().to_string())
                .collect();
            if let Some(doc_test_patch_file) = pkg.doc_test_patch_file.clone() {
                files_may_contain_test_block.push(doc_test_patch_file.display().to_string());
            }
            Ok(RuntestDriverItem {
                package_name,
                driver_file,
                files_may_contain_test_block,
                driver_kind: DriverKind::Blackbox,
                patch_file: pkg.patch_file.clone().or(pkg.doc_test_patch_file.clone()),
            })
        }
        GeneratedTestDriver::WhiteboxTest(it) => {
            let package_name = pkg.full_name();
            let driver_file = it.display().to_string();
            let files_may_contain_test_block = pkg
                .files
                .iter()
                .chain(pkg.wbtest_files.iter())
                .map(|(f, _)| f.display().to_string())
                .collect();
            Ok(RuntestDriverItem {
                package_name,
                driver_file,
                files_may_contain_test_block,
                driver_kind: DriverKind::Whitebox,
                patch_file: pkg.patch_file.clone(),
            })
        }
    }
}

pub fn gen_package_core(
    m: &ModuleDB,
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<RuntestDepItem> {
    let core_out = pkg.artifact.with_extension("core");
    let mi_out = pkg.artifact.with_extension("mi");

    let backend_filtered: Vec<PathBuf> = moonutil::common::backend_filter(
        &pkg.files,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );
    let mbt_deps = backend_filtered
        .iter()
        .map(|f| f.display().to_string())
        .collect();

    let mut mi_deps = vec![];
    for dep in pkg.imports.iter() {
        let full_import_name = dep.path.make_full_path();
        if !m.contains_package(&full_import_name) {
            bail!(
                "{}: the imported package `{}` could not be located.",
                m.source_dir
                    .join(pkg.rel.fs_full_name())
                    .join(MOON_PKG_JSON)
                    .display(),
                full_import_name,
            );
        }
        let cur_pkg = m.get_package_by_name(&full_import_name);
        let d = cur_pkg.artifact.with_extension("mi");
        let alias = dep.alias.clone().unwrap_or(cur_pkg.last_name().into());
        mi_deps.push(MiAlias {
            name: d.display().to_string(),
            alias,
        });
    }

    let package_full_name = pkg.full_name();
    let package_source_dir = pkg.root_path.to_string_lossy().into_owned();

    Ok(RuntestDepItem {
        core_out: core_out.display().to_string(),
        mi_out: mi_out.display().to_string(),
        mbt_deps,
        mi_deps,
        package_full_name,
        original_package_full_name: None,
        package_source_dir,
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: false,
        is_third_party: pkg.is_third_party,
        is_whitebox_test: false,
        is_blackbox_test: false,
        no_mi: false,
        patch_file: None,
    })
}

pub fn gen_package_internal_test(
    m: &ModuleDB,
    pkg: &Package,
    moonc_opt: &MooncOpt,
    patch_file: Option<PathBuf>,
) -> anyhow::Result<RuntestDepItem> {
    let pkgname = pkg.artifact.file_stem().unwrap().to_str().unwrap();
    let core_out = pkg
        .artifact
        .with_file_name(format!("{}.internal_test.core", pkgname));
    let mi_out = pkg
        .artifact
        .with_file_name(format!("{}.internal_test.mi", pkgname));

    let backend_filtered = moonutil::common::backend_filter(
        &pkg.files,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );
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
        if !m.contains_package(&full_import_name) {
            bail!(
                "{}: the imported package `{}` could not be located.",
                m.source_dir
                    .join(pkg.rel.fs_full_name())
                    .join(MOON_PKG_JSON)
                    .display(),
                full_import_name,
            );
        }
        let cur_pkg = m.get_package_by_name(&full_import_name);
        let d = cur_pkg.artifact.with_extension("mi");
        let alias = dep.alias.clone().unwrap_or(cur_pkg.last_name().into());
        mi_deps.push(MiAlias {
            name: d.display().to_string(),
            alias,
        });
    }

    let package_full_name = pkg.full_name();
    let package_source_dir = pkg.root_path.to_string_lossy().into_owned();

    Ok(RuntestDepItem {
        core_out: core_out.display().to_string(),
        mi_out: mi_out.display().to_string(),
        mbt_deps,
        mi_deps,
        package_full_name,
        original_package_full_name: None,
        package_source_dir,
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: true,
        is_third_party: pkg.is_third_party,
        is_whitebox_test: false,
        is_blackbox_test: false,
        no_mi: true,
        patch_file,
    })
}

pub fn gen_package_whitebox_test(
    m: &ModuleDB,
    pkg: &Package,
    moonc_opt: &MooncOpt,
    patch_file: Option<PathBuf>,
) -> anyhow::Result<RuntestDepItem> {
    let pkgname = pkg.artifact.file_stem().unwrap().to_str().unwrap();
    let core_out = pkg
        .artifact
        .with_file_name(format!("{}.whitebox_test.core", pkgname));
    let mi_out = pkg
        .artifact
        .with_file_name(format!("{}.whitebox_test.mi", pkgname));

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
    let mut mbt_deps: Vec<String> = backend_filtered
        .iter()
        .map(|f| f.display().to_string())
        .collect();

    for item in pkg.generated_test_drivers.iter() {
        if let GeneratedTestDriver::WhiteboxTest(path) = item {
            mbt_deps.push(path.display().to_string());
        }
    }

    let mut mi_deps = vec![];
    for dep in pkg.imports.iter().chain(pkg.wbtest_imports.iter()) {
        let full_import_name = dep.path.make_full_path();
        if !m.contains_package(&full_import_name) {
            bail!(
                "{}: the imported package `{}` could not be located.",
                m.source_dir
                    .join(pkg.rel.fs_full_name())
                    .join(MOON_PKG_JSON)
                    .display(),
                full_import_name,
            );
        }
        let cur_pkg = m.get_package_by_name(&full_import_name);
        let d = cur_pkg.artifact.with_extension("mi");
        let alias = dep.alias.clone().unwrap_or(cur_pkg.last_name().into());
        mi_deps.push(MiAlias {
            name: d.display().to_string(),
            alias,
        });
    }

    let package_full_name = pkg.full_name();
    let package_source_dir = pkg.root_path.to_string_lossy().into_owned();

    Ok(RuntestDepItem {
        core_out: core_out.display().to_string(),
        mi_out: mi_out.display().to_string(),
        mbt_deps,
        mi_deps,
        package_full_name,
        original_package_full_name: None,
        package_source_dir,
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: true,
        is_third_party: pkg.is_third_party,
        is_whitebox_test: true,
        is_blackbox_test: false,
        no_mi: true,
        patch_file,
    })
}

pub fn gen_package_blackbox_test(
    m: &ModuleDB,
    pkg: &Package,
    moonc_opt: &MooncOpt,
    patch_file: Option<PathBuf>,
) -> anyhow::Result<RuntestDepItem> {
    let self_in_test_import = self_in_test_import(pkg);

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

    let pkgname = pkg.artifact.file_stem().unwrap().to_str().unwrap();
    let core_out = pkg
        .artifact
        .with_file_name(format!("{}.blackbox_test.core", pkgname));
    let mi_out = pkg
        .artifact
        .with_file_name(format!("{}.blackbox_test.mi", pkgname));

    let backend_filtered = moonutil::common::backend_filter(
        &pkg.test_files,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );
    let mut mbt_deps: Vec<String> = backend_filtered
        .iter()
        .map(|f| f.display().to_string())
        .collect();

    for item in pkg.generated_test_drivers.iter() {
        if let GeneratedTestDriver::BlackboxTest(path) = item {
            mbt_deps.push(path.display().to_string());
        }
    }

    let mut mi_deps = vec![];

    // add cur pkg as .mi dependency at build-package stage if it's not set in test_imports
    if !self_in_test_import {
        mi_deps.push(MiAlias {
            name: pkg
                .artifact
                .with_file_name(format!("{}.mi", pkgname))
                .display()
                .to_string(),
            alias: pkg.last_name().into(),
        });
    }

    for dep in pkg.imports.iter().chain(pkg.test_imports.iter()) {
        let full_import_name = dep.path.make_full_path();
        if !m.contains_package(&full_import_name) {
            bail!(
                "{}: the imported package `{}` could not be located.",
                m.source_dir
                    .join(pkg.rel.fs_full_name())
                    .join(MOON_PKG_JSON)
                    .display(),
                full_import_name,
            );
        }
        let cur_pkg = m.get_package_by_name(&full_import_name);
        let d = cur_pkg.artifact.with_extension("mi");
        let alias = dep.alias.clone().unwrap_or(cur_pkg.last_name().into());
        mi_deps.push(MiAlias {
            name: d.display().to_string(),
            alias,
        });
    }

    // this is used for `-pkg` flag in `moonc build-package`, shouldn't be `pkg.full_name()` since we aren't build that package, otherwise we might encounter an error like "4015] Error: Type StructName has no method method_name"(however, StructName does has method method_name).
    // actually, `-pkg` flag is not necessary for blackbox test, but we still keep it for consistency
    let package_full_name = pkg.full_name() + "_blackbox_test";
    let package_source_dir: String = pkg.root_path.to_string_lossy().into_owned();

    Ok(RuntestDepItem {
        core_out: core_out.display().to_string(),
        mi_out: mi_out.display().to_string(),
        mbt_deps,
        mi_deps,
        package_full_name,
        original_package_full_name: Some(pkg.full_name()),
        package_source_dir,
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: true,
        is_third_party: pkg.is_third_party,
        is_whitebox_test: false,
        is_blackbox_test: true,
        no_mi: true,
        patch_file,
    })
}

fn get_pkg_topo_order<'a>(
    m: &'a ModuleDB,
    leaf: &Package,
    with_wbtest_import: bool,
    with_test_import: bool,
) -> Vec<&'a Package> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut pkg_topo_order: Vec<&Package> = vec![];
    fn dfs<'a>(
        m: &'a ModuleDB,
        pkg_topo_order: &mut Vec<&'a Package>,
        visited: &mut HashSet<String>,
        cur_pkg_full_name: &String,
        with_wbtest_import: bool,
        with_test_import: bool,
    ) {
        if visited.contains(cur_pkg_full_name) {
            return;
        }
        visited.insert(cur_pkg_full_name.clone());
        let cur_pkg = m.get_package_by_name(cur_pkg_full_name);
        let imports = cur_pkg
            .imports
            .iter()
            .chain(if with_wbtest_import {
                cur_pkg.wbtest_imports.iter()
            } else {
                [].iter()
            })
            .chain(if with_test_import {
                cur_pkg.test_imports.iter()
            } else {
                [].iter()
            });

        for dep in imports {
            dfs(
                m,
                pkg_topo_order,
                visited,
                &dep.path.make_full_path(),
                false,
                false,
            );
        }

        pkg_topo_order.push(cur_pkg);
    }
    dfs(
        m,
        &mut pkg_topo_order,
        &mut visited,
        &leaf.full_name(),
        with_wbtest_import,
        with_test_import,
    );
    pkg_topo_order
}

fn get_package_sources(pkg_topo_order: &[&Package]) -> Vec<(String, String)> {
    let mut package_sources = vec![];
    for pkg in pkg_topo_order {
        package_sources.push((pkg.full_name(), pkg.root_path.display().to_string()));
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

    let pkg_topo_order: Vec<&Package> = get_pkg_topo_order(m, pkg, false, false);

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
    let package_sources = get_package_sources(&pkg_topo_order);

    let package_full_name = pkg.full_name();

    Ok(RuntestLinkDepItem {
        out: out.display().to_string(),
        core_deps,
        package_full_name,
        package_sources,
        package_path: pkg.root_path.clone(),
        link: pkg.link.clone(),
        install_path: None,
        bin_name: None,
        native_stub: pkg.native_stub.clone(),
    })
}

pub fn gen_link_whitebox_test(
    m: &ModuleDB,
    pkg: &Package,
    _moonc_opt: &MooncOpt,
) -> anyhow::Result<RuntestLinkDepItem> {
    let out = pkg
        .artifact
        .with_file_name(format!("{}.whitebox_test.wat", pkg.last_name()));

    let pkg_topo_order: Vec<&Package> = get_pkg_topo_order(m, pkg, true, false);

    let mut core_deps = vec![];
    for cur_pkg in pkg_topo_order.iter() {
        let d = if cur_pkg.full_name() == pkg.full_name() {
            cur_pkg
                .artifact
                .with_file_name(format!("{}.whitebox_test.core", cur_pkg.last_name()))
        } else {
            cur_pkg.artifact.with_extension("core")
        };
        core_deps.push(d.display().to_string());
    }

    let package_sources = get_package_sources(&pkg_topo_order);

    let package_full_name = pkg.full_name();

    Ok(RuntestLinkDepItem {
        out: out.display().to_string(),
        core_deps,
        package_full_name,
        package_sources,
        package_path: pkg.root_path.clone(),
        link: pkg.link.clone(),
        install_path: None,
        bin_name: None,
        native_stub: pkg.native_stub.clone(),
    })
}

pub fn gen_link_blackbox_test(
    m: &ModuleDB,
    pkg: &Package,
    _moonc_opt: &MooncOpt,
) -> anyhow::Result<RuntestLinkDepItem> {
    let pkgname = pkg.artifact.file_stem().unwrap().to_str().unwrap();
    let out = pkg
        .artifact
        .with_file_name(format!("{}.blackbox_test.wat", pkg.last_name()));

    let pkg_topo_order: Vec<&Package> = get_pkg_topo_order(m, pkg, false, true);

    let mut core_deps = vec![];
    for cur_pkg in pkg_topo_order.iter() {
        let d = if cur_pkg.full_name() == pkg.full_name() {
            // add the cur pkg .core in link-core stage
            // make sure that the current package `.core` is placed before `blackbox_test.core`
            core_deps.push(
                pkg.artifact
                    .with_file_name(format!("{}.core", pkgname))
                    .display()
                    .to_string(),
            );

            cur_pkg
                .artifact
                .with_file_name(format!("{}.blackbox_test.core", cur_pkg.last_name()))
        } else {
            cur_pkg.artifact.with_extension("core")
        };
        core_deps.push(d.display().to_string());
    }

    let mut package_sources = get_package_sources(&pkg_topo_order);

    // add blackbox test pkg into `package_sources`, which will be passed to `-pkg-source` in `link-core`
    package_sources.push((
        pkg.full_name() + "_blackbox_test",
        pkg.root_path.display().to_string(),
    ));

    // this will be passed to link-core `-main`
    let package_full_name = pkg.full_name() + "_blackbox_test";

    Ok(RuntestLinkDepItem {
        out: out.display().to_string(),
        core_deps,
        package_full_name,
        package_sources,
        package_path: pkg.root_path.clone(),
        link: pkg.link.clone(),
        install_path: None,
        bin_name: None,
        native_stub: pkg.native_stub.clone(),
    })
}

pub fn contain_mbt_test_file(pkg: &Package, moonc_opt: &MooncOpt) -> bool {
    let backend_filtered = moonutil::common::backend_filter(
        &pkg.files,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );
    backend_filtered.iter().any(|f| {
        let filename = f.file_name().unwrap().to_str().unwrap().to_string();
        filename.ends_with("_test.mbt")
    })
}

pub fn gen_runtest(
    m: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<N2RuntestInput> {
    let mut build_items = vec![];
    let mut link_items = vec![];
    let mut test_drivers = vec![];
    let mut compile_stub_items = vec![];

    let filter_pkg = moonbuild_opt
        .test_opt
        .as_ref()
        .and_then(|f| f.filter_package.as_ref());

    let patch_file = moonbuild_opt
        .test_opt
        .as_ref()
        .and_then(|opt| opt.patch_file.clone());

    let (whitebox_patch_file, blackbox_patch_file, internal_patch_file) = patch_file
        .map(|pf| {
            let name = pf.file_name().unwrap().to_str().unwrap();
            match name {
                n if n.ends_with(WHITEBOX_TEST_PATCH) => (Some(pf), None, None),
                n if n.ends_with(BLACKBOX_TEST_PATCH) => (None, Some(pf), None),
                _ => (None, None, Some(pf)),
            }
        })
        .unwrap_or((None, None, None));

    for (pkgname, pkg) in m.get_all_packages().iter() {
        if pkg.is_main {
            continue;
        }

        build_items.push(gen_package_core(m, pkg, moonc_opt)?);
        if pkg.native_stub.is_some() {
            compile_stub_items.push(RuntestLinkDepItem {
                out: pkg.artifact.with_extension(O_EXT).display().to_string(),
                core_deps: vec![],
                package_sources: vec![],
                package_full_name: pkg.full_name(),
                package_path: pkg.root_path.clone(),
                link: pkg.link.clone(),
                install_path: None,
                bin_name: None,
                native_stub: pkg.native_stub.clone(),
            });
        }

        if pkg.is_third_party {
            continue;
        }

        if let Some(filter_pkg) = filter_pkg {
            if !filter_pkg.contains(pkgname) {
                continue;
            }
        }

        // todo: only generate the test driver when there is test block exist
        for item in pkg.generated_test_drivers.iter() {
            if let GeneratedTestDriver::InternalTest(_) = item {
                test_drivers.push(gen_package_test_driver(item, pkg)?);
                build_items.push(gen_package_internal_test(
                    m,
                    pkg,
                    moonc_opt,
                    internal_patch_file.clone(),
                )?);
                link_items.push(gen_link_internal_test(m, pkg, moonc_opt)?);
            }
        }

        if !pkg.wbtest_files.is_empty() || whitebox_patch_file.is_some() {
            for item in pkg.generated_test_drivers.iter() {
                if let GeneratedTestDriver::WhiteboxTest(_) = item {
                    test_drivers.push(gen_package_test_driver(item, pkg)?);
                    build_items.push(gen_package_whitebox_test(
                        m,
                        pkg,
                        moonc_opt,
                        whitebox_patch_file.clone(),
                    )?);
                    link_items.push(gen_link_whitebox_test(m, pkg, moonc_opt)?);
                }
            }
        }

        if !pkg.test_files.is_empty()
            || blackbox_patch_file.is_some()
            || pkg.doc_test_patch_file.is_some()
        {
            for item in pkg.generated_test_drivers.iter() {
                if let GeneratedTestDriver::BlackboxTest(_) = item {
                    test_drivers.push(gen_package_test_driver(item, pkg)?);
                    build_items.push(gen_package_blackbox_test(
                        m,
                        pkg,
                        moonc_opt,
                        blackbox_patch_file
                            .clone()
                            .or(pkg.doc_test_patch_file.clone()),
                    )?);
                    link_items.push(gen_link_blackbox_test(m, pkg, moonc_opt)?);
                }
            }
        }
    }

    Ok(N2RuntestInput {
        build_items,
        link_items,
        test_drivers,
        compile_stub_items,
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
        ids: if item.no_mi {
            vec![core_output_id]
        } else {
            vec![core_output_id, mi_output_id]
        },
        explicit: 1,
    };

    let coverage_args = coverage_args(
        moonc_opt.build_opt.enable_coverage && !item.is_third_party,
        &item.package_full_name,
        item.original_package_full_name.as_deref(),
        false,
    );

    let mut build = Build::new(loc, ins, outs);

    let (debug_flag, strip_flag) = (
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.strip_flag,
    );

    let command = CommandBuilder::new("moonc")
        .arg("build-package")
        .args_with_cond(moonc_opt.render, vec!["-error-format", "json"])
        .args(&item.mbt_deps)
        .lazy_args_with_cond(item.warn_list.is_some(), || {
            vec!["-w".to_string(), item.warn_list.clone().unwrap()]
        })
        .lazy_args_with_cond(item.alert_list.is_some(), || {
            vec!["-alert".to_string(), item.alert_list.clone().unwrap()]
        })
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
        .args_with_cond(debug_flag && !strip_flag, vec!["-g", "-O0"])
        .arg_with_cond(debug_flag && strip_flag, "-O0")
        .arg_with_cond(!debug_flag && !strip_flag, "-g")
        // .arg_with_cond(!debug_flag && strip_flag, "")
        .arg_with_cond(moonc_opt.link_opt.source_map, "-source-map")
        // Coverage arg
        .args(coverage_args.iter())
        .args(moonc_opt.extra_build_opt.iter())
        .arg_with_cond(item.is_whitebox_test, "-whitebox-test")
        .arg_with_cond(item.is_blackbox_test, "-blackbox-test")
        .arg_with_cond(item.no_mi, "-no-mi")
        .lazy_args_with_cond(item.patch_file.is_some(), || {
            vec![
                "-patch-file".to_string(),
                item.patch_file.as_ref().unwrap().display().to_string(),
            ]
        })
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build.desc = Some(format!(
        "build-package: {}",
        get_desc_name(&item.package_full_name, &item.core_out)
    ));
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

    let (debug_flag, strip_flag) = (
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.strip_flag,
    );

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
        .arg("-pkg-config-path") // tell moonc where moon.pkg.json is
        .arg(&item.package_path.join(MOON_PKG_JSON).display().to_string())
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
                &format!(
                    "{}:{}",
                    MOONBITLANG_CORE,
                    &moonutil::moon_dir::core().display()
                ),
            ],
        )
        .args([
            "-exported_functions",
            "moonbit_test_driver_internal_execute,moonbit_test_driver_finish",
        ])
        .args_with_cond(
            moonc_opt.link_opt.target_backend == moonutil::common::TargetBackend::Js,
            ["-js-format", "cjs", "-no-dts"],
        )
        .args(["-target", moonc_opt.link_opt.target_backend.to_flag()])
        .args_with_cond(debug_flag && !strip_flag, vec!["-g", "-O0"])
        .arg_with_cond(debug_flag && strip_flag, "-O0")
        .arg_with_cond(!debug_flag && !strip_flag, "-g")
        // .arg_with_cond(!debug_flag && strip_flag, "")
        .arg_with_cond(moonc_opt.link_opt.source_map, "-source-map")
        .args(moonc_opt.extra_link_opt.iter())
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build.desc = Some(format!(
        "link-core: {}",
        get_desc_name(&item.package_full_name, &item.out)
    ));
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

    for item in input.build_items.iter() {
        let build = gen_runtest_build_command(&mut graph, item, moonc_opt);
        graph.add_build(build)?;
    }

    let is_native_backend = moonc_opt.link_opt.target_backend == TargetBackend::Native;

    for item in input.link_items.iter() {
        let (build, fid) = gen_runtest_link_command(&mut graph, item, moonc_opt);
        let mut default_fid = fid;
        graph.add_build(build)?;

        if is_native_backend {
            let (build, fid) = gen_compile_exe_command(&mut graph, item, moonc_opt);
            default_fid = fid;
            graph.add_build(build)?;
        }
        default.push(default_fid);
    }
    for item in input.test_drivers.iter() {
        let build = gen_generate_test_driver_command(&mut graph, item, moonc_opt, moonbuild_opt);
        graph.add_build(build)?;
    }

    if is_native_backend {
        for item in input.compile_stub_items.iter() {
            let (build, fid) = gen_compile_stub_command(&mut graph, item, moonc_opt);
            graph.add_build(build)?;
            default.push(fid);
        }
    }

    if default.is_empty() {
        eprintln!(
            "{}: no test entry found(test block in main package is not support for now)",
            "Warning".yellow().bold()
        );
        std::process::exit(0);
    }

    let mut hashes = n2graph::Hashes::default();
    let n2_db_path = &moonbuild_opt.target_dir.join("build.moon_db");
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

fn gen_generate_test_driver_command(
    graph: &mut n2graph::Graph,
    item: &RuntestDriverItem,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> Build {
    let (driver_file, files_contain_test_block) =
        (&item.driver_file, &item.files_may_contain_test_block);

    let ins = BuildIns {
        ids: files_contain_test_block
            .iter()
            .map(|f| graph.files.id_from_canonical(f.to_string()))
            .collect(),
        explicit: files_contain_test_block.len(),
        implicit: 0,
        order_only: 0,
    };
    let outs = BuildOuts {
        explicit: 0,
        ids: vec![graph.files.id_from_canonical(driver_file.to_string())],
    };

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("build")),
        line: 0,
    };

    let coverage_args = coverage_args(
        moonc_opt.build_opt.enable_coverage,
        &item.package_name,
        None,
        true,
    );

    let mut build = Build::new(loc, ins, outs);

    let patch_file = item.patch_file.as_ref().and_then(|p| {
        let filename = p.to_str().unwrap();
        match item.driver_kind {
            DriverKind::Whitebox if filename.ends_with(WHITEBOX_TEST_PATCH) => Some(p),
            DriverKind::Blackbox if filename.ends_with(BLACKBOX_TEST_PATCH) => Some(p),
            DriverKind::Internal
                if !filename.ends_with(WHITEBOX_TEST_PATCH)
                    && !filename.ends_with(BLACKBOX_TEST_PATCH) =>
            {
                Some(p)
            }
            _ => None,
        }
    });

    let command = CommandBuilder::new(
        &std::env::current_exe()
            .map_or_else(|_| "moon".into(), |x| x.to_string_lossy().into_owned()),
    )
    .arg("generate-test-driver")
    .arg("--source-dir")
    .arg(&moonbuild_opt.source_dir.display().to_string())
    .arg("--target-dir")
    .arg(&moonbuild_opt.raw_target_dir.display().to_string())
    .args(["--package", &item.package_name])
    .arg_with_cond(moonbuild_opt.sort_input, "--sort-input")
    .args(["--target", moonc_opt.build_opt.target_backend.to_flag()])
    .args(["--driver-kind", item.driver_kind.to_string()])
    .args(coverage_args.iter())
    .arg_with_cond(!moonc_opt.build_opt.debug_flag, "--release")
    .lazy_args_with_cond(patch_file.is_some(), || {
        vec![
            "--patch-file".to_string(),
            patch_file.unwrap().display().to_string(),
        ]
    })
    .build();

    build.cmdline = Some(command);
    build.desc = Some(format!(
        "gen-test-driver: {}_{}_test",
        item.package_name,
        item.driver_kind.to_string()
    ));
    build
}
