//! This module solves the dependency relationship between packages.

mod model;
mod solve;
mod verify;

use crate::{discover::DiscoverResult, pkg_solve::verify::verify};
use log::info;
use moonutil::mooncakes::result::ResolvedEnv;

pub use model::{DepEdge, DepRelationship, SolveError};
use solve::solve_only;

/// Solves the dependency relationship between packages, and validate the graph
/// is valid for compilation.
pub fn solve(
    modules: &ResolvedEnv,
    packages: &DiscoverResult,
) -> Result<DepRelationship, SolveError> {
    info!("Starting dependency resolution");

    let res = solve_only(modules, packages)?;
    verify(&res, packages)?;

    info!("Dependency resolution completed successfully");
    Ok(res)
}
