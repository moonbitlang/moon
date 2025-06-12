use moonutil::mooncakes::ModuleSource;
use petgraph::prelude::DiGraphMap;

use crate::{
    model::BuildTarget,
    pkg_name::{PackageFQNWithSource, PackagePath},
};

#[derive(Debug, Clone)]
pub struct DepEdge {
    /// The short alias for this import item. This should be unique among all
    /// imports available to the current build target.
    pub short_alias: String,
}

/// The dependency relationship between build targets
#[derive(Clone, Debug, Default)]
pub struct DepRelationship {
    pub dep_graph: DiGraphMap<BuildTarget, DepEdge>,
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
        "Conflicting import alias '{alias}' found in package {package_fqn}. \
        Both {first_import} and {second_import} use the same alias"
    )]
    ConflictingImportAlias {
        alias: String,
        package_fqn: PackageFQNWithSource,
        first_import: PackageFQNWithSource,
        second_import: PackageFQNWithSource,
    },
}
