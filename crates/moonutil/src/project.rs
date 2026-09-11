// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Project, workspace, and target-directory discovery.
//!
//! This is the canonical import surface for code that needs to locate a MoonBit
//! module or workspace before building, resolving dependencies, or packaging.

pub use crate::dirs::{
    ModuleRef, PackageDirs, PackageDirsError, ProjectContext, ProjectManifest, ProjectNotFound,
    ProjectProbe, ProjectQuery, SelectedProject, SingleFilePackageDirs, SourceModulePackageDirs,
    SourceTargetDirs, WorkspaceEditTarget, WorkspaceEnv, WorkspaceLayout, current_workspace_env,
};
pub use crate::workspace::{
    MoonWork, canonical_workspace_module_dirs, workspace_manifest_path, write_workspace,
};
