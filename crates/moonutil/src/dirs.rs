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

use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::common::{BUILD_DIR, MOON_MOD_JSON, MOON_WORK};
use crate::workspace::{canonical_workspace_module_dirs, read_workspace};

#[derive(Debug, Error)]
pub enum PackageDirsError {
    #[error(
        "not in a Moon project (no moon.mod.json or moon.work.json found starting from {0} or its ancestors)"
    )]
    NotInProject(PathBuf),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, clap::Parser, Serialize, Deserialize, Clone)]
pub struct SourceTargetDirs {
    // NOTE: This separates "working directory" vs "project root".
    //
    // - `-C` changes the process working directory early (like `cd DIR && moon ...`).
    // - `--manifest-path` pins the project root to a specific project manifest
    //   (`moon.mod.json` or `moon.work.json`) without changing the working
    //   directory.
    /// Change to DIR before doing anything else (must appear before the subcommand). Relative paths in other options and arguments are interpreted relative to DIR. Example: `moon -C a run .` runs the same as invoking `moon run .` from within `a`.
    #[arg(short = 'C', value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// Path to `moon.mod.json` or `moon.work.json` to use as the project manifest (does not change the working directory).
    #[arg(long = "manifest-path", global = true, value_name = "PATH")]
    pub manifest_path: Option<PathBuf>,

    /// The target directory. Defaults to `<project-root>/_build`.
    #[clap(long, global = true)]
    pub target_dir: Option<PathBuf>,
}

impl SourceTargetDirs {
    pub fn try_into_package_dirs(&self) -> Result<PackageDirs, PackageDirsError> {
        let project_root = if let Some(manifest_path) = &self.manifest_path {
            resolve_manifest_root(manifest_path).map_err(PackageDirsError::from)?
        } else {
            let start_dir = Self::current_dir()?;
            let project_root = find_ancestor_with_work(&start_dir)
                .map_err(PackageDirsError::from)?
                .or_else(|| find_ancestor_with_mod(&start_dir));
            project_root.ok_or_else(|| PackageDirsError::NotInProject(start_dir.clone()))?
        };

        let target_dir = self.resolve_target_dir(&project_root)?;

        Ok(PackageDirs {
            source_dir: project_root,
            target_dir,
        })
    }

    pub fn try_into_workspace_module_dirs(&self) -> Result<WorkspaceModuleDirs, PackageDirsError> {
        let (project_root, module_dir) = if let Some(manifest_path) = &self.manifest_path {
            let manifest_path = dunce::canonicalize(manifest_path)
                .with_context(|| {
                    format!(
                        "failed to resolve manifest path `{}`",
                        manifest_path.display()
                    )
                })
                .map_err(PackageDirsError::from)?;

            if manifest_path.is_dir() {
                return Err(PackageDirsError::from(anyhow::anyhow!(
                    "`--manifest-path` must point to `{}` or `{}` (got directory `{}`)",
                    MOON_MOD_JSON,
                    MOON_WORK,
                    manifest_path.display()
                )));
            }

            let file_name = manifest_path.file_name().and_then(|s| s.to_str());
            let manifest_root = manifest_path
                .parent()
                .context("manifest path has no parent directory")
                .map(Path::to_path_buf)
                .map_err(PackageDirsError::from)?;

            match file_name {
                Some(MOON_MOD_JSON) => {
                    let module_dir = manifest_root;
                    let project_root = find_ancestor_with_work(&module_dir)
                        .map_err(PackageDirsError::from)?
                        .unwrap_or_else(|| module_dir.clone());
                    (project_root, Some(module_dir))
                }
                Some(MOON_WORK) => (manifest_root, None),
                _ => {
                    return Err(PackageDirsError::from(anyhow::anyhow!(
                        "`--manifest-path` must point to `{}` or `{}` (got `{}`)",
                        MOON_MOD_JSON,
                        MOON_WORK,
                        manifest_path.display()
                    )));
                }
            }
        } else {
            let start_dir = Self::current_dir()?;
            let project_root = find_ancestor_with_work(&start_dir)
                .map_err(PackageDirsError::from)?
                .or_else(|| find_ancestor_with_mod(&start_dir))
                .ok_or_else(|| PackageDirsError::NotInProject(start_dir.clone()))?;
            let module_dir = find_ancestor_with_mod(&start_dir);
            (project_root, module_dir)
        };

        let target_dir = self.resolve_target_dir(&project_root)?;

        Ok(WorkspaceModuleDirs {
            project_root,
            module_dir,
            target_dir,
        })
    }

