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

use anyhow::{bail, Context, Ok};
use colored::Colorize;
use moonutil::compiler_flags::{
    make_archiver_command, make_cc_command, make_linker_command, ArchiverConfigBuilder,
    CCConfigBuilder, LinkerConfigBuilder, OptLevel, OutputType, CC,
};
use moonutil::module::ModuleDB;
use moonutil::moon_dir::MOON_DIRS;
use moonutil::package::{JsFormat, LinkDepItem, Package};

use super::cmd_builder::CommandBuilder;
use super::n2_errors::{N2Error, N2ErrorKind};
use super::util::calc_link_args;
use crate::gen::MiAlias;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use moonutil::common::{
    BuildOpt, MoonbuildOpt, MooncOpt, TargetBackend, A_EXT, DYN_EXT, MOONBITLANG_CORE,
    MOON_PKG_JSON, O_EXT, SUB_PKG_POSTFIX,
};
use n2::graph::{self as n2graph, Build, BuildIns, BuildOuts, FileLoc};
use n2::load::State;
use n2::smallmap::SmallMap;

#[derive(Debug)]
pub struct BuildInterfaceItem {
    pub mi_out: String,
    pub mbti_deps: String,
    pub mi_deps: Vec<MiAlias>,
    pub package_full_name: String,
    pub package_source_dir: String,
}

#[derive(Debug)]
pub struct BuildDepItem {
    pub core_out: String,
    pub mi_out: String,
    pub mbt_deps: Vec<String>,
    pub mi_deps: Vec<MiAlias>, // do not need add parent's mi files
    pub package_full_name: String,
    pub package_source_dir: String,
    pub warn_list: Option<String>,
    pub alert_list: Option<String>,
    pub is_main: bool,
    pub is_third_party: bool,
    pub enable_value_tracing: bool,

    // which virtual pkg to implement (mi path, virtual pkg name, virtual pkg path)
    pub mi_of_virtual_pkg_to_impl: Option<(String, String, String)>,
}

type BuildLinkDepItem = moonutil::package::LinkDepItem;

#[derive(Debug)]
pub struct N2BuildInput {
    // for virtual pkg
    pub build_interface_items: Vec<BuildInterfaceItem>,
    pub build_default_virtual_items: Vec<BuildDepItem>,

    pub build_items: Vec<BuildDepItem>,
    pub link_items: Vec<BuildLinkDepItem>, // entry points
    pub compile_stub_items: Vec<BuildLinkDepItem>,
}

fn to_opt_level(release: bool, debug: bool) -> OptLevel {
    match (release, debug) {
        (true, false) => OptLevel::Speed,
        (true, true) => OptLevel::Debug,
        (false, true) => OptLevel::Debug,
        (false, false) => OptLevel::None,
    }
}

pub fn gen_build_interface_item(m: &ModuleDB, pkg: &Package) -> anyhow::Result<BuildInterfaceItem> {
    let virtual_mbti_file_path = pkg.virtual_mbti_file.as_ref().unwrap();

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

    let mi_out = pkg.artifact.with_extension("mi");
    let package_full_name = pkg.full_name();
    let package_source_dir: String = pkg.root_path.to_string_lossy().into_owned();

    Ok(BuildInterfaceItem {
        mi_out: mi_out.display().to_string(),
        mbti_deps: virtual_mbti_file_path.display().to_string(),
        mi_deps,
        package_full_name,
        package_source_dir,
    })
}

