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

use std::collections::HashMap;

use indexmap::IndexSet;
use moonutil::resolution::ModuleSource;
use moonutil::target::TargetBackend;
use petgraph::prelude::DiGraphMap;
use slotmap::SparseSecondaryMap;

use crate::{
    model::{BuildTarget, PackageId, TargetKind},
    pkg_name::{PackageFQN, PackageFQNWithSource, PackagePath},
};

#[derive(Debug, Clone)]
pub struct DepEdge {
    /// The named alias for this import item. Named aliases must be unique among
    /// imports available to the current build target.
    pub short_alias: arcstr::Substr,
    /// Whether this dependency imports all names unqualified in addition to
    /// exposing the package through `short_alias`.
    pub import_all: bool,
    /// The kind of the import, whether it's a imported for source, test, or others.
    pub kind: TargetKind,
}

/// Represents resolved virtual package user information.
#[derive(Clone, Debug, Default)]
pub struct VirtualUser {
    /// The list of virtual package overridings this user applies.
    ///
    /// This is a map from the virtual package to the actual package that
    /// implements it. If the virtual package has a default, it is not included
    /// in this map.
    pub overrides: SparseSecondaryMap<PackageId, PackageId>,
}

/// Imports between package build targets, virtual-package associations, and backend support.
#[derive(Clone, Debug, Default)]
pub struct PackageRelations {
    /// A graph with build targets as nodes and dependency relationship as edges.
    ///
    /// The edges should point from dependent (downstream) to dependency (upstream).
    pub dep_graph: DiGraphMap<BuildTarget, DepEdge>,

    /// A map from package to the resolved virtual packages it uses, if any.
    ///
    /// If a package uses virtual packages but does not have an entry in this
    /// map, it means it uses the default implementations of all virtual
    /// packages. This is because a virtual package with default implementation
    /// is the same as a normal package.
    pub virtual_users: SparseSecondaryMap<PackageId, VirtualUser>,

    /// A map from package to the virtual package it implements, if any.
    pub virt_impl: SparseSecondaryMap<PackageId, PackageId>,

    /// Per-build-target supported backends after dependency propagation.
    ///
    /// This is the realizable backend set of each build target:
    /// `declared(package) ∩ realizable(dep1) ∩ realizable(dep2) ...`.
    /// The set may be stricter than package-level `supported_targets`.
    pub realizable_supported_targets: HashMap<BuildTarget, IndexSet<TargetBackend>>,
}

#[derive(Debug, thiserror::Error)]
pub enum PackageResolutionError {
    #[error("Cannot find import '{import}' in {package_fqn}")]
    ImportNotFound {
        import: String,
        package_fqn: PackageFQNWithSource,
    },

    #[error(
        "Import {import} exists in global environment, 
        but its containing module is not imported by {module}, \
        thus cannot be imported by its package '{pkg}'"
    )]
    ImportNotImportedByModule {
        import: PackageFQNWithSource,
        module: ModuleSource,
        pkg: PackagePath,
    },

    #[error("Import loop detected: {loop_path}")]
    ImportLoop { loop_path: ImportLoop },

    #[error(
        "Conflicting import alias: \
        Both {first_import} ({first_import_kind:?}) and {second_import} ({second_import_kind:?}) \
        are imported into {package_fqn} with the same alias '{alias}'."
    )]
    ConflictingImportAlias {
        alias: String,
        package_node: BuildTarget,
        package_fqn: PackageFQNWithSource,
        first_import_node: BuildTarget,
        first_import: PackageFQNWithSource,
        first_import_kind: TargetKind,
        second_import_node: BuildTarget,
        second_import: PackageFQNWithSource,
        second_import_kind: TargetKind,
    },

    #[error(
        "Package {package} tries to import {dependency}, \
        but the latter is an implementation of a virtual package, \
        and thus cannot be imported directly."
    )]
    CannotImportVirtualImplementation {
        package: PackageFQNWithSource,
        dependency: PackageFQNWithSource,
    },

    #[error(
        "Package {package} declares implementation target {implements}, \
        but it is not a virtual package."
    )]
    ImplementTargetNotVirtual {
        package: PackageFQNWithSource,
        implements: PackageFQNWithSource,
    },

    #[error(
        "Package {package} declares a virtual override {virtual_override}, \
        but that package is not implementing a virtual package."
    )]
    OverrideNotImplementor {
        package: PackageFQNWithSource,
        virtual_override: PackageFQNWithSource,
    },

    #[error(
        "Virtual package {virtual_pkg} is overridden twice in package {package}: \
        first by {first_override} and again by {second_override}."
    )]
    VirtualOverrideConflict {
        package: PackageFQNWithSource,
        virtual_pkg: PackageFQNWithSource,
        first_override: PackageFQNWithSource,
        second_override: PackageFQNWithSource,
    },

    #[error(
        "Cannot import internal package {dependency} in {importer} \
        due to internal visibility rules"
    )]
    InternalImportForbidden {
        importer_node: BuildTarget,
        importer: PackageFQNWithSource,
        dependency_node: BuildTarget,
        dependency: PackageFQNWithSource,
    },

    #[error("Multiple errors occurred during package solving:\n{0}")]
    Multiple(MultipleError),
}

#[derive(Debug, thiserror::Error)]
pub struct MultipleError(pub Vec<PackageResolutionError>);

impl std::fmt::Display for MultipleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, err) in self.0.iter().enumerate() {
            writeln!(f, "Error {}: {}", i + 1, err)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ImportLoop(pub Vec<PackageFQN>);

impl std::fmt::Display for ImportLoop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, pkg) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, " -> ")?;
            }
            write!(f, "{}", pkg)?;
        }
        Ok(())
    }
}
