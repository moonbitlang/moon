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
    pub fn query(&self) -> Result<ProjectQuery, PackageDirsError> {
        self.query_from(Self::current_dir()?)
    }

    pub fn query_from(
        &self,
        start_dir: impl AsRef<Path>,
    ) -> Result<ProjectQuery, PackageDirsError> {
        ProjectQuery::new(self, start_dir.as_ref())
    }

    pub fn source_root_package_dirs(
        &self,
        source_root: impl AsRef<Path>,
    ) -> Result<PackageDirs, PackageDirsError> {
        let source_dir = dunce::canonicalize(source_root.as_ref())
            .context("failed to resolve source directory")
            .map_err(PackageDirsError::from)?;
        let target_dir = self.resolve_target_dir(&source_dir)?;
        Ok(PackageDirs::from_source_and_target(source_dir, target_dir))
    }

    pub fn single_file_package_dirs(
        &self,
        file_path: impl AsRef<Path>,
    ) -> Result<SingleFilePackageDirs, PackageDirsError> {
        // This only builds the synthetic package directories. Whether a command
        // may fall back to single-file mode depends on that command's argv.
        let file_path = dunce::canonicalize(file_path.as_ref())
            .with_context(|| {
                format!(
                    "failed to resolve file path `{}`",
                    file_path.as_ref().display()
                )
            })
            .map_err(PackageDirsError::from)?;
        let source_dir = file_path
            .parent()
            .context("file path must have a parent directory")
            .map(Path::to_path_buf)
            .map_err(PackageDirsError::from)?;
        let package_dirs = self.source_root_package_dirs(source_dir)?;
        Ok(SingleFilePackageDirs {
            file_path,
            package_dirs,
        })
    }

    pub fn work_root(&self, prefer_existing_workspace: bool) -> Result<PathBuf, PackageDirsError> {
        let start_dir = Self::current_dir()?;
        let mut query = if prefer_existing_workspace {
            self.query_from(start_dir)?
        } else {
            ProjectQuery::new_with_workspace_env(self, &start_dir, WorkspaceEnv::Off)?
        };
        query.work_root(prefer_existing_workspace)
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

pub struct SingleFilePackageDirs {
    pub file_path: PathBuf,
    pub package_dirs: PackageDirs,
}

#[derive(Debug)]
pub enum ProjectProbe {
    Found(ProjectContext),
    NotFound(ProjectNotFound),
}

#[derive(Debug)]
pub struct ProjectNotFound {
    error: PackageDirsError,
}

impl ProjectNotFound {
    pub fn into_error(self) -> PackageDirsError {
        self.error
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleRef {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRef {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectContext {
    Workspace {
        root: PathBuf,
        manifest_path: PathBuf,
        selected_module: Option<ModuleRef>,
    },
    Module {
        root: PathBuf,
        manifest_path: PathBuf,
    },
}

impl ProjectContext {
    pub fn root(&self) -> &Path {
        match self {
            Self::Workspace { root, .. } => root,
            Self::Module { root, .. } => root,
        }
    }

    pub fn manifest_path(&self) -> &Path {
        match self {
            Self::Workspace { manifest_path, .. } => manifest_path,
            Self::Module { manifest_path, .. } => manifest_path,
        }
    }

    pub fn selected_module(&self) -> Option<ModuleRef> {
        match self {
            Self::Workspace {
                selected_module, ..
            } => selected_module.clone(),
            Self::Module {
                root,
                manifest_path,
            } => Some(ModuleRef {
                root: root.clone(),
                manifest_path: manifest_path.clone(),
            }),
        }
    }

    pub fn workspace_ref(&self) -> Option<WorkspaceRef> {
        match self {
            Self::Workspace {
                root,
                manifest_path,
                ..
            } => Some(WorkspaceRef {
                root: root.clone(),
                manifest_path: manifest_path.clone(),
            }),
            Self::Module { .. } => None,
        }
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
    let source_target_dirs = SourceTargetDirs {
        cwd: None,
        manifest_path: None,
        target_dir: None,
    };
    let mut query = source_target_dirs.query_from(source_dir)?;
    query.workspace_root_for_sync().map_err(Into::into)
}

pub fn resolve_work_root(
    manifest_path: Option<&Path>,
    prefer_existing_workspace: bool,
) -> anyhow::Result<PathBuf> {
    let source_target_dirs = SourceTargetDirs {
        cwd: None,
        manifest_path: manifest_path.map(Path::to_path_buf),
        target_dir: None,
    };
    source_target_dirs
        .work_root(prefer_existing_workspace)
        .map_err(Into::into)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ManifestKind {
    Module,
    Workspace,
}

#[derive(Clone)]
struct ManifestInput {
    path: PathBuf,
    root: PathBuf,
    kind: ManifestKind,
}

struct WorkspaceFacts {
    // Keep this as cheap path-level discovery. Parsed `moon.work` content and
    // member canonicalization are projections, because many commands only need
    // the workspace root or manifest path.
    manifest_path: PathBuf,
    root: PathBuf,
}

pub struct ProjectQuery {
    // Inputs and cheap markers are captured once, then higher-level methods
    // project only the artifacts their callers ask for.
    target_dir: Option<PathBuf>,
    start_dir: PathBuf,
    workspace_env: WorkspaceEnv,
    explicit_manifest: Option<ManifestInput>,
    module_dir: Option<PathBuf>,
    module_manifest_path: Option<PathBuf>,
    workspace: Option<WorkspaceFacts>,
    workspace_members: Option<Vec<PathBuf>>,
}

fn module_manifest_path(module_dir: &Path) -> PathBuf {
    if module_dir.join(MOON_MOD).exists() {
        module_dir.join(MOON_MOD)
    } else {
        module_dir.join(MOON_MOD_JSON)
    }
}

impl WorkspaceFacts {
    fn from_manifest_path(manifest_path: PathBuf) -> Result<Self, PackageDirsError> {
        let root = manifest_root(&manifest_path).map_err(PackageDirsError::from)?;
        Ok(Self {
            manifest_path,
            root,
        })
    }

    fn from_pinned_manifest_path(manifest_path: &Path) -> Result<Self, PackageDirsError> {
        let manifest_path = dunce::canonicalize(manifest_path)
            .context("failed to resolve pinned workspace path")
            .map_err(PackageDirsError::from)?;
        let root = manifest_root(&manifest_path).map_err(PackageDirsError::from)?;
        Ok(Self {
            manifest_path,
            root,
        })
    }
}

impl ProjectQuery {
    fn new(
        source_target_dirs: &SourceTargetDirs,
        start_dir: &Path,
    ) -> Result<Self, PackageDirsError> {
        let workspace_env = current_workspace_env().map_err(PackageDirsError::from)?;
        Self::new_with_workspace_env(source_target_dirs, start_dir, workspace_env)
    }

    fn new_with_workspace_env(
        source_target_dirs: &SourceTargetDirs,
        start_dir: &Path,
        workspace_env: WorkspaceEnv,
    ) -> Result<Self, PackageDirsError> {
        let start_dir = dunce::canonicalize(start_dir)
            .with_context(|| {
                format!(
                    "failed to resolve source directory `{}`",
                    start_dir.display()
                )
            })
            .map_err(PackageDirsError::from)?;
        let explicit_manifest = source_target_dirs
            .manifest_path
            .as_deref()
            .map(manifest_input_from_path)
            .transpose()
            .map_err(PackageDirsError::from)?;
        let module_dir = match &explicit_manifest {
            Some(manifest) if manifest.kind == ManifestKind::Module => Some(manifest.root.clone()),
            Some(manifest) => find_ancestor_with_mod(&manifest.root),
            None => find_ancestor_with_mod(&start_dir),
        };
        let module_manifest_path = match &explicit_manifest {
            Some(manifest) if manifest.kind == ManifestKind::Module => Some(manifest.path.clone()),
            _ => module_dir.as_deref().map(module_manifest_path),
        };
        let workspace = match &workspace_env {
            WorkspaceEnv::Off => None,
            WorkspaceEnv::Pinned(workspace_path) => {
                Some(WorkspaceFacts::from_pinned_manifest_path(workspace_path)?)
            }
            WorkspaceEnv::Auto => None,
        };

        Ok(Self {
            target_dir: source_target_dirs.target_dir.clone(),
            start_dir,
            workspace_env,
            explicit_manifest,
            module_dir,
            module_manifest_path,
            workspace,
            workspace_members: None,
        })
    }

    pub fn probe_project(&mut self) -> Result<ProjectProbe, PackageDirsError> {
        match self.resolve_project_context() {
            Ok(project) => Ok(ProjectProbe::Found(project)),
            Err(error) if error.allows_single_file_fallback() => {
                Ok(ProjectProbe::NotFound(ProjectNotFound { error }))
            }
            Err(error) => Err(error),
        }
    }

    pub fn project(&mut self) -> Result<ProjectContext, PackageDirsError> {
        match self.probe_project()? {
            ProjectProbe::Found(project) => Ok(project),
            ProjectProbe::NotFound(not_found) => Err(not_found.into_error()),
        }
    }

    pub fn package_dirs(&mut self) -> Result<PackageDirs, PackageDirsError> {
        let project = self.project()?;
        let target_dir = self.resolve_target_dir(project.root())?;
        Ok(PackageDirs::from_source_and_target_with_manifest(
            project.root().to_path_buf(),
            target_dir,
            Some(project.manifest_path().to_path_buf()),
        ))
    }

    pub fn selected_module(&mut self) -> Result<Option<ModuleRef>, PackageDirsError> {
        Ok(self.project()?.selected_module())
    }

    pub fn workspace_ref(&mut self) -> Result<Option<WorkspaceRef>, PackageDirsError> {
        Ok(self.project()?.workspace_ref())
    }

    pub fn workspace_members(&mut self) -> Result<Option<Vec<PathBuf>>, PackageDirsError> {
        if self.workspace_ref()?.is_none() {
            return Ok(None);
        }
        self.ensure_workspace_members()
            .map(|member_dirs| member_dirs.map(<[PathBuf]>::to_vec))
    }

    pub fn work_root(
        &mut self,
        prefer_existing_workspace: bool,
    ) -> Result<PathBuf, PackageDirsError> {
        if prefer_existing_workspace && let Some(work_root) = self.workspace_root_for_sync()? {
            return Ok(work_root);
        }

        let start_dir = self.work_start_dir();
        if let Some(module_dir) = find_ancestor_with_mod(&start_dir) {
            Ok(module_dir)
        } else {
            Ok(start_dir)
        }
    }

    fn resolve_project_context(&mut self) -> Result<ProjectContext, PackageDirsError> {
        if let Some(manifest) = self.explicit_manifest.clone() {
            return self.project_context_from_manifest(&manifest);
        }

        self.project_context_from_start_dir()
    }

    fn project_context_from_manifest(
        &mut self,
        manifest: &ManifestInput,
    ) -> Result<ProjectContext, PackageDirsError> {
        match &self.workspace_env {
            WorkspaceEnv::Off => match manifest.kind {
                ManifestKind::Module => Ok(ProjectContext::Module {
                    root: manifest.root.clone(),
                    manifest_path: manifest.path.clone(),
                }),
                ManifestKind::Workspace => {
                    self.module_context_with_workspace_disabled(manifest.root.clone())
                }
            },
            WorkspaceEnv::Auto => match manifest.kind {
                ManifestKind::Module => {
                    if let Some(workspace) = self.find_applicable_workspace_from(&manifest.root)? {
                        Ok(ProjectContext::Workspace {
                            root: workspace.root.clone(),
                            manifest_path: workspace.manifest_path.clone(),
                            selected_module: Some(ModuleRef {
                                root: manifest.root.clone(),
                                manifest_path: manifest.path.clone(),
                            }),
                        })
                    } else {
                        Ok(ProjectContext::Module {
                            root: manifest.root.clone(),
                            manifest_path: manifest.path.clone(),
                        })
                    }
                }
                ManifestKind::Workspace => Ok(ProjectContext::Workspace {
                    root: manifest.root.clone(),
                    manifest_path: manifest.path.clone(),
                    selected_module: None,
                }),
            },
            WorkspaceEnv::Pinned(_) => match manifest.kind {
                ManifestKind::Module => {
                    self.project_context_from_pinned_workspace(Some(ModuleRef {
                        root: manifest.root.clone(),
                        manifest_path: manifest.path.clone(),
                    }))
                }
                ManifestKind::Workspace => Ok(ProjectContext::Workspace {
                    root: manifest.root.clone(),
                    manifest_path: manifest.path.clone(),
                    selected_module: None,
                }),
            },
        }
    }

    fn project_context_from_start_dir(&mut self) -> Result<ProjectContext, PackageDirsError> {
        match &self.workspace_env {
            WorkspaceEnv::Off => {
                self.module_context_with_workspace_disabled(self.start_dir.clone())
            }
            WorkspaceEnv::Pinned(_) => {
                let workspace = self.pinned_workspace_ref();
                let module_dir = match self.module_dir.clone() {
                    // When invoked at a pinned workspace root nested under an
                    // unrelated module, the outer module is not the selection.
                    Some(module_dir)
                        if self.start_dir.starts_with(&workspace.root)
                            && !module_dir.starts_with(&workspace.root) =>
                    {
                        None
                    }
                    Some(module_dir) => {
                        if self.workspace_contains_member(&module_dir)? {
                            Some(module_dir)
                        } else {
                            return Err(PackageDirsError::PinnedWorkspaceDoesNotApply {
                                workspace: workspace.manifest_path.clone(),
                                module: module_dir,
                            });
                        }
                    }
                    None => None,
                };

                Ok(ProjectContext::Workspace {
                    root: workspace.root.clone(),
                    manifest_path: workspace.manifest_path.clone(),
                    selected_module: module_dir.map(|root| ModuleRef {
                        manifest_path: module_manifest_path(&root),
                        root,
                    }),
                })
            }
            WorkspaceEnv::Auto => {
                let start_dir = self.start_dir.clone();
                if let Some(workspace) = self.find_applicable_workspace_from(&start_dir)? {
                    return Ok(ProjectContext::Workspace {
                        root: workspace.root.clone(),
                        manifest_path: workspace.manifest_path.clone(),
                        selected_module: self.module_dir.as_ref().map(|root| ModuleRef {
                            root: root.clone(),
                            manifest_path: self
                                .module_manifest_path
                                .clone()
                                .unwrap_or_else(|| module_manifest_path(root)),
                        }),
                    });
                }

                if let Some(module_dir) = &self.module_dir {
                    return Ok(ProjectContext::Module {
                        root: module_dir.clone(),
                        manifest_path: self
                            .module_manifest_path
                            .clone()
                            .unwrap_or_else(|| module_manifest_path(module_dir)),
                    });
                }

                Err(PackageDirsError::NotInProject(self.start_dir.clone()))
            }
        }
    }

    fn module_context_with_workspace_disabled(
        &self,
        error_start_dir: PathBuf,
    ) -> Result<ProjectContext, PackageDirsError> {
        let Some(module_dir) = &self.module_dir else {
            return Err(PackageDirsError::WorkspaceDisabledNotInModule(
                error_start_dir,
            ));
        };

        Ok(ProjectContext::Module {
            root: module_dir.clone(),
            manifest_path: self
                .module_manifest_path
                .clone()
                .unwrap_or_else(|| module_manifest_path(module_dir)),
        })
    }

    fn project_context_from_pinned_workspace(
        &mut self,
        selected_module: Option<ModuleRef>,
    ) -> Result<ProjectContext, PackageDirsError> {
        let workspace = self.pinned_workspace_ref();
        if let Some(module) = &selected_module
            && !self.workspace_contains_member(&module.root)?
        {
            return Err(PackageDirsError::PinnedWorkspaceDoesNotApply {
                workspace: workspace.manifest_path.clone(),
                module: module.root.clone(),
            });
        }

        Ok(ProjectContext::Workspace {
            root: workspace.root.clone(),
            manifest_path: workspace.manifest_path.clone(),
            selected_module,
        })
    }

    fn workspace_root_for_sync(&mut self) -> Result<Option<PathBuf>, PackageDirsError> {
        match &self.workspace_env {
            WorkspaceEnv::Off => Ok(None),
            WorkspaceEnv::Auto => {
                let start_dir = self.work_start_dir();
                Ok(self
                    .find_applicable_workspace_from(&start_dir)?
                    .map(|workspace| workspace.root))
            }
            WorkspaceEnv::Pinned(_) => {
                let workspace = self.pinned_workspace_ref();
                let start_dir = self.work_start_dir();
                if start_dir.starts_with(&workspace.root) {
                    if let Some(module_dir) = self.module_dir.clone()
                        && module_dir.starts_with(&workspace.root)
                        && !self.workspace_contains_member(&module_dir)?
                    {
                        return Err(PackageDirsError::PinnedWorkspaceDoesNotApply {
                            workspace: workspace.manifest_path.clone(),
                            module: module_dir,
                        });
                    }
                    return Ok(Some(workspace.root.clone()));
                }

                let Some(module_dir) = self.module_dir.clone() else {
                    return Ok(Some(workspace.root.clone()));
                };
                if self.workspace_contains_member(&module_dir)? {
                    Ok(Some(workspace.root.clone()))
                } else {
                    Err(PackageDirsError::PinnedWorkspaceDoesNotApply {
                        workspace: workspace.manifest_path.clone(),
                        module: module_dir,
                    })
                }
            }
        }
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

    fn workspace_contains_member(&mut self, module_dir: &Path) -> Result<bool, PackageDirsError> {
        Ok(self
            .ensure_workspace_members()?
            .is_some_and(|members| members.iter().any(|member_dir| member_dir == module_dir)))
    }

    fn find_applicable_workspace_from(
        &mut self,
        source_dir: &Path,
    ) -> Result<Option<WorkspaceRef>, PackageDirsError> {
        if self.workspace.is_none() {
            self.workspace = find_applicable_workspace_manifest_path(source_dir)
                .map_err(PackageDirsError::from)?
                .map(WorkspaceFacts::from_manifest_path)
                .transpose()?;
        }
        Ok(self.workspace.as_ref().map(|workspace| WorkspaceRef {
            root: workspace.root.clone(),
            manifest_path: workspace.manifest_path.clone(),
        }))
    }

    fn ensure_workspace_members(&mut self) -> Result<Option<&[PathBuf]>, PackageDirsError> {
        let Some(workspace) = &self.workspace else {
            return Ok(None);
        };
        if self.workspace_members.is_none() {
            let moon_work =
                read_workspace_file(&workspace.manifest_path).map_err(PackageDirsError::from)?;
            let member_dirs = canonical_workspace_module_dirs(&workspace.root, &moon_work)
                .map_err(PackageDirsError::from)?;
            self.workspace_members = Some(member_dirs);
        }
        Ok(self.workspace_members.as_deref())
    }

    fn work_start_dir(&self) -> PathBuf {
        self.explicit_manifest
            .as_ref()
            .map(|manifest| manifest.root.clone())
            .unwrap_or_else(|| self.start_dir.clone())
    }

    fn pinned_workspace_ref(&self) -> WorkspaceRef {
        let workspace = self
            .workspace
            .as_ref()
            .expect("pinned workspace discovery must include workspace facts");
        WorkspaceRef {
            root: workspace.root.clone(),
            manifest_path: workspace.manifest_path.clone(),
        }
    }
}

#[cfg(test)]
fn project_query_from_start_dir(
    start_dir: PathBuf,
    workspace_env: &WorkspaceEnv,
) -> Result<ProjectQuery, PackageDirsError> {
    let mut query = ProjectQuery {
        target_dir: None,
        start_dir: dunce::canonicalize(start_dir)
            .context("failed to resolve source directory")
            .map_err(PackageDirsError::from)?,
        workspace_env: workspace_env.clone(),
        explicit_manifest: None,
        module_dir: None,
        module_manifest_path: None,
        workspace: None,
        workspace_members: None,
    };
    let start_dir = query.start_dir.clone();
    query.module_dir = find_ancestor_with_mod(&start_dir);
    query.module_manifest_path = query.module_dir.as_deref().map(module_manifest_path);
    query.workspace = match workspace_env {
        WorkspaceEnv::Off => None,
        WorkspaceEnv::Pinned(workspace_path) => {
            Some(WorkspaceFacts::from_pinned_manifest_path(workspace_path)?)
        }
        WorkspaceEnv::Auto => None,
    };
    Ok(query)
}

#[cfg(test)]
fn resolve_project_context_from_start_dir(
    start_dir: PathBuf,
    workspace_env: &WorkspaceEnv,
) -> Result<ProjectContext, PackageDirsError> {
    project_query_from_start_dir(start_dir, workspace_env)?.project()
}

#[cfg(test)]
fn resolve_project_context_from_manifest_path(
    manifest_path: &Path,
    workspace_env: &WorkspaceEnv,
) -> Result<ProjectContext, PackageDirsError> {
    let manifest = manifest_input_from_path(manifest_path).map_err(PackageDirsError::from)?;
    let mut query = ProjectQuery {
        target_dir: None,
        start_dir: manifest.root.clone(),
        workspace_env: workspace_env.clone(),
        module_dir: match manifest.kind {
            ManifestKind::Module => Some(manifest.root.clone()),
            ManifestKind::Workspace => find_ancestor_with_mod(&manifest.root),
        },
        module_manifest_path: match manifest.kind {
            ManifestKind::Module => Some(manifest.path.clone()),
            ManifestKind::Workspace => find_ancestor_with_mod(&manifest.root)
                .as_deref()
                .map(module_manifest_path),
        },
        workspace: match (workspace_env, manifest.kind) {
            (WorkspaceEnv::Off, _) => None,
            (WorkspaceEnv::Pinned(workspace_path), _) => {
                Some(WorkspaceFacts::from_pinned_manifest_path(workspace_path)?)
            }
            (WorkspaceEnv::Auto, _) => None,
        },
        explicit_manifest: Some(manifest),
        workspace_members: None,
    };
    query.project()
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

fn manifest_input_from_path(manifest_path: &Path) -> anyhow::Result<ManifestInput> {
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

    let kind = match file_name {
        Some(MOON_MOD | MOON_MOD_JSON) => ManifestKind::Module,
        Some(MOON_WORK) => ManifestKind::Workspace,
        _ => unreachable!("manifest file name was validated above"),
    };

    let root = manifest_path
        .parent()
        .context("manifest path has no parent directory")
        .map(Path::to_path_buf)?;

    Ok(ManifestInput {
        path: manifest_path,
        root,
        kind,
    })
}

pub fn resolve_manifest_root(manifest_path: &Path) -> anyhow::Result<PathBuf> {
    manifest_input_from_path(manifest_path).map(|manifest| manifest.root)
}

#[cfg(test)]
mod tests {
    use super::{
        PackageDirs, PackageDirsError, ProjectContext, ProjectProbe, WorkspaceEnv,
        parse_workspace_env, project_query_from_start_dir,
        resolve_project_context_from_manifest_path, resolve_project_context_from_start_dir,
    };
    use crate::common::{DEP_PATH, MOON_MOD, MOON_MOD_JSON};
    use std::{
        ffi::OsString,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TestProject {
        root: PathBuf,
    }

    impl TestProject {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "moonutil-dirs-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn path(&self) -> &Path {
            &self.root
        }

        fn join(&self, path: impl AsRef<Path>) -> PathBuf {
            self.root.join(path)
        }
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn write_file(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn canonical(path: impl AsRef<Path>) -> PathBuf {
        dunce::canonicalize(path).unwrap()
    }

    fn write_json_module(path: &Path, name: &str) {
        write_file(
            &path.join(MOON_MOD_JSON),
            &format!(
                r#"{{
  "name": "{name}",
  "version": "0.1.0"
}}
"#
            ),
        );
    }

    fn nested_workspace_under_unrelated_module() -> TestProject {
        let project = TestProject::new();
        write_json_module(&project.join("outer"), "alice/outer");
        write_file(
            &project.join("outer/ws/moon.work"),
            r#"members = [
  "./app",
]
"#,
        );
        write_json_module(&project.join("outer/ws/app"), "alice/app");
        project
    }

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
        let project = TestProject::new();
        write_file(
            &project.join(MOON_MOD),
            r#"name = "alice/app"

version = "0.1.0"
"#,
        );

        let selection = resolve_project_context_from_start_dir(
            project.path().to_path_buf(),
            &WorkspaceEnv::Auto,
        )
        .unwrap();
        let ProjectContext::Module { manifest_path, .. } = selection else {
            panic!("expected module context");
        };
        assert_eq!(manifest_path, canonical(project.join(MOON_MOD)));
    }

    #[test]
    fn project_probe_reports_not_found_without_project() {
        let project = TestProject::new();
        let mut query =
            project_query_from_start_dir(project.path().to_path_buf(), &WorkspaceEnv::Auto)
                .unwrap();

        let ProjectProbe::NotFound(not_found) = query.probe_project().unwrap() else {
            panic!("expected project probe to report not found");
        };
        assert!(matches!(
            not_found.into_error(),
            PackageDirsError::NotInProject(path) if path == canonical(project.path())
        ));
    }

    #[test]
    fn workspace_members_are_projected_on_demand() {
        let project = TestProject::new();
        write_file(
            &project.join("moon.work"),
            "members = [\n  \"./missing\",\n]\n",
        );

        let mut query =
            project_query_from_start_dir(project.path().to_path_buf(), &WorkspaceEnv::Auto)
                .unwrap();
        assert!(
            matches!(query.project().unwrap(), ProjectContext::Workspace { .. }),
            "workspace root should resolve without canonicalizing members"
        );
        assert!(
            query.workspace_members().is_err(),
            "workspace members projection should canonicalize member paths on demand"
        );
    }

    #[test]
    fn pinned_workspace_root_under_unrelated_outer_module_succeeds() {
        let project = nested_workspace_under_unrelated_module();
        let workspace_path = canonical(project.join("outer/ws/moon.work"));

        let selection = resolve_project_context_from_start_dir(
            project.join("outer/ws"),
            &WorkspaceEnv::Pinned(workspace_path.clone()),
        )
        .unwrap();

        let ProjectContext::Workspace {
            root,
            manifest_path,
            selected_module,
        } = selection
        else {
            panic!("expected workspace context");
        };
        assert_eq!(root, canonical(project.join("outer/ws")));
        assert_eq!(selected_module, None);
        assert_eq!(manifest_path, workspace_path);
    }

    #[test]
    fn pinned_workspace_rejects_unlisted_module_under_workspace_root() {
        let project = nested_workspace_under_unrelated_module();
        let workspace_path = canonical(project.join("outer/ws/moon.work"));
        write_json_module(&project.join("outer/ws/tools"), "alice/tools");

        let err = resolve_project_context_from_start_dir(
            project.join("outer/ws/tools"),
            &WorkspaceEnv::Pinned(workspace_path.clone()),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            PackageDirsError::PinnedWorkspaceDoesNotApply { workspace, module }
                if workspace == workspace_path && module == canonical(project.join("outer/ws/tools"))
        ));
    }

    #[test]
    fn pinned_workspace_rejects_unlisted_module_outside_workspace_root() {
        let project = nested_workspace_under_unrelated_module();
        let workspace_path = canonical(project.join("outer/ws/moon.work"));

        let err = resolve_project_context_from_start_dir(
            project.join("outer"),
            &WorkspaceEnv::Pinned(workspace_path.clone()),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            PackageDirsError::PinnedWorkspaceDoesNotApply { workspace, module }
                if workspace == workspace_path && module == canonical(project.join("outer"))
        ));
    }

    #[test]
    fn pinned_workspace_rejects_unlisted_manifest_path_module() {
        let project = nested_workspace_under_unrelated_module();
        let workspace_path = canonical(project.join("outer/ws/moon.work"));
        let json_module = project.join("outer/ws/json-tool");
        let dsl_module = project.join("outer/ws/dsl-tool");
        write_json_module(&json_module, "alice/json-tool");
        write_file(
            &dsl_module.join(MOON_MOD),
            r#"name = "alice/dsl-tool"

version = "0.1.0"
"#,
        );

        for manifest_path in [json_module.join(MOON_MOD_JSON), dsl_module.join(MOON_MOD)] {
            let err = resolve_project_context_from_manifest_path(
                &manifest_path,
                &WorkspaceEnv::Pinned(workspace_path.clone()),
            )
            .unwrap_err();

            assert!(matches!(
                err,
                PackageDirsError::PinnedWorkspaceDoesNotApply { workspace, module }
                    if workspace == workspace_path && module == canonical(manifest_path.parent().unwrap())
            ));
        }
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
