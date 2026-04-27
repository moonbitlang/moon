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
    ffi::OsString,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::common::{BUILD_DIR, DEP_PATH, MOON_MOD, MOON_MOD_JSON, MOON_WORK};
use crate::workspace::{
    canonical_workspace_module_dirs, read_workspace_file, workspace_manifest_path,
};

/// Set to a non-`0` value to disable workspace mode entirely.
pub const MOON_NO_WORKSPACE: &str = "MOON_NO_WORKSPACE";
pub const MOON_WORK_ENV: &str = "MOON_WORK";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceEnv {
    Auto,
    Off,
    Pinned(PathBuf),
}

#[derive(Debug, Error)]
pub enum PackageDirsError {
    #[error(
        "not in a Moon project (no moon.mod, moon.mod.json, or moon.work found starting from {0} or its ancestors)"
    )]
    NotInProject(PathBuf),
    #[error(
        "not in a Moon module (workspace mode is disabled by MOON_WORK=off and no moon.mod or moon.mod.json was found starting from {0} or its ancestors)"
    )]
    WorkspaceDisabledNotInModule(PathBuf),
    #[error("pinned workspace `{workspace}` from MOON_WORK does not apply to module `{module}`")]
    PinnedWorkspaceDoesNotApply { workspace: PathBuf, module: PathBuf },
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
    //   (`moon.mod.json` or `moon.work`) without changing the working directory.
    /// Change to DIR before doing anything else (must appear before the subcommand). Relative paths in other options and arguments are interpreted relative to DIR. Example: `moon -C a run .` runs the same as invoking `moon run .` from within `a`.
    #[arg(short = 'C', value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// Path to `moon.mod.json` or `moon.work` to use as the project manifest (does not change the working directory).
    #[arg(
        long = "manifest-path",
        global = true,
        value_name = "PATH",
        hide = true
    )]
    pub manifest_path: Option<PathBuf>,

    /// The target directory. Defaults to `<project-root>/_build`.
    #[clap(long, global = true)]
    pub target_dir: Option<PathBuf>,
}

impl SourceTargetDirs {
    pub fn try_into_package_dirs(&self) -> Result<PackageDirs, PackageDirsError> {
        self.package_dirs_from(Self::current_dir()?)
    }

    pub fn package_dirs_from(
        &self,
        start_dir: impl AsRef<Path>,
    ) -> Result<PackageDirs, PackageDirsError> {
        let start_dir = dunce::canonicalize(start_dir.as_ref())
            .context("failed to resolve source directory")
            .map_err(PackageDirsError::from)?;
        let workspace_env = current_workspace_env().map_err(PackageDirsError::from)?;
        let project = if let Some(manifest_path) = &self.manifest_path {
            Self::resolve_project_selection_from_manifest_path(manifest_path, &workspace_env)?
        } else {
            resolve_project_selection_from_start_dir(start_dir, &workspace_env)?
        };
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
        let workspace_env = current_workspace_env().map_err(PackageDirsError::from)?;
        if let Some(manifest_path) = &self.manifest_path {
            return Self::resolve_project_selection_from_manifest_path(
                manifest_path,
                &workspace_env,
            );
        }

        let start_dir = Self::current_dir()?;
        resolve_project_selection_from_start_dir(start_dir, &workspace_env)
    }

