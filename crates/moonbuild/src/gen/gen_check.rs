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
use super::gen_build::{gen_build_interface_command, gen_build_interface_item, BuildInterfaceItem};
use super::n2_errors::{N2Error, N2ErrorKind};
use super::util::self_in_test_import;
use crate::r#gen::MiAlias;
use anyhow::bail;
use colored::Colorize;
use indexmap::map::IndexMap;
use moonutil::module::ModuleDB;
use moonutil::package::Package;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use moonutil::common::{
    get_desc_name, CheckOpt, MoonbuildOpt, MooncOpt, MOON_PKG_JSON, SUB_PKG_POSTFIX,
};
use n2::graph::{self as n2graph, Build, BuildIns, BuildOuts, FileLoc};
use n2::load::State;
use n2::smallmap::SmallMap;

#[derive(Debug)]
pub struct CheckDepItem {
    pub mi_out: String,
    pub mbt_deps: Vec<String>,
    /// MoonBit source files that only need doc testing
    pub doctest_only_mbt_deps: Vec<String>,
    /// `mbt.md` files
    pub mbt_md_deps: Vec<String>,
    pub mi_deps: Vec<MiAlias>,
    pub package_full_name: String,
    pub package_source_dir: String,
    /// Canonical absolute path to the module directory (parent of moon.mod.json)
    pub workspace_root: Arc<Path>,
    pub warn_list: Option<String>,
    pub alert_list: Option<String>,
    pub is_main: bool,
    pub is_third_party: bool,
    pub patch_file: Option<PathBuf>,
    pub no_mi: bool,
    pub is_whitebox_test: bool,
    pub is_blackbox_test: bool,

    pub need_check_default_virtual: bool,
    // which virtual pkg to implement (mi path, virtual pkg name, virtual pkg path)
    pub mi_of_virtual_pkg_to_impl: Option<(String, String, String)>,
}

#[derive(Debug)]
pub struct N2CheckInput {
    pub dep_items: Vec<CheckDepItem>,
    pub check_interface_items: Vec<BuildInterfaceItem>,
}

