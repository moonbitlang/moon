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
use moonutil::module::ModuleDB;
use moonutil::package::{JsFormat, LinkDepItem, Package};

use super::cmd_builder::CommandBuilder;
use super::n2_errors::{N2Error, N2ErrorKind};
use crate::gen::MiAlias;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use moonutil::common::{
    BuildOpt, MoonbuildOpt, MooncOpt, TargetBackend, MOONBITLANG_CORE, MOON_PKG_JSON, O_EXT,
};
use n2::graph::{self as n2graph, Build, BuildIns, BuildOuts, FileLoc};
use n2::load::State;
use n2::smallmap::SmallMap;

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
}

type BuildLinkDepItem = moonutil::package::LinkDepItem;

#[derive(Debug)]
pub struct N2BuildInput {
    pub build_items: Vec<BuildDepItem>,
    pub link_items: Vec<BuildLinkDepItem>, // entry points
    pub compile_stub_items: Vec<BuildLinkDepItem>,
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
    let package_source_dir: String = pkg.root_path.to_string_lossy().into_owned();

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
    })
}

pub fn gen_build_link_item(
    m: &ModuleDB,
    pkg: &Package,
    _moonc_opt: &MooncOpt,
) -> anyhow::Result<BuildLinkDepItem> {
    let out = pkg.artifact.with_extension("wat"); // TODO: extension is determined by build option
    let package_full_name = pkg.full_name();

    let tp = super::util::topo_from_node(m, pkg)?;
    let core_deps = super::util::nodes_to_cores(m, &tp);
    let package_sources = super::util::nodes_to_pkg_sources(m, &tp);

    Ok(BuildLinkDepItem {
        out: out.display().to_string(),
        core_deps,
        package_sources,
        package_full_name,
        package_path: pkg.root_path.clone(),
        link: pkg.link.clone(),
        install_path: pkg.install_path.clone(),
        bin_name: pkg.bin_name.clone(),
        native_stub: pkg.native_stub.clone(),
    })
}

pub fn gen_build(
    m: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<N2BuildInput> {
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

        build_items.push(gen_build_build_item(m, pkg, moonc_opt)?);

        if pkg.native_stub.is_some() {
            compile_stub_items.push(BuildLinkDepItem {
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

        if (is_main || pkg.need_link) && !pkg.is_third_party {
            // link need add *.core files recursively
            link_items.push(gen_build_link_item(m, pkg, moonc_opt)?);
        }
    }
    Ok(N2BuildInput {
        build_items,
        link_items,
        compile_stub_items,
    })
}

pub fn gen_build_command(
    graph: &mut n2graph::Graph,
    item: &BuildDepItem,
    moonc_opt: &MooncOpt,
) -> (Build, n2graph::FileId) {
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
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build.desc = Some(format!("link-core: {}", item.package_full_name));
    (build, artifact_id)
}

pub fn gen_compile_exe_command(
    graph: &mut n2graph::Graph,
    item: &BuildLinkDepItem,
    moonc_opt: &MooncOpt,
) -> (Build, n2graph::FileId) {
    let c_artifact_path = PathBuf::from(&item.out)
        .with_extension("c")
        .display()
        .to_string();

    let artifact_output_path = PathBuf::from(&item.out)
        .with_extension(moonc_opt.link_opt.target_backend.to_extension())
        .display()
        .to_string();

    let artifact_id = graph.files.id_from_canonical(artifact_output_path.clone());

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("compile-exe")),
        line: 0,
    };

    let mut input_ids = vec![graph.files.id_from_canonical(c_artifact_path.clone())];
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

    let native_cc = item.native_cc(moonc_opt.link_opt.target_backend).unwrap();
    let native_cc_flags = item
        .native_cc_flags(moonc_opt.link_opt.target_backend)
        .map(|it| it.split(" ").collect::<Vec<_>>())
        .unwrap_or_default();
    let native_cc_link_flags = item
        .native_cc_link_flags(moonc_opt.link_opt.target_backend)
        .map(|it| it.split(" ").collect::<Vec<_>>())
        .unwrap_or_default();

    let command = CommandBuilder::new(native_cc)
        .arg(&c_artifact_path)
        .args_with_cond(!native_cc_flags.is_empty(), native_cc_flags)
        .args_with_cond(!native_cc_link_flags.is_empty(), native_cc_link_flags)
        .lazy_args_with_cond(native_stub_deps.is_some(), || {
            native_stub_deps.unwrap().into()
        })
        .args(vec!["-o", &artifact_output_path])
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build.desc = Some(format!("compile-exe: {}", item.package_full_name));
    (build, artifact_id)
}

pub fn gen_compile_stub_command(
    graph: &mut n2graph::Graph,
    item: &LinkDepItem,
    moonc_opt: &MooncOpt,
) -> (Build, n2graph::FileId) {
    let artifact_output_path = PathBuf::from(&item.out).display().to_string();

    let artifact_id = graph.files.id_from_canonical(artifact_output_path.clone());

    let loc = FileLoc {
        filename: Rc::new(PathBuf::from("compile-stub")),
        line: 0,
    };

    let inputs = item
        .native_stub
        .as_ref()
        .unwrap()
        .iter()
        .map(|f| item.package_path.join(f).display().to_string())
        .collect::<Vec<_>>();
    let input_cnt = inputs.len();
    let input_ids = inputs
        .iter()
        .map(|f| graph.files.id_from_canonical(f.clone()))
        .collect::<Vec<_>>();

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

    let native_cc = item.native_cc(moonc_opt.link_opt.target_backend).unwrap();
    let native_cc_flags = item
        .native_cc_flags(moonc_opt.link_opt.target_backend)
        .map(|it| it.split(" ").collect::<Vec<_>>())
        .unwrap_or_default();
    let native_cc_link_flags = item
        .native_cc_link_flags(moonc_opt.link_opt.target_backend)
        .map(|it| it.split(" ").collect::<Vec<_>>())
        .unwrap_or_default();

    let windows_with_cl = cfg!(windows) && native_cc == "cl";

    let command = CommandBuilder::new(native_cc)
        .arg("-c")
        .args(inputs)
        .args_with_cond(!native_cc_flags.is_empty(), native_cc_flags)
        .args_with_cond(!native_cc_link_flags.is_empty(), native_cc_link_flags)
        .args_with_cond(!windows_with_cl, vec!["-o", &artifact_output_path])
        .arg_with_cond(windows_with_cl, &format!("-Fo{}", artifact_output_path))
        .build();
    log::debug!("Command: {}", command);
    build.cmdline = Some(command);
    build.desc = Some(format!("compile-stub: {}", item.package_full_name));
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
        let (build, fid) = gen_build_command(&mut graph, item, moonc_opt);
        graph.add_build(build)?;
        default.push(fid);
    }

    let is_native_backend = moonc_opt.link_opt.target_backend == TargetBackend::Native;

    let mut has_link_item = false;
    for item in input.link_items.iter() {
        if !has_link_item {
            has_link_item = true;
            default.clear();
        }
        let (build, fid) = gen_link_command(&mut graph, item, moonc_opt);
        let mut default_fid = fid;
        graph.add_build(build)?;

        if is_native_backend {
            let (build, fid) = gen_compile_exe_command(&mut graph, item, moonc_opt);
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
                    TargetBackend::Native => "".to_string(),
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

    if is_native_backend {
        for item in input.compile_stub_items.iter() {
            let (build, fid) = gen_compile_stub_command(&mut graph, item, moonc_opt);
            graph.add_build(build)?;
            default.push(fid);
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