pub fn gen_build_build_item(
    m: &ModuleDB,
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<BuildDepItem> {
    let core_out = pkg.artifact.with_extension("core");
    let mi_out = pkg.artifact.with_extension("mi");

    let backend_filtered = moonutil::common::backend_filter(
        &pkg.files,
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.target_backend,
    );

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
            full_import_name = format!("{}{}", full_import_name, SUB_PKG_POSTFIX);
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

    Ok(BuildDepItem {
        core_out: core_out.display().to_string(),
        mi_out: mi_out.display().to_string(),
        mbt_deps,
        mi_deps,
        package_full_name,
        package_source_dir,
        warn_list: pkg.warn_list.clone(),
        alert_list: pkg.alert_list.clone(),
        is_main: pkg.is_main,
        is_third_party: pkg.is_third_party,
        enable_value_tracing: pkg.enable_value_tracing,
        mi_of_virtual_pkg_to_impl: impl_virtual_pkg,
    })
}

pub fn gen_build_link_item(
    m: &ModuleDB,
    pkg: &Package,
    moonc_opt: &MooncOpt,
) -> anyhow::Result<BuildLinkDepItem> {
    let out = pkg.artifact.with_extension("wat"); // TODO: extension is determined by build option
    let package_full_name = if pkg.is_sub_package {
        pkg.full_name().replace(SUB_PKG_POSTFIX, "")
    } else {
        pkg.full_name()
    };

    let mut core_core_and_abort_core = if moonc_opt.nostd {
        vec![]
    } else {
        moonutil::moon_dir::core_core(moonc_opt.link_opt.target_backend)
    };
    let tp = super::util::topo_from_node(m, pkg)?;
    let core_deps = super::util::nodes_to_cores(m, &tp);
    core_core_and_abort_core.extend(core_deps);
    let mut core_deps = core_core_and_abort_core;

    let package_sources = super::util::nodes_to_pkg_sources(m, &tp);

    replace_virtual_pkg_core_with_impl_pkg_core(m, pkg, &mut core_deps)?;

    Ok(BuildLinkDepItem {
        out: out.display().to_string(),
        core_deps,
        package_sources,
        package_full_name,
        package_path: pkg.root_path.clone(),
        link: Some(calc_link_args(m, pkg)),
        install_path: pkg.install_path.clone(),
        bin_name: pkg.bin_name.clone(),
        stub_lib: pkg.stub_lib.clone(),
    })
}

pub fn replace_virtual_pkg_core_with_impl_pkg_core(
    m: &ModuleDB,
    pkg: &Package,
    core_deps: &mut [String],
) -> anyhow::Result<()> {
    if let Some(overrides) = pkg.overrides.as_ref() {
        for implementation in overrides {
            let impl_pkg = m.get_package_by_name(implementation);
            let virtual_pkg = m.get_package_by_name(impl_pkg.implement.as_ref().unwrap());
            // replace .core of the imported virtual pkg with impl pkg
            for core_dep in core_deps.iter_mut() {
                if core_dep
                    == &virtual_pkg
                        .artifact
                        .with_extension("core")
                        .display()
                        .to_string()
                {
                    *core_dep = impl_pkg
                        .artifact
                        .with_extension("core")
                        .display()
                        .to_string();
                }
            }
        }
    }
    Ok(())
}

pub fn gen_build(
    m: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<N2BuildInput> {
    let mut build_interface_items = vec![];
    let mut build_default_virtual_items = vec![];
    let mut build_items = vec![];
    let mut link_items = vec![];
    let mut compile_stub_items = vec![];

    let pkgs_to_build = if let Some(BuildOpt {
        filter_package: Some(filter_package),
        ..
    }) = moonbuild_opt.build_opt.as_ref()
    {
        &m.get_filtered_packages_and_its_deps_by_pkgname(filter_package)?
    } else {
        m.get_all_packages()
    };

    for (_, pkg) in pkgs_to_build {
        let is_main = pkg.is_main;

        if let Some(v) = pkg.virtual_pkg.as_ref() {
            // don't need to build for virtual pkg in core since it is already bundled
            if !(pkg.full_name().starts_with(MOONBITLANG_CORE) && pkg.is_third_party) {
                build_interface_items.push(gen_build_interface_item(m, pkg)?);
                if v.has_default {
                    build_default_virtual_items.push(gen_build_build_item(m, pkg, moonc_opt)?);
                }
            }
        } else {
            build_items.push(gen_build_build_item(m, pkg, moonc_opt)?);
        }

        if pkg.stub_lib.is_some() {
            compile_stub_items.push(BuildLinkDepItem {
                out: pkg.artifact.with_extension(O_EXT).display().to_string(),
                core_deps: vec![],
                package_sources: vec![],
                package_full_name: pkg.full_name(),
                package_path: pkg.root_path.clone(),
                link: pkg.link.clone(),
                install_path: None,
                bin_name: None,
                stub_lib: pkg.stub_lib.clone(),
            });
        }

        let force_link = pkg.force_link;
        let needs_link = pkg
            .link
            .as_ref()
            .is_some_and(|l| l.need_link(moonc_opt.build_opt.target_backend));
        if (is_main || force_link || needs_link) && !pkg.is_third_party {
            // link need add *.core files recursively
            link_items.push(gen_build_link_item(m, pkg, moonc_opt)?);
        }
    }
    Ok(N2BuildInput {
        build_interface_items,
        build_default_virtual_items,
        build_items,
        link_items,
        compile_stub_items,
    })
}

pub fn gen_build_interface_command(
    graph: &mut n2graph::Graph,
    item: &BuildInterfaceItem,
    moonc_opt: &MooncOpt,
) -> (Build, n2graph::FileId) {
    let mi_output_id = graph.files.id_from_canonical(item.mi_out.clone());

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("build")),
        line: 0,
    };

    let mut inputs = vec![item.mbti_deps.clone()];
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
        ids: vec![mi_output_id],
        explicit: 1,
    };

    let mut build = Build::new(loc, ins, outs);

    let command = CommandBuilder::new("moonc")
        .arg("build-interface")
        .arg(&item.mbti_deps)
        .arg("-o")
        .arg(&item.mi_out)
        .args_with_prefix_separator(mi_files_with_alias, "-i")
        .arg("-pkg")
        .arg(&item.package_full_name)
        .arg("-pkg-sources")
        .arg(&format!(
            "{}:{}",
            &item.package_full_name, &item.package_source_dir
        ))
        .arg("-virtual")
        .args_with_cond(
            !moonc_opt.nostd,
            [
                "-std-path",
                moonutil::moon_dir::core_bundle(moonc_opt.link_opt.target_backend)
                    .to_str()
                    .unwrap(),
            ],
        )
        .arg("-error-format=json")
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build.desc = Some(format!("build-interface: {}", item.package_full_name));
    (build, mi_output_id)
}

