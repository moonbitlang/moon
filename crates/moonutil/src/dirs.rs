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

use crate::common::{BUILD_DIR, DEP_PATH, MOON_MOD_JSON, MOON_WORK, MOON_WORK_JSON};
use crate::workspace::{
    canonical_workspace_module_dirs, read_workspace_file, workspace_manifest_path,
};

/// Set to a non-`0` value to disable workspace mode entirely.
pub const MOON_NO_WORKSPACE: &str = "MOON_NO_WORKSPACE";

#[derive(Debug, Error)]
pub enum PackageDirsError {
    #[error(
        "not in a Moon project (no moon.mod.json, moon.work, or moon.work.json found starting from {0} or its ancestors)"
    )]
    NotInProject(PathBuf),
    #[error(
        "not in a Moon module (workspace mode is disabled by MOON_NO_WORKSPACE and no moon.mod.json was found starting from {0} or its ancestors)"
    )]
    WorkspaceDisabledNotInModule(PathBuf),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl PackageDirsError {
    pub fn allows_single_file_fallback(&self) -> bool {
        matches!(
            self,
            Self::NotInProject(_) | Self::WorkspaceDisabledNotInModule(_)
        )
    }
}

#[derive(Debug, clap::Parser, Serialize, Deserialize, Clone)]
pub struct SourceTargetDirs {
    // NOTE: This separates "working directory" vs "project root".
    //
    // - `-C` changes the process working directory early (like `cd DIR && moon ...`).
    // - `--manifest-path` pins the project root to a specific project manifest
    //   (`moon.mod.json`, `moon.work`, or `moon.work.json`) without changing the working
    //   directory.
    /// Change to DIR before doing anything else (must appear before the subcommand). Relative paths in other options and arguments are interpreted relative to DIR. Example: `moon -C a run .` runs the same as invoking `moon run .` from within `a`.
    #[arg(short = 'C', value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// Path to `moon.mod.json`, `moon.work`, or `moon.work.json` to use as the project manifest (does not change the working directory).
    #[arg(long = "manifest-path", global = true, value_name = "PATH")]
    pub manifest_path: Option<PathBuf>,

    /// The target directory. Defaults to `<project-root>/_build`.
    #[clap(long, global = true)]
    pub target_dir: Option<PathBuf>,
}

impl SourceTargetDirs {
    pub fn try_into_package_dirs(&self) -> Result<PackageDirs, PackageDirsError> {
        let project = self.resolve_project_selection()?;
        let target_dir = self.resolve_target_dir(&project.project_root)?;

        Ok(PackageDirs::from_source_and_target_with_manifest(
            project.project_root,
            target_dir,
            Some(project.project_manifest_path),
        ))
    }

    pub fn package_dirs_from_source_root(
        &self,
        source_root: impl AsRef<Path>,
    ) -> Result<PackageDirs, PackageDirsError> {
        let source_dir = dunce::canonicalize(source_root.as_ref())
            .context("failed to resolve source directory")
            .map_err(PackageDirsError::from)?;
        let target_dir = self.resolve_target_dir(&source_dir)?;
        Ok(PackageDirs::from_source_and_target(source_dir, target_dir))
    }

    pub fn try_into_workspace_module_dirs(&self) -> Result<WorkspaceModuleDirs, PackageDirsError> {
        let project = self.resolve_project_selection()?;
        let target_dir = self.resolve_target_dir(&project.project_root)?;

        Ok(WorkspaceModuleDirs {
            mooncakes_dir: PackageDirs::mooncakes_dir_for_source(&project.project_root),
            project_root: project.project_root,
            module_dir: project.module_dir,
            target_dir,
            project_manifest_path: Some(project.project_manifest_path),
        })
    }

