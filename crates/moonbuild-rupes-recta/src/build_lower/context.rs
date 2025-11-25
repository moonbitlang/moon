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

//! Build plan lowering context and core implementation.

use std::path::PathBuf;

use log::debug;
use moonutil::mooncakes::{DirSyncResult, result::ResolvedEnv};
use n2::graph::{Build, Graph as N2Graph};
use tracing::{Level, instrument};

use crate::{
    ResolveOutput,
    build_lower::artifact::LegacyLayout,
    build_plan::{BuildPlan, FileDependencyKind},
    discover::{DiscoverResult, DiscoveredPackage},
    model::{BuildPlanNode, BuildTarget, RunBackend},
    pkg_solve::DepRelationship,
};

use super::{
    BuildOptions, LoweringError,
    utils::{build_ins, build_n2_fileloc, build_outs},
};

pub(crate) struct BuildPlanLowerContext<'a> {
    // What we're building
    pub(crate) graph: N2Graph,

    // folder layout
    pub(crate) layout: LegacyLayout,

    // External state
    pub(crate) packages: &'a DiscoverResult,
    pub(crate) modules: &'a ResolvedEnv,
    pub(crate) module_dirs: &'a DirSyncResult,
    pub(crate) rel: &'a DepRelationship,
    pub(crate) build_plan: &'a BuildPlan,
    pub(crate) opt: &'a BuildOptions,
}

impl<'a> BuildPlanLowerContext<'a> {
    pub(super) fn new(
        layout: LegacyLayout,
        resolve_output: &'a ResolveOutput,
        build_plan: &'a BuildPlan,
        opt: &'a BuildOptions,
    ) -> Self {
        Self {
            graph: N2Graph::default(),
            layout,
            rel: &resolve_output.pkg_rel,
            modules: &resolve_output.module_rel,
            packages: &resolve_output.pkg_dirs,
            module_dirs: &resolve_output.module_dirs,
            build_plan,
            opt,
        }
    }

    /// Some nodes are no-op in n2 build graph. Early bailing.
    fn is_node_noop(&self, node: BuildPlanNode) -> bool {
        (!self.opt.target_backend.is_native()) && matches!(node, BuildPlanNode::MakeExecutable(_))
    }

