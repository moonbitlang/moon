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

use std::collections::{HashMap, HashSet};

use crate::{
    ResolveOutput,
    build_plan::{FileDependencyKind, InputDirective},
    model::{BuildPlanNode, BuildTarget},
    prebuild::PrebuildOutput,
};
use tracing::{Level, debug, instrument};

use super::{BuildEnvironment, BuildPlan, BuildPlanConstructError};

/// The struct responsible for holding the states and dependencies used during
/// the construction of a build plan.
pub(super) struct BuildPlanConstructor<'a> {
    // Input environment
    pub(super) input: &'a ResolveOutput,
    pub(super) build_env: &'a BuildEnvironment,
    pub(super) input_directive: &'a InputDirective,
    pub(super) prebuild_config: Option<&'a PrebuildOutput>,

    /// The resulting build plan
    pub(super) res: BuildPlan,

    /// Currently pending nodes that need to be processed.
    pub(super) pending: Vec<BuildPlanNode>,
    pub(super) resolved: HashSet<BuildPlanNode>,

    /// Debug-only: record call-sites that requested each node via `need_node`.
    /// Used to improve diagnostics when dependency construction panics.
    /// Only compiled in debug builds (cfg(debug_assertions)).
    #[cfg(debug_assertions)]
    pub(super) need_node_sources: HashMap<BuildPlanNode, Vec<(&'static str, u32, u32)>>,
}

