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

//! Legacy metadata JSON (`package.json`) conversion for IDE & tools usage.

use std::path::Path;

use indexmap::IndexMap;
use moonutil::{
    common::TargetBackend,
    cond_expr::{CompileCondition, OptLevel},
    module::ModuleDBJSON,
    moon_dir::core,
    package::{AliasJSON, PackageJSON},
};

use crate::{
    ResolveOutput,
    build_lower::artifact::{LegacyLayout, LegacyLayoutBuilder},
    cond_comp::file_metadatas,
    model::{BuildTarget, PackageId, TargetKind},
    pkg_solve::DepEdge,
};

/// Generate `package.json`, which is a metadata file shared by IDE plugins and
/// other tools.
pub fn gen_metadata_json(
    ctx: &ResolveOutput,
    source_dir: &Path,
    target_dir: &Path,
    opt_level: OptLevel,
    backend: TargetBackend,
) -> ModuleDBJSON {
    // Get the main module info
    let &[main_module_id] = ctx.local_modules() else {
        panic!("Currently only one local module is supported");
    };
    let main_module = ctx.module_rel.mod_name_from_id(main_module_id);
    let main_module_json = ctx.module_rel.module_info(main_module_id);

    let layout = LegacyLayoutBuilder::default()
        .main_module(Some(main_module.clone()))
        .opt_level(opt_level)
        .stdlib_dir(Some(core()))
        .run_mode(moonutil::common::RunMode::Check)
        .target_base_dir(target_dir.to_owned())
        .build()
        .expect("Failed to build legacy layout");

    let packages = ctx
        .pkg_dirs
        .all_packages()
        .map(|(id, _)| gen_package_json(ctx, &layout, id, backend))
        .collect();

    ModuleDBJSON {
        source_dir: source_dir.to_string_lossy().into_owned(),
        packages,
        name: main_module.name().to_string(),
        deps: main_module_json.deps.keys().cloned().collect(),
        backend: backend.to_string(),
        opt_level: format!("{:?}", opt_level).to_lowercase(),
        source: main_module_json.source.clone(),
    }
}

fn gen_package_json(
    ctx: &ResolveOutput,
    layout: &LegacyLayout,
    pkg_id: PackageId,
    backend: TargetBackend,
) -> PackageJSON {
    let pkg = ctx.pkg_dirs.get_package(pkg_id);
    let is_in_workspace = ctx.local_modules().contains(&pkg.module);

    // Source file collection
    let mut files = IndexMap::new();
    let mut wbtest_files = IndexMap::new();
    let mut test_files = IndexMap::new();
    for (path, test_kind, cond) in
        file_metadatas(&pkg.raw, pkg.source_files.iter().map(|x| x.as_path()))
    {
        match test_kind {
            crate::cond_comp::FileTestKind::NoTest => files.insert(path.to_owned(), cond),
            crate::cond_comp::FileTestKind::Whitebox => wbtest_files.insert(path.to_owned(), cond),
            crate::cond_comp::FileTestKind::Blackbox => test_files.insert(path.to_owned(), cond),
        };
    }
    let mbt_md_files = pkg
        .mbt_md_files
        .iter()
        .cloned()
        .map(|x| {
            (
                x,
                CompileCondition {
                    backend: TargetBackend::all().to_vec(),
                    optlevel: OptLevel::all().to_vec(),
                },
            )
        })
        .collect();

    // Dependencies collection
    let mut deps: Vec<AliasJSON> = ctx
        .pkg_rel
        .dep_graph
        .edges(pkg_id.build_target(TargetKind::Source))
        .filter(|(_, _, edge)| edge.kind == TargetKind::Source)
        .map(edge_to_alias_json(ctx))
        .collect();
    deps.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.alias.cmp(&b.alias)));

    let mut wbtest_deps: Vec<AliasJSON> = ctx
        .pkg_rel
        .dep_graph
        .edges(pkg_id.build_target(TargetKind::WhiteboxTest))
        .filter(|(_, _, edge)| edge.kind == TargetKind::WhiteboxTest)
        .map(edge_to_alias_json(ctx))
        .collect();
    wbtest_deps.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.alias.cmp(&b.alias)));

    let mut test_deps: Vec<AliasJSON> = ctx
        .pkg_rel
        .dep_graph
        .edges(pkg_id.build_target(TargetKind::BlackboxTest))
        .filter(|(_, _, edge)| edge.kind == TargetKind::BlackboxTest)
        .map(edge_to_alias_json(ctx))
        .collect();
    test_deps.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.alias.cmp(&b.alias)));

    PackageJSON {
        is_main: pkg.raw.is_main,
        is_third_party: !is_in_workspace,
        root_path: pkg.root_path.to_string_lossy().into_owned(),
        root: pkg.fqn.module().name().to_string(),
        rel: pkg.fqn.package().to_string(),
        files,
        wbtest_files,
        test_files,
        mbt_md_files,
        deps,
        wbtest_deps,
        test_deps,
        artifact: layout
            .mi_of_build_target(
                &ctx.pkg_dirs,
                &pkg_id.build_target(TargetKind::Source),
                backend,
            )
            .to_string_lossy()
            .into_owned(),
    }
}

fn edge_to_alias_json(
    ctx: &ResolveOutput,
) -> impl FnMut((BuildTarget, BuildTarget, &DepEdge)) -> AliasJSON + '_ {
    |(_this, dep, edge)| AliasJSON {
        path: ctx.pkg_dirs.get_package(dep.package).fqn.to_string(),
        alias: edge.short_alias.to_string(),
        fspath: ctx
            .pkg_dirs
            .get_package(dep.package)
            .root_path
            .to_string_lossy()
            .into_owned(),
    }
}