    fn current_dir() -> Result<PathBuf, PackageDirsError> {
        let start_dir = std::env::current_dir()
            .context("failed to get current directory")
            .map_err(PackageDirsError::from)?;
        dunce::canonicalize(start_dir)
            .context("failed to resolve current directory")
            .map_err(PackageDirsError::from)
    }

    fn resolve_target_dir(&self, project_root: &Path) -> Result<PathBuf, PackageDirsError> {
        let target_dir = self
            .target_dir
            .clone()
            .unwrap_or_else(|| project_root.join(BUILD_DIR));
        if !target_dir.exists() {
            std::fs::create_dir_all(&target_dir)
                .context("failed to create target directory")
                .map_err(PackageDirsError::from)?;
        }
        dunce::canonicalize(target_dir)
            .context("failed to set target directory")
            .map_err(PackageDirsError::from)
    }
}

pub struct PackageDirs {
    pub source_dir: PathBuf,
    pub target_dir: PathBuf,
}

pub struct WorkspaceModuleDirs {
    /// Root used for workspace/project-wide resolution and default `_build`.
    pub project_root: PathBuf,
    /// Selected module root, if the command was invoked from within a module.
    pub module_dir: Option<PathBuf>,
    pub target_dir: PathBuf,
}

impl WorkspaceModuleDirs {
    pub fn require_module_dir(&self, command: &str) -> anyhow::Result<&PathBuf> {
        self.module_dir.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "`moon {command}` cannot infer a target module in workspace `{}`. Run it from a workspace member or pass `--manifest-path <member>/{}`.",
                self.project_root.display(),
                MOON_MOD_JSON
            )
        })
    }
}

pub fn check_moon_mod_exists(source_dir: &Path) -> bool {
    source_dir.join(MOON_MOD_JSON).exists()
}

pub fn check_moon_work_exists(source_dir: &Path) -> bool {
    source_dir.join(MOON_WORK).exists()
}

pub fn find_ancestor_with_mod(source_dir: &Path) -> Option<PathBuf> {
    source_dir
        .ancestors()
        .find(|dir| check_moon_mod_exists(dir))
        .map(|p| p.to_path_buf())
}

pub fn find_ancestor_with_work(source_dir: &Path) -> anyhow::Result<Option<PathBuf>> {
    let mut module_root = None;

    for dir in source_dir.ancestors() {
        if check_moon_work_exists(dir) {
            let Some(module_root) = module_root else {
                // A workspace still applies from nested non-module directories.
                return Ok(Some(dir.to_path_buf()));
            };

            // After we have entered a module, only ancestor workspaces that
            // explicitly list that module still apply.
            let workspace = read_workspace(dir)?.context(format!(
                "failed to parse workspace file `{}`",
                dir.join(MOON_WORK).display()
            ))?;
            for member_dir in canonical_workspace_module_dirs(dir, &workspace)? {
                if member_dir == module_root {
                    return Ok(Some(dir.to_path_buf()));
                }
            }
        }

        if module_root.is_none() && check_moon_mod_exists(dir) {
            module_root = Some(dir);
        }
    }

    Ok(None)
}

pub fn resolve_manifest_root(manifest_path: &Path) -> anyhow::Result<PathBuf> {
    let manifest_path = dunce::canonicalize(manifest_path).with_context(|| {
        format!(
            "failed to resolve manifest path `{}`",
            manifest_path.display()
        )
    })?;

    if manifest_path.is_dir() {
        anyhow::bail!(
            "`--manifest-path` must point to `{}` or `{}` (got directory `{}`)",
            MOON_MOD_JSON,
            MOON_WORK,
            manifest_path.display()
        );
    }

    let file_name = manifest_path.file_name().and_then(|s| s.to_str());
    if file_name != Some(MOON_MOD_JSON) && file_name != Some(MOON_WORK) {
        anyhow::bail!(
            "`--manifest-path` must point to `{}` or `{}` (got `{}`)",
            MOON_MOD_JSON,
            MOON_WORK,
            manifest_path.display()
        );
    }

    manifest_path
        .parent()
        .context("manifest path has no parent directory")
        .map(Path::to_path_buf)
}
