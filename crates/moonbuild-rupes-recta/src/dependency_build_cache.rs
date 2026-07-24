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
//! resolution facts, and explicit input identities form the portable
//! dependency-graph identity.

use std::path::PathBuf;

use n2::graph::BuildId;

#[derive(Debug, Clone)]
pub struct DependencyBuildInput {
    pub label: String,
    pub path: PathBuf,
    pub identity: InputIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputIdentity {
    /// The label is sufficient because another identity fact covers the
    /// immutable bytes, such as a prepared registry archive checksum.
    Logical,
    /// Hash the physical file bytes and include the digest.
    Content,
    /// Resolve an executable through `PATH` when necessary, then hash its
    /// bytes. The physical installation path is not part of the identity.
    Tool,
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
    pub kind: DependencyBuildKind,
    pub package: String,
    pub working_directory: String,
    pub canonical_args: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub resolution: Vec<DependencyResolution>,
    pub inputs: Vec<DependencyBuildInput>,
    pub outputs: Vec<DependencyBuildOutput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DependencyBuildKind {
    MooncBuildCore,
    CStubObject,
    CStubLibrary,
    NativeRuntime,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DependencyResolution {
    pub module: String,
    pub version: String,
    pub source_checksum: Option<String>,
}