pub fn gen_build_command(
    graph: &mut n2graph::Graph,
    item: &BuildDepItem,
    moonc_opt: &MooncOpt,
    need_build_default_virtual: bool,
) -> (Build, n2graph::FileId) {
    let core_output_id = graph.files.id_from_canonical(item.core_out.clone());
    let mi_output_id = graph.files.id_from_canonical(item.mi_out.clone());

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("build")),
        line: 0,
    };

    let mut inputs = item.mbt_deps.clone();
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
        ids: if need_build_default_virtual {
            vec![core_output_id]
        } else {
            vec![core_output_id, mi_output_id]
        },
        explicit: 1,
    };

    // WORKAROUND: do not test coverage on coverage library itself, because of cyclic dependency
    let enable_coverage = moonc_opt.build_opt.enable_coverage
        && !super::is_skip_coverage_lib(&item.package_full_name)
        && !item.is_third_party;
    // WORKAROUND: lang core/builtin and core/coverage should be able to cover themselves
    let self_coverage = enable_coverage && super::is_self_coverage_lib(&item.package_full_name);

    let mut build = Build::new(loc, ins, outs);

    let (debug_flag, strip_flag) = (
        moonc_opt.build_opt.debug_flag,
        moonc_opt.build_opt.strip_flag,
    );

    let command = CommandBuilder::new("moonc")
        .arg("build-package")
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
        .arg_with_cond(enable_coverage, "-enable-coverage")
        .arg_with_cond(self_coverage, "-coverage-package-override=@self")
        .args(moonc_opt.extra_build_opt.iter())
        .arg_with_cond(item.enable_value_tracing, "-enable-value-tracing")
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
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build.desc = Some(format!("build-package: {}", item.package_full_name));
    (build, core_output_id)
}

pub fn gen_link_command(
    graph: &mut n2graph::Graph,
    item: &BuildLinkDepItem,
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

    let exports = item.exports(moonc_opt.link_opt.target_backend);
    let export_memory_name = item.export_memory_name(moonc_opt.link_opt.target_backend);
    let heap_start_address = item.heap_start_address(moonc_opt.link_opt.target_backend);
    let import_memory = item.import_memory(moonc_opt.link_opt.target_backend);
    let memory_limits = item.memory_limits(moonc_opt.link_opt.target_backend);
    let shared_memory = item.shared_memory(moonc_opt.link_opt.target_backend);
    let link_flags = item.link_flags(moonc_opt.link_opt.target_backend);

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
        .args(["-target", moonc_opt.link_opt.target_backend.to_flag()])
        .args_with_cond(debug_flag && !strip_flag, vec!["-g", "-O0"])
        .arg_with_cond(debug_flag && strip_flag, "-O0")
        .arg_with_cond(!debug_flag && !strip_flag, "-g")
        // .arg_with_cond(!debug_flag && strip_flag, "")
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
        .lazy_args_with_cond(import_memory.is_some(), || {
            let im = import_memory.unwrap();
            vec![
                "-import-memory-module".to_string(),
                im.module.clone(),
                "-import-memory-name".to_string(),
                im.name.clone(),
            ]
        })
        .lazy_args_with_cond(memory_limits.is_some(), || {
            let ml = memory_limits.unwrap();
            vec![
                "-memory-limits-min".to_string(),
                ml.min.to_string(),
                "-memory-limits-max".to_string(),
                ml.max.to_string(),
            ]
        })
        .lazy_args_with_cond(shared_memory.is_some(), || {
            let sm = shared_memory.unwrap_or(false);
            let mut args = vec![];
            if sm {
                args.push("-shared-memory".to_string())
            }
            args
        })
        .lazy_args_with_cond(heap_start_address.is_some(), || {
            vec![
                "-heap-start-address".to_string(),
                heap_start_address.unwrap().to_string(),
            ]
        })
        .lazy_args_with_cond(link_flags.is_some(), || link_flags.unwrap().into())
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
    build.desc = Some(format!("link-core: {}", item.package_full_name));
    (build, artifact_id)
}

