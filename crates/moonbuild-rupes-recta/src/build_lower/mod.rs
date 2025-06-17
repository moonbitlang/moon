//! Lowers a [Build plan](crate::build_plan) into `n2`'s Build graph

use crate::build_plan::BuildPlan;

mod artifact;
mod compiler;

/// Lowers a [`BuildPlan`] into a n2 [Build Graph](n2::graph::Graph).
pub fn lower_build_plan(build_plan: &BuildPlan) -> n2::graph::Graph {
    todo!()
}
