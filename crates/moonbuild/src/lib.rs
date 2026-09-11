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

#![warn(clippy::clone_on_ref_ptr)]

pub mod bench;
pub mod benchmark;
pub mod doc_http;
pub mod entry;
pub mod execution;
pub mod expect;
pub mod new;
pub mod runtest;
pub mod section_capture;
pub mod test_utils;
pub mod upgrade;

use indexmap::IndexMap;
use moonbuild_rupes_recta::{
    ResolveOutput, build_plan::ArtifactKey, model::BackendConfig,
    target_layout::ArtifactPathResolver,
};
use moonutil::{cond_expr::OptLevel as BuildProfile, target::TargetBackend};
use std::path::PathBuf;

/// Build metadata containing information needed for build context and results.
/// The build graph is kept separate to allow execute_build to take ownership of it.
pub struct BuildMeta {
    /// The result of the resolve step, containing package metadata
    pub resolve_output: ResolveOutput,

    /// The list of artifacts that will be produced
    pub artifacts: IndexMap<ArtifactKey, Vec<PathBuf>>,

    /// The backend and backend-specific configuration used by this build.
    pub backend: BackendConfig,

    /// The main optimization level used in this compile process
    pub opt_level: BuildProfile,

    /// Physical artifact path resolver selected for this build.
    pub artifact_paths: ArtifactPathResolver,
}

impl BuildMeta {
    pub fn target_backend(&self) -> TargetBackend {
        self.backend.target_backend()
    }
}
