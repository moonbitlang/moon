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

//! This module solves the dependency relationship between packages.

mod model;
mod solve;
mod verify;

use crate::{
    discover::DiscoverResult,
    pkg_solve::verify::{compute_realizable_supported_targets, verify},
};
use log::info;
use moonutil::{resolution::ResolvedEnv, target::TargetBackend, user_log::UserLog};
use tracing::{Level, instrument};

pub use model::{DepEdge, DepRelationship, SolveError};
use solve::solve_only;

/// Solves the dependency relationship between packages, and validate the graph
/// is valid for compilation.
#[instrument(level = Level::DEBUG, skip_all)]
pub fn solve(
    modules: &ResolvedEnv,
    packages: &DiscoverResult,
    enable_coverage: bool,
    backend: TargetBackend,
    user_log: &UserLog,
) -> Result<DepRelationship, SolveError> {
    info!("Starting dependency resolution");

    let mut res = solve_only(modules, packages, enable_coverage, backend, user_log)?;
    verify(&res, packages, user_log)?;
    res.realizable_supported_targets = compute_realizable_supported_targets(&res, packages);

    info!("Dependency resolution completed successfully");
    Ok(res)
}
