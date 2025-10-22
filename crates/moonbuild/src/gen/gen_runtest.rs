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

use anyhow::{Ok, bail};
use indexmap::IndexMap;
use log::info;
use moonutil::common::{
    BLACKBOX_TEST_PATCH, DriverKind, GeneratedTestDriver, MOONBITLANG_CORE, MOONBITLANG_COVERAGE,
    O_EXT, RunMode, SUB_PKG_POSTFIX, TEST_INFO_FILE, TargetBackend, WHITEBOX_TEST_PATCH,
    get_desc_name,
};
use moonutil::compiler_flags::CC;
use moonutil::cond_expr::OptLevel;
use moonutil::module::ModuleDB;
use moonutil::package::Package;
use moonutil::path::{ImportPath, PathComponent};
use petgraph::graph::NodeIndex;

use super::cmd_builder::CommandBuilder;
use super::gen_build::{
    BuildInterfaceItem, gen_build_interface_item, replace_virtual_pkg_core_with_impl_pkg_core,
};
use super::util::{calc_link_args, self_in_test_import};
use super::{is_self_coverage_lib, is_skip_coverage_lib};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use moonutil::common::{MOON_PKG_JSON, MoonbuildOpt, MooncOpt};
use n2::graph::{self as n2graph, Build, BuildIns, BuildOuts, FileLoc};
use n2::load::State;
use n2::smallmap::SmallMap;

use crate::r#gen::gen_build::gen_build_interface_command;
use crate::r#gen::gen_build::{
    gen_archive_stub_to_static_lib_command, gen_compile_exe_command, gen_compile_runtime_command,
    gen_compile_shared_runtime_command, gen_compile_stub_command, gen_link_exe_command,
    gen_link_stub_to_dynamic_lib_command,
};
use crate::r#gen::gen_check::warn_about_alias_duplication;
use crate::r#gen::n2_errors::{N2Error, N2ErrorKind};
use crate::r#gen::{MiAlias, SKIP_TEST_LIBS, coverage_args};

#[derive(Debug)]
pub struct RuntestDepItem {
    pub core_out: String,
    pub mi_out: String,
    pub mbt_deps: Vec<String>,
    /// MoonBit source files that only need doc testing
    pub doctest_only_mbt_deps: Vec<String>,
    /// `mbt.md` files
    pub mbt_md_deps: Vec<String>,
    pub mi_deps: Vec<MiAlias>, // do not need add parent's mi files
    pub package_full_name: String,
    pub original_package_full_name: Option<String>,
    pub package_source_dir: String,
    /// Canonical absolute path to the module directory (parent of moon.mod.json)
    pub workspace_root: Arc<Path>,
    pub warn_list: Option<String>,
    pub alert_list: Option<String>,
    pub is_main: bool,
    pub is_third_party: bool,
    pub is_internal_test: bool,
    pub is_whitebox_test: bool,
    pub is_blackbox_test: bool,
    pub no_mi: bool,
    pub patch_file: Option<PathBuf>,

    // which virtual pkg to implement (mi path, virtual pkg name, virtual pkg path)
    pub mi_of_virtual_pkg_to_impl: Option<(String, String, String)>,

    pub enable_value_tracing: bool,
}

type RuntestLinkDepItem = moonutil::package::LinkDepItem;

#[derive(Debug)]
pub struct RuntestDriverItem {
    pub driver_kind: DriverKind,
    pub package_name: String,
    pub driver_file: PathBuf,
    pub info_file: PathBuf,
    pub files_may_contain_test_block: Vec<String>,
    pub doctest_only_files: Vec<String>,
    pub patch_file: Option<PathBuf>,
}

