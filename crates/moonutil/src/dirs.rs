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

use crate::constants::{
    BUILD_DIR, DEP_PATH, MOON_BIN_DIR, MOON_MOD, MOON_MOD_JSON, MOON_NO_WORKSPACE, MOON_WORK,
    MOON_WORK_ENV,
};
use crate::user_log::UserLog;
use crate::workspace::{
    MoonWork, PREFERRED_TARGET_DEPRECATION_WARNING, canonical_workspace_module_dirs,
    workspace_manifest_path,
};

const COLOCATED_MODULE_NOT_IN_WORKSPACE_WARNING: &str = "`moon.work` takes precedence over the module manifest in the same directory, but that module is not listed as a workspace member. Add `.` to `members` to select it from the workspace root.";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum WorkspaceEnv {
    #[default]
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
    fn allows_single_file_fallback(&self) -> bool {
        matches!(
            self,
            Self::NotInProject(_) | Self::WorkspaceDisabledNotInModule(_)
        )
    }
}

#[derive(Debug, clap::Parser, Serialize, Deserialize, Clone)]
pub struct SourceTargetDirs {
    /// Change to DIR before doing anything else (must appear before the subcommand). Relative paths in other options and arguments are interpreted relative to DIR. Example: `moon -C a run .` runs the same as invoking `moon run .` from within `a`.
    #[arg(short = 'C', value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// The target directory. Defaults to `<project-root>/_build`, or
    /// `<source-dir>/_build/<file-name>` for a standalone file.
    #[clap(long, global = true)]
    pub target_dir: Option<PathBuf>,
}

impl SourceTargetDirs {
    /// Build a project query from the current directory using an already parsed
    /// workspace environment.
    pub fn query(&self, workspace_env: WorkspaceEnv) -> Result<ProjectQuery, PackageDirsError> {
        self.query_from(Self::current_dir()?, workspace_env)
    }

    /// Build a project query from an explicit start directory and an already
    /// parsed workspace environment.
    pub fn query_from(
        &self,
        start_dir: impl AsRef<Path>,
        workspace_env: WorkspaceEnv,
    ) -> Result<ProjectQuery, PackageDirsError> {
        ProjectQuery::new(self, start_dir.as_ref(), workspace_env)
    }

    pub fn source_root_package_dirs(
        &self,
        source_root: impl AsRef<Path>,
    ) -> Result<PackageDirs, PackageDirsError> {
        let source_dir = dunce::canonicalize(source_root.as_ref())
            .context("failed to resolve source directory")
            .map_err(PackageDirsError::from)?;
        let target_dir = resolve_target_dir(self.target_dir.as_deref(), &source_dir)?;
        let mooncake_bin_dir = target_dir.join(MOON_BIN_DIR);
        let mooncakes_dir = source_dir.join(DEP_PATH);
        Ok(PackageDirs {
            source_dir,
            target_dir,
            mooncake_bin_dir,
            mooncakes_dir,
            project_manifest: ProjectManifest::None,
        })
    }

    pub fn source_module_package_dirs(
        &self,
        source_path: impl AsRef<Path>,
    ) -> Result<Option<SourceModulePackageDirs>, PackageDirsError> {
        let Some(module_root) = find_enclosing_module_root(source_path.as_ref()) else {
            return Ok(None);
        };
        let package_dirs = self.source_root_package_dirs(&module_root)?;
        Ok(Some(SourceModulePackageDirs {
            module_root,
            package_dirs,
        }))
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
        // Keep the complete filename so supported source extensions with the
        // same stem cannot share build state.
        let file_name = file_path
            .file_name()
            .context("file path must have a file name")
            .map_err(PackageDirsError::from)?;
        let package_dirs = self.source_root_package_dirs(source_dir)?;
        let target_dir = prepare_target_dir(package_dirs.target_dir.join(file_name))?;
        let package_dirs = PackageDirs {
            mooncake_bin_dir: target_dir.join(MOON_BIN_DIR),
            target_dir,
            ..package_dirs
        };
        Ok(SingleFilePackageDirs {
            file_path,
            package_dirs,
        })
    }

