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
use moonutil::toolchain::BINARIES;

use super::{
    BuildOptions, CommandArgMap, Commandline, LoweringError,
    products::ProductTable,
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
    pub(crate) products: ProductTable,
}

pub(super) struct ActionProducts {
    outputs: Vec<RealizedArtifact>,
    dependencies: Vec<RealizedArtifact>,
}

struct RealizedArtifact {
    artifact: PlannedArtifact,
    paths: Vec<PathBuf>,
}

impl ActionProducts {
    fn new(plan: &BuildActionPlan<'_>, products: &ProductTable, action: BuildActionId) -> Self {
        let outputs = Self::realize(plan.output_artifacts(action), products);
        let dependencies = Self::realize(plan.dependency_artifacts(action), products);
        Self {
            outputs,
            dependencies,
        }
    }

    fn realize(
        artifacts: impl IntoIterator<Item = PlannedArtifact>,
        products: &ProductTable,
    ) -> Vec<RealizedArtifact> {
        artifacts
            .into_iter()
            .map(|artifact| RealizedArtifact {
                paths: products.paths(&artifact).to_vec(),
                artifact,
            })
            .collect()
    }

    pub(super) fn dependency_paths(&self) -> Vec<PathBuf> {
        Self::paths(&self.dependencies)
    }

    pub(super) fn output_paths(&self) -> Vec<PathBuf> {
        Self::paths(&self.outputs)
    }

    pub(super) fn single_output_path(&self) -> PathBuf {
        match self.outputs.as_slice() {
            [artifact] => Self::single_realized_path(artifact),
            [] => unreachable!("expected exactly one output artifact"),
            _ => unreachable!(
                "expected one output artifact, got {:?}",
                self.output_artifacts()
            ),
        }
    }

    pub(super) fn single_output_path_matching(
        &self,
        matches: impl Fn(&PlannedArtifact) -> bool,
    ) -> PathBuf {
        self.optional_single_output_path_matching(matches)
            .unwrap_or_else(|| unreachable!("expected one matching output artifact"))
    }

    pub(super) fn optional_single_output_path_matching(
        &self,
        matches: impl Fn(&PlannedArtifact) -> bool,
    ) -> Option<PathBuf> {
        Self::single_matching_path(&self.outputs, matches)
    }

    pub(super) fn single_dependency_path_matching(
        &self,
        matches: impl Fn(&PlannedArtifact) -> bool,
    ) -> PathBuf {
        Self::single_matching_path(&self.dependencies, matches)
            .unwrap_or_else(|| unreachable!("expected one matching dependency artifact"))
    }

    pub(super) fn dependency_paths_matching(
        &self,
        matches: impl Fn(&PlannedArtifact) -> bool,
    ) -> Vec<PathBuf> {
        self.dependencies
            .iter()
            .filter(|realized| matches(&realized.artifact))
            .flat_map(|realized| realized.paths.iter().cloned())
            .collect()
    }

    pub(super) fn dependency_artifacts(&self) -> impl Iterator<Item = &PlannedArtifact> {
        self.dependencies.iter().map(|realized| &realized.artifact)
    }

    fn paths(realized: &[RealizedArtifact]) -> Vec<PathBuf> {
        realized
            .iter()
            .flat_map(|artifact| artifact.paths.iter().cloned())
            .collect()
    }

    fn output_artifacts(&self) -> Vec<&PlannedArtifact> {
        self.outputs
            .iter()
            .map(|realized| &realized.artifact)
            .collect()
    }

    fn single_matching_path(
        realized: &[RealizedArtifact],
        matches: impl Fn(&PlannedArtifact) -> bool,
    ) -> Option<PathBuf> {
        let matched = realized
            .iter()
            .filter(|realized| matches(&realized.artifact))
            .collect::<Vec<_>>();
        match matched.as_slice() {
            [artifact] => Self::optional_single_realized_path(artifact),
            [] => None,
            _ => unreachable!("expected at most one matching artifact"),
        }
    }

    fn single_realized_path(artifact: &RealizedArtifact) -> PathBuf {
        Self::optional_single_realized_path(artifact)
            .unwrap_or_else(|| unreachable!("expected exactly one path for artifact"))
    }

    fn optional_single_realized_path(artifact: &RealizedArtifact) -> Option<PathBuf> {
        match artifact.paths.as_slice() {
            [path] => Some(path.clone()),
            [] => None,
            _ => unreachable!(
                "expected one path for artifact, got {:?}: {:?}",
                artifact.paths, artifact.artifact
            ),
        }
    }
}

impl<'a> LoweringContext<'a> {
    pub(super) fn new(
        layout: LegacyLayout,
        resolve_output: &'a ResolveOutput,
        plan: &'a BuildActionPlan<'a>,
        opt: &'a BuildOptions,
    ) -> Self {
        let products = ProductTable::new(&layout, resolve_output, plan, opt);
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
            products,
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
        let action_products = ActionProducts::new(self.plan, &self.products, id);

        // Lower the action to its commands. This step should be infallible.
        let cmd = match action {
            BuildAction::Check { target, info } => self.lower_check(&action_products, target, info),
            BuildAction::EmitProof { target, info } => {
                self.lower_emit_proof(&action_products, target, info)
            }
            BuildAction::Prove { target, info } => self.lower_prove(&action_products, target, info),
            BuildAction::BuildCore { target, info } => {
                self.lower_build_mbt(&action_products, target, info)
            }
            BuildAction::BuildCStub {
                package,
                index,
                info,
            } => self.lower_build_c_stub(&action_products, package, index, info),
            BuildAction::ArchiveOrLinkCStubs { package, info } => {
                self.lower_archive_or_link_c_stubs(&action_products, package, info)
            }
            BuildAction::LinkCore {
                target,
                info,
                make_executable_info,
            } => self.lower_link_core(&action_products, target, info, make_executable_info),
            BuildAction::MakeExecutable {
                target,
                info: Some(info),
            } => self.lower_make_exe(&action_products, target, info),
            BuildAction::MakeExecutable { info: None, .. } => {
                panic!("native MakeExecutable actions should have executable info")
            }
            BuildAction::GenerateTestInfo { target, info } => {
                self.lower_gen_test_driver(&action_products, target, info)
            }
            BuildAction::GenerateMbti { target } => {
                self.lower_generate_mbti(&action_products, target)
            }
            BuildAction::BuildVirtual { package } => self.lower_parse_mbti(package),
            BuildAction::Bundle { module, targets } => {
                self.lower_bundle(&action_products, module, targets)
            }
            BuildAction::BuildRuntimeLib => self
                .lower_compile_runtime(&action_products)
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
        let mut ins = action_products.dependency_paths();
        ins.extend(cmd.extra_inputs);
        // Track tool binary dependencies so that n2 detects when compilers
        // or other toolchain binaries change (e.g. after a toolchain update)
        // and triggers a rebuild.
        if self.plan.needs_moonc_tool_dep(id) {
            ins.push(BINARIES.moonc.clone());
        }
        ins.sort(); // make sure the order is deterministic
        let ins = build_ins(&mut self.graph, ins);

        let output_paths = action_products.output_paths();
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
        out.extend(self.products.paths(artifact).iter().cloned());
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