#[derive(Debug)]
pub struct N2RuntestInput {
    // for virtual pkg
    pub build_interface_items: Vec<BuildInterfaceItem>,
    pub build_default_virtual_items: Vec<RuntestDepItem>,

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
                log::info!(
                    "Coverage library is not available in core module. Skipping relevant operations."
                );
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
                    sub_package: false,
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
    moonc_opt: &MooncOpt,
    target_dir: &Path,
    g: &GeneratedTestDriver,
    pkg: &Package,
) -> anyhow::Result<RuntestDriverItem> {
    let package_name = pkg.full_name();

    let driver_kind = match g {
        GeneratedTestDriver::InternalTest(_) => DriverKind::Internal,
        GeneratedTestDriver::BlackboxTest(_) => DriverKind::Blackbox,
        GeneratedTestDriver::WhiteboxTest(_) => DriverKind::Whitebox,
    };
    let mut files_that_may_contain_test_block: Vec<String> = match driver_kind {
        DriverKind::Internal => &pkg.files,
        DriverKind::Blackbox => &pkg.test_files,
        DriverKind::Whitebox => &pkg.wbtest_files,
    }
    .iter()
    .filter(|(_, v)| {
        v.eval(
            OptLevel::from_debug_flag(moonc_opt.build_opt.debug_flag),
            moonc_opt.build_opt.target_backend,
        )
    })
    .map(|(f, _)| f.display().to_string())
    .collect();
    if matches!(driver_kind, DriverKind::Blackbox) {
        files_that_may_contain_test_block
            .extend(pkg.mbt_md_files.keys().map(|x| x.display().to_string()));
    }
    let doctest_only_files = match driver_kind {
        DriverKind::Blackbox => pkg
            .files
            .iter()
            .filter(|(_, v)| {
                v.eval(
                    OptLevel::from_debug_flag(moonc_opt.build_opt.debug_flag),
                    moonc_opt.build_opt.target_backend,
                )
            })
            .map(|(f, _)| f.display().to_string())
            .collect(),
        DriverKind::Internal | DriverKind::Whitebox => vec![],
    };

    let test_info = target_dir
        .join(pkg.rel.fs_full_name())
        .join(format!("__{driver_kind}_{TEST_INFO_FILE}"));
    let driver_file = match g {
        GeneratedTestDriver::InternalTest(it) => it.clone(),
        GeneratedTestDriver::BlackboxTest(it) => it.clone(),
        GeneratedTestDriver::WhiteboxTest(it) => it.clone(),
    };

    Ok(RuntestDriverItem {
        package_name,
        driver_file,
        info_file: test_info,
        doctest_only_files,
        files_may_contain_test_block: files_that_may_contain_test_block,
        driver_kind,
        patch_file: pkg.patch_file.clone(),
    })
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
        let mut full_import_name = dep.path.make_full_path();
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
        if dep.sub_package {
            full_import_name = format!("{full_import_name}{SUB_PKG_POSTFIX}");
        }
        let cur_pkg = m.get_package_by_name(&full_import_name);
        let d = cur_pkg.artifact.with_extension("mi");
        let alias = dep.alias.clone().unwrap_or(cur_pkg.last_name().into());
        mi_deps.push(MiAlias {
            name: d.display().to_string(),
            alias,
        });
    }

    let package_full_name = if pkg.is_sub_package {
        pkg.full_name().replace(SUB_PKG_POSTFIX, "")
    } else {
        pkg.full_name()
    };

    let package_source_dir = pkg.root_path.to_string_lossy().into_owned();

    let impl_virtual_pkg = if let Some(impl_virtual_pkg) = pkg.implement.as_ref() {
        let impl_virtual_pkg = m.get_package_by_name(impl_virtual_pkg);

        let virtual_pkg_mi = impl_virtual_pkg
            .artifact
            .with_extension("mi")
            .display()
            .to_string();

        Some((
            virtual_pkg_mi,
            impl_virtual_pkg.full_name(),
            impl_virtual_pkg.root_path.display().to_string(),
        ))
    } else {
        None
    };

    let workspace_root: Arc<Path> = Arc::clone(&pkg.module_root);

    Ok(RuntestDepItem {
        core_out: core_out.display().to_string(),
        mi_out: mi_out.display().to_string(),
        mbt_deps,
        doctest_only_mbt_deps: vec![],
        mbt_md_deps: vec![],
        mi_deps,
        package_full_name,
        original_package_full_name: None,
        package_source_dir,
        workspace_root,
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: pkg.is_main,
        is_third_party: pkg.is_third_party,
        is_internal_test: false,
        is_whitebox_test: false,
        is_blackbox_test: false,
        no_mi: false,
        patch_file: None,
        mi_of_virtual_pkg_to_impl: impl_virtual_pkg,
        enable_value_tracing: pkg.enable_value_tracing,
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
        .with_file_name(format!("{pkgname}.internal_test.core"));
    let mi_out = pkg
        .artifact
        .with_file_name(format!("{pkgname}.internal_test.mi"));

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
        let mut full_import_name = dep.path.make_full_path();
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
        if dep.sub_package {
            full_import_name = format!("{full_import_name}{SUB_PKG_POSTFIX}");
        }
        let cur_pkg = m.get_package_by_name(&full_import_name);
        let d = cur_pkg.artifact.with_extension("mi");
        let alias = dep.alias.clone().unwrap_or(cur_pkg.last_name().into());
        mi_deps.push(MiAlias {
            name: d.display().to_string(),
            alias,
        });
    }

    let package_full_name = if pkg.is_sub_package {
        pkg.full_name().replace(SUB_PKG_POSTFIX, "")
    } else {
        pkg.full_name()
    };

    let package_source_dir = pkg.root_path.to_string_lossy().into_owned();

    let workspace_root: Arc<Path> = Arc::clone(&pkg.module_root);

    Ok(RuntestDepItem {
        core_out: core_out.display().to_string(),
        mi_out: mi_out.display().to_string(),
        mbt_deps,
        doctest_only_mbt_deps: vec![],
        mbt_md_deps: vec![],
        mi_deps,
        package_full_name,
        original_package_full_name: None,
        package_source_dir,
        workspace_root,
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: true,
        is_third_party: pkg.is_third_party,
        is_internal_test: true,
        is_whitebox_test: false,
        is_blackbox_test: false,
        no_mi: true,
        patch_file,
        mi_of_virtual_pkg_to_impl: None,
        enable_value_tracing: pkg.enable_value_tracing,
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
        .with_file_name(format!("{pkgname}.whitebox_test.core"));
    let mi_out = pkg
        .artifact
        .with_file_name(format!("{pkgname}.whitebox_test.mi"));

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
    let imports_and_wbtest_imports = get_imports(pkg, false);
    for dep in imports_and_wbtest_imports.iter() {
        let mut full_import_name = dep.path.make_full_path();
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
        if dep.sub_package {
            full_import_name = format!("{full_import_name}{SUB_PKG_POSTFIX}");
        }
        let cur_pkg = m.get_package_by_name(&full_import_name);
        let d = cur_pkg.artifact.with_extension("mi");
        let alias = dep.alias.clone().unwrap_or(cur_pkg.last_name().into());
        mi_deps.push(MiAlias {
            name: d.display().to_string(),
            alias,
        });
    }

    let package_full_name = if pkg.is_sub_package {
        pkg.full_name().replace(SUB_PKG_POSTFIX, "")
    } else {
        pkg.full_name()
    };

    let package_source_dir = pkg.root_path.to_string_lossy().into_owned();

    let workspace_root: Arc<Path> = Arc::clone(&pkg.module_root);

    Ok(RuntestDepItem {
        core_out: core_out.display().to_string(),
        mi_out: mi_out.display().to_string(),
        mbt_deps,
        doctest_only_mbt_deps: vec![],
        mbt_md_deps: vec![],
        mi_deps,
        package_full_name,
        original_package_full_name: None,
        package_source_dir,
        workspace_root,
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: true,
        is_third_party: pkg.is_third_party,
        is_internal_test: false,
        is_whitebox_test: true,
        is_blackbox_test: false,
        no_mi: true,
        patch_file,
        mi_of_virtual_pkg_to_impl: None,
        enable_value_tracing: pkg.enable_value_tracing,
    })
}

pub fn gen_package_blackbox_test(
    m: &ModuleDB,
    pkg: &Package,
    moonc_opt: &MooncOpt,
    patch_file: Option<PathBuf>,
) -> anyhow::Result<RuntestDepItem> {
    let self_in_test_import = self_in_test_import(pkg);

    warn_about_alias_duplication(self_in_test_import, pkg);

    let pkgname = pkg.artifact.file_stem().unwrap().to_str().unwrap();
    let core_out = pkg
        .artifact
        .with_file_name(format!("{pkgname}.blackbox_test.core"));
    let mi_out = pkg
        .artifact
        .with_file_name(format!("{pkgname}.blackbox_test.mi"));

    let mbt_files_filtered = moonutil::common::backend_filter(
        &pkg.files,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );
    let doctest_only_mbt_deps = mbt_files_filtered
        .iter()
        .map(|f| f.display().to_string())
        .collect();

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
    let mbt_md_deps = pkg
        .mbt_md_files
        .keys()
        .map(|f| f.display().to_string())
        .collect();

    let mut mi_deps = vec![];

    // add cur pkg as .mi dependency at build-package stage if it's not set in test_imports
    // The current package might be an implementation of a virtual package, which
    // in this case the mi of the virtual package should be used instead.
    if !self_in_test_import {
        if let Some(implement) = pkg.implement.as_ref() {
            let impl_pkg = m.get_package_by_name(implement);
            let virtual_pkg_mi = impl_pkg.artifact.with_extension("mi").display().to_string();
            mi_deps.push(MiAlias {
                name: virtual_pkg_mi,
                alias: impl_pkg.last_name().into(),
            });
        } else {
            mi_deps.push(MiAlias {
                name: pkg
                    .artifact
                    .with_file_name(format!("{pkgname}.mi"))
                    .display()
                    .to_string(),
                alias: pkg.last_name().into(),
            });
        }
    }
    let self_alias = pkg.last_name();

    let imports_and_test_imports = get_imports(pkg, true);

    for dep in imports_and_test_imports.iter() {
        let mut full_import_name = dep.path.make_full_path();
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
        if dep.sub_package {
            full_import_name = format!("{full_import_name}{SUB_PKG_POSTFIX}");
        }
        let cur_pkg = m.get_package_by_name(&full_import_name);
        let d = cur_pkg.artifact.with_extension("mi");
        let alias = dep.alias.clone().unwrap_or(cur_pkg.last_name().into());
        let alias = if alias == self_alias {
            // This behavior is temporary -- remove the alias if it is the same
            // as the current package name, since the current package must take
            // precedence over the imported package in blackbox tests.
            //
            // To remove the alias, we insert the full name of the package
            // instead of the alias, in the alias field.
            full_import_name.clone()
        } else {
            alias
        };
        mi_deps.push(MiAlias {
            name: d.display().to_string(),
            alias,
        });
    }

    // this is used for `-pkg` flag in `moonc build-package`, shouldn't be `pkg.full_name()` since we aren't build that package, otherwise we might encounter an error like "4015] Error: Type StructName has no method method_name"(however, StructName does has method method_name).
    // actually, `-pkg` flag is not necessary for blackbox test, but we still keep it for consistency
    let package_full_name = pkg.full_name() + "_blackbox_test";
    let package_source_dir: String = pkg.root_path.to_string_lossy().into_owned();

    let workspace_root: Arc<Path> = Arc::clone(&pkg.module_root);

    Ok(RuntestDepItem {
        core_out: core_out.display().to_string(),
        mi_out: mi_out.display().to_string(),
        mbt_deps,
        doctest_only_mbt_deps,
        mbt_md_deps,
        mi_deps,
        package_full_name,
        original_package_full_name: Some(pkg.full_name()),
        package_source_dir,
        workspace_root,
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: true,
        is_third_party: pkg.is_third_party,
        is_internal_test: false,
        is_whitebox_test: false,
        is_blackbox_test: true,
        no_mi: true,
        patch_file,
        mi_of_virtual_pkg_to_impl: None,
        enable_value_tracing: pkg.enable_value_tracing,
    })
}

// Filter out sub-package imports from pkg.imports if their non-sub-package version exists in pkg.test_imports or pkg.wbtest_imports
pub fn get_imports(pkg: &Package, with_test_import: bool) -> Vec<&moonutil::path::ImportComponent> {
    let mut imports = Vec::new();
    let other_imports = if with_test_import {
        &pkg.test_imports
    } else {
        &pkg.wbtest_imports
    };

    // Add filtered imports from pkg.imports
    for import_item in pkg.imports.iter() {
        // If this is not a sub-package import, keep it
        if !import_item.sub_package {
            imports.push(import_item);
        } else {
            // For sub-package imports, check if the same package (non-sub-package) exists in test_imports or wbtest_imports
            let base_path = import_item.path.make_full_path();
            let exists_in_other_imports = other_imports.iter().any(|test_import| {
                test_import.path.make_full_path() == base_path && !test_import.sub_package
            });

            // Keep the import only if it doesn't exist in test_imports as non-sub-package
            if !exists_in_other_imports {
                imports.push(import_item);
            }
        }
    }
    // Add other imports
    imports.extend(other_imports.iter());
    imports
}

/// Performs a topological sort (DFS) to get package dependencies in the correct order.
///
/// This function handles virtual packages by:
/// 1. Tracking virtual-to-implementation mappings via the `overrides` field
/// 2. Resolving virtual packages to their implementations before recursion
/// 3. Including transitive dependencies of implementations
///
/// This ensures that when gathering `.core` files for `moonc link-core`, all required
/// dependencies are included, even if they are transitive dependencies of virtual packages.
///
/// See also: `util::topo_from_node` which uses similar logic for the build command.
fn get_pkg_topo_order<'a>(
    m: &'a ModuleDB,
    leaf: &Package,
    with_wbtest_import: bool,
    with_test_import: bool,
) -> Vec<&'a Package> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut pkg_topo_order: Vec<&Package> = vec![];
    // Track virtual package implementations: virtual_pkg_name -> impl_pkg_name
    let mut virtual_impl: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    fn dfs<'a>(
        m: &'a ModuleDB,
        pkg_topo_order: &mut Vec<&'a Package>,
        visited: &mut HashSet<String>,
        virtual_impl: &mut std::collections::HashMap<String, String>,
        cur_pkg_full_name: &String,
        with_wbtest_import: bool,
        with_test_import: bool,
    ) {
        if visited.contains(cur_pkg_full_name) {
            return;
        }
        visited.insert(cur_pkg_full_name.clone());
        let cur_pkg = m.get_package_by_name(cur_pkg_full_name);

        // Record virtual package implementations from overrides
        if let Some(overrides) = cur_pkg.overrides.as_ref() {
            for implement in overrides.iter() {
                let implement_pkg = m.get_package_by_name(implement);
                if let Some(virtual_pkg) = implement_pkg.implement.as_ref() {
                    virtual_impl.insert(virtual_pkg.clone(), implement_pkg.full_name());
                }
            }
        }

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
            let neighbor_full_name = dep.path.make_full_path();
            let neighbor_pkg = m.get_package_by_name(&neighbor_full_name);

            // Resolve virtual packages to their implementations
            let neighbor_no_virtual = if let Some(virtual_info) = &neighbor_pkg.virtual_pkg {
                // If neighbor is a virtual package, find its implementation
                if let Some(impl_pkg) = virtual_impl.get(&neighbor_full_name) {
                    impl_pkg.clone()
                } else if virtual_info.has_default {
                    neighbor_full_name
                } else {
                    // Skip virtual packages without implementation
                    // This shouldn't happen in a valid project, but we handle it gracefully
                    neighbor_full_name
                }
            } else {
                neighbor_full_name
            };

            dfs(
                m,
                pkg_topo_order,
                visited,
                virtual_impl,
                &neighbor_no_virtual,
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
        &mut virtual_impl,
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
    moonc_opt: &MooncOpt,
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

    let mut core_core_and_abort_core = if moonc_opt.nostd {
        vec![]
    } else {
        moonutil::moon_dir::core_core(moonc_opt.link_opt.target_backend)
    };
    core_core_and_abort_core.extend(core_deps);
    let mut core_deps = core_core_and_abort_core;

    let package_sources = get_package_sources(&pkg_topo_order);
    let package_full_name = if pkg.is_sub_package {
        pkg.full_name().replace(SUB_PKG_POSTFIX, "")
    } else {
        pkg.full_name()
    };

    replace_virtual_pkg_core_with_impl_pkg_core(m, pkg, &mut core_deps)?;

    Ok(RuntestLinkDepItem {
        out: out.display().to_string(),
        core_deps,
        package_full_name,
        package_sources,
        package_path: pkg.root_path.clone(),
        link: Some(calc_link_args(m, pkg)),
        install_path: None,
        bin_name: None,
        stub_lib: pkg.stub_lib.clone(),
    })
}

pub fn gen_link_whitebox_test(
    m: &ModuleDB,
    pkg: &Package,
    moonc_opt: &MooncOpt,
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

    let mut core_core_and_abort_core = if moonc_opt.nostd {
        vec![]
    } else {
        moonutil::moon_dir::core_core(moonc_opt.link_opt.target_backend)
    };
    core_core_and_abort_core.extend(core_deps);
    let mut core_deps = core_core_and_abort_core;

    let package_sources = get_package_sources(&pkg_topo_order);
    let package_full_name = if pkg.is_sub_package {
        pkg.full_name().replace(SUB_PKG_POSTFIX, "")
    } else {
        pkg.full_name()
    };

    replace_virtual_pkg_core_with_impl_pkg_core(m, pkg, &mut core_deps)?;

    Ok(RuntestLinkDepItem {
        out: out.display().to_string(),
        core_deps,
        package_full_name,
        package_sources,
        package_path: pkg.root_path.clone(),
        link: Some(calc_link_args(m, pkg)),
        install_path: None,
        bin_name: None,
        stub_lib: pkg.stub_lib.clone(),
    })
}

pub fn gen_link_blackbox_test(
    m: &ModuleDB,
    pkg: &Package,
    moonc_opt: &MooncOpt,
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
                    .with_file_name(format!("{pkgname}.core"))
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

    let mut core_core_and_abort_core = if moonc_opt.nostd {
        vec![]
    } else {
        moonutil::moon_dir::core_core(moonc_opt.link_opt.target_backend)
    };
    core_core_and_abort_core.extend(core_deps);
    let mut core_deps = core_core_and_abort_core;

    let mut package_sources = get_package_sources(&pkg_topo_order);

    // add blackbox test pkg into `package_sources`, which will be passed to `-pkg-source` in `link-core`
    package_sources.push((
        pkg.full_name() + "_blackbox_test",
        pkg.root_path.display().to_string(),
    ));

    // this will be passed to link-core `-main`
    let package_full_name = if pkg.is_sub_package {
        pkg.full_name().replace(SUB_PKG_POSTFIX, "")
    } else {
        pkg.full_name()
    } + "_blackbox_test";

    replace_virtual_pkg_core_with_impl_pkg_core(m, pkg, &mut core_deps)?;

    Ok(RuntestLinkDepItem {
        out: out.display().to_string(),
        core_deps,
        package_full_name,
        package_sources,
        package_path: pkg.root_path.clone(),
        link: Some(calc_link_args(m, pkg)),
        install_path: None,
        bin_name: None,
        stub_lib: pkg.stub_lib.clone(),
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
    let mut build_interface_items = vec![];
    let mut build_default_virtual_items = vec![];
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
        if pkg.stub_lib.is_some() {
            compile_stub_items.push(RuntestLinkDepItem {
                out: pkg.artifact.with_extension(O_EXT).display().to_string(),
                core_deps: vec![],
                package_sources: vec![],
                package_full_name: pkg.full_name(),
                package_path: pkg.root_path.clone(),
                link: Some(calc_link_args(m, pkg)),
                install_path: None,
                bin_name: None,
                stub_lib: pkg.stub_lib.clone(),
            });
        }

        if let Some(v) = pkg.virtual_pkg.as_ref() {
            // don't need to build for virtual pkg in core since it is already bundled
            if !(pkg.full_name().starts_with(MOONBITLANG_CORE) && pkg.is_third_party) {
                build_interface_items.push(gen_build_interface_item(m, pkg)?);
                if v.has_default {
                    build_default_virtual_items.push(gen_package_core(m, pkg, moonc_opt)?);
                }
            }
        } else {
            build_items.push(gen_package_core(m, pkg, moonc_opt)?);
        }

        if pkg.is_third_party {
            continue;
        }

        if let Some(filter_pkg) = filter_pkg
            && !filter_pkg.contains(pkgname)
        {
            continue;
        }

        let has_internal_test = {
            let mut res = false;
            for (path, _) in &pkg.files {
                let content = std::fs::read_to_string(path)?;
                for line in content.lines() {
                    if line.starts_with("test") {
                        res = true;
                        break;
                    }
                }
            }
            res
        };

        if has_internal_test {
            for item in pkg.generated_test_drivers.iter() {
                if let GeneratedTestDriver::InternalTest(_) = item {
                    test_drivers.push(gen_package_test_driver(
                        moonc_opt,
                        &moonbuild_opt.target_dir,
                        item,
                        pkg,
                    )?);
                    build_items.push(gen_package_internal_test(
                        m,
                        pkg,
                        moonc_opt,
                        internal_patch_file.clone(),
                    )?);
                    link_items.push(gen_link_internal_test(m, pkg, moonc_opt)?);
                }
            }
        }

        if !pkg.wbtest_files.is_empty() || whitebox_patch_file.is_some() {
            for item in pkg.generated_test_drivers.iter() {
                if let GeneratedTestDriver::WhiteboxTest(_) = item {
                    test_drivers.push(gen_package_test_driver(
                        moonc_opt,
                        &moonbuild_opt.target_dir,
                        item,
                        pkg,
                    )?);
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

        if pkg.virtual_pkg.as_ref().is_none_or(|x| x.has_default)
            && !SKIP_TEST_LIBS.contains(&pkg.full_name().as_str()) // FIXME: not efficient
            && (!pkg.test_files.is_empty()
                || !pkg.mbt_md_files.is_empty()
                || !pkg.files.is_empty()
                || blackbox_patch_file.is_some())
        {
            for item in pkg.generated_test_drivers.iter() {
                if let GeneratedTestDriver::BlackboxTest(_) = item {
                    test_drivers.push(gen_package_test_driver(
                        moonc_opt,
                        &moonbuild_opt.target_dir,
                        item,
                        pkg,
                    )?);
                    build_items.push(gen_package_blackbox_test(
                        m,
                        pkg,
                        moonc_opt,
                        blackbox_patch_file.clone(),
                    )?);
                    link_items.push(gen_link_blackbox_test(m, pkg, moonc_opt)?);
                }
            }
        }
    }

    Ok(N2RuntestInput {
        build_interface_items,
        build_default_virtual_items,
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
    need_build_default_virtual: bool,
) -> Build {
    let core_output_id = graph.files.id_from_canonical(item.core_out.clone());
    let mi_output_id = graph.files.id_from_canonical(item.mi_out.clone());

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("build")),
        line: 0,
    };

    let mut inputs = item.mbt_deps.clone();
    inputs.extend_from_slice(&item.doctest_only_mbt_deps);
    inputs.extend_from_slice(&item.mbt_md_deps);
    inputs.extend(item.mi_deps.iter().map(|a| a.name.clone()));
    // add $pkgname.mi as input if need_build_virtual since it is used by --check-mi
    if need_build_default_virtual {
        inputs.push(
            PathBuf::from(&item.core_out)
                .with_extension("mi")
                .display()
                .to_string(),
        );
    }
    if let Some((mi_path, _, _)) = item.mi_of_virtual_pkg_to_impl.as_ref() {
        inputs.push(mi_path.clone());
    }
    if let Some(ref patch_file) = item.patch_file {
        inputs.push(patch_file.display().to_string());
    }

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
        ids: if item.no_mi || need_build_default_virtual {
            vec![core_output_id]
        } else {
            vec![core_output_id, mi_output_id]
        },
        explicit: 1,
    };

    let coverage_args = coverage_args(
        enable_coverage_during_compile(moonc_opt, item.is_blackbox_test, item.is_third_party),
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
        .args(&item.mbt_md_deps)
        .args(
            item.doctest_only_mbt_deps
                .iter()
                .flat_map(|x| ["-doctest-only", x]),
        )
        .lazy_args_with_cond(item.warn_list.is_some(), || {
            vec!["-w".to_string(), item.warn_list.clone().unwrap()]
        })
        .lazy_args_with_cond(item.alert_list.is_some(), || {
            vec!["-alert".to_string(), item.alert_list.clone().unwrap()]
        })
        .args_with_cond(item.is_third_party, ["-w", "-a", "-alert", "-all"])
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
        .arg_with_cond(item.is_blackbox_test, "-include-doctests")
        .arg_with_cond(item.no_mi, "-no-mi")
        .lazy_args_with_cond(item.patch_file.is_some(), || {
            vec![
                "-patch-file".to_string(),
                item.patch_file.as_ref().unwrap().display().to_string(),
            ]
        })
        .args_with_cond(
            need_build_default_virtual,
            vec![
                "-check-mi".to_string(),
                PathBuf::from(&item.core_out)
                    .with_extension("mi")
                    .display()
                    .to_string(),
                "-no-mi".to_string(),
            ],
        )
        .lazy_args_with_cond(item.mi_of_virtual_pkg_to_impl.as_ref().is_some(), || {
            let (mi_path, pkg_name, pkg_path) = item.mi_of_virtual_pkg_to_impl.as_ref().unwrap();
            vec![
                "-check-mi".to_string(),
                mi_path.clone(),
                "-impl-virtual".to_string(),
                // implementation package should not been import so here don't emit .mi
                "-no-mi".to_string(),
                "-pkg-sources".to_string(),
                format!("{}:{}", &pkg_name, &pkg_path,),
            ]
        })
        .arg_with_cond(
            item.is_internal_test || item.is_whitebox_test || item.is_blackbox_test,
            "-test-mode",
        )
        .arg_with_cond(item.enable_value_tracing, "-enable-value-tracing")
        .arg("-workspace-path")
        .arg(&item.workspace_root.display().to_string())
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
                .map(|(pkg, src)| format!("{pkg}:{src}")),
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
        // note: this is a workaround for windows cl, x86_64-pc-windows-gnu also need to consider
        .args_with_cond(
            cfg!(target_os = "windows")
                && moonc_opt.link_opt.target_backend == TargetBackend::LLVM
                && CC::default().is_msvc(),
            ["-llvm-target", "x86_64-pc-windows-msvc"],
        )
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

    if input.link_items.is_empty() {
        anyhow::bail!(
            "Cannot find tests to run. Please check if you have supplied the correct package name for testing."
        );
    }

    for item in input.build_items.iter() {
        let build = gen_runtest_build_command(&mut graph, item, moonc_opt, false);
        graph.add_build(build)?;
    }

    let is_native_backend = moonc_opt.link_opt.target_backend == TargetBackend::Native;
    let is_llvm_backend = moonc_opt.link_opt.target_backend == TargetBackend::LLVM;

    // compile runtime.o or libruntime.so
    let mut runtime_path = None;

    if is_native_backend || is_llvm_backend {
        fn gen_shared_runtime(
            graph: &mut n2graph::Graph,
            target_dir: &std::path::Path,
            default: &mut Vec<n2graph::FileId>,
        ) -> anyhow::Result<PathBuf> {
            let (build, path) = gen_compile_shared_runtime_command(graph, target_dir);
            graph.add_build(build)?;
            // we explicitly add it to default because shared runtime is not a target or depended by any target
            default.push(graph.files.id_from_canonical(path.display().to_string()));
            Ok(path)
        }

        fn gen_runtime(
            graph: &mut n2graph::Graph,
            target_dir: &std::path::Path,
        ) -> anyhow::Result<PathBuf> {
            let (build, path) = gen_compile_runtime_command(graph, target_dir);
            graph.add_build(build)?;
            Ok(path)
        }

        runtime_path = Some(if moonbuild_opt.use_tcc_run {
            gen_shared_runtime(&mut graph, &moonbuild_opt.target_dir, &mut default)?
        } else {
            gen_runtime(&mut graph, &moonbuild_opt.target_dir)?
        });
    }

    for item in input.link_items.iter() {
        let (build, fid) = gen_runtest_link_command(&mut graph, item, moonc_opt);
        let mut default_fid = fid;
        graph.add_build(build)?;

        if is_native_backend && !moonbuild_opt.use_tcc_run {
            let (build, fid) = gen_compile_exe_command(
                &mut graph,
                item,
                moonc_opt,
                moonbuild_opt,
                runtime_path.as_ref().unwrap().display().to_string(),
            );
            default_fid = fid;
            graph.add_build(build)?;
        }
        if is_llvm_backend {
            let (build, fid) = gen_link_exe_command(
                &mut graph,
                item,
                moonc_opt,
                moonbuild_opt,
                runtime_path.as_ref().unwrap().display().to_string(),
            );
            graph.add_build(build)?;
            default_fid = fid;
        }

        default.push(default_fid);
    }
    for item in input.test_drivers.iter() {
        let build = gen_generate_test_driver_command(&mut graph, item, moonc_opt, moonbuild_opt);
        graph.add_build(build)?;
    }

    for item in input.build_interface_items.iter() {
        let (build, _) = gen_build_interface_command(&mut graph, item, moonc_opt);
        graph.add_build(build)?;
    }

    for item in input.build_default_virtual_items.iter() {
        let build = gen_runtest_build_command(&mut graph, item, moonc_opt, true);
        graph.add_build(build)?;
    }

    if is_native_backend || is_llvm_backend {
        for item in input.compile_stub_items.iter() {
            let builds = gen_compile_stub_command(&mut graph, item, moonc_opt, moonbuild_opt);
            for (build, _fid) in builds {
                graph.add_build(build)?;
                // don't need to add fid to default, since it would be deps of test.exe
            }
            if !moonbuild_opt.use_tcc_run {
                let (build, _) =
                    gen_archive_stub_to_static_lib_command(&mut graph, item, moonc_opt);
                graph.add_build(build)?;
            } else {
                let (build, fid) = gen_link_stub_to_dynamic_lib_command(
                    &mut graph,
                    item,
                    runtime_path.as_ref().unwrap(),
                    moonc_opt,
                    moonbuild_opt,
                );
                graph.add_build(build)?;
                default.push(fid);
            }
        }
    }

    if default.is_empty() {
        anyhow::bail!(
            "No default build found. This should be already handled \
            by previous checks, might be a build system bug."
        );
    }

    let mut hashes = n2graph::Hashes::default();
    let n2_db_path = &moonbuild_opt.target_dir.join("build.moon_db");
    if !n2_db_path.parent().unwrap().exists() {
        std::fs::create_dir_all(n2_db_path.parent().unwrap()).unwrap();
    }
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

    let mut in_files = files_contain_test_block.clone();
    in_files.extend_from_slice(&item.doctest_only_files);

    let ins = BuildIns {
        explicit: in_files.len(),
        ids: in_files
            .into_iter()
            .map(|f| graph.files.id_from_canonical(f.to_string()))
            .collect(),
        implicit: 0,
        order_only: 0,
    };
    let outs = BuildOuts {
        explicit: 0,
        ids: [driver_file, &item.info_file]
            .into_iter()
            .map(|x| graph.files.id_from_canonical(x.display().to_string()))
            .collect(),
    };

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("build")),
        line: 0,
    };

    // Note: this controls whether to emit the coverage collection command in
    // the generated test driver, so it's always the same as `enable_coverage`,
    // unlike the body of black box tests which don't need such.
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
    // Output files
    .arg("--output-driver")
    .arg(&driver_file.display().to_string())
    .arg("--output-metadata")
    .arg(&item.info_file.display().to_string())
    // Input files
    .args(&item.files_may_contain_test_block)
    .args(
        item.doctest_only_files
            .iter()
            .flat_map(|x| ["--doctest-only", x]),
    )
    // Patch file
    .lazy_args_with_cond(patch_file.is_some(), || {
        vec![
            "--patch-file".to_string(),
            patch_file.unwrap().display().to_string(),
        ]
    })
    // Configuration
    .args(["--target", moonc_opt.build_opt.target_backend.to_flag()])
    .args(["--pkg-name", &item.package_name])
    .arg_with_cond(matches!(moonbuild_opt.run_mode, RunMode::Bench), "--bench")
    // coverage args directly from our friendly function
    .args(coverage_args)
    // Driver kind
    .arg("--driver-kind")
    .arg(&item.driver_kind.to_string())
    .build();

    build.cmdline = Some(command);
    build.desc = Some(format!(
        "gen-test-driver: {}_{}_test",
        item.package_name, item.driver_kind
    ));
    build
}

/// This is part of the easier quick fix for the coverage racing condition.
/// `moonc` can't tell between an extracted doctest in blackbox test and a
/// regular file, so we can just disable coverage for blackbox tests altogether.
fn enable_coverage_during_compile(
    moonc_opt: &MooncOpt,
    is_blackbox: bool,
    is_3rd_party: bool,
) -> bool {
    moonc_opt.build_opt.enable_coverage && !is_blackbox && !is_3rd_party
}