    pub fn workspace_creation_root(&self) -> Result<PathBuf, PackageDirsError> {
        let start_dir = Self::current_dir()?;
        Ok(self
            .query_from(&start_dir, WorkspaceEnv::Off)?
            .workspace_creation_root())
    }

    pub fn workspace_edit_target(
        &self,
        workspace_env: WorkspaceEnv,
        user_log: &UserLog,
    ) -> Result<WorkspaceEditTarget, PackageDirsError> {
        let start_dir = Self::current_dir()?;
        self.query_from(&start_dir, workspace_env)?
            .workspace_edit_target(user_log)
    }

    fn current_dir() -> Result<PathBuf, PackageDirsError> {
        let start_dir = std::env::current_dir()
            .context("failed to get current directory")
            .map_err(PackageDirsError::from)?;
        dunce::canonicalize(start_dir)
            .context("failed to resolve current directory")
            .map_err(PackageDirsError::from)
    }
}

fn resolve_target_dir(
    configured_target_dir: Option<&Path>,
    project_root: &Path,
) -> Result<PathBuf, PackageDirsError> {
    let target_dir = configured_target_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| project_root.join(BUILD_DIR));
    prepare_target_dir(target_dir)
}

fn prepare_target_dir(target_dir: PathBuf) -> Result<PathBuf, PackageDirsError> {
    if !target_dir.exists() {
        std::fs::create_dir_all(&target_dir)
            .context("failed to create target directory")
            .map_err(PackageDirsError::from)?;
    }
    dunce::canonicalize(target_dir)
        .context("failed to set target directory")
        .map_err(PackageDirsError::from)
}

/// The project manifest selected during project query.
///
/// Downstream stages should treat this as authoritative instead of rediscovering
/// whether a directory belongs to a module or workspace.
#[derive(Debug, PartialEq, Eq)]
pub enum ProjectManifest {
    None,
    Module(PathBuf),
    Workspace(WorkspaceLayout),
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkspaceLayout {
    manifest_path: PathBuf,
    root: PathBuf,
    manifest: MoonWork,
    members: Vec<PathBuf>,
}

impl WorkspaceLayout {
    fn read(manifest_path: PathBuf) -> Result<Self, PackageDirsError> {
        let root = manifest_path
            .parent()
            .context("workspace manifest path has no parent directory")
            .map(Path::to_path_buf)
            .map_err(PackageDirsError::from)?;
        let manifest = MoonWork::read(&manifest_path).map_err(PackageDirsError::from)?;
        let members =
            canonical_workspace_module_dirs(&root, &manifest).map_err(PackageDirsError::from)?;
        Ok(Self {
            manifest_path,
            root,
            manifest,
            members,
        })
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> &MoonWork {
        &self.manifest
    }

    pub fn members(&self) -> &[PathBuf] {
        &self.members
    }

    fn contains_member(&self, module_dir: &Path) -> bool {
        self.members.iter().any(|member| member == module_dir)
    }
}

/// Authoritative directories resolved at the start of a project operation.
///
/// Command setup and nested project handoffs construct this once. Downstream
/// stages should pass it through instead of deriving paths again.
pub struct PackageDirs {
    pub source_dir: PathBuf,
    pub target_dir: PathBuf,
    pub mooncake_bin_dir: PathBuf,
    pub mooncakes_dir: PathBuf,
    pub project_manifest: ProjectManifest,
}

pub struct SelectedProject {
    context: ProjectContext,
    target_dir: Option<PathBuf>,
    workspace: Option<WorkspaceLayout>,
}

impl SelectedProject {
    pub fn context(&self) -> &ProjectContext {
        &self.context
    }