pub fn gen_compile_runtime_command(
    graph: &mut n2graph::Graph,
    target_dir: &Path,
) -> (Build, PathBuf) {
    let runtime_dot_c_path = &MOON_DIRS.moon_lib_path.join("runtime.c");

    let ins = BuildIns {
        ids: vec![graph
            .files
            .id_from_canonical(runtime_dot_c_path.display().to_string())],
        explicit: 1,
        implicit: 0,
        order_only: 0,
    };

    let artifact_output_path = target_dir.join(format!("runtime.{}", O_EXT));

    let artifact_id = graph
        .files
        .id_from_canonical(artifact_output_path.display().to_string());
    let outs = BuildOuts {
        ids: vec![artifact_id],
        explicit: 1,
    };

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("compile-runtime")),
        line: 0,
    };

    let cc_cmd = make_cc_command(
        CC::default(),
        None,
        CCConfigBuilder::default()
            .no_sys_header(true)
            .output_ty(OutputType::Object)
            .opt_level(OptLevel::Speed)
            .debug_info(true)
            // always link moonbitrun in this mode
            .link_moonbitrun(true)
            .define_use_shared_runtime_macro(false)
            .build()
            .unwrap(),
        &[],
        &[&runtime_dot_c_path.display().to_string()],
        &target_dir.display().to_string(),
        &artifact_output_path.display().to_string(),
    );

    let command = CommandBuilder::from_iter(cc_cmd).build();
    log::debug!("Command: {}", command);
    let mut build = Build::new(loc, ins, outs);
    build.cmdline = Some(command);
    build.desc = Some(format!("compile-runtime: {}", runtime_dot_c_path.display()));
    (build, artifact_output_path)
}

pub fn gen_compile_shared_runtime_command(
    graph: &mut n2graph::Graph,
    target_dir: &Path,
) -> (Build, PathBuf) {
    let runtime_dot_c_path = &MOON_DIRS
        .moon_lib_path
        .join("runtime.c")
        .display()
        .to_string();

    let artifact_output_path = target_dir.join(format!("libruntime.{}", moonutil::common::DYN_EXT));

    let ins = BuildIns {
        ids: vec![graph.files.id_from_canonical(runtime_dot_c_path.clone())],
        explicit: 1,
        implicit: 0,
        order_only: 0,
    };

    let artifact_id = graph
        .files
        .id_from_canonical(artifact_output_path.display().to_string());
    let outs = BuildOuts {
        ids: vec![artifact_id],
        explicit: 1,
    };

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("compile-shared-runtime")),
        line: 0,
    };

    let cc_cmd = make_cc_command(
        CC::default(),
        None,
        CCConfigBuilder::default()
            .no_sys_header(true)
            .output_ty(OutputType::SharedLib)
            .opt_level(OptLevel::Speed)
            .debug_info(true)
            // don't link moonbitrun in this mode as it is provided to tcc
            .link_moonbitrun(false)
            .define_use_shared_runtime_macro(false)
            .build()
            .unwrap(),
        &[],
        &[&runtime_dot_c_path],
        &target_dir.display().to_string(),
        &artifact_output_path.display().to_string(),
    );

    let command = CommandBuilder::from_iter(cc_cmd).build();

    log::debug!("Command: {}", command);
    let mut build = Build::new(loc, ins, outs);
    build.cmdline = Some(command);
    build.desc = Some(format!(
        "compile-shared-runtime: {}",
        artifact_output_path.display()
    ));
    (build, artifact_output_path)
}