    fn resolve_project_selection_from_manifest_path(
        manifest_path: &Path,
        workspace_env: &WorkspaceEnv,
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
                MOON_MOD,
                MOON_MOD_JSON,
                MOON_WORK,
                manifest_path.display()
            )));
        }

        let file_name = manifest_path.file_name().and_then(|s| s.to_str());
        let manifest_dir = manifest_path
            .parent()
            .context("manifest path has no parent directory")
            .map(Path::to_path_buf)
            .map_err(PackageDirsError::from)?;

        if matches!(workspace_env, WorkspaceEnv::Off) {
            return match file_name {
                Some(MOON_MOD | MOON_MOD_JSON) => Ok(ProjectSelection {
                    project_root: manifest_dir.clone(),
                    module_dir: Some(manifest_dir),
                    project_manifest_path: manifest_path,
                }),
                Some(MOON_WORK) => project_selection_with_workspace_disabled(manifest_dir),
                _ => Err(PackageDirsError::from(anyhow::anyhow!(
                    "`--manifest-path` must point to `{}`, `{}`, or `{}` (got `{}`)",
                    MOON_MOD,
                    MOON_MOD_JSON,
                    MOON_WORK,
                    manifest_path.display()
                ))),
            };
        }

        match file_name {
            Some(MOON_MOD | MOON_MOD_JSON) => match workspace_env {
                WorkspaceEnv::Pinned(workspace_path) => {
                    project_selection_from_pinned_workspace(workspace_path, Some(manifest_dir))
                }
                WorkspaceEnv::Auto => {
                    if let Some(project_manifest_path) =
                        find_applicable_workspace_manifest_path(&manifest_dir)
                            .map_err(PackageDirsError::from)?
                    {
                        let project_root = manifest_root(&project_manifest_path)
                            .map_err(PackageDirsError::from)?;
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
                WorkspaceEnv::Off => unreachable!("handled above"),
            },
            Some(MOON_WORK) => Ok(ProjectSelection {
                project_root: manifest_dir,
                module_dir: None,
                project_manifest_path: manifest_path,
            }),
            _ => Err(PackageDirsError::from(anyhow::anyhow!(
                "`--manifest-path` must point to `{}`, `{}`, or `{}` (got `{}`)",
                MOON_MOD,
                MOON_MOD_JSON,
                MOON_WORK,
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
                "`moon {command}` cannot infer a target module in workspace `{}`. Run it from a workspace member or use `moon -C <member> {command} ...`.",
                self.project_root.display(),
            )
        })
    }
}

pub fn check_moon_mod_exists(source_dir: &Path) -> bool {
    source_dir.join(MOON_MOD).exists() || source_dir.join(MOON_MOD_JSON).exists()
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
    match current_workspace_env()? {
        WorkspaceEnv::Off => Ok(None),
        WorkspaceEnv::Auto => find_applicable_workspace_manifest_path(source_dir)?
            .map(|path| manifest_root(&path))
            .transpose(),
        WorkspaceEnv::Pinned(workspace_path) => {
            let workspace_root = manifest_root(&workspace_path)?;
            if source_dir.starts_with(&workspace_root) {
                if let Some(module_dir) = find_ancestor_with_mod(source_dir)
                    && module_dir.starts_with(&workspace_root)
                {
                    let workspace = read_workspace_file(&workspace_path)?;
                    let member_dirs = canonical_workspace_module_dirs(&workspace_root, &workspace)?;
                    if !member_dirs
                        .iter()
                        .any(|member_dir| member_dir == &module_dir)
                    {
                        return Err(PackageDirsError::PinnedWorkspaceDoesNotApply {
                            workspace: workspace_path.to_path_buf(),
                            module: module_dir,
                        }
                        .into());
                    }
                }
                return Ok(Some(workspace_root));
            }

            let workspace = read_workspace_file(&workspace_path)?;
            let member_dirs = canonical_workspace_module_dirs(&workspace_root, &workspace)?;
            let Some(module_dir) = find_ancestor_with_mod(source_dir) else {
                return Ok(Some(workspace_root));
            };
            if member_dirs
                .iter()
                .any(|member_dir| member_dir == &module_dir)
            {
                Ok(Some(workspace_root))
            } else {
                Err(PackageDirsError::PinnedWorkspaceDoesNotApply {
                    workspace: workspace_path.to_path_buf(),
                    module: module_dir,
                }
                .into())
            }
        }
    }
}

fn project_selection_with_workspace_disabled(
    start_dir: PathBuf,
) -> Result<ProjectSelection, PackageDirsError> {
    let Some(module_dir) = find_ancestor_with_mod(&start_dir) else {
        return Err(PackageDirsError::WorkspaceDisabledNotInModule(start_dir));
    };

    let project_manifest_path = module_manifest_path(&module_dir);
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

fn module_manifest_path(module_dir: &Path) -> PathBuf {
    if module_dir.join(MOON_MOD).exists() {
        module_dir.join(MOON_MOD)
    } else {
        module_dir.join(MOON_MOD_JSON)
    }
}

fn resolve_project_selection_from_start_dir(
    start_dir: PathBuf,
    workspace_env: &WorkspaceEnv,
) -> Result<ProjectSelection, PackageDirsError> {
    match workspace_env {
        WorkspaceEnv::Off => project_selection_with_workspace_disabled(start_dir),
        WorkspaceEnv::Pinned(workspace_path) => {
            let project_root = manifest_root(workspace_path).map_err(PackageDirsError::from)?;
            let workspace = read_workspace_file(workspace_path).map_err(PackageDirsError::from)?;
            let member_dirs = canonical_workspace_module_dirs(&project_root, &workspace)
                .map_err(PackageDirsError::from)?;
            let module_dir = match find_ancestor_with_mod(&start_dir) {
                Some(module_dir)
                    if start_dir.starts_with(&project_root)
                        && !module_dir.starts_with(&project_root) =>
                {
                    None
                }
                Some(module_dir) => {
                    if member_dirs
                        .iter()
                        .any(|member_dir| member_dir == &module_dir)
                    {
                        Some(module_dir)
                    } else {
                        return Err(PackageDirsError::PinnedWorkspaceDoesNotApply {
                            workspace: workspace_path.to_path_buf(),
                            module: module_dir,
                        });
                    }
                }
                None => None,
            };
            Ok(ProjectSelection {
                project_root,
                module_dir,
                project_manifest_path: workspace_path.to_path_buf(),
            })
        }
        WorkspaceEnv::Auto => {
            let module_dir = find_ancestor_with_mod(&start_dir);

            if let Some(project_manifest_path) = find_applicable_workspace_manifest_path(&start_dir)
                .map_err(PackageDirsError::from)?
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
                let project_manifest_path = module_manifest_path(&module_dir);
                return Ok(ProjectSelection {
                    project_root: module_dir.clone(),
                    module_dir: Some(module_dir),
                    project_manifest_path,
                });
            }

            Err(PackageDirsError::NotInProject(start_dir))
        }
    }
}

fn project_selection_from_pinned_workspace(
    workspace_path: &Path,
    module_dir: Option<PathBuf>,
) -> Result<ProjectSelection, PackageDirsError> {
    let project_root = manifest_root(workspace_path).map_err(PackageDirsError::from)?;
    let workspace = read_workspace_file(workspace_path).map_err(PackageDirsError::from)?;
    let member_dirs = canonical_workspace_module_dirs(&project_root, &workspace)
        .map_err(PackageDirsError::from)?;
    if let Some(module_dir) = &module_dir
        && !member_dirs
            .iter()
            .any(|member_dir| member_dir == module_dir)
    {
        return Err(PackageDirsError::PinnedWorkspaceDoesNotApply {
            workspace: workspace_path.to_path_buf(),
            module: module_dir.to_path_buf(),
        });
    }

    Ok(ProjectSelection {
        project_root,
        module_dir,
        project_manifest_path: workspace_path.to_path_buf(),
    })
}

pub fn current_workspace_env() -> anyhow::Result<WorkspaceEnv> {
    parse_workspace_env(
        std::env::var_os(MOON_WORK_ENV),
        std::env::var_os(MOON_NO_WORKSPACE),
    )
}

pub fn workspace_env_deprecation_warning() -> Option<&'static str> {
    let moon_work = std::env::var_os(MOON_WORK_ENV);
    let moon_no_workspace = std::env::var_os(MOON_NO_WORKSPACE);

    match (moon_work, moon_no_workspace) {
        (_, None) => None,
        (Some(_), Some(_)) => Some(
            "`MOON_NO_WORKSPACE` is deprecated and ignored because `MOON_WORK` is set. Use `MOON_WORK=off` to disable workspace mode.",
        ),
        (None, Some(_)) => Some(
            "`MOON_NO_WORKSPACE` is deprecated. Use `MOON_WORK=off` to disable workspace mode.",
        ),
    }
}

