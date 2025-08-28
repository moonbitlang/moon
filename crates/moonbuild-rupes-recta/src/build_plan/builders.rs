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

//! Individual build methods for different node types.

use indexmap::{set::MutableValues, IndexSet};
use log::{debug, trace};
use petgraph::visit::DfsPostOrder;

use crate::{
    cond_comp::{self, CompileCondition},
    model::{BuildPlanNode, BuildTarget, TargetKind},
};

use super::{
    constructor::BuildPlanConstructor, BuildCStubsInfo, BuildPlanConstructError, BuildTargetInfo,
    LinkCoreInfo, MakeExecutableInfo,
};

impl<'a> BuildPlanConstructor<'a> {
    pub(super) fn build_check(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        // Check depends on `.mi` of all dependencies, which practically
        // means the Check of all dependencies.
        for dep in self
            .build_deps
            .dep_graph
            .neighbors_directed(target, petgraph::Direction::Outgoing)
        {
            let dep_node = self.need_node(BuildPlanNode::Check(dep));
            self.add_edge(node, dep_node);
        }

        self.populate_target_info(target);
        self.resolved_node(node);

        Ok(())
    }

    pub(super) fn build_build(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        // Build depends on `.mi`` of all dependencies. Although Check can
        // also emit `.mi` files, since we're building, this action actually
        // means we need to build all dependencies.
        self.need_node(node);
        for dep in self
            .build_deps
            .dep_graph
            .neighbors_directed(target, petgraph::Direction::Outgoing)
        {
            let dep_node = self.need_node(BuildPlanNode::BuildCore(dep));
            self.add_edge(node, dep_node);
        }

        // If the given target is a test, we will also need to generate the test driver.
        if target.kind.is_test() {
            let gen_test_info = BuildPlanNode::GenerateTestInfo(target);
            self.need_node(gen_test_info);
            self.add_edge(node, gen_test_info);
        }

        self.populate_target_info(target);
        self.resolved_node(node);

        Ok(())
    }

    pub(super) fn build_gen_test_info(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        self.need_node(node);

        self.populate_target_info(target);
        self.resolved_node(node);
        Ok(())
    }

    pub(super) fn resolve_mbt_files_for_node(&self, target: BuildTarget) -> BuildTargetInfo {
        use crate::cond_comp::FileTestKind::*;
        use TargetKind::*;

        // FIXME: Should we resolve test drivers' paths, or should we leave it
        // in the lowering phase? The path to the test driver depends on the
        // artifact layout, so we might not be able to do that here, unless we
        // add some kind of `SpecialFile::TestDriver` or something.
        let pkg = self.packages.get_package(target.package);
        let compile_condition = CompileCondition {
            optlevel: self.build_env.opt_level,
            test_kind: target.kind.into(),
            backend: self.build_env.target_backend,
        };
        let source_files = cond_comp::filter_files(
            &pkg.raw,
            pkg.source_files.iter().map(|x| x.as_path()),
            &compile_condition,
        );

        let mut regular_files = vec![];
        let mut whitebox_files = vec![];
        let mut doctest_files = vec![];
        for (file, file_kind) in source_files {
            match (target.kind, file_kind) {
                (Source | SubPackage | InlineTest, NoTest) => regular_files.push(file.to_owned()),

                (WhiteboxTest, NoTest) => regular_files.push(file.to_owned()),
                (WhiteboxTest, Whitebox) => whitebox_files.push(file.to_owned()),

                (BlackboxTest, Blackbox) => regular_files.push(file.to_owned()),
                (BlackboxTest, NoTest) => doctest_files.push(file.to_owned()),

                _ => panic!(
                    "Unexpected file kind {:?} for target {:?} in package {}, \
                    this is a bug in the build system!",
                    file_kind, target, pkg.fqn
                ),
            }
        }

        BuildTargetInfo {
            regular_files,
            whitebox_files,
            doctest_files,
        }
    }

    pub(super) fn build_build_c_stubs(
        &mut self,
        node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        // Depends on nothing, but anyway needs to be inserted into the graph.
        self.need_node(node);

        // Resolve the C stub files
        let pkg = self.packages.get_package(target.package);
        let c_source = pkg.c_stub_files.clone();

        let c_info = BuildCStubsInfo { c_stubs: c_source };
        self.res.c_stubs_info.insert(target, c_info);
        self.resolved_node(node);

        Ok(())
    }