    fn resolve_project_selection(&self) -> Result<ProjectSelection, PackageDirsError> {
        if let Some(manifest_path) = &self.manifest_path {
            return Self::resolve_project_selection_from_manifest_path(manifest_path);
        }

        let start_dir = Self::current_dir()?;
        if disable_workspace_from_env() {
            return project_selection_with_workspace_disabled(start_dir);
        }

        let module_dir = find_ancestor_with_mod(&start_dir);

        if let Some(project_manifest_path) =
            find_applicable_workspace_manifest_path(&start_dir).map_err(PackageDirsError::from)?
        {
            let project_root =
                manifest_root(&project_manifest_path).map_err(PackageDirsError::from)?;
            return Ok(ProjectSelection {
                project_root,
                module_dir,
                project_manifest_path,
            });
        }

        if let Some(module_dir) = module_dir {
            let project_manifest_path = module_dir.join(MOON_MOD_JSON);
            return Ok(ProjectSelection {
                project_root: module_dir.clone(),
                module_dir: Some(module_dir),
                project_manifest_path,
            });
        }

        Err(PackageDirsError::NotInProject(start_dir))
    }

    fn resolve_project_selection_from_manifest_path(
        manifest_path: &Path,
    ) -> Result<ProjectSelection, PackageDirsError> {
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
                "`--manifest-path` must point to `{}`, `{}`, or `{}` (got directory `{}`)",
                MOON_MOD_JSON,
                MOON_WORK,
                MOON_WORK_JSON,
                manifest_path.display()
            )));
        }

        let file_name = manifest_path.file_name().and_then(|s| s.to_str());
        let manifest_dir = manifest_path
            .parent()
            .context("manifest path has no parent directory")
            .map(Path::to_path_buf)
            .map_err(PackageDirsError::from)?;

        if disable_workspace_from_env() {
            return match file_name {
                Some(MOON_MOD_JSON) => Ok(ProjectSelection {
                    project_root: manifest_dir.clone(),
                    module_dir: Some(manifest_dir),
                    project_manifest_path: manifest_path,
                }),
                Some(MOON_WORK) | Some(MOON_WORK_JSON) => {
                    project_selection_with_workspace_disabled(manifest_dir)
                }
                _ => Err(PackageDirsError::from(anyhow::anyhow!(
                    "`--manifest-path` must point to `{}`, `{}`, or `{}` (got `{}`)",
                    MOON_MOD_JSON,
                    MOON_WORK,
                    MOON_WORK_JSON,
                    manifest_path.display()
                ))),
            };
        }

        match file_name {
            Some(MOON_MOD_JSON) => {
                if let Some(project_manifest_path) =
                    find_applicable_workspace_manifest_path(&manifest_dir)
                        .map_err(PackageDirsError::from)?
                {
                    let project_root =
                        manifest_root(&project_manifest_path).map_err(PackageDirsError::from)?;
                    Ok(ProjectSelection {
                        project_root,
                        module_dir: Some(manifest_dir),
                        project_manifest_path,
                    })
                } else {
                    Ok(ProjectSelection {
                        project_root: manifest_dir.clone(),
                        module_dir: Some(manifest_dir),
                        project_manifest_path: manifest_path,
                    })
                }
            }
            Some(MOON_WORK) | Some(MOON_WORK_JSON) => Ok(ProjectSelection {
                project_root: manifest_dir,
                module_dir: None,
                project_manifest_path: manifest_path,
            }),
            _ => Err(PackageDirsError::from(anyhow::anyhow!(
                "`--manifest-path` must point to `{}`, `{}`, or `{}` (got `{}`)",
                MOON_MOD_JSON,
                MOON_WORK,
                MOON_WORK_JSON,
                manifest_path.display()
            ))),
        }
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
    pub mooncakes_dir: PathBuf,
    pub project_manifest_path: Option<PathBuf>,
}

impl PackageDirs {
    pub fn mooncakes_dir_for_source(source_dir: &Path) -> PathBuf {
        source_dir.join(DEP_PATH)
    }

    pub fn from_source_and_target(source_dir: PathBuf, target_dir: PathBuf) -> Self {
        Self::from_source_and_target_with_manifest(source_dir, target_dir, None)
    }

    pub fn from_source_and_target_with_manifest(
        source_dir: PathBuf,
        target_dir: PathBuf,
        project_manifest_path: Option<PathBuf>,
    ) -> Self {
        Self {
            source_dir: source_dir.clone(),
            target_dir,
            mooncakes_dir: Self::mooncakes_dir_for_source(&source_dir),
            project_manifest_path,
        }
    }
}

