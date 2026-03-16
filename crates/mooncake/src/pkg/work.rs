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

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use moonutil::{
    common::{
        MOON_MOD_JSON, MOON_WORK, MOONBITLANG_CORE, read_module_desc_file_in_dir,
        write_module_json_to_file,
    },
    dependency::SourceDependencyInfo,
    module::{MoonMod, convert_module_to_mod_json},
    mooncakes::{
        ModuleId, ModuleName, ModuleSource,
        result::{
            DependencyEdge, DependencyKind, ResolvedEnv, ResolvedModule, ResolvedRootModules,
        },
    },
    version::as_caret_version_req,
    workspace::{canonical_workspace_module_dirs, read_workspace},
};

use crate::{
    registry,
    resolver::{ResolveConfig, resolve_with_default_env_and_resolver},
};

pub fn sync_workspace(source_dir: &Path, quiet: bool) -> anyhow::Result<i32> {
    let workspace = read_workspace(source_dir)?.context(format!(
        "`moon work sync` requires `{}` at `{}`",
        MOON_WORK,
        source_dir.display()
    ))?;
    let member_dirs = canonical_workspace_module_dirs(source_dir, &workspace)?;
    let roots = workspace_roots(&member_dirs)?;
    let resolved_env = resolve_workspace(roots)?;
    let updated = sync_workspace_manifests(&resolved_env)?;

    if !quiet {
        if updated.is_empty() {
            println!("Workspace manifests are already in sync");
        } else {
            println!("Synced workspace manifests:");
            for path in updated {
                let display = path.strip_prefix(source_dir).unwrap_or(&path);
                println!("{}", display.display());
            }
        }
    }

    Ok(0)
}

fn resolve_workspace(roots: ResolvedRootModules) -> anyhow::Result<ResolvedEnv> {
    let mut includes_core = false;
    for (_, module) in roots.iter() {
        if module.module_info().name == MOONBITLANG_CORE {
            includes_core = true;
            break;
        }
    }

    if includes_core && roots.len() != 1 {
        anyhow::bail!("workspaces that include `moonbitlang/core` are not supported yet");
    }

    let resolve_config = ResolveConfig {
        registry: registry::default_registry(),
        inject_std: !includes_core,
    };
    resolve_with_default_env_and_resolver(&resolve_config, roots).map_err(Into::into)
}

fn workspace_roots(member_dirs: &[PathBuf]) -> anyhow::Result<ResolvedRootModules> {
    let mut roots = ResolvedRootModules::with_key();

    for member_dir in member_dirs {
        let module = Arc::new(read_module_desc_file_in_dir(member_dir)?);
        let source = moonutil::mooncakes::ModuleSource::from_local_module(&module, member_dir);
        roots.insert(ResolvedModule::new(source, module));
    }

    Ok(roots)
}

fn sync_workspace_manifests(resolved_env: &ResolvedEnv) -> anyhow::Result<Vec<PathBuf>> {
    let mut updated = Vec::new();

    for &id in resolved_env.input_module_ids() {
        let module_dir = local_module_dir(resolved_env.module_source(id)).context(format!(
            "workspace root `{}` is not backed by a local path",
            resolved_env.module_source(id)
        ))?;

        let mut module = Arc::unwrap_or_clone(Arc::clone(resolved_env.module_info(id)));
        if !sync_manifest_versions(resolved_env, id, &mut module)? {
            continue;
        }

        let new_json = convert_module_to_mod_json(module);
        let manifest_path = module_dir.join(MOON_MOD_JSON);
        write_module_json_to_file(&new_json, module_dir)
            .context(format!("failed to write `{}`", manifest_path.display()))?;
        updated.push(manifest_path);
    }

    updated.sort();
    Ok(updated)
}

fn local_module_dir(source: &ModuleSource) -> Option<&Path> {
    match source.source() {
        moonutil::mooncakes::ModuleSourceKind::Local(path) => Some(path.as_path()),
        _ => None,
    }
}

fn sync_manifest_versions(
    resolved_env: &ResolvedEnv,
    id: ModuleId,
    module: &mut MoonMod,
) -> anyhow::Result<bool> {
    let mut changed = false;

    for (dep_name, dep) in &mut module.deps {
        changed |=
            sync_source_dependency(resolved_env, id, dep_name, DependencyKind::Regular, dep)?;
    }

    for (dep_name, dep) in module.bin_deps.iter_mut().flat_map(|deps| deps.iter_mut()) {
        changed |= sync_source_dependency(
            resolved_env,
            id,
            dep_name,
            DependencyKind::Binary,
            &mut dep.common,
        )?;
    }

    Ok(changed)
}

fn sync_source_dependency(
    resolved_env: &ResolvedEnv,
    id: ModuleId,
    dep_name: &str,
    kind: DependencyKind,
    dep: &mut SourceDependencyInfo,
) -> anyhow::Result<bool> {
    let dep_name: ModuleName = dep_name.into();
    let dep_key = DependencyEdge {
        name: dep_name.clone(),
        kind,
    };
    let dep_id = resolved_env.dep_with_key(id, &dep_key).context(format!(
        "resolved workspace graph is missing direct dependency `{}` for `{}`",
        dep_name,
        resolved_env.module_source(id)
    ))?;
    let version = as_caret_version_req(resolved_env.module_source(dep_id).version().clone());

    if dep.version == version {
        return Ok(false);
    }

    dep.version = version;
    Ok(true)
}