    /// Performs the construction of two actions in consecutive: Make Executable
    /// and Link Core.
    ///
    /// The two actions are always created together (Link Core is always a
    /// direct dependency of Make Executable, and there's no other actions that
    /// depends on Link Core), and both actions require traversing through the
    /// list of dependencies, so it's better to create both nodes at once,
    /// instead of in separate functions.
    pub(super) fn build_make_exec_link_core(
        &mut self,
        make_exec_node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        /*
            Link-core requires traversing all output of the current package's
            all transitive dependencies, and emitting them in DFS post-order.

            There are a couple of replacements needed to be done when the
            traversal completes:
            - Whitebox tests need to replace the normal package in the
                dependency graph (at the same position as the normal package).
                This is technically a circular dependency but anyway :)
            - Virtual package overrides need to replace their overridden
                packages in the dependency graph. This is done by not adding
                virtual packages at all when collecting the targets.
                TODO: virtual packages are not yet implemented here.
        */

        debug!("Building MakeExecutable for target: {:?}", target);
        debug!("Performing DFS post-order traversal to collect dependencies");
        // This DFS is shared by both LinkCore and MakeExecutable actions.
        let mut dfs = DfsPostOrder::new(&self.build_deps.dep_graph, target);
        // This is the link core sources
        let mut link_core_deps = IndexSet::new();
        // This is the C stub sources
        let mut c_stub_deps = Vec::new();
        // DFS itself
        while let Some(next) = dfs.next(&self.build_deps.dep_graph) {
            if next.kind == TargetKind::WhiteboxTest {
                // Replace whitebox tests, if any
                let source_target = next.package.build_target(TargetKind::Source);
                if let Some(source_idx) = link_core_deps.get_index_of(&source_target) {
                    let source_mut = link_core_deps
                        .get_index_mut2(source_idx)
                        .expect("Source index is valid");
                    *source_mut = next;
                    continue;
                } else {
                    // No source target found, resort to regular path
                }
            }

            // Regular package
            link_core_deps.insert(next);
            // If there's any C stubs, add it (native only)
            let pkg = self.packages.get_package(next.package);
            trace!("DFS post iterated: {}", pkg.fqn);
            if self.build_env.target_backend.is_native() && !pkg.c_stub_files.is_empty() {
                c_stub_deps.push(next);
            }
        }

        let link_core_node = self.need_node(BuildPlanNode::LinkCore(target));

        // Add edges to all dependencies
        // Note that we have already replaced unnecessary dependencies
        for target in &link_core_deps {
            let dep_node = BuildPlanNode::BuildCore(*target);
            self.need_node(dep_node);
            self.add_edge(link_core_node, dep_node);
        }

        let targets = link_core_deps.into_iter().collect::<Vec<_>>();
        let link_core_info = LinkCoreInfo {
            linked_order: targets.clone(),
            // std: self.build_env.std, // TODO: move to per-package
        };
        self.res.link_core_info.insert(target, link_core_info);

        self.resolved_node(link_core_node);

        // Add edge from make exec to link core
        self.add_edge(make_exec_node, link_core_node);

        // Add dependencies of make exec
        for target in &c_stub_deps {
            let dep_node = self.need_node(BuildPlanNode::BuildCStubs(*target));
            self.add_edge(make_exec_node, dep_node);
        }
        let c_stub_deps = c_stub_deps.into_iter().collect::<Vec<_>>();
        self.res.make_executable_info.insert(
            target,
            MakeExecutableInfo {
                link_c_stubs: c_stub_deps.clone(),
            },
        );

        // Native backends also needs a runtime library
        if self.build_env.target_backend.is_native() {
            let rt_node = self.need_node(BuildPlanNode::BuildRuntimeLib);
            self.add_edge(make_exec_node, rt_node);
        }

        self.resolved_node(make_exec_node);

        Ok(())
    }

    pub(super) fn build_bundle(
        &mut self,
        _node: BuildPlanNode,
        module_id: moonutil::mooncakes::ModuleId,
    ) -> Result<(), BuildPlanConstructError> {
        // Bundling a module gathers the build result of all its non-virtual packages
        for &pkg_id in self
            .packages
            .packages_for_module(module_id)
            .expect("Module should exist")
            .values()
        {
            let pkg = self.packages.get_package(pkg_id);
            if pkg.raw.virtual_pkg.is_some() {
                continue;
            }

            let build_node = BuildPlanNode::BuildCore(BuildTarget {
                package: pkg_id,
                kind: TargetKind::Source,
            });
            self.need_node(build_node);
            self.add_edge(_node, build_node);
        }

        Ok(())
    }

    pub(super) fn build_runtime_lib(
        &mut self,
        _node: BuildPlanNode,
    ) -> Result<(), BuildPlanConstructError> {
        // Nothing specific to do here ;)
        self.resolved_node(_node);
        Ok(())
    }

    pub(super) fn build_generate_mbti(
        &mut self,
        _node: BuildPlanNode,
        target: BuildTarget,
    ) -> Result<(), BuildPlanConstructError> {
        // Generate mbti relies on the `.mi` files spitted out by `moonc`, which
        // usually means `moonc check` instead of `moonc build`.
        let check_node = self.need_node(BuildPlanNode::Check(target));
        self.add_edge(_node, check_node);
        self.resolved_node(_node);
        Ok(())
    }
}
