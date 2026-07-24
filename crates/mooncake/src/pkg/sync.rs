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

//! Sync dependencies with mod/pkg definition

use std::{path::Path, sync::Arc};

use anyhow::Context;
use indexmap::IndexMap;
use moonutil::{
    build_options::{MoonbuildOpt, MooncOpt},
    cli_support::AutoSyncFlags,
    front_matter::MbtMdHeader,
    manifest::{MoonMod, read_module_desc_file_in_dir},
    project::{
        DependencySource, MoonWork, PackageDirs, ProjectManifest, WorkspaceEnv,
        canonical_workspace_module_dirs, read_workspace_file,
    },
    resolution::{DirSyncResult, ModuleSource, ResolvedEnv, ResolvedModule, ResolvedRootModules},
};
use semver::Version;

use crate::prepared_source::PreparedDependencySources;

#[derive(Debug, Clone, Copy)]
pub struct SyncOutputOptions {
    quiet: bool,
    verbose: bool,
}

impl SyncOutputOptions {
    pub fn new(quiet: bool, verbose: bool) -> Self {
        Self { quiet, verbose }
    }

    pub fn quiet(self) -> bool {
        self.quiet
    }

    pub fn verbose(self) -> bool {
        self.verbose
    }

    pub fn with_quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }
}

impl Default for SyncOutputOptions {
    fn default() -> Self {
        Self::new(false, true)
    }
}

/// Given the specified source directory, resolve the module dependency relation
/// and their directories
///
/// TODO: support registry config
pub fn auto_sync(
    dirs: &PackageDirs,
    cli: &AutoSyncFlags,
    output_options: SyncOutputOptions,
    no_std: bool,
    workspace_env: WorkspaceEnv,
    include_bin_deps: bool,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult, Option<MoonWork>)> {
    if let ProjectManifest::Workspace(project_manifest) = &dirs.project_manifest
        && !matches!(workspace_env, WorkspaceEnv::Off)
    {
        let workspace_root = project_manifest
            .parent()
            .context("workspace manifest path has no parent directory")?;
        return resolve_workspace_sync(
            dirs,
            cli,
            output_options,
            no_std,
            workspace_root,
            read_workspace_file(project_manifest)?,
            include_bin_deps,
        );
    }

    let mut module = read_module_desc_file_in_dir(&dirs.source_dir)?;
    if !include_bin_deps {
        module.bin_deps = None;
    }
    let module = Arc::new(module);
    let source = ModuleSource::from_local_module(&module, &dirs.source_dir);
    let (roots, _) = ResolvedModule::only_one_module(source, module);

    let (resolved_env, prepared_sources) =
        super::install::install_impl(dirs, roots, output_options, false, cli.dont_sync(), no_std)?;
    let sync_result = prepared_sources.into_module_dirs();
    log::debug!("Dir sync result: {:?}", sync_result);
    Ok((resolved_env, sync_result, None))
}

fn resolve_workspace_sync(
    dirs: &PackageDirs,
    cli: &AutoSyncFlags,
    output_options: SyncOutputOptions,
    no_std: bool,
    workspace_root: &Path,
    workspace: MoonWork,
    include_bin_deps: bool,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult, Option<MoonWork>)> {
    let mut roots = ResolvedRootModules::with_key();
    for member_dir in canonical_workspace_module_dirs(workspace_root, &workspace)? {
        let mut module = read_module_desc_file_in_dir(&member_dir)?;
        if !include_bin_deps {
            module.bin_deps = None;
        }
        let module = Arc::new(module);
        let source = ModuleSource::from_local_module(&module, &member_dir);
        roots.insert(ResolvedModule::new(source, module));
    }

    let (resolved_env, prepared_sources) =
        super::install::install_impl(dirs, roots, output_options, false, cli.dont_sync(), no_std)?;
    let sync_result = prepared_sources.into_module_dirs();
    log::debug!("Dir sync result: {:?}", sync_result);
    Ok((resolved_env, sync_result, Some(workspace)))
}

pub fn auto_sync_for_single_mbt_md(
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
    mooncake_bin_dir: &Path,
    mooncakes_dir: &Path,
    front_matter_config: Option<MbtMdHeader>,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult, Arc<MoonMod>)> {
    let mut deps = IndexMap::new();

    // don't sync for gen-test-driver
    let dont_sync = front_matter_config.is_none();

    if let Some(deps_map) =
        front_matter_config.and_then(|config| config.moonbit.unwrap_or_default().deps)
    {
        for (k, v) in deps_map.iter() {
            deps.insert(k.clone(), v.clone());
        }
    }

    let m = Arc::new(MoonMod {
        name: moonutil::constants::SINGLE_FILE_TEST_MODULE.to_string(),
        version: Some(Version::new(0, 0, 1)),
        deps,
        warn_list: moonc_opt.build_opt.warn_list.clone(),
        ..Default::default()
    });
    let ms = ModuleSource::single_file(&m, &moonbuild_opt.source_dir);
    let (roots, _) = ResolvedModule::only_one_module(ms, Arc::clone(&m));
    let dirs = PackageDirs {
        source_dir: moonbuild_opt.source_dir.clone(),
        target_dir: moonbuild_opt.target_dir.clone(),
        mooncake_bin_dir: mooncake_bin_dir.to_path_buf(),
        mooncakes_dir: mooncakes_dir.to_path_buf(),
        dependency_source: DependencySource::ProjectLocal,
        project_manifest: ProjectManifest::None,
    };

    let (resolved_env, prepared_sources) = super::install::install_impl(
        &dirs,
        roots,
        SyncOutputOptions::new(moonbuild_opt.quiet, true),
        moonbuild_opt.verbose,
        dont_sync,
        false,
    )?;
    let dir_sync_result = prepared_sources.into_module_dirs();
    log::debug!("Dir sync result: {:?}", dir_sync_result);
    Ok((resolved_env, dir_sync_result, m))
}

pub fn auto_sync_for_single_file_rr(
    dirs: &PackageDirs,
    sync_flags: &AutoSyncFlags,
    front_matter_deps: Option<&IndexMap<String, moonutil::dependency::SourceDependencyInfo>>,
    output_options: SyncOutputOptions,
) -> anyhow::Result<(ResolvedEnv, PreparedDependencySources)> {
    let mut synth_deps = IndexMap::new();
    if let Some(deps_map) = front_matter_deps {
        for (k, v) in deps_map.iter() {
            synth_deps.insert(k.clone(), v.clone());
        }
    }

    let m = Arc::new(MoonMod {
        name: moonutil::constants::SINGLE_FILE_TEST_MODULE.to_string(),
        version: Some(Version::new(0, 0, 1)),
        deps: synth_deps,
        ..Default::default()
    });
    let ms = ModuleSource::single_file(&m, &dirs.source_dir);
    let (roots, _) = ResolvedModule::only_one_module(ms, Arc::clone(&m));

    let (resolved_env, prepared_sources) = super::install::install_impl(
        dirs,
        roots,
        output_options,
        false,
        sync_flags.dont_sync(),
        false,
    )?;

    log::debug!("Dir sync result: {:?}", prepared_sources.module_dirs());
    Ok((resolved_env, prepared_sources))
}