pub fn gen_compile_exe_command(
    graph: &mut n2graph::Graph,
    item: &BuildLinkDepItem,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    runtime_path: String,
) -> (Build, n2graph::FileId) {
    let path = PathBuf::from(&item.out);

    let target_dir = path.parent().unwrap();

    let c_artifact_path = path.with_extension("c").display().to_string();

    let artifact_output_path =
        path.with_extension(moonc_opt.link_opt.target_backend.to_extension());

    let artifact_id = graph
        .files
        .id_from_canonical(artifact_output_path.display().to_string());

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("compile-exe")),
        line: 0,
    };

    let mut input_ids = vec![
        graph.files.id_from_canonical(c_artifact_path.clone()),
        graph.files.id_from_canonical(runtime_path.clone()),
    ];
    let mut input_cnt = input_ids.len();
    let native_stub_deps = item.native_stub_deps();
    if let Some(native_stub_deps) = native_stub_deps {
        input_cnt += native_stub_deps.len();
        input_ids.extend(
            native_stub_deps
                .iter()
                .map(|f| graph.files.id_from_canonical(f.clone())),
        );
    }

    let ins = BuildIns {
        ids: input_ids,
        explicit: input_cnt,
        implicit: 0,
        order_only: 0,
    };

    let outs = BuildOuts {
        ids: vec![artifact_id],
        explicit: 1,
    };

    let mut build = Build::new(loc, ins, outs);

    let native_cc = item.native_cc(moonc_opt.link_opt.target_backend);
    let native_cc_flags = item
        .native_cc_flags(moonc_opt.link_opt.target_backend)
        .map(|it| it.split(" ").collect::<Vec<_>>())
        .unwrap_or_default();
    let native_cc_link_flags = item
        .native_cc_link_flags(moonc_opt.link_opt.target_backend)
        .map(|it| it.split(" ").collect::<Vec<_>>())
        .unwrap_or_default();

    let mut native_flags = vec![];
    native_flags.extend(native_cc_flags);
    native_flags.extend(native_cc_link_flags);

    let cpath = &c_artifact_path;
    let rtpath = &runtime_path;
    let mut sources: Vec<&str> = vec![cpath, rtpath];

    if let Some(native_stub_deps) = native_stub_deps {
        sources.extend(native_stub_deps.iter().map(|f| f.as_str()));
    }

    let cc_cmd = make_cc_command(
        CC::default(),
        native_cc.map(|cc| {
            CC::try_from_path(cc)
                .context(format!(
                    "{}: failed to find native cc: {}",
                    "Error".red(),
                    cc
                ))
                .unwrap()
        }),
        CCConfigBuilder::default()
            .no_sys_header(true)
            .output_ty(OutputType::Executable)
            .opt_level(to_opt_level(
                !moonc_opt.build_opt.debug_flag,
                moonc_opt.build_opt.debug_flag,
            ))
            .debug_info(moonc_opt.build_opt.debug_flag)
            .link_moonbitrun(!moonbuild_opt.use_tcc_run) // if use tcc, we cannot link moonbitrun
            .define_use_shared_runtime_macro(moonbuild_opt.use_tcc_run)
            .build()
            .unwrap(),
        &native_flags,
        &sources,
        &target_dir.display().to_string(),
        &artifact_output_path.display().to_string(),
    );

    let command = CommandBuilder::from_iter(cc_cmd).build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build.desc = Some(format!("compile-exe: {}", item.package_full_name));
    (build, artifact_id)
}

pub fn gen_archive_stub_to_static_lib_command(
    graph: &mut n2graph::Graph,
    item: &LinkDepItem,
    moonc_opt: &MooncOpt,
) -> (Build, n2graph::FileId) {
    let out = PathBuf::from(&item.out);

    let inputs = item
        .stub_lib
        .as_ref()
        .unwrap()
        .iter()
        .map(|f| {
            out.parent()
                .unwrap()
                .join(f)
                .with_extension(O_EXT)
                .display()
                .to_string()
        })
        .collect::<Vec<_>>();

    let pkgname = out.file_stem().unwrap().to_str().unwrap().to_string();
    let artifact_output_path = out
        .with_file_name(format!("lib{}.{}", pkgname, A_EXT))
        .display()
        .to_string();
    let artifact_id = graph.files.id_from_canonical(artifact_output_path.clone());

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("archive-stub")),
        line: 0,
    };

    let input_ids = inputs
        .iter()
        .map(|f| graph.files.id_from_canonical(f.clone()))
        .collect::<Vec<_>>();
    let ins = BuildIns {
        ids: input_ids,
        explicit: inputs.len(),
        implicit: 0,
        order_only: 0,
    };

    let outs = BuildOuts {
        ids: vec![artifact_id],
        explicit: 1,
    };

    let mut build = Build::new(loc, ins, outs);

    let native_stub_cc = item.native_stub_cc(moonc_opt.link_opt.target_backend);

    let cc_cmd = make_archiver_command(
        CC::default(),
        native_stub_cc.map(|cc| {
            CC::try_from_path(cc)
                .context(format!(
                    "{}: failed to find native cc: {}",
                    "Error".red(),
                    cc
                ))
                .unwrap()
        }),
        ArchiverConfigBuilder::default()
            .archive_moonbitrun(false)
            .build()
            .unwrap(),
        &inputs.iter().map(|f| f.as_str()).collect::<Vec<_>>(),
        &artifact_output_path,
    );

    let command = CommandBuilder::from_iter(cc_cmd).build();
    build.cmdline = Some(command);
    build.desc = Some(format!("archive-stub: {}", artifact_output_path));

    (build, artifact_id)
}

