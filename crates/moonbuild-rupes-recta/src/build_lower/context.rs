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

//! Lowering context and core implementation.

use std::path::PathBuf;

use log::debug;
use moonutil::mooncakes::{DirSyncResult, result::ResolvedEnv};
use n2::graph::{Build, Graph as N2Graph};
use tracing::{Level, instrument};

use crate::{
    ResolveOutput,
    build_action_plan::{BuildAction, BuildActionId, BuildActionPlan, PlannedArtifact},
    build_lower::artifact::LegacyLayout,
    discover::{DiscoverResult, DiscoveredPackage},
    model::BuildTarget,
    pkg_solve::DepRelationship,
};
use moonutil::BINARIES;

use super::{
    BuildOptions, CommandArgMap, Commandline, LoweringError,
    utils::{build_ins, build_n2_fileloc, build_outs},
};

pub(crate) struct LoweringContext<'a> {
    // What we're building
    pub(crate) graph: N2Graph,

    pub(crate) command_args_by_output: CommandArgMap,

    // folder layout
    pub(crate) layout: LegacyLayout,

    // External state
    pub(crate) packages: &'a DiscoverResult,
    pub(crate) modules: &'a ResolvedEnv,
    pub(crate) module_dirs: &'a DirSyncResult,
    pub(crate) rel: &'a DepRelationship,
    pub(crate) plan: &'a BuildActionPlan<'a>,
    pub(crate) opt: &'a BuildOptions,
}

impl<'a> LoweringContext<'a> {
    pub(super) fn new(
        layout: LegacyLayout,
        resolve_output: &'a ResolveOutput,
        plan: &'a BuildActionPlan<'a>,
        opt: &'a BuildOptions,
    ) -> Self {
        Self {
            graph: N2Graph::default(),
            command_args_by_output: CommandArgMap::new(),
            layout,
            rel: &resolve_output.pkg_rel,
            modules: &resolve_output.module_rel,
            packages: &resolve_output.pkg_dirs,
            module_dirs: &resolve_output.module_dirs,
            plan,
            opt,
        }
    }

    /// Some actions are no-op in n2 build graph. Early bailing.
    fn is_action_noop(&self, action: BuildAction<'_>) -> bool {
        (!self.opt.target_backend.is_native())
            && matches!(action, BuildAction::MakeExecutable { .. })
    }

