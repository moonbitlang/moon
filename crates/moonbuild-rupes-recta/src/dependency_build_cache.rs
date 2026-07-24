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

//! Registry dependency build actions that may be prepared before a standalone
//! script's project-local build graph.
//!
//! Physical paths are invocation-local adapters. Labels, canonical arguments,
//! resolution facts, and the contents of inputs marked `hash_content` form the
//! portable dependency-graph identity.

use std::path::PathBuf;

use n2::graph::BuildId;

#[derive(Debug, Clone)]
pub struct DependencyBuildInput {
    pub label: String,
    pub path: PathBuf,
    pub hash_content: bool,
}

#[derive(Debug, Clone)]
pub struct DependencyBuildOutput {
    pub label: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DependencyBuildAction {
    pub build_id: BuildId,
    pub description: DependencyBuildDescription,
}

#[derive(Debug, Clone)]
pub struct DependencyBuildDescription {
    pub package: String,
    pub canonical_args: Vec<String>,
    pub resolution: Vec<String>,
    pub inputs: Vec<DependencyBuildInput>,
    pub outputs: Vec<DependencyBuildOutput>,
}