pub fn gen_link_stub_to_dynamic_lib_command(
    graph: &mut n2graph::Graph,
    item: &LinkDepItem,
    runtime_path: &Path,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> (Build, n2graph::FileId) {
    let out = PathBuf::from(&item.out);

    let mut inputs = item
        .stub_lib
        .as_ref()
        .unwrap()
        .iter()
        .map(|f| {
            out.parent()
                .unwrap()
                .join(f)
                .with_extension(O_EXT)
                .display()
                .to_string()
        })
        .collect::<Vec<_>>();
    inputs.push(runtime_path.display().to_string());

    let pkgname = out.file_stem().unwrap().to_str().unwrap().to_string();
    let target_dir = out.parent().unwrap();
    let artifact_output_path = out
        .with_file_name(format!("lib{}.{}", pkgname, DYN_EXT))
        .display()
        .to_string();
    let artifact_id = graph.files.id_from_canonical(artifact_output_path.clone());

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("link-stub")),
        line: 0,
    };

    let input_ids = inputs
        .iter()
        .map(|f| graph.files.id_from_canonical(f.clone()))
        .collect::<Vec<_>>();
    let ins = BuildIns {
        ids: input_ids,
        explicit: inputs.len(),
        implicit: 0,
        order_only: 0,
    };

    // IMPORTANT: pop the last input, it's libruntime{.dylib, .so}
    inputs.pop();

    let outs = BuildOuts {
        ids: vec![artifact_id],
        explicit: 1,
    };

    let mut build = Build::new(loc, ins, outs);

    let native_stub_cc = item.native_stub_cc(moonc_opt.link_opt.target_backend);
    let native_stub_cc_link_flags = item
        .native_stub_cc_link_flags(moonc_opt.link_opt.target_backend)
        .map(|it| it.split(" ").collect::<Vec<_>>())
        .unwrap_or_default();

    let native_cc_flags = item
        .native_cc_flags(moonc_opt.link_opt.target_backend)
        .map(|it| it.split(" ").collect::<Vec<_>>())
        .unwrap_or_default();
    let native_cc_link_flags = item
        .native_cc_link_flags(moonc_opt.link_opt.target_backend)
        .map(|it| it.split(" ").collect::<Vec<_>>())
        .unwrap_or_default();

    // TODO: There's too many kinds of flags, need to document what each one do
    let cc_flags = native_stub_cc_link_flags
        .into_iter()
        .chain(native_cc_flags.into_iter())
        .chain(native_cc_link_flags.into_iter())
        .collect::<Vec<_>>();

    let shared_runtime_dir = Some(runtime_path.parent().unwrap());
    let cc_cmd = make_linker_command::<_, &Path>(
        CC::default(),
        native_stub_cc.map(|cc| {
            CC::try_from_path(cc)
                .context(format!(
                    "{}: failed to find native cc: {}",
                    "Error".red(),
                    cc
                ))
                .unwrap()
        }),
        LinkerConfigBuilder::default()
            .link_moonbitrun(!moonbuild_opt.use_tcc_run)
            .link_shared_runtime(shared_runtime_dir)
            .output_ty(OutputType::SharedLib)
            .build()
            .unwrap(),
        &cc_flags,
        &inputs.iter().map(|f| f.as_str()).collect::<Vec<_>>(),
        &target_dir.display().to_string(),
        &artifact_output_path,
    );

    let command = CommandBuilder::from_iter(cc_cmd).build();
    build.cmdline = Some(command);
    build.desc = Some(format!("link-stub: {}", artifact_output_path));

    (build, artifact_id)
}