impl<'a> BuildPlanConstructor<'a> {
    pub(super) fn new(
        resolved: &'a ResolveOutput,
        build_env: &'a BuildEnvironment,
        input_directive: &'a InputDirective,
        prebuild_config: Option<&'a PrebuildOutput>,
    ) -> Self {
        Self {
            input: resolved,
            build_env,
            input_directive,
            prebuild_config,

            res: BuildPlan::default(),
            pending: Vec::new(),
            resolved: HashSet::new(),
            #[cfg(debug_assertions)]
            need_node_sources: HashMap::new(),
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

        // Add the input nodes to the pending list
        for i in input {
            self.need_node(i);
            self.res.input_nodes.push(i);
        }

        while let Some(node) = self.pending.pop() {
            // check if the node is already resolved
            if self.resolved.contains(&node) {
                // Already resolved, skip
                continue;
            }

            // Debug builds: wrap dependency computation in a panic-reporting shim.
            // If a panic occurs, we log the need_node call-sites for this node and
            // then resume unwinding to preserve the original backtrace.
            #[cfg(debug_assertions)]
            {
                self.build_action_dependencies_with_panic_report(node)?;
            }

            // Release builds: keep the hot path clean — no catch_unwind, no extra logging.
            #[cfg(not(debug_assertions))]
            {
                self.build_action_dependencies(node)?;
            }
        }

        self.postprocess_coalesce();

        Ok(())
    }

    /// Coalesce redundant nodes as a postprocess step.
    ///
    /// `BuildCore(...)` and `Check(...)` both produce `.mi` files, so having
    /// both in the graph will cause later stages to not know which one to use,
    /// and result in an error. This function moves all edges from `Check(...)`
    /// nodes to their corresponding `BuildCore(...)` nodes, if they exist. This
    /// is also a fix for the virtual package semantics, because virtual
    /// packages don't know if they will be built or checked.
    fn postprocess_coalesce(&mut self) {
        // list of nodes to coalesce and their input/output edges
        let mut plan = HashMap::new();
        for node in self.res.all_nodes() {
            if let BuildPlanNode::Check(build_target) = node {
                // Coalesce to BuildCore if it exists
                if self
                    .res
                    .graph
                    .contains_node(BuildPlanNode::BuildCore(build_target))
                {
                    debug!("Coalescing Check node {:?} to BuildCore", node);
                    let in_edges = self
                        .res
                        .graph
                        .edges_directed(node, petgraph::Incoming)
                        .map(|(source, _, _)| source)
                        .collect::<Vec<_>>();
                    let out_edges = self
                        .res
                        .graph
                        .edges_directed(node, petgraph::Outgoing)
                        .map(|(_, target, &edge)| (target, edge))
                        .collect::<Vec<_>>();
                    plan.insert(
                        node,
                        (BuildPlanNode::BuildCore(build_target), in_edges, out_edges),
                    );
                }
            }
        }

        // Perform the coalescing
        for (&from, (to, in_edges, out_edges)) in &plan {
            let to = *to;
            // Input edges
            for &source in in_edges {
                // Check if source is also coalesced
                let source = if let Some((new_source, _, _)) = plan.get(&source) {
                    *new_source
                } else {
                    source
                };

                // Insert or update the edge
                if let Some(w) = self.res.graph.edge_weight_mut(source, to) {
                    if let FileDependencyKind::BuildCore { mi, .. } = w {
                        *mi = true
                    } else {
                        // Already depending on all files
                    }
                } else {
                    // Insert the edge
                    self.res.graph.add_edge(
                        source,
                        to,
                        crate::build_plan::FileDependencyKind::BuildCore {
                            mi: true,
                            core: false,
                        },
                    );
                }
            }

            // Output edges
            for &(target, edge) in out_edges {
                // Skip if target is also coalesced -- handled in its own iteration
                if plan.contains_key(&target) {
                    continue;
                }

                self.res.graph.add_edge(to, target, edge);
            }
            self.res.graph.remove_node(from);
        }
    }

    /// Tell the build graph that we need to calculate the graph portion of a
    /// new node. To deduplicate pending nodes, this should be called before
    /// adding relevant edges to the graph (since the latter will also add the
    /// node into the graph).
    // #[track_caller] lets us capture the call-site (file:line:column) of need_node
    // so we can report where a node was requested if a later dependency build panics.
    #[cfg_attr(debug_assertions, track_caller)]
    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn need_node(&mut self, node: BuildPlanNode) -> BuildPlanNode {
        #[cfg(debug_assertions)]
        {
            // Record the call-site that scheduled this node for later diagnostics.
            let loc = std::panic::Location::caller();
            self.need_node_sources.entry(node).or_default().push((
                loc.file(),
                loc.line(),
                loc.column(),
            ));
            // Emit a debug log so we can correlate scheduling with later panics in logs.
            tracing::debug!(
                target: "build_plan",
                ?node,

                "need_node called"
            );
        }

        if !self.resolved.contains(&node) {
            self.pending.push(node);
            self.res.graph.add_node(node);
        }
        node
    }

    /// Tell the build graph that the given node has been resolved into a
    /// concrete action specification.
    #[instrument(level = Level::DEBUG, skip(self))]
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
                let pkg = self.input.pkg_dirs.get_package(build_target.package);
                if pkg.has_implementation() {
                    assert!(
                        self.res.build_target_infos.contains_key(&build_target),
                        "Build target info for {:?} should be present when resolving node {:?}",
                        build_target,
                        node
                    );
                }
            }
            BuildPlanNode::BuildCStub(build_target, _)
            | BuildPlanNode::ArchiveCStubs(build_target) => {
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
            BuildPlanNode::Bundle(module_id) => {
                assert!(
                    self.res.bundle_info.contains_key(&module_id),
                    "Bundle info for module {:?} should be present when resolving node {:?}",
                    module_id,
                    node
                );
            }
            BuildPlanNode::BuildRuntimeLib => (),
            BuildPlanNode::BuildDocs => (),
            BuildPlanNode::RunPrebuild(pkg, idx) => {
                assert!(
                    self.res.prebuild_info.contains_key(&pkg),
                    "Prebuild info for package {:?} should be present when resolving node {:?}",
                    pkg,
                    node
                );
                let v = &self.res.prebuild_info[&pkg];
                assert!(
                    (idx as usize) < v.len() && v[idx as usize].is_some(),
                    "Prebuild info for package {:?} index {} should be present when resolving node {:?}",
                    pkg,
                    idx,
                    node
                );
            }
            BuildPlanNode::BuildVirtual(_build_target) => (),
        }
    }

    /// Add an edge from `start` to `end`, depending on all files produced by `end`.
    pub(super) fn add_edge(&mut self, start: BuildPlanNode, end: BuildPlanNode) {
        self.res
            .graph
            .add_edge(start, end, FileDependencyKind::AllFiles);
    }

    /// Add an edge from `start` to `end`, depending on specific files produced by `end`.
    pub(super) fn add_edge_spec(
        &mut self,
        start: BuildPlanNode,
        end: BuildPlanNode,
        edge: FileDependencyKind,
    ) {
        // verify edge kind
        match (edge, end) {
            (FileDependencyKind::BuildCore { .. }, BuildPlanNode::BuildCore(..)) => {}
            (FileDependencyKind::BuildCore { .. }, _) => {
                panic!("BuildCore edge can only point to BuildCore node")
            }
            _ => (),
        }

        self.res.graph.add_edge(start, end, edge);
    }

    /// Debug-only helper that runs build_action_dependencies with panic capture and reporting.
    /// - On panic, logs all call-sites that scheduled the node via need_node, then resumes unwind
    ///   to keep the original panic and backtrace intact.
    /// - Keeps release path and performance unchanged.
    #[cfg(debug_assertions)]
    fn build_action_dependencies_with_panic_report(
        &mut self,
        node: BuildPlanNode,
    ) -> Result<(), BuildPlanConstructError> {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        match catch_unwind(AssertUnwindSafe(|| self.build_action_dependencies(node))) {
            Ok(r) => r,
            Err(payload) => {
                // Report source call-sites for this node to aid debugging
                if let Some(sources) = self.need_node_sources.get(&node) {
                    tracing::error!(target: "build_plan", ?node, "build_action_dependencies panicked for node; below are need_node call-sites:");
                    for (file, line, column) in sources {
                        tracing::error!(
                            target: "build_plan",
                            file = *file,
                            line = *line,
                            column = *column,
                            "need_node at {file}:{line}:{column}"
                        );
                    }
                } else {
                    tracing::error!(target: "build_plan", ?node, "build_action_dependencies panicked for node; no need_node sources recorded");
                }
                std::panic::resume_unwind(payload);
            }
        }
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
            BuildPlanNode::BuildCStub(target, index) => {
                self.build_build_c_stub(node, target, index)
            }
            BuildPlanNode::ArchiveCStubs(target) => self.build_link_c_stubs(node, target),
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
            BuildPlanNode::BuildDocs => self.build_build_docs(node),
            BuildPlanNode::RunPrebuild(package_id, index) => {
                self.build_run_prebuild(node, package_id, index)
            }
            BuildPlanNode::BuildVirtual(target) => self.build_parse_mbti(node, target),
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
