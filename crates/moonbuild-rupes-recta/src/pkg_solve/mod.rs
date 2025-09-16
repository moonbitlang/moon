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

//! This module solves the dependency relationship between packages.

mod model;
mod solve;
mod verify;

use crate::{discover::DiscoverResult, pkg_solve::verify::verify};
use log::info;
use moonutil::mooncakes::result::ResolvedEnv;
use tracing::{instrument, Level};

pub use model::{DepEdge, DepRelationship, SolveError};
use solve::solve_only;

/// Solves the dependency relationship between packages, and validate the graph
/// is valid for compilation.
#[instrument(level = Level::DEBUG, skip_all)]
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