    pub(super) fn get_package(&self, target: BuildTarget) -> &DiscoveredPackage {
        self.packages.get_package(target.package)
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    pub(super) fn lower_action(&mut self, id: BuildActionId) -> Result<(), LoweringError> {
        let action = self.plan.action(id);
        if self.is_action_noop(action) {
            return Ok(());
        }

        // Lower the action to its commands. This step should be infallible.
        let cmd = match action {
            BuildAction::Check { target, info } => self.lower_check(target, info),
            BuildAction::EmitProof { target, info } => self.lower_emit_proof(target, info),
            BuildAction::Prove { target, info } => self.lower_prove(target, info),
            BuildAction::BuildCore { target, info } => self.lower_build_mbt(target, info),
            BuildAction::BuildCStub {
                package,
                index,
                info,
            } => self.lower_build_c_stub(package, index, info),
            BuildAction::ArchiveOrLinkCStubs { package, info } => {
                self.lower_archive_or_link_c_stubs(id, package, info)
            }
            BuildAction::LinkCore {
                target,
                info,
                make_executable_info,
            } => self.lower_link_core(target, info, make_executable_info),
            BuildAction::MakeExecutable {
                target,
                info: Some(info),
            } => self.lower_make_exe(id, target, info),
            BuildAction::MakeExecutable { info: None, .. } => {
                panic!("native MakeExecutable actions should have executable info")
            }
            BuildAction::GenerateTestInfo { target, info } => {
                self.lower_gen_test_driver(target, info)
            }
            BuildAction::GenerateMbti { target } => self.lower_generate_mbti(target),
            BuildAction::BuildVirtual { package } => self.lower_parse_mbti(package),
            BuildAction::Bundle { module, targets } => self.lower_bundle(module, targets),
            BuildAction::BuildRuntimeLib => self
                .lower_compile_runtime()
                .map_err(LoweringError::RuntimeNativeToolchain)?,
            BuildAction::BuildDocs { module } => self.lower_build_docs(module),
            BuildAction::RunPrebuild { info, .. } => self.lower_run_prebuild(info),
            BuildAction::RunMoonLexPrebuild { package, index } => {
                self.lower_moon_lex_prebuild(package, index)
            }
            BuildAction::RunMoonYaccPrebuild { package, index } => {
                self.lower_moon_yacc_prebuild(package, index)
            }
        };

        // Collect n2 inputs and outputs.
        //
        // MAINTAINERS: some of the inputs and outputs might be calculated
        // twice, once for the commandline and another here. This is currently
        // not a performance concern, but if you have found a way to optimize
        // this, or if you are duplicating a lot of code for it, please refactor.
        let mut ins = vec![];
        for artifact in self.plan.dependency_artifacts(id) {
            self.append_planned_artifact(&artifact, &mut ins);
        }
        ins.extend(cmd.extra_inputs);
        // Track tool binary dependencies so that n2 detects when compilers
        // or other toolchain binaries change (e.g. after a toolchain update)
        // and triggers a rebuild.
        if self.plan.needs_moonc_tool_dep(id) {
            ins.push(BINARIES.moonc.clone());
        }
        ins.sort(); // make sure the order is deterministic
        let ins = build_ins(&mut self.graph, ins);

        let mut output_paths = vec![];
        for artifact in self.plan.output_artifacts(id) {
            self.append_planned_artifact(&artifact, &mut output_paths);
        }
        if let Commandline::Args(args) = &cmd.commandline {
            for output_path in &output_paths {
                self.command_args_by_output
                    .insert(output_path.clone(), args.clone());
            }
        }
        let outs = build_outs(&mut self.graph, output_paths);

        // Construct n2 build node
        let fqn = self
            .plan
            .package_for_error(id)
            .map(|x| self.get_package(x).fqn.clone());
        let mut build = Build::new(
            build_n2_fileloc(self.plan.fileloc(id, self.modules, self.packages)),
            ins,
            outs,
        );
        build.cmdline = Some(cmd.commandline.to_n2_string());
        build.desc = Some(self.plan.human_desc(id, self.modules, self.packages));
        // n2 can't capture and replay command outputs. this is a workaround to
        // avoid losing warnings from `moonc`. According to legacy code, this
        // only triggers for `Check` nodes.
        //
        // FIXME: Revisit for other `moonc` invocations, e.g. `BuildCore`.
        build.can_dirty_on_output = self.plan.can_dirty_on_output(id);

        self.debug_print_command_and_files(id, &build);
        self.lowered(build).map_err(|e| LoweringError::N2 {
            package: fqn.into(),
            action: id,
            source: e,
        })
    }

    /// Append the concrete path(s) corresponding to a planned artifact.
    #[instrument(level = Level::DEBUG, skip(self, out))]
    pub(super) fn append_planned_artifact(
        &self,
        artifact: &PlannedArtifact,
        out: &mut Vec<PathBuf>,
    ) {
        match artifact {
            PlannedArtifact::PackageInterface { producer, target } => {
                self.append_package_interface(*producer, *target, out);
            }
            PlannedArtifact::PackageCoreIr { target, .. } => {
                out.push(self.layout.core_of_build_target(
                    self.packages,
                    target,
                    self.opt.target_backend.into(),
                ));
            }
            PlannedArtifact::ProofInterface { producer, target } => {
                match self.plan.action(*producer) {
                    BuildAction::EmitProof { .. } => {
                        out.push(self.layout.emit_proof_mi_path(self.packages, target));
                    }
                    BuildAction::Prove { .. } => {
                        out.push(self.layout.prove_mi_path(self.packages, target));
                    }
                    _ => panic!("proof interface producer should be a proof action"),
                }
            }
            PlannedArtifact::ProofWhyml { producer, target } => match self.plan.action(*producer) {
                BuildAction::EmitProof { .. } => {
                    out.push(self.layout.emit_proof_whyml_path(self.packages, target));
                }
                BuildAction::Prove { .. } => {
                    out.push(self.layout.prove_whyml_path(self.packages, target));
                }
                _ => panic!("proof whyml producer should be a proof action"),
            },
            PlannedArtifact::ProofReport { target, .. } => {
                out.push(self.layout.prove_report_path(self.packages, target));
            }
            PlannedArtifact::CStubObject { package, index, .. } => {
                let pkg = self.packages.get_package(*package);
                let file_name = &pkg.c_stub_files[*index as usize];
                out.push(
                    self.layout.c_stub_object_path(
                        self.packages,
                        *package,
                        file_name
                            .file_stem()
                            .expect("c stub file should have a file name"),
                        self.opt.target_backend.into(),
                        self.opt.os,
                    ),
                );
            }
            PlannedArtifact::CStubLibrary { package, .. } => {
                if self.opt.use_tcc_run() {
                    out.push(self.layout.c_stub_link_dylib_path(
                        self.packages,
                        *package,
                        self.opt.target_backend.into(),
                        self.opt.os,
                    ));
                } else {
                    out.push(self.layout.c_stub_archive_path(
                        self.packages,
                        *package,
                        self.opt.target_backend.into(),
                        self.opt.os,
                    ));
                }
            }
            PlannedArtifact::LinkedCore { target, .. } => {
                out.push(self.layout.linked_core_of_build_target(
                    self.packages,
                    target,
                    self.opt.target_backend.into(),
                    self.opt.native_target,
                    self.opt.os,
                    self.opt.output_wat,
                ));
            }
            PlannedArtifact::Executable { target, .. } => {
                out.push(self.layout.executable_of_build_target(
                    self.packages,
                    target,
                    self.opt.executable_artifact(true),
                ))
            }
            PlannedArtifact::GeneratedTestDriver { target, .. } => {
                out.push(self.layout.generated_test_driver(
                    self.packages,
                    target,
                    self.opt.target_backend.into(),
                ));
            }
            PlannedArtifact::GeneratedTestMetadata { target, .. } => {
                out.push(self.layout.generated_test_driver_metadata(
                    self.packages,
                    target,
                    self.opt.target_backend.into(),
                ));
            }
            PlannedArtifact::BundleResult { module, .. } => {
                let module_name = self.modules.module_source(*module);
                out.push(
                    self.layout
                        .bundle_result_path(self.opt.target_backend.into(), module_name.name()),
                );
            }
            PlannedArtifact::RuntimeLib { .. } => {
                out.push(self.layout.runtime_output_path(
                    self.opt.target_backend,
                    self.opt.use_tcc_run(),
                    self.opt.os,
                ));
            }
            PlannedArtifact::GeneratedMbti { target, .. } => {
                out.push(self.layout.generated_mbti_path(
                    self.packages,
                    target,
                    self.opt.target_backend.into(),
                ));
            }
            PlannedArtifact::DocsDir { .. } => {
                // The output is a whole folder
                out.push(self.layout.doc_dir())
            }
            PlannedArtifact::VirtualPackageInterface { package, .. } => {
                // The interface generated from `.mbti` is the `.mi` of the source target
                let t = package.build_target(crate::model::TargetKind::Source);
                out.push(self.layout.mi_of_build_target(
                    self.packages,
                    &t,
                    self.opt.target_backend.into(),
                ));
            }
            PlannedArtifact::MoonLexGeneratedSource { package, index, .. } => {
                let pkg_info = self.packages.get_package(*package);
                let mbtlex_file = &pkg_info.mbt_lex_files[*index as usize];
                out.push(mbtlex_file.with_extension("mbt"));
            }
            PlannedArtifact::MoonYaccGeneratedSource { package, index, .. } => {
                let pkg_info = self.packages.get_package(*package);
                let mbtyacc_file = &pkg_info.mbt_yacc_files[*index as usize];
                out.push(mbtyacc_file.with_extension("mbt"));
            }
            PlannedArtifact::KnownPath { path, .. } => out.push(path.clone()),
        }
    }

    pub(super) fn planned_artifact_paths(
        &self,
        artifacts: impl IntoIterator<Item = PlannedArtifact>,
    ) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        for artifact in artifacts {
            self.append_planned_artifact(&artifact, &mut paths);
        }
        paths
    }

