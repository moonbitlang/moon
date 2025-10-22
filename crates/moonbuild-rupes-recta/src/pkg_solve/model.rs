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

use moonutil::mooncakes::ModuleSource;
use petgraph::prelude::DiGraphMap;
use slotmap::SparseSecondaryMap;

use crate::{
    model::{BuildTarget, PackageId, TargetKind},
    pkg_name::{PackageFQNWithSource, PackagePath},
};

#[derive(Debug, Clone)]
pub struct DepEdge {
    /// The short alias for this import item. This should be unique among all
    /// imports available to the current build target.
    pub short_alias: arcstr::Substr,
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

/// The dependency relationship between build targets
#[derive(Clone, Debug, Default)]
pub struct DepRelationship {
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
}

#[derive(Debug, thiserror::Error)]
pub enum SolveError {
    #[error(
        "Duplicated package name found across all packages currently found. \
        The first one is found in {first}, \
        and the second one is found in {second}"
    )]
    DuplicatedPackageFQN {
        first: PackageFQNWithSource,
        second: PackageFQNWithSource,
    },

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

    #[error("Import loop detected: {loop_path:?}")]
    ImportLoop { loop_path: Vec<BuildTarget> },

    #[error(
        "Conflicting import alias '{alias}' found \
        in package {package_fqn} ({package_node:?}). \
        Both {first_import_node:?} {first_import} (in {first_import_kind:?} import) \
        and {second_import_node:?} {second_import} (in {second_import_kind:?} import) \
        use the same alias."
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
        "Forbidden internal import: package {importer} ({importer_node:?}) \
cannot import {dependency} ({dependency_node:?}) due to internal visibility rules"
    )]
    InternalImportForbidden {
        importer_node: BuildTarget,
        importer: PackageFQNWithSource,
        dependency_node: BuildTarget,
        dependency: PackageFQNWithSource,
    },

    #[error("Multiple errors occurred during package solving: {0}")]
    Multiple(MultipleError),
}

#[derive(Debug, thiserror::Error)]
pub struct MultipleError(pub Vec<SolveError>);

impl std::fmt::Display for MultipleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, err) in self.0.iter().enumerate() {
            writeln!(f, "Error {}: {}", i + 1, err)?;
        }
        Ok(())
    }
}
