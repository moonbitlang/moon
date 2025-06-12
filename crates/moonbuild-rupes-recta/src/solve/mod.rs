//! This module solves the dependency relationship between packages.

mod model;
mod solve;

use crate::discover::DiscoverResult;
use log::info;
use moonutil::mooncakes::result::ResolvedEnv;

use model::{DepRelationship, SolveError};
use solve::solve_only;

/// Solves the dependency relationship between packages, and validate the graph
/// is valid for compilation.
pub fn solve(
    modules: &ResolvedEnv,
    packages: &DiscoverResult,
) -> Result<DepRelationship, SolveError> {
    info!("Starting dependency resolution");

    let res = solve_only(modules, packages)?;
    // TODO: validate

    info!("Dependency resolution completed successfully");
    Ok(res)
}
