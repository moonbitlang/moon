use moonutil::mooncakes::ModuleSource;
use petgraph::prelude::DiGraphMap;

use crate::{
    model::BuildTarget,
    pkg_name::{PackageFQNWithSource, PackagePath},
};

#[derive(Debug, Clone)]
pub struct DepEdge {
    pub shortname: arcstr::Substr,
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
    ImportLoop {
        loop_path: Vec<PackageFQNWithSource>,
    },
}
