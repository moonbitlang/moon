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

#![warn(clippy::clone_on_ref_ptr)]

pub mod compile;
pub mod execution;
mod metadata;
pub mod prepare;

use indexmap::IndexMap;
use moonbuild_rupes_recta::{
    ResolvedProject, build_plan::ArtifactKey, target_layout::ArtifactPathResolver,
};
use moonutil::{cond_expr::OptLevel as BuildProfile, target::TargetBackend};
use std::path::PathBuf;

/// Build metadata containing information needed for build context and results.
/// The build graph is kept separate to allow execute_build to take ownership of it.
pub struct BuildMeta {
    /// The project declarations with resolved module and package dependencies.
    pub resolved_project: ResolvedProject,

    /// The list of artifacts that will be produced
    pub artifacts: IndexMap<ArtifactKey, Vec<PathBuf>>,

    /// The backend used to produce the artifacts, without compiler options.
    target_backend: TargetBackend,

    /// The main optimization level used in this compile process
    pub opt_level: BuildProfile,

    /// Physical artifact path resolver selected for this build.
    pub artifact_paths: ArtifactPathResolver,
}

impl BuildMeta {
    pub fn target_backend(&self) -> TargetBackend {
        self.target_backend
    }
}
