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

//! Project, workspace, and target-directory discovery.
//!
//! This is the canonical import surface for code that needs to locate a MoonBit
//! module or workspace before building, resolving dependencies, or packaging.

pub use crate::dirs::{
    DependencySource, ModuleRef, PackageDirs, PackageDirsError, ProjectContext, ProjectManifest,
    ProjectNotFound, ProjectProbe, ProjectQuery, SingleFilePackageDirs, SourceModulePackageDirs,
    SourceTargetDirs, WorkRootSelection, WorkspaceEnv, WorkspaceRef, current_workspace_env,
};
pub use crate::workspace::{
    MoonWork, canonical_workspace_module_dirs, format_workspace_file, read_workspace,
    read_workspace_file, workspace_manifest_path, write_workspace,
};