pub fn gen_compile_stub_command(
    graph: &mut n2graph::Graph,
    item: &LinkDepItem,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> Vec<(Build, n2graph::FileId)> {
    let inputs = item
        .stub_lib
        .as_ref()
        .unwrap()
        .iter()
        .map(|f| item.package_path.join(f));

    let mut res = vec![];

    for input in inputs {
        let artifact_output_path = PathBuf::from(&item.out)
            .with_file_name(input.with_extension(O_EXT).file_name().unwrap())
            .display()
            .to_string();
        let artifact_id = graph.files.id_from_canonical(artifact_output_path.clone());

        let loc = FileLoc {
            filename: Rc::new(PathBuf::from("compile-stub")),
            line: 0,
        };

        let input_ids = vec![graph.files.id_from_canonical(input.display().to_string())];
        let ins = BuildIns {
            ids: input_ids,
            explicit: 1,
            implicit: 0,
            order_only: 0,
        };

        let outs = BuildOuts {
            ids: vec![artifact_id],
            explicit: 1,
        };

        let mut build = Build::new(loc, ins, outs);

        let native_stub_cc = item.native_stub_cc(moonc_opt.link_opt.target_backend);
        let native_stub_cc_flags = item
            .native_stub_cc_flags(moonc_opt.link_opt.target_backend)
            .map(|it| it.split(" ").collect::<Vec<_>>())
            .unwrap_or_default();

        let cpath = &input.display().to_string();
        let sources: Vec<&str> = vec![cpath];

        let cc_cmd = make_cc_command(
            CC::default(),
            native_stub_cc.map(|cc| {
                CC::try_from_path(cc)
                    .context(format!(
                        "{}: failed to find native cc: {}",
                        "Error".red(),
                        cc
                    ))
                    .unwrap()
            }),
            CCConfigBuilder::default()
                .no_sys_header(true)
                .output_ty(OutputType::Object)
                .opt_level(to_opt_level(
                    !moonc_opt.build_opt.debug_flag,
                    moonc_opt.build_opt.debug_flag,
                ))
                .debug_info(moonc_opt.build_opt.debug_flag)
                .link_moonbitrun(!moonbuild_opt.use_tcc_run) // if use tcc, we cannot link moonbitrun
                .define_use_shared_runtime_macro(moonbuild_opt.use_tcc_run)
                .build()
                .unwrap(),
            &native_stub_cc_flags,
            &sources,
            &MOON_DIRS.moon_lib_path.display().to_string(),
            &artifact_output_path,
        );
        let command = CommandBuilder::from_iter(cc_cmd).build();
        log::debug!("Command: {}", command);
        build.cmdline = Some(command);
        build.desc = Some(format!("compile-stub: {}", input.display()));
        res.push((build, artifact_id));
    }

    res
}

// for llvm target
pub fn gen_link_exe_command(
    graph: &mut n2graph::Graph,
    item: &BuildLinkDepItem,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    runtime_path: String,
) -> (Build, n2graph::FileId) {
    let o_artifact_path = PathBuf::from(&item.out)
        .with_extension(O_EXT)
        .display()
        .to_string();

    let artifact_output_path = PathBuf::from(&item.out)
        .with_extension(moonc_opt.link_opt.target_backend.to_extension())
        .display()
        .to_string();

    let artifact_id = graph.files.id_from_canonical(artifact_output_path.clone());

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("link-exe")),
        line: 0,
    };

    let input_ids = vec![
        graph.files.id_from_canonical(o_artifact_path.clone()),
        graph.files.id_from_canonical(runtime_path.clone()),
    ];
    let ins = BuildIns {
        ids: input_ids,
        explicit: 2,
        implicit: 0,
        order_only: 0,
    };

    let outs = BuildOuts {
        ids: vec![artifact_id],
        explicit: 1,
    };

    let mut build = Build::new(loc, ins, outs);

    let native_cc = item.native_cc(moonc_opt.link_opt.target_backend);
    let native_cc_flags = item
        .native_cc_flags(moonc_opt.link_opt.target_backend)
        .map(|it| it.split(" ").collect::<Vec<_>>())
        .unwrap_or_default();
    let native_cc_link_flags = item
        .native_cc_link_flags(moonc_opt.link_opt.target_backend)
        .map(|it| it.split(" ").collect::<Vec<_>>())
        .unwrap_or_default();

    let mut native_flags = vec![];
    native_flags.extend(native_cc_flags);
    native_flags.extend(native_cc_link_flags);

    let sources: Vec<&str> = vec![&runtime_path, &o_artifact_path];

    let cc_cmd = make_cc_command(
        CC::default(),
        native_cc.map(|cc| {
            CC::try_from_path(cc)
                .context(format!(
                    "{}: failed to find native cc: {}",
                    "Error".red(),
                    cc
                ))
                .unwrap()
        }),
        CCConfigBuilder::default()
            .no_sys_header(true)
            .output_ty(OutputType::Executable)
            .opt_level(to_opt_level(
                !moonc_opt.build_opt.debug_flag,
                moonc_opt.build_opt.debug_flag,
            ))
            .debug_info(moonc_opt.build_opt.debug_flag)
            .link_moonbitrun(!moonbuild_opt.use_tcc_run) // if use tcc, we cannot link moonbitrun
            .define_use_shared_runtime_macro(moonbuild_opt.use_tcc_run)
            .build()
            .unwrap(),
        &native_flags,
        &sources,
        &PathBuf::from(&item.out)
            .parent()
            .unwrap()
            .display()
            .to_string(),
        &artifact_output_path,
    );

    let command = CommandBuilder::from_iter(cc_cmd).build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build.desc = Some(format!("link-exe: {}", item.package_full_name));
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
        let (build, fid) = gen_build_command(&mut graph, item, moonc_opt, false);
        graph.add_build(build)?;
        default.push(fid);
    }

    let is_native_backend = moonc_opt.link_opt.target_backend == TargetBackend::Native;

    let is_llvm_backend = moonc_opt.link_opt.target_backend == TargetBackend::LLVM;

    // compile runtime.o or libruntime.so
    let mut runtime_path = None;

    if is_native_backend || is_llvm_backend {
        fn gen_shared_runtime(
            graph: &mut n2graph::Graph,
            target_dir: &Path,
            default: &mut Vec<n2graph::FileId>,
        ) -> anyhow::Result<PathBuf> {
            let (build, path) = gen_compile_shared_runtime_command(graph, target_dir);
            graph.add_build(build)?;
            // we explicitly add it to default because shared runtime is not a target or depended by any target
            default.push(graph.files.id_from_canonical(path.display().to_string()));
            Ok(path)
        }

        fn gen_runtime(graph: &mut n2graph::Graph, target_dir: &Path) -> anyhow::Result<PathBuf> {
            let (build, path) = gen_compile_runtime_command(graph, target_dir);
            graph.add_build(build)?;
            Ok(path)
        }

        runtime_path = Some(if moonbuild_opt.use_tcc_run {
            gen_shared_runtime(&mut graph, target_dir, &mut default)?
        } else {
            gen_runtime(&mut graph, target_dir)?
        });
    }

    let mut has_link_item = false;
    for item in input.link_items.iter() {
        if !has_link_item {
            has_link_item = true;
            default.clear();
        }
        let (build, fid) = gen_link_command(&mut graph, item, moonc_opt);
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
            graph.add_build(build)?;
            default_fid = fid;
        } else if is_llvm_backend {
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

        // if we need to install the artifact to a specific path
        if let Some(install_path) = item.install_path.as_ref() {
            let bin_script_content = if cfg!(target_os = "windows") {
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../moonbuild/template/moon_bin_script_template/windows.ps1"
                ))
            } else {
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../moonbuild/template/moon_bin_script_template/unix.sh"
                ))
            };

            let bin_script_name = item.bin_name.clone().unwrap_or(
                PathBuf::from(&item.out)
                    .file_stem()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
            );
            #[cfg(target_os = "windows")]
            let bin_script_name = PathBuf::from(&bin_script_name)
                .with_extension("ps1")
                .display()
                .to_string();

            let bin_script_path = install_path.join(bin_script_name);
            if bin_script_path.exists() && bin_script_path.is_dir() {
                anyhow::bail!(
                    "bin install failed, there is a directory {:?} already exists",
                    bin_script_path
                );
            }

            if !bin_script_path.exists() {
                let artifact_output_path = PathBuf::from(&item.out)
                    .with_extension(moonc_opt.link_opt.output_format.to_str())
                    .display()
                    .to_string();

                let runtime = match moonc_opt.link_opt.target_backend {
                    TargetBackend::Native | TargetBackend::LLVM => "".to_string(),
                    TargetBackend::Wasm | TargetBackend::WasmGC => "moonrun".to_string(),
                    TargetBackend::Js => "node".to_string(),
                };

                let bin_script_content = bin_script_content
                    .replace("$runtime", &runtime)
                    .replace("$artifact_output_path", &artifact_output_path);

                std::fs::write(&bin_script_path, bin_script_content).with_context(|| {
                    format!("Failed to write bin script to {:?}", bin_script_path)
                })?;
                #[cfg(unix)]
                {
                    std::fs::set_permissions(
                        &bin_script_path,
                        std::os::unix::fs::PermissionsExt::from_mode(0o755),
                    )
                    .with_context(|| {
                        format!("Failed to set permissions for {:?}", bin_script_path)
                    })?;
                }
            }
        }
    }

    for item in input.build_interface_items.iter() {
        let (build, _) = gen_build_interface_command(&mut graph, item, moonc_opt);
        graph.add_build(build)?;
    }

    for item in input.build_default_virtual_items.iter() {
        // here we don't put the fid to default, if nobody depends on the default virtual pkg impl, it will not be built
        let (build, _) = gen_build_command(&mut graph, item, moonc_opt, true);
        graph.add_build(build)?;
    }

    if is_native_backend {
        for item in input.compile_stub_items.iter() {
            let builds = gen_compile_stub_command(&mut graph, item, moonc_opt, moonbuild_opt);
            for (build, fid) in builds {
                graph.add_build(build)?;
                // add the fid to default, since we want stub.c to be compiled for a non-main package
                default.push(fid);
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

    let mut hashes = n2graph::Hashes::default();
    let n2_db_path = &target_dir.join("build.moon_db");
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
