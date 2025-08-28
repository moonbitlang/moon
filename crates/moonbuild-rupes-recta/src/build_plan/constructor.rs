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

//! Core build plan construction logic.

use std::collections::HashSet;

use log::debug;

use crate::{
    discover::DiscoverResult,
    model::{BuildPlanNode, BuildTarget, TargetKind},
    pkg_solve::DepRelationship,
};

use super::{BuildEnvironment, BuildPlan, BuildPlanConstructError};

/// The struct responsible for holding the states and dependencies used during
/// the construction of a build plan.
pub(super) struct BuildPlanConstructor<'a> {
    // Input environment
    pub(super) packages: &'a DiscoverResult,
    pub(super) build_deps: &'a DepRelationship,
    pub(super) build_env: &'a BuildEnvironment,

    /// The resulting build plan
    pub(super) res: BuildPlan,

    /// Currently pending nodes that need to be processed.
    pub(super) pending: Vec<BuildPlanNode>,
    pub(super) resolved: HashSet<BuildPlanNode>,
}

impl<'a> BuildPlanConstructor<'a> {
    pub(super) fn new(
        packages: &'a DiscoverResult,
        build_deps: &'a DepRelationship,
        build_env: &'a BuildEnvironment,
    ) -> Self {
        Self {
            packages,
            build_deps,
            build_env,
            res: BuildPlan::default(),
            pending: Vec::new(),
            resolved: HashSet::new(),
        }
    }

    pub(super) fn finish(self) -> BuildPlan {
        self.res
    }

    pub(super) fn build(
        &mut self,
        input: impl Iterator<Item = BuildPlanNode>,
    ) -> Result<(), BuildPlanConstructError> {
        assert!(
            self.pending.is_empty(),
            "Pending nodes should be empty before starting the build"
        );

        // Add the input node to the pending list
        for i in input {
            if self.should_skip_start_node(i) {
                continue;
            }
            self.need_node(i);
            self.res.input_nodes.push(i);
        }

        while let Some(node) = self.pending.pop() {
            // check if the node is already resolved
            if self.resolved.contains(&node) {
                // Already resolved, skip
                continue;
            }

            self.build_action_dependencies(node)?;
        }
        Ok(())
    }

    /// Determine whether this starting node should be skipped based on rules.
    ///
    /// This function currently handles:
    /// - Skipping nodes of no real use:
    ///   - Whitebox test nodes with no white box test files
    ///
    /// # Note
    ///
    /// Currently, removal of invalid starting nodes due to standard library
    /// special cases is handled in [`crate::compile`], not here. Whether we
    /// should merge the two functions is a subject of discussion.
    fn should_skip_start_node(&mut self, node: BuildPlanNode) -> bool {
        if let Some(tgt) = node.extract_target() {
            if tgt.kind == TargetKind::WhiteboxTest {
                // check if we actually have whitebox test files
                self.populate_target_info(tgt);
                let info = self
                    .res
                    .get_build_target_info(&tgt)
                    .expect("just populated");
                if info.whitebox_files.is_empty() {
                    // No whitebox test files, skip this node
                    debug!(
                        "Skipping whitebox test node {:?} with no whitebox files",
                        tgt
                    );
                    return true;
                }
            }
        }

        false
    }

    /// Tell the build graph that we need to calculate the graph portion of a
    /// new node. To deduplicate pending nodes, this should be called before
    /// adding relevant edges to the graph (since the latter will also add the
    /// node into the graph).
    pub(super) fn need_node(&mut self, node: BuildPlanNode) -> BuildPlanNode {
        if !self.resolved.contains(&node) {
            self.pending.push(node);
            self.res.graph.add_node(node);
        }
        node
    }

    /// Tell the build graph that the given node has been resolved into a
    /// concrete action specification.
    pub(super) fn resolved_node(&mut self, node: BuildPlanNode) {
        debug_assert!(
            !self.resolved.contains(&node),
            "Node {:?} should not be resolved twice",
            node
        );
        debug_assert!(
            self.res.graph.contains_node(node),
            "Node {:?} should be in the graph before resolving",
            node
        );
        self.resolved.insert(node);

        // Ensure the resolved data is present in the build plan.
        // Panics if the node is not present in the resolved data.
        self.ensure_resolved(node);
    }

    fn ensure_resolved(&self, node: BuildPlanNode) {
        match node {
            BuildPlanNode::Check(build_target)
            | BuildPlanNode::BuildCore(build_target)
            | BuildPlanNode::GenerateTestInfo(build_target) => {
                assert!(
                    self.res.build_target_infos.contains_key(&build_target),
                    "Build target info for {:?} should be present when resolving node {:?}",
                    build_target,
                    node
                );
            }
            BuildPlanNode::BuildCStubs(build_target) => {
                assert!(
                    self.res.c_stubs_info.contains_key(&build_target),
                    "C stubs info for {:?} should be present when resolving node {:?}",
                    build_target,
                    node
                );
            }
            BuildPlanNode::LinkCore(build_target) => {
                assert!(
                    self.res.link_core_info.contains_key(&build_target),
                    "Link core info for {:?} should be present when resolving node {:?}",
                    build_target,
                    node
                );
            }
            BuildPlanNode::MakeExecutable(build_target) => {
                assert!(
                    self.res.make_executable_info.contains_key(&build_target),
                    "Make executable info for {:?} should be present when resolving node {:?}",
                    build_target,
                    node
                );
            }
            BuildPlanNode::GenerateMbti(_build_target) => (),
            BuildPlanNode::Bundle(_module_id) => (),
            BuildPlanNode::BuildRuntimeLib => (),
        }
    }

    pub(super) fn add_edge(&mut self, start: BuildPlanNode, end: BuildPlanNode) {
        self.res.graph.add_edge(start, end, ());
    }

    /// Calculate the build action's dependencies and insert relevant edges to the
    /// build action graph.
    fn build_action_dependencies(
        &mut self,
        node: BuildPlanNode,
    ) -> Result<(), BuildPlanConstructError> {
        match node {
            BuildPlanNode::Check(target) => self.build_check(node, target),
            BuildPlanNode::BuildCore(target) => self.build_build(node, target),
            BuildPlanNode::BuildCStubs(target) => self.build_build_c_stubs(node, target),
            BuildPlanNode::LinkCore(_) => {
                panic!(
                    "Link core should not appear in the wild without \
                    accompanied by MakeExecutable. Anytime it is met in the \
                    pending list, it should be already resolved."
                )
            }
            BuildPlanNode::MakeExecutable(target) => self.build_make_exec_link_core(node, target),
            BuildPlanNode::GenerateTestInfo(target) => self.build_gen_test_info(node, target),
            BuildPlanNode::Bundle(module_id) => self.build_bundle(node, module_id),
            BuildPlanNode::BuildRuntimeLib => self.build_runtime_lib(node),
            BuildPlanNode::GenerateMbti(target) => self.build_generate_mbti(node, target),
        }
    }

    /// Populate the target info for the given target, if not already present.
    pub(super) fn populate_target_info(&mut self, target: BuildTarget) {
        if self.res.build_target_infos.contains_key(&target) {
            // Already populated
            return;
        }

        // Resolve the source files
        let info = self.resolve_mbt_files_for_node(target);
        self.res.build_target_infos.insert(target, info);
    }
}