    pub fn package_dirs(self) -> Result<PackageDirs, PackageDirsError> {
        let source_dir = self.context.root().to_path_buf();
        let target_dir = resolve_target_dir(self.target_dir.as_deref(), &source_dir)?;
        let mooncake_bin_dir = target_dir.join(MOON_BIN_DIR);
        let mooncakes_dir = source_dir.join(DEP_PATH);
        let project_manifest = match self.workspace {
            Some(workspace) => ProjectManifest::Workspace(workspace),
            None => ProjectManifest::Module(self.context.manifest_path().to_path_buf()),
        };
        Ok(PackageDirs {
            source_dir,
            target_dir,
            mooncake_bin_dir,
            mooncakes_dir,
            project_manifest,
        })
    }
}

pub struct SingleFilePackageDirs {
    pub file_path: PathBuf,
    pub package_dirs: PackageDirs,
}

pub struct SourceModulePackageDirs {
    pub module_root: PathBuf,
    pub package_dirs: PackageDirs,
}

/// Existing workspace state, or the directory where a maintenance command
/// should create it.
pub enum WorkspaceEditTarget {
    CreateAt(PathBuf),
    Existing(WorkspaceLayout),
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
}

fn has_module_manifest(source_dir: &Path) -> bool {
    source_dir.join(MOON_MOD).exists() || source_dir.join(MOON_MOD_JSON).exists()
}

fn find_enclosing_module_root(source_dir: &Path) -> Option<PathBuf> {
    source_dir
        .ancestors()
        .find(|dir| has_module_manifest(dir))
        .map(|p| p.to_path_buf())
}

pub struct ProjectQuery {
    target_dir: Option<PathBuf>,
    start_dir: PathBuf,
    workspace_env: WorkspaceEnv,
    module_dir: Option<PathBuf>,
    module_manifest_path: Option<PathBuf>,
    workspace: Option<WorkspaceLayout>,
}

fn module_manifest_path(module_dir: &Path) -> PathBuf {
    if module_dir.join(MOON_MOD).exists() {
        module_dir.join(MOON_MOD)
    } else {
        module_dir.join(MOON_MOD_JSON)
    }
}

impl ProjectQuery {
    fn new(
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
        let module_dir = find_enclosing_module_root(&start_dir);
        let module_manifest_path = module_dir.as_deref().map(module_manifest_path);
        let workspace = match &workspace_env {
            WorkspaceEnv::Off => None,
            WorkspaceEnv::Pinned(workspace_path) => {
                let manifest_path = dunce::canonicalize(workspace_path)
                    .context("failed to resolve pinned workspace path")
                    .map_err(PackageDirsError::from)?;
                Some(WorkspaceLayout::read(manifest_path)?)
            }
            WorkspaceEnv::Auto => find_applicable_workspace(&start_dir)?,
        };

        Ok(Self {
            target_dir: source_target_dirs.target_dir.clone(),
            start_dir,
            workspace_env,
            module_dir,
            module_manifest_path,
            workspace,
        })
    }

    pub fn probe_project(&self) -> Result<ProjectProbe, PackageDirsError> {
        let project_context = self.project_context_from_start_dir();

        match project_context {
            Ok(project) => Ok(ProjectProbe::Found(project)),
            Err(error) if error.allows_single_file_fallback() => {
                Ok(ProjectProbe::NotFound(ProjectNotFound { error }))
            }
            Err(error) => Err(error),
        }
    }

    pub fn select(self, user_log: &UserLog) -> Result<SelectedProject, PackageDirsError> {
        match self.probe_project()? {
            ProjectProbe::Found(project) => {
                self.emit_workspace_warnings(user_log);
                Ok(SelectedProject {
                    context: project,
                    target_dir: self.target_dir,
                    workspace: self.workspace,
                })
            }
            ProjectProbe::NotFound(not_found) => Err(not_found.into_error()),
        }
    }

    fn workspace_edit_target(
        self,
        user_log: &UserLog,
    ) -> Result<WorkspaceEditTarget, PackageDirsError> {
        if self.workspace.is_some() {
            // Pinned workspaces still need the same applicability validation as
            // normal project selection before a maintenance command edits them.
            self.project_context_from_start_dir()?;
            self.emit_workspace_warnings(user_log);
            #[expect(
                clippy::unnecessary_unwrap,
                reason = "validation must borrow the workspace before it is moved into the result"
            )]
            return Ok(WorkspaceEditTarget::Existing(
                self.workspace
                    .expect("workspace was present before maintenance selection"),
            ));
        }

        let root = self.workspace_creation_root();
        let Some(manifest_path) = workspace_manifest_path(&root) else {
            return Ok(WorkspaceEditTarget::CreateAt(root));
        };