fn pkg_to_check_item(
    m: &ModuleDB,
    source_dir: &Path,
    packages: &IndexMap<String, Package>,
    pkg: &Package,
    moonc_opt: &MooncOpt,
    need_check_default_virtual: bool,
) -> anyhow::Result<CheckDepItem> {
    let mut out = pkg.artifact.with_extension("mi");
    if need_check_default_virtual {
        let file_stem = format!(
            "{}_{}",
            out.file_stem().unwrap().to_str().unwrap(),
            "default.mi"
        );
        out = out.with_file_name(file_stem);
    }

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
        let mut full_import_name = dep.path.make_full_path();
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
        if dep.sub_package {
            full_import_name = format!("{full_import_name}{SUB_PKG_POSTFIX}");
        }
        let cur_pkg = &packages[&full_import_name];
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
    let package_source_dir: String = pkg.root_path.to_string_lossy().into_owned();

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

    Ok(CheckDepItem {
        mi_out: out.display().to_string(),
        mbt_deps,
        mi_deps,
        doctest_only_mbt_deps: vec![],
        mbt_md_deps: vec![],
        package_full_name,
        package_source_dir,
        workspace_root,
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: pkg.is_main,
        is_third_party: pkg.is_third_party,
        is_whitebox_test: false,
        is_blackbox_test: false,
        patch_file: pkg.patch_file.as_ref().and_then(|p| {
            let file_stem = p.file_stem().unwrap().to_str().unwrap();
            (!file_stem.ends_with("_wbtest") && !file_stem.ends_with("_test")).then_some(p.clone())
        }),
        no_mi: pkg.no_mi,
        mi_of_virtual_pkg_to_impl: impl_virtual_pkg,
        need_check_default_virtual,
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

    let imports_and_wbtest_imports = super::gen_runtest::get_imports(pkg, false);
    for dep in imports_and_wbtest_imports.iter() {
        let mut full_import_name = dep.path.make_full_path();
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
        if dep.sub_package {
            full_import_name = format!("{full_import_name}{SUB_PKG_POSTFIX}");
        }
        let cur_pkg = &packages[&full_import_name];
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
    let package_source_dir: String = pkg.root_path.to_string_lossy().into_owned();

    let workspace_root: Arc<Path> = Arc::clone(&pkg.module_root);

    Ok(CheckDepItem {
        mi_out: out.display().to_string(),
        mbt_deps,
        mi_deps,
        doctest_only_mbt_deps: vec![],
        mbt_md_deps: vec![],
        package_full_name,
        package_source_dir,
        workspace_root,
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: pkg.is_main,
        is_third_party: pkg.is_third_party,
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
        mi_of_virtual_pkg_to_impl: None,
        need_check_default_virtual: false,
    })
}

pub(super) fn warn_about_alias_duplication(self_in_test_import: bool, pkg: &Package) {
    if !self_in_test_import {
        if let Some(violating) = pkg
            .test_imports
            .iter()
            .chain(pkg.imports.iter())
            .find(|import| {
                import
                    .alias
                    .as_ref()
                    .is_some_and(|alias| alias.eq(pkg.last_name()))
            })
        {
            eprintln!(
                "{}: Duplicate alias `{}` at \"{}\". \
                \"test-import\" will automatically add \"import\" and current \
                package as dependency so you don't need to add it manually. \
                If you're test-importing a dependency with the same default \
                alias as your current package, considering give it a different \
                alias than the current package. \
                Violating import: `{}`",
                "Warning".yellow(),
                pkg.last_name(),
                pkg.root_path.join(MOON_PKG_JSON).display(),
                violating.path.make_full_path()
            );
        }
    }
}

fn pkg_with_test_to_check_item(
    source_dir: &Path,
    packages: &IndexMap<String, Package>,
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<CheckDepItem> {
    let self_in_test_import = self_in_test_import(pkg);

    warn_about_alias_duplication(self_in_test_import, pkg);

    let out = pkg
        .artifact
        .with_file_name(format!("{}.blackbox_test.mi", pkg.last_name()));

    // FIXME: This part is exactly the same as `gen_runtest::gen_package_blackbox_test`.
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
    let mbt_deps: Vec<String> = backend_filtered
        .iter()
        .map(|f| f.display().to_string())
        .collect::<Vec<_>>();

    let mbt_md_deps = pkg
        .mbt_md_files
        .keys()
        .map(|f| f.display().to_string())
        .collect();

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

    let imports_and_test_imports = super::gen_runtest::get_imports(pkg, true);
    for dep in imports_and_test_imports.iter() {
        let mut full_import_name = dep.path.make_full_path();
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
        if dep.sub_package {
            full_import_name = format!("{full_import_name}{SUB_PKG_POSTFIX}");
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
    let package_full_name = if pkg.is_sub_package {
        pkg.full_name().replace(SUB_PKG_POSTFIX, "")
    } else {
        pkg.full_name()
    } + "_blackbox_test";
    let package_source_dir: String = pkg.root_path.to_string_lossy().into_owned();

    let workspace_root: Arc<Path> = Arc::clone(&pkg.module_root);

    Ok(CheckDepItem {
        mi_out: out.display().to_string(),
        mbt_deps,
        mi_deps,
        doctest_only_mbt_deps,
        mbt_md_deps,
        package_full_name,
        package_source_dir,
        workspace_root,
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: pkg.is_main,
        is_third_party: pkg.is_third_party,
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
        mi_of_virtual_pkg_to_impl: None,
        need_check_default_virtual: false,
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
    let mut check_interface_items = vec![];

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
        // skip virtual moonbitlang/core/abort (gen_moonbitlang_abort_pkg)
        if pkg
            .full_name()
            .starts_with(moonutil::common::MOONBITLANG_CORE)
            && pkg.is_third_party
        {
            continue;
        }

        if pkg.virtual_pkg.is_none() {
            let item = pkg_to_check_item(m, &pkg.root_path, pkgs_to_check, pkg, moonc_opt, false)?;
            dep_items.push(item);
        } else {
            check_interface_items.push(gen_build_interface_item(m, pkg)?);
            if pkg.virtual_pkg.as_ref().is_some_and(|v| v.has_default) {
                let item =
                    pkg_to_check_item(m, &pkg.root_path, pkgs_to_check, pkg, moonc_opt, true)?;
                dep_items.push(item);
            }
        }

        // do not check test files for third party packages
        if !pkg.is_third_party {
            if !pkg.wbtest_files.is_empty() {
                let item =
                    pkg_with_wbtest_to_check_item(&pkg.root_path, pkgs_to_check, pkg, moonc_opt)?;
                dep_items.push(item);
            }
            if !pkg.test_files.is_empty() {
                let item =
                    pkg_with_test_to_check_item(&pkg.root_path, pkgs_to_check, pkg, moonc_opt)?;
                dep_items.push(item);
            }
        }
    }

    // dbg!(&dep_items);
    Ok(N2CheckInput {
        dep_items,
        check_interface_items,
    })
}

pub fn gen_check_command(
    graph: &mut n2graph::Graph,
    item: &CheckDepItem,
    moonc_opt: &MooncOpt,
    need_check_default_virtual: bool,
) -> Build {
    let mi_output_id = graph.files.id_from_canonical(item.mi_out.clone());
    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("check")),
        line: 0,
    };

    let original_mi_out = item.mi_out.replace("_default", "");

    let mut inputs = item.mbt_deps.clone();
    inputs.extend_from_slice(&item.doctest_only_mbt_deps);
    inputs.extend_from_slice(&item.mbt_md_deps);
    inputs.extend(item.mi_deps.iter().map(|a| a.name.clone()));
    // add $pkgname.mi as input if need_build_virtual since it is used by --check-mi
    if need_check_default_virtual {
        inputs.push(original_mi_out.clone());
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
                "@all-raise-throw-unsafe-test_import_all+deprecated",
            ],
        )
        .args(&item.mbt_deps)
        .args(&item.mbt_md_deps)
        .args(
            item.doctest_only_mbt_deps
                .iter()
                .flat_map(|v| ["-doctest-only", v]),
        )
        .arg_with_cond(item.is_blackbox_test, "-include-doctests")
        .lazy_args_with_cond(item.warn_list.is_some(), || {
            vec!["-w".to_string(), item.warn_list.clone().unwrap()]
        })
        .lazy_args_with_cond(item.alert_list.is_some(), || {
            vec!["-alert".to_string(), item.alert_list.clone().unwrap()]
        })
        .args_with_cond(item.is_third_party, ["-w", "-a", "-alert", "-all"])
        .arg("-o")
        .arg(&item.mi_out)
        .arg("-pkg")
        .arg(&item.package_full_name)
        .arg_with_cond(item.is_main && !item.is_blackbox_test, "-is-main")
        .arg_with_cond(moonc_opt.single_file, "-single-file")
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
        .args_with_cond(
            need_check_default_virtual,
            vec!["-check-mi".to_string(), original_mi_out],
        )
        .lazy_args_with_cond(item.mi_of_virtual_pkg_to_impl.as_ref().is_some(), || {
            let (mi_path, pkg_name, pkg_path) = item.mi_of_virtual_pkg_to_impl.as_ref().unwrap();
            vec![
                "-check-mi".to_string(),
                mi_path.clone(),
                "-pkg-sources".to_string(),
                format!("{}:{}", &pkg_name, &pkg_path,),
            ]
        })
        .arg("-workspace-path")
        .arg(&item.workspace_root.display().to_string())
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build.desc = Some(format!(
        "check: {}",
        get_desc_name(&item.package_full_name, &item.mi_out)
    ));
    build.can_dirty_on_output = true;
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
        let build = gen_check_command(&mut graph, item, moonc_opt, item.need_check_default_virtual);
        graph.add_build(build)?;
    }

    for item in input.check_interface_items.iter() {
        let (build, _) = gen_build_interface_command(&mut graph, item, moonc_opt);
        graph.add_build(build)?;
    }

    let mut hashes = n2graph::Hashes::default();
    let n2_db_path = &target_dir.join("check.moon_db");
    if !n2_db_path.parent().unwrap().exists() {
        std::fs::create_dir_all(n2_db_path.parent().unwrap()).unwrap();
    }
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
