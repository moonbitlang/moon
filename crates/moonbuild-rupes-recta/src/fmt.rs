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

//! The formatter's pipeline
//!
//! The formatter only needs a bare minimum project to run, so its pipeline
//! bypasses thew regular compilation pipeline of resolving and discovering
//! modules and packages.
//!
//! This pipeline still strives to use as much of the existing infrastructure
//! as possible.
//!
//! # Maintainers
//!
//! If a similar no-resolving, files-only command is needed, refactor this
//! module into a more generic one, probably named "source utility" or similar.

use log::*;
use std::path::Path;

use moonutil::mooncakes::{ModuleId, ModuleSource, result::ResolvedEnv};
use n2::graph::Build;

use crate::{
    build_lower::{
        artifact::{LegacyLayout, LegacyLayoutBuilder},
        build_ins, build_n2_fileloc, build_outs,
    },
    discover::{DiscoverResult, DiscoveredPackage, discover_packages_for_mod},
    resolve::ResolveError,
};

pub struct FmtResolveOutput {
    pub module_rel: ResolvedEnv,
    pub pkg_dirs: DiscoverResult,
    pub main_module_id: ModuleId,
}

/// Perform a barebones, faked resolving process for `moon fmt`
pub fn resolve_for_fmt(source_dir: &Path) -> Result<FmtResolveOutput, ResolveError> {
    info!(
        "Resolving formatter environment for {}",
        source_dir.display()
    );

    // Generate a barebones ResolvedEnv with just the local module
    // This is not the normal resolving process, but we don't care here ;)
    #[allow(clippy::disallowed_methods)] // we are not using the `resolve` module
    let m = moonutil::common::read_module_desc_file_in_dir(source_dir)
        .map_err(ResolveError::SyncModulesError)?;
    let ms = ModuleSource::from_local_module(&m, source_dir).ok_or_else(|| {
        ResolveError::SyncModulesError(anyhow::anyhow!("Malformed module manifest"))
    })?;
    let (modules, id) = ResolvedEnv::only_one_module(ms, m);
    let ms = modules.mod_name_from_id(id);
    debug!("Resolved main module id = {:?}, name = {}", id, ms);

    // Find packages
    let mut discover_res = DiscoverResult::default();
    discover_packages_for_mod(&mut discover_res, &modules, source_dir, id, ms)?;
    info!("Package discovery completed for module {}", ms);

    Ok(FmtResolveOutput {
        module_rel: modules,
        pkg_dirs: discover_res,
        main_module_id: id,
    })
}

pub struct FmtConfig {
    /// Enable `///|` block-lines in formatting
    pub block_style: bool,

    /// Checks the formatting without writing to files
    pub check_only: bool,

    /// Extra arguments to pass to the formatter
    pub extra_args: Vec<String>,
}

/// Generate the necessary build graph for the formatter operation
///
/// TODO: This function is verbose. Refactor and check [`crate::build_lower`]
/// before proceeding.
pub fn build_graph_for_fmt(
    resolved: &FmtResolveOutput,
    cfg: &FmtConfig,
    target_dir: &Path,
) -> anyhow::Result<n2::graph::Graph> {
    let ms = resolved
        .module_rel
        .mod_name_from_id(resolved.main_module_id);
    info!("Building format graph for module {}", ms);

    let layout = LegacyLayoutBuilder::default()
        .target_base_dir(target_dir.into())
        .main_module(Some(ms.clone()))
        .stdlib_dir(None)
        .opt_level(moonutil::cond_expr::OptLevel::Release) // we don't care
        .run_mode(moonutil::common::RunMode::Format) // this too
        .build()
        .expect("Should be valid layout");

    debug!("Layout built for formatting (module={})", ms);

    let mut graph = n2::graph::Graph::default();

    for &id in resolved
        .pkg_dirs
        .packages_for_module(resolved.main_module_id)
        .expect("We only have one module, this should succeed")
        .values()
    {
        let pkg = resolved.pkg_dirs.get_package(id);
        info!("Processing package {}", pkg.fqn);
        build_for_package(&mut graph, cfg, &layout, pkg)?;
    }

    Ok(graph)
}

fn build_for_package(
    graph: &mut n2::graph::Graph,
    cfg: &FmtConfig,
    layout: &LegacyLayout,
    pkg: &DiscoveredPackage,
) -> anyhow::Result<()> {
    for file in &pkg.source_files {
        format_node(graph, cfg, layout, pkg, file)?;
    }
    for file in &pkg.mbt_md_files {
        format_node(graph, cfg, layout, pkg, file)?;
    }
    Ok(())
}

fn format_node(
    graph: &mut n2::graph::Graph,
    cfg: &FmtConfig,
    layout: &LegacyLayout,
    pkg: &DiscoveredPackage,
    file: &Path,
) -> anyhow::Result<()> {
    let out_file = layout
        .format_artifact_path(&pkg.fqn, file.file_name().expect("Should have filename"))
        .to_string_lossy()
        .into_owned();
    let cmd: Vec<String> = if cfg.check_only {
        let mut cmd = vec![
            "moon".into(),
            "tool".into(),
            "format-and-diff".into(),
            "--old".into(),
            file.to_string_lossy().into_owned(),
            "--new".into(),
            out_file.clone(),
        ];
        if cfg.block_style {
            cmd.push("-block-style".into());
        }
        cmd.extend_from_slice(&cfg.extra_args);
        cmd
    } else {
        let mut cmd = vec![
            "moonfmt".into(),
            file.to_string_lossy().into_owned(),
            "-w".into(),
            "-o".into(),
            out_file.clone(),
        ];
        cmd.extend_from_slice(&cfg.extra_args);
        if cfg.block_style {
            cmd.push("--block-style".into());
        }
        cmd
    };

    let ins = build_ins(graph, [file]);
    let outs = build_outs(graph, [&out_file]);
    let mut build = Build::new(
        build_n2_fileloc(format!("format {}", file.display())),
        ins,
        outs,
    );
    build.cmdline = Some(shlex::try_join(cmd.iter().map(|x| x.as_str())).expect("no null"));
    graph.add_build(build)?;
    Ok(())
}