pub struct WorkspaceModuleDirs {
    /// Root used for workspace/project-wide resolution and default `_build`.
    pub project_root: PathBuf,
    /// Selected module root, if the command was invoked from within a module.
    pub module_dir: Option<PathBuf>,
    pub target_dir: PathBuf,
    pub mooncakes_dir: PathBuf,
    pub project_manifest_path: Option<PathBuf>,
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
    workspace_manifest_path(source_dir).is_some()
}

pub fn find_ancestor_with_mod(source_dir: &Path) -> Option<PathBuf> {
    source_dir
        .ancestors()
        .find(|dir| check_moon_mod_exists(dir))
        .map(|p| p.to_path_buf())
}

pub fn find_ancestor_with_work(source_dir: &Path) -> anyhow::Result<Option<PathBuf>> {
    if disable_workspace_from_env() {
        return Ok(None);
    }

    find_applicable_workspace_manifest_path(source_dir)?
        .map(|path| manifest_root(&path))
        .transpose()
}

fn disable_workspace_from_env() -> bool {
    match std::env::var(MOON_NO_WORKSPACE) {
        Ok(value) => value != "0",
        Err(std::env::VarError::NotPresent) => false,
        Err(std::env::VarError::NotUnicode(_)) => true,
    }
}

fn project_selection_with_workspace_disabled(
    start_dir: PathBuf,
) -> Result<ProjectSelection, PackageDirsError> {
    let Some(module_dir) = find_ancestor_with_mod(&start_dir) else {
        return Err(PackageDirsError::WorkspaceDisabledNotInModule(start_dir));
    };

    let project_manifest_path = module_dir.join(MOON_MOD_JSON);
    Ok(ProjectSelection {
        project_root: module_dir.clone(),
        module_dir: Some(module_dir),
        project_manifest_path,
    })
}

struct ProjectSelection {
    project_root: PathBuf,
    module_dir: Option<PathBuf>,
    project_manifest_path: PathBuf,
}

fn find_applicable_workspace_manifest_path(source_dir: &Path) -> anyhow::Result<Option<PathBuf>> {
    let mut module_root = None;

    for dir in source_dir.ancestors() {
        let Some(workspace_path) = workspace_manifest_path(dir) else {
            if module_root.is_none() && check_moon_mod_exists(dir) {
                module_root = Some(dir);
            }
            continue;
        };

        let Some(module_root) = module_root else {
            return Ok(Some(workspace_path));
        };

        let workspace = read_workspace_file(&workspace_path)?;
        for member_dir in canonical_workspace_module_dirs(dir, &workspace)? {
            if member_dir == module_root {
                return Ok(Some(workspace_path));
            }
        }
    }

    Ok(None)
}

fn manifest_root(manifest_path: &Path) -> anyhow::Result<PathBuf> {
    manifest_path
        .parent()
        .context("manifest path has no parent directory")
        .map(Path::to_path_buf)
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
            "`--manifest-path` must point to `{}`, `{}`, or `{}` (got directory `{}`)",
            MOON_MOD_JSON,
            MOON_WORK,
            MOON_WORK_JSON,
            manifest_path.display()
        );
    }

    let file_name = manifest_path.file_name().and_then(|s| s.to_str());
    if file_name != Some(MOON_MOD_JSON)
        && file_name != Some(MOON_WORK)
        && file_name != Some(MOON_WORK_JSON)
    {
        anyhow::bail!(
            "`--manifest-path` must point to `{}`, `{}`, or `{}` (got `{}`)",
            MOON_MOD_JSON,
            MOON_WORK,
            MOON_WORK_JSON,
            manifest_path.display()
        );
    }

    manifest_path
        .parent()
        .context("manifest path has no parent directory")
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::PackageDirs;
    use crate::common::DEP_PATH;
    use std::path::PathBuf;

    #[test]
    fn mooncakes_dir_tracks_project_root() {
        let project = PathBuf::from("project");
        assert_eq!(
            PackageDirs::mooncakes_dir_for_source(&project),
            project.join(DEP_PATH)
        );
    }

    #[test]
    fn mooncakes_dir_comes_from_source_even_with_custom_target() {
        let project = PathBuf::from("project");
        let target = PathBuf::from("tmp-target");
        let dirs = PackageDirs::from_source_and_target(project.clone(), target);
        assert_eq!(dirs.mooncakes_dir, project.join(DEP_PATH));
    }
}