    fn append_package_interface(
        &self,
        producer: BuildActionId,
        target: BuildTarget,
        out: &mut Vec<PathBuf>,
    ) {
        match self.plan.action(producer) {
            BuildAction::Check { info, .. } if info.check_mi_against.is_some() => {
                // Generate a `.mi` artifact including the case besides normal
                // cases:
                // * --no-mi is enabled: need add mi to the artifacts so that n2
                // run the command for it and notice in this case, the command
                // will always be executed because it doesn't produce any output
                // so n2 will think it's always dirty.
                // * implementing a virtual package: no need to generate mi
                // though, but still need to declare a `.mi` artifact so that n2
                // executes it. And it actually produces a useless `.mi` file.
                // So that n2 can check its timestamp to decide whether it
                // needs to be rebuilt.
                //
                // Not generating `.mi` for the special case:
                // * moonbitlang/core/abort when working on non-core packages.
                // First, abort is injected as a dependency for every package.
                // When working on core/non-core, abort will have a different
                // PackageId. When working on non-core packages, the mi artifact
                // of abort is not needed, and it avoids checking
                // moonbitlang/core/abort, which is unnecessary. When working on
                // core, the abort mi is returned as `Regular` below. So it will
                // be actually checked.
                match self.layout.mi_of_build_target_impl_virtual(
                    self.packages,
                    &target,
                    self.opt.target_backend.into(),
                ) {
                    crate::build_lower::artifact::MiPathResult::StdAbort(_) => {}
                    crate::build_lower::artifact::MiPathResult::Std(p) => {
                        // this should not happen because there is no
                        // implementation package in stdlib other than abort
                        tracing::warn!(
                            "stdlib mi should not be needed for check as an implementation package: {:?}",
                            p
                        );
                    }
                    crate::build_lower::artifact::MiPathResult::Regular(p) => {
                        out.push(p);
                    }
                }
            }
            BuildAction::Check { .. } | BuildAction::BuildCore { .. } => {
                out.push(self.layout.mi_of_build_target(
                    self.packages,
                    &target,
                    self.opt.target_backend.into(),
                ));
            }
            _ => panic!("package interface producer should be Check or BuildCore"),
        }
    }

    fn lowered(&mut self, build: Build) -> Result<(), anyhow::Error> {
        self.graph.add_build(build)?;
        Ok(())
    }

    /// **For debug use only.** Prints debug information about a lowered action,
    /// the n2 build it's mapped into, and its input and output files.
    #[doc(hidden)]
    fn debug_print_command_and_files(&mut self, action: BuildActionId, build: &Build) {
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
                action, build.cmdline, in_files, out_files
            );
        }
    }
}
