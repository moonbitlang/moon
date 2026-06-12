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

//! Realized paths for logical build products.

use std::{collections::HashMap, path::PathBuf};

use crate::{
    ResolveOutput,
    build_action_plan::{BuildActionPlan, PlannedArtifact},
    build_lower::BuildOptions,
    target_layout::ArtifactPathResolver,
};

pub(crate) struct ProductTable {
    paths_by_artifact: HashMap<PlannedArtifact, Vec<PathBuf>>,
}

impl ProductTable {
    pub(crate) fn new(
        artifact_paths: &ArtifactPathResolver,
        resolve_output: &ResolveOutput,
        plan: &BuildActionPlan<'_>,
        opt: &BuildOptions,
    ) -> Self {
        let mut paths_by_artifact = HashMap::new();
        for action in plan.action_ids() {
            for artifact in plan.output_artifacts(action) {
                let paths = artifact_paths.paths_for_planned_artifact(
                    &artifact,
                    plan,
                    &resolve_output.pkg_dirs,
                    &resolve_output.module_rel,
                    opt.artifact_path_options(),
                );
                paths_by_artifact.insert(artifact, paths);
            }
        }
        Self { paths_by_artifact }
    }

    pub(crate) fn paths(&self, artifact: &PlannedArtifact) -> &[PathBuf] {
        self.paths_by_artifact
            .get(artifact)
            .unwrap_or_else(|| panic!("planned artifact should be realized: {artifact:?}"))
    }
}