fn parse_workspace_env(
    moon_work: Option<OsString>,
    moon_no_workspace: Option<OsString>,
) -> anyhow::Result<WorkspaceEnv> {
    if let Some(value) = moon_work {
        if value.is_empty() {
            return Ok(WorkspaceEnv::Auto);
        }

        let value_str = value.to_string_lossy();
        if value_str == "auto" {
            return Ok(WorkspaceEnv::Auto);
        }
        if value_str == "off" {
            return Ok(WorkspaceEnv::Off);
        }

        return canonicalize_workspace_env_path(PathBuf::from(value)).map(WorkspaceEnv::Pinned);
    }

    match moon_no_workspace {
        Some(value) if value.to_string_lossy() != "0" => Ok(WorkspaceEnv::Off),
        _ => Ok(WorkspaceEnv::Auto),
    }
}

fn canonicalize_workspace_env_path(path: PathBuf) -> anyhow::Result<PathBuf> {
    let path = dunce::canonicalize(&path)
        .with_context(|| format!("failed to resolve MOON_WORK path `{}`", path.display()))?;

    if path.is_dir() {
        anyhow::bail!(
            "MOON_WORK must point to `{}` (got directory `{}`)",
            MOON_WORK,
            path.display()
        );
    }

    if path.file_name().and_then(|name| name.to_str()) != Some(MOON_WORK) {
        anyhow::bail!(
            "MOON_WORK must point to `{}` (got `{}`)",
            MOON_WORK,
            path.display()
        );
    }

    Ok(path)
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
            MOON_MOD,
            MOON_MOD_JSON,
            MOON_WORK,
            manifest_path.display()
        );
    }

    let file_name = manifest_path.file_name().and_then(|s| s.to_str());
    if file_name != Some(MOON_MOD)
        && file_name != Some(MOON_MOD_JSON)
        && file_name != Some(MOON_WORK)
    {
        anyhow::bail!(
            "`--manifest-path` must point to `{}`, `{}`, or `{}` (got `{}`)",
            MOON_MOD,
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

#[cfg(test)]
mod tests {
    use super::{
        PackageDirs, WorkspaceEnv, parse_workspace_env, resolve_project_selection_from_start_dir,
    };
    use crate::common::{DEP_PATH, MOON_MOD};
    use std::{
        ffi::OsString,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

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

    #[test]
    fn auto_selection_preserves_dsl_module_manifest_path() {
        let test_root = std::env::temp_dir().join(format!(
            "moonutil-dirs-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&test_root).unwrap();
        std::fs::write(
            test_root.join(MOON_MOD),
            r#"name = "alice/app"

version = "0.1.0"
"#,
        )
        .unwrap();

        let project =
            resolve_project_selection_from_start_dir(test_root.clone(), &WorkspaceEnv::Auto)
                .unwrap();
        assert_eq!(project.project_manifest_path, test_root.join(MOON_MOD));

        std::fs::remove_dir_all(test_root).unwrap();
    }

    #[test]
    fn parse_workspace_env_defaults_to_auto() {
        assert_eq!(parse_workspace_env(None, None).unwrap(), WorkspaceEnv::Auto);
    }

    #[test]
    fn parse_workspace_env_accepts_auto_and_empty() {
        assert_eq!(
            parse_workspace_env(Some(OsString::from("auto")), None).unwrap(),
            WorkspaceEnv::Auto
        );
        assert_eq!(
            parse_workspace_env(Some(OsString::from("")), None).unwrap(),
            WorkspaceEnv::Auto
        );
    }

    #[test]
    fn parse_workspace_env_accepts_off() {
        assert_eq!(
            parse_workspace_env(Some(OsString::from("off")), None).unwrap(),
            WorkspaceEnv::Off
        );
    }

    #[test]
    fn parse_workspace_env_falls_back_to_legacy_disable_switch() {
        assert_eq!(
            parse_workspace_env(None, Some(OsString::from("1"))).unwrap(),
            WorkspaceEnv::Off
        );
        assert_eq!(
            parse_workspace_env(None, Some(OsString::from("0"))).unwrap(),
            WorkspaceEnv::Auto
        );
    }

    #[test]
    fn parse_workspace_env_prefers_moon_work_over_legacy_switch() {
        assert_eq!(
            parse_workspace_env(Some(OsString::from("auto")), Some(OsString::from("1"))).unwrap(),
            WorkspaceEnv::Auto
        );
    }
}
