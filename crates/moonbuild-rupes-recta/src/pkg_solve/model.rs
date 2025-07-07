use moonutil::mooncakes::ModuleSource;
use petgraph::prelude::DiGraphMap;

use crate::{
    model::{BuildTarget, TargetKind},
    pkg_name::{PackageFQNWithSource, PackagePath},
};

#[derive(Debug, Clone)]
pub struct DepEdge {
    /// The short alias for this import item. This should be unique among all
    /// imports available to the current build target.
    pub short_alias: String,
    /// The kind of the import, whether it's a imported for source, test, or others.
    pub kind: TargetKind,
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
}
