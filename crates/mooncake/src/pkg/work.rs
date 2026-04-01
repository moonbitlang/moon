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
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, bail};
use moonutil::{
    common::{
        MOON_MOD_JSON, MOON_WORK, MOON_WORK_JSON, MOONBITLANG_CORE, read_module_desc_file_in_dir,
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
    workspace::{
        MoonWork, canonical_workspace_module_dirs, read_workspace, workspace_manifest_path,
        write_workspace,
    },
};

use crate::{
    registry,
    resolver::{ResolveConfig, resolve_with_default_env_and_resolver},
};

pub fn init_workspace(
    workspace_root: &Path,
    paths: &[PathBuf],
    quiet: bool,
) -> anyhow::Result<i32> {
    if let Some(workspace_path) = workspace_manifest_path(workspace_root) {
        bail!(
            "workspace file `{}` already exists",
            workspace_path.display()
        );
    }

    let use_paths = if paths.is_empty() {
        Vec::new()
    } else {
        let mut use_paths = BTreeSet::new();
        for path in paths {
            let member_dir = resolve_workspace_member(path)?;
            use_paths.insert(workspace_use_path(workspace_root, &member_dir));
        }
        use_paths.into_iter().collect()
    };
    let workspace = MoonWork {
        use_paths,
        preferred_target: None,
    };
    write_workspace(workspace_root, &workspace).context(format!(
        "failed to write `{}`",
        workspace_root.join(MOON_WORK).display()
    ))?;

    if !quiet {
        println!("Created {}", MOON_WORK);
    }

    Ok(0)
}

pub fn use_workspace(workspace_root: &Path, paths: &[PathBuf], quiet: bool) -> anyhow::Result<i32> {
    let existing = read_workspace(workspace_root)?;
    let preferred_target = match existing.as_ref() {
        Some(workspace) => workspace.preferred_target,
        None => None,
    };

    let mut use_paths = Vec::new();
    let mut member_dirs = BTreeSet::new();
    if let Some(workspace) = existing.as_ref() {
        for use_path in &workspace.use_paths {
            let member_dir = if use_path.is_absolute() {
                use_path.to_path_buf()
            } else {
                workspace_root.join(use_path)
            };
            let member_dir = dunce::canonicalize(&member_dir).with_context(|| {
                format!(
                    "failed to resolve workspace member `{}` from `{}`",
                    use_path.display(),
                    workspace_root.display()
                )
            })?;
            if !member_dir.is_dir() {
                bail!(
                    "workspace member `{}` is not a directory",
                    use_path.display()
                );
            }
            member_dirs.insert(member_dir);
            use_paths.push(use_path.clone());
        }
    }

    let previous_len = use_paths.len();
    for path in paths {
        let member_dir = resolve_workspace_member(path)?;
        if member_dirs.insert(member_dir.clone()) {
            use_paths.push(workspace_use_path(workspace_root, &member_dir));
        }
    }

    let workspace = MoonWork {
        use_paths,
        preferred_target,
    };
    write_workspace(workspace_root, &workspace).context(format!(
        "failed to write `{}`",
        workspace_root.join(MOON_WORK).display()
    ))?;

    if !quiet {
        if existing.is_none() {
            println!("Created {}", MOON_WORK);
        } else if workspace.use_paths.len() == previous_len {
            println!("{} is already up to date", MOON_WORK);
        } else {
            println!("Updated {}", MOON_WORK);
        }
    }

    Ok(0)
}

pub fn sync_workspace(source_dir: &Path, quiet: bool) -> anyhow::Result<i32> {
    let workspace = read_workspace(source_dir)?.context(format!(
        "`moon work sync` requires `{}` or `{}` at `{}`",
        MOON_WORK,
        MOON_WORK_JSON,
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

fn resolve_workspace_member(path: &Path) -> anyhow::Result<PathBuf> {
    let member_dir = dunce::canonicalize(path)
        .with_context(|| format!("failed to resolve workspace member `{}`", path.display()))?;
    if !member_dir.is_dir() {
        bail!("workspace member `{}` is not a directory", path.display());
    }
    read_module_desc_file_in_dir(&member_dir).with_context(|| {
        format!(
            "workspace member `{}` does not contain `{}`",
            path.display(),
            MOON_MOD_JSON
        )
    })?;
    Ok(member_dir)
}

fn workspace_use_path(workspace_root: &Path, member_dir: &Path) -> PathBuf {
    let Ok(relative) = member_dir.strip_prefix(workspace_root) else {
        return member_dir.to_path_buf();
    };

    if relative.as_os_str().is_empty() {
        return PathBuf::from(".");
    }

    PathBuf::from(".").join(relative)
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
    let version = Some(resolved_env.module_source(dep_id).version().clone());

    if dep.version() == version.as_ref() {
        return Ok(false);
    }

    dep.set_version(version);
    Ok(true)
}