        // `MOON_WORK=off` disables project selection, but workspace maintenance
        // still operates on an existing moon.work at the local module root.
        let workspace = WorkspaceLayout::read(manifest_path)?;
        if workspace.manifest().preferred_target.is_some() {
            user_log.warn(PREFERRED_TARGET_DEPRECATION_WARNING);
        }
        Ok(WorkspaceEditTarget::Existing(workspace))
    }

    fn workspace_creation_root(self) -> PathBuf {
        self.module_dir.unwrap_or(self.start_dir)
    }

    fn project_context_from_start_dir(&self) -> Result<ProjectContext, PackageDirsError> {
        match &self.workspace_env {
            WorkspaceEnv::Off => {
                self.module_context_with_workspace_disabled(self.start_dir.clone())
            }
            WorkspaceEnv::Pinned(_) => {
                let workspace = self
                    .workspace
                    .as_ref()
                    .expect("pinned workspace discovery must include a workspace layout");
                let module_dir = match self.module_dir.clone() {
                    // When invoked at a pinned workspace root nested under an
                    // unrelated module, the outer module is not the selection.
                    Some(module_dir)
                        if self.start_dir.starts_with(workspace.root())
                            && !module_dir.starts_with(workspace.root()) =>
                    {
                        None
                    }
                    Some(module_dir) => {
                        if workspace.contains_member(&module_dir) {
                            Some(module_dir)
                        } else {
                            return Err(PackageDirsError::PinnedWorkspaceDoesNotApply {
                                workspace: workspace.manifest_path().to_path_buf(),
                                module: module_dir,
                            });
                        }
                    }
                    None => None,
                };

                Ok(ProjectContext::Workspace {
                    root: workspace.root().to_path_buf(),
                    manifest_path: workspace.manifest_path().to_path_buf(),
                    selected_module: module_dir.map(|root| ModuleRef {
                        manifest_path: module_manifest_path(&root),
                        root,
                    }),
                })
            }
            WorkspaceEnv::Auto => {
                if let Some(workspace) = &self.workspace {
                    return Ok(ProjectContext::Workspace {
                        root: workspace.root().to_path_buf(),
                        manifest_path: workspace.manifest_path().to_path_buf(),
                        // Only explicit workspace members may become selected modules.
                        selected_module: match self.module_dir.clone() {
                            Some(root) if workspace.contains_member(&root) => Some(ModuleRef {
                                manifest_path: self
                                    .module_manifest_path
                                    .clone()
                                    .unwrap_or_else(|| module_manifest_path(&root)),
                                root,
                            }),
                            _ => None,
                        },
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

    fn emit_workspace_warnings(&self, user_log: &UserLog) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        if workspace.manifest().preferred_target.is_some() {
            user_log.warn(PREFERRED_TARGET_DEPRECATION_WARNING);
        }
        if matches!(self.workspace_env, WorkspaceEnv::Auto)
            && self.module_dir.as_deref() == Some(workspace.root())
            && !workspace.contains_member(workspace.root())
        {
            user_log.warn(COLOCATED_MODULE_NOT_IN_WORKSPACE_WARNING);
        }
    }
}

#[cfg(test)]
fn project_query_from_start_dir(
    start_dir: PathBuf,
    workspace_env: &WorkspaceEnv,
) -> Result<ProjectQuery, PackageDirsError> {
    SourceTargetDirs {
        cwd: None,
        target_dir: None,
    }
    .query_from(start_dir, workspace_env.clone())
}

#[cfg(test)]
fn resolve_project_context_from_start_dir(
    start_dir: PathBuf,
    workspace_env: &WorkspaceEnv,
) -> Result<ProjectContext, PackageDirsError> {
    let selected = project_query_from_start_dir(start_dir, workspace_env)?
        .select(&UserLog::new(log::LevelFilter::Error))?;
    Ok(selected.context)
}

pub fn current_workspace_env() -> anyhow::Result<(WorkspaceEnv, Option<&'static str>)> {
    let moon_work = std::env::var_os(MOON_WORK_ENV);
    let moon_no_workspace = std::env::var_os(MOON_NO_WORKSPACE);
    // TODO: Remove `MOON_NO_WORKSPACE` compatibility after the deprecation window.
    let warning = workspace_env_deprecation_warning(moon_work.as_ref(), moon_no_workspace.as_ref());
    let workspace_env = parse_workspace_env(moon_work, moon_no_workspace)?;
    Ok((workspace_env, warning))
}

fn workspace_env_deprecation_warning(
    moon_work: Option<&OsString>,
    moon_no_workspace: Option<&OsString>,
) -> Option<&'static str> {
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
fn find_applicable_workspace(
    source_dir: &Path,
) -> Result<Option<WorkspaceLayout>, PackageDirsError> {
    let mut module_root = None;

    for dir in source_dir.ancestors() {
        let Some(workspace_path) = workspace_manifest_path(dir) else {
            if module_root.is_none() && has_module_manifest(dir) {
                module_root = Some(dir);
            }
            continue;
        };

        let workspace = WorkspaceLayout::read(workspace_path)?;

        if let Some(module_root) = module_root {
            if workspace
                .members()
                .iter()
                .any(|member_dir| member_dir == module_root)
            {
                return Ok(Some(workspace));
            }
            continue;
        }

        return Ok(Some(workspace));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{
        PackageDirsError, ProjectContext, ProjectManifest, ProjectProbe, SourceTargetDirs,
        WorkspaceEnv, parse_workspace_env, project_query_from_start_dir,
        resolve_project_context_from_start_dir,
    };
    use crate::constants::{DEP_PATH, MOON_BIN_DIR, MOON_MOD, MOON_MOD_JSON};
    use std::{
        ffi::OsString,
        path::{Path, PathBuf},
    };

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

    fn nested_workspace_under_unrelated_module() -> tempfile::TempDir {
        let project = tempfile::tempdir().expect("create test project");
        write_json_module(&project.path().join("outer"), "alice/outer");
        write_file(
            &project.path().join("outer/ws/moon.work"),
            r#"members = [
  "./app",
]
"#,
        );
        write_json_module(&project.path().join("outer/ws/app"), "alice/app");
        project
    }

    #[test]
    fn package_dirs_follow_source_and_target_roots() {
        let project = tempfile::tempdir().expect("create test project");
        let target = project.path().join("tmp-target");
        let dirs = SourceTargetDirs {
            cwd: None,
            target_dir: Some(target),
        }
        .source_root_package_dirs(project.path())
        .unwrap();
        assert_eq!(dirs.mooncakes_dir, canonical(project.path()).join(DEP_PATH));
        assert_eq!(
            dirs.mooncake_bin_dir,
            canonical(project.path().join("tmp-target")).join(MOON_BIN_DIR)
        );
    }

    #[test]
    fn single_file_package_dirs_scope_target_to_complete_filename() {
        let project = tempfile::tempdir().expect("create test project");
        write_file(&project.path().join("first.mbt"), "fn main {}\n");
        write_file(&project.path().join("second.mbtx"), "fn main {}\n");
        let source_target_dirs = SourceTargetDirs {
            cwd: None,
            target_dir: None,
        };

        let first = source_target_dirs
            .single_file_package_dirs(project.path().join("first.mbt"))
            .unwrap();
        let second = source_target_dirs
            .single_file_package_dirs(project.path().join("second.mbtx"))
            .unwrap();

        assert_eq!(
            first.package_dirs.target_dir,
            canonical(project.path().join("_build/first.mbt"))
        );
        assert_eq!(
            second.package_dirs.target_dir,
            canonical(project.path().join("_build/second.mbtx"))
        );
        assert_ne!(
            first.package_dirs.target_dir,
            second.package_dirs.target_dir
        );
    }

    #[test]
    fn single_file_package_dirs_scope_configured_target_to_filename() {
        let project = tempfile::tempdir().expect("create test project");
        write_file(
            &project.path().join("main.mbt.md"),
            "```mbt\nfn main {}\n```\n",
        );

        let dirs = SourceTargetDirs {
            cwd: None,
            target_dir: Some(project.path().join("target")),
        }
        .single_file_package_dirs(project.path().join("main.mbt.md"))
        .unwrap();

        assert_eq!(
            dirs.package_dirs.target_dir,
            canonical(project.path().join("target/main.mbt.md"))
        );
        assert_eq!(
            dirs.package_dirs.mooncake_bin_dir,
            dirs.package_dirs.target_dir.join(MOON_BIN_DIR)
        );
    }

    #[test]
    fn auto_selection_preserves_dsl_module_manifest_path() {
        let project = tempfile::tempdir().expect("create test project");
        write_file(
            &project.path().join(MOON_MOD),
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
        assert_eq!(manifest_path, canonical(project.path().join(MOON_MOD)));

        write_file(
            &project.path().join("moon.work"),
            r#"members = [
  ".",
]
"#,
        );
        let selection = resolve_project_context_from_start_dir(
            project.path().to_path_buf(),
            &WorkspaceEnv::Auto,
        )
        .unwrap();
        let ProjectContext::Workspace {
            selected_module: Some(selected_module),
            ..
        } = selection
        else {
            panic!("expected workspace context with selected module");
        };
        assert_eq!(
            selected_module.manifest_path,
            canonical(project.path().join(MOON_MOD))
        );
    }

    #[test]
    fn project_probe_reports_not_found_without_project() {
        let project = tempfile::tempdir().expect("create test project");
        let query = project_query_from_start_dir(project.path().to_path_buf(), &WorkspaceEnv::Auto)
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
    fn selected_workspace_uses_the_layout_loaded_during_query() {
        let project = tempfile::tempdir().expect("create test project");
        write_file(
            &project.path().join("moon.work"),
            r#"members = [
  "./app",
]
"#,
        );
        write_json_module(&project.path().join("app"), "alice/app");

        let query =
            project_query_from_start_dir(project.path().join("app"), &WorkspaceEnv::Auto).unwrap();

        // Once discovery has selected a workspace, later phases consume that
        // exact layout instead of reopening moon.work.
        write_file(&project.path().join("moon.work"), "invalid = [");
        let dirs = query
            .select(&crate::user_log::UserLog::new(log::LevelFilter::Error))
            .unwrap()
            .package_dirs()
            .unwrap();

        let ProjectManifest::Workspace(workspace) = dirs.project_manifest else {
            panic!("expected workspace project manifest");
        };
        assert_eq!(
            workspace.members(),
            &[canonical(project.path().join("app"))]
        );
    }

    #[test]
    fn pinned_workspace_root_under_unrelated_outer_module_succeeds() {
        let project = nested_workspace_under_unrelated_module();
        let workspace_path = canonical(project.path().join("outer/ws/moon.work"));

        let selection = resolve_project_context_from_start_dir(
            project.path().join("outer/ws"),
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
        assert_eq!(root, canonical(project.path().join("outer/ws")));
        assert_eq!(selected_module, None);
        assert_eq!(manifest_path, workspace_path);
    }

    #[test]
    fn pinned_workspace_rejects_unlisted_module_under_workspace_root() {
        let project = nested_workspace_under_unrelated_module();
        let workspace_path = canonical(project.path().join("outer/ws/moon.work"));
        write_json_module(&project.path().join("outer/ws/tools"), "alice/tools");

        let err = resolve_project_context_from_start_dir(
            project.path().join("outer/ws/tools"),
            &WorkspaceEnv::Pinned(workspace_path.clone()),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            PackageDirsError::PinnedWorkspaceDoesNotApply { workspace, module }
                if workspace == workspace_path
                    && module == canonical(project.path().join("outer/ws/tools"))
        ));
    }

    #[test]
    fn pinned_workspace_rejects_unlisted_module_outside_workspace_root() {
        let project = nested_workspace_under_unrelated_module();
        let workspace_path = canonical(project.path().join("outer/ws/moon.work"));

        let err = resolve_project_context_from_start_dir(
            project.path().join("outer"),
            &WorkspaceEnv::Pinned(workspace_path.clone()),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            PackageDirsError::PinnedWorkspaceDoesNotApply { workspace, module }
                if workspace == workspace_path && module == canonical(project.path().join("outer"))
        ));
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