    pub(super) fn get_package(&self, target: BuildTarget) -> &DiscoveredPackage {
        self.packages.get_package(target.package)
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_node(&mut self, node: BuildPlanNode) -> Result<(), LoweringError> {
        if self.is_node_noop(node) {
            return Ok(());
        }

        // Lower the action to its commands. This step should be infallible.
        let cmd = match node {
            BuildPlanNode::Check(target) => {
                let info = self
                    .build_plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for Check nodes");
                self.lower_check(node, target, info)
            }
            BuildPlanNode::BuildCore(target) => {
                let info = self
                    .build_plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for BuildCore nodes");
                self.lower_build_mbt(node, target, info)
            }
            BuildPlanNode::BuildCStub(target, index) => {
                let info = self
                    .build_plan
                    .get_c_stubs_info(target)
                    .expect("C stub info should be present for BuildCStub nodes");
                self.lower_build_c_stub(target, index, info)
            }
            BuildPlanNode::ArchiveOrLinkCStubs(_target) => {
                let info = self
                    .build_plan
                    .get_c_stubs_info(_target)
                    .expect("C stubs info should be present for BuildCStubs nodes");
                self.lower_archive_or_link_c_stubs(node, _target, info)
            }
            BuildPlanNode::LinkCore(target) => {
                let info = self
                    .build_plan
                    .get_link_core_info(&target)
                    .expect("Link core info should be present for LinkCore nodes");
                self.lower_link_core(node, target, info)
            }
            BuildPlanNode::MakeExecutable(target) => {
                let info = self
                    .build_plan
                    .get_make_executable_info(&target)
                    .expect("Make executable info should be present for MakeExecutable nodes");
                self.lower_make_exe(target, info)
            }
            BuildPlanNode::GenerateMbti(target) => self.lower_generate_mbti(target),
            BuildPlanNode::BuildVirtual(target) => self.lower_parse_mbti(node, target),
            BuildPlanNode::Bundle(module_id) => self.lower_bundle(node, module_id),
            BuildPlanNode::GenerateTestInfo(target) => {
                let info = self
                    .build_plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for GenerateTestInfo nodes");
                self.lower_gen_test_driver(node, target, info)
            }
            BuildPlanNode::BuildRuntimeLib => self.lower_compile_runtime(),
            BuildPlanNode::BuildDocs => self.lower_build_docs(),
            BuildPlanNode::RunPrebuild(pkg, idx) => self.lower_run_prebuild(pkg, idx),
            BuildPlanNode::RunMoonLexPrebuild(pkg, idx) => self.lower_moon_lex_prebuild(pkg, idx),
            BuildPlanNode::RunMoonYaccPrebuild(pkg, idx) => self.lower_moon_yacc_prebuild(pkg, idx),
        };

        // Collect n2 inputs and outputs.
        //
        // TODO: some of the inputs and outputs might be calculated twice,
        // once for the commandline and another here. Will this hurt perf?
        let mut ins = vec![];
        for (n, edge) in self.build_plan.dependency_edges(node) {
            self.append_artifact_of(n, edge, &mut ins);
        }
        ins.extend(cmd.extra_inputs);
        ins.sort(); // make sure the order is deterministic
        let ins = build_ins(&mut self.graph, ins);

        let mut outs = vec![];
        self.append_all_artifacts_of(node, &mut outs);
        let outs = build_outs(&mut self.graph, outs);

        // Construct n2 build node
        let fqn = node
            .extract_target()
            .map(|x| self.get_package(x).fqn.clone());
        let mut build = Build::new(
            build_n2_fileloc(node.string_id(self.modules, self.packages)),
            ins,
            outs,
        );
        build.cmdline = Some(cmd.commandline.to_n2_string());
        // n2 can't capture and replay command outputs. this is a workaround to
        // avoid losing warnings from `moonc`. According to legacy code, this
        // only triggers for `Check` nodes.
        //
        // FIXME: Revisit for other `moonc` invocations, e.g. `BuildCore`.
        build.can_dirty_on_output = matches!(node, BuildPlanNode::Check(_));

        self.debug_print_command_and_files(node, &build);
        self.lowered(build).map_err(|e| LoweringError::N2 {
            package: fqn.into(),
            node,
            source: e,
        })
    }

    /// Append the output artifacts of the given node to the provided vector.
    #[instrument(level = Level::DEBUG, skip(self, out))]
    pub(super) fn append_artifact_of(
        &self,
        node: BuildPlanNode,
        edge: FileDependencyKind,
        out: &mut Vec<PathBuf>,
    ) {
        match node {
            BuildPlanNode::Check(target) => {
                let info = self
                    .build_plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for Check nodes");

                if !info.no_mi() && info.check_mi_against.is_none() {
                    out.push(self.layout.mi_of_build_target(
                        self.packages,
                        &target,
                        self.opt.target_backend.into(),
                    ));
                }
            }
            BuildPlanNode::BuildCore(target) => {
                let info = self
                    .build_plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for BuildCore nodes");
                let (mi, core) = match edge {
                    FileDependencyKind::BuildCore { mi, core } => (mi, core),
                    _ => (true, true),
                };
                if mi && info.check_mi_against.is_none() && !info.no_mi() && !target.kind.is_test()
                {
                    out.push(self.layout.mi_of_build_target(
                        self.packages,
                        &target,
                        self.opt.target_backend.into(),
                    ));
                }
                if core {
                    out.push(self.layout.core_of_build_target(
                        self.packages,
                        &target,
                        self.opt.target_backend.into(),
                    ));
                }
            }
            BuildPlanNode::BuildCStub(package, index) => {
                let pkg = self.packages.get_package(package);
                let file_name = &pkg.c_stub_files[index as usize];
                out.push(
                    self.layout.c_stub_object_path(
                        self.packages,
                        package,
                        file_name
                            .file_stem()
                            .expect("c stub file should have a file name"),
                        self.opt.target_backend.into(),
                        self.opt.os,
                    ),
                );
            }
            BuildPlanNode::ArchiveOrLinkCStubs(_target) => {
                if self.opt.target_backend == RunBackend::NativeTccRun {
                    out.push(self.layout.c_stub_link_dylib_path(
                        self.packages,
                        _target,
                        self.opt.target_backend.into(),
                        self.opt.os,
                    ));
                } else {
                    out.push(self.layout.c_stub_archive_path(
                        self.packages,
                        _target,
                        self.opt.target_backend.into(),
                        self.opt.os,
                    ));
                }
            }
            BuildPlanNode::LinkCore(target) => {
                out.push(self.layout.linked_core_of_build_target(
                    self.packages,
                    &target,
                    self.opt.target_backend.into(),
                    self.opt.os,
                    self.opt.output_wat,
                ));
            }
            BuildPlanNode::MakeExecutable(target) => {
                out.push(self.layout.executable_of_build_target(
                    self.packages,
                    &target,
                    self.opt.target_backend,
                    self.opt.os,
                    true,
                    self.opt.output_wat,
                ))
            }
            BuildPlanNode::GenerateTestInfo(target) => {
                let meta = if let FileDependencyKind::GenerateTestInfo { meta } = edge {
                    meta
                } else {
                    true
                };
                out.push(self.layout.generated_test_driver(
                    self.packages,
                    &target,
                    self.opt.target_backend.into(),
                ));
                if meta {
                    out.push(self.layout.generated_test_driver_metadata(
                        self.packages,
                        &target,
                        self.opt.target_backend.into(),
                    ));
                }
            }
            BuildPlanNode::Bundle(id) => {
                let module_name = self.modules.mod_name_from_id(id);
                out.push(
                    self.layout
                        .bundle_result_path(self.opt.target_backend.into(), module_name.name()),
                );
            }
            BuildPlanNode::BuildRuntimeLib => {
                out.push(
                    self.layout
                        .runtime_output_path(self.opt.target_backend, self.opt.os),
                );
            }
            BuildPlanNode::GenerateMbti(_target) => {
                out.push(self.layout.generated_mbti_path(
                    self.packages,
                    &_target,
                    self.opt.target_backend.into(),
                ));
            }
            BuildPlanNode::BuildDocs => {
                // The output is a whole folder
                out.push(self.layout.doc_dir())
            }
            BuildPlanNode::RunPrebuild(pkg, idx) => {
                let cfg = self
                    .build_plan
                    .get_prebuild_info(pkg, idx)
                    .expect("Prebuild info should be populated before lowering run prebuild");
                out.extend(cfg.resolved_outputs.iter().cloned());
            }
            BuildPlanNode::BuildVirtual(_target) => {
                // The interface generated from `.mbti` is the `.mi` of the source target
                let t = _target.build_target(crate::model::TargetKind::Source);
                out.push(self.layout.mi_of_build_target(
                    self.packages,
                    &t,
                    self.opt.target_backend.into(),
                ));
            }
            BuildPlanNode::RunMoonLexPrebuild(pkg, idx) => {
                // FIXME: The output path logic should match that in build_plan/builders.rs
                let pkg_info = self.packages.get_package(pkg);
                let mbtlex_file = &pkg_info.mbt_lex_files[idx as usize];
                out.push(mbtlex_file.with_extension("mbt"));
            }
            BuildPlanNode::RunMoonYaccPrebuild(pkg, idx) => {
                let pkg_info = self.packages.get_package(pkg);
                let mbtyacc_file = &pkg_info.mbt_yacc_files[idx as usize];
                out.push(mbtyacc_file.with_extension("mbt"));
            }
        }
    }

    /// Convenience alias for depending on all artifacts from a node.
    #[inline]
    pub(super) fn append_all_artifacts_of(&self, node: BuildPlanNode, out: &mut Vec<PathBuf>) {
        self.append_artifact_of(node, FileDependencyKind::AllFiles, out);
    }

    fn lowered(&mut self, build: Build) -> Result<(), anyhow::Error> {
        self.graph.add_build(build)?;
        Ok(())
    }

    /// **For debug use only.** Prints debug information about a specific build
    /// plan node, the n2 build it's mapped into, and the input and output files
    /// of it.
    #[doc(hidden)]
    fn debug_print_command_and_files(&mut self, node: BuildPlanNode, build: &Build) {
        if log::log_enabled!(log::Level::Debug) {
            let in_files = build
                .ins
                .ids
                .iter()
                .map(|id| {
                    &self
                        .graph
                        .files
                        .by_id
                        .lookup(*id)
                        .expect("Input file should exist")
                        .name
                })
                .collect::<Vec<_>>();
            let out_files = build
                .outs
                .ids
                .iter()
                .map(|id| {
                    &self
                        .graph
                        .files
                        .by_id
                        .lookup(*id)
                        .expect("Output file should exist")
                        .name
                })
                .collect::<Vec<_>>();

            debug!(
                "lowered: {:?}\n into {:?};\n ins: {:?};\n outs: {:?}",
                node, build.cmdline, in_files, out_files
            );
        }
    }
}
