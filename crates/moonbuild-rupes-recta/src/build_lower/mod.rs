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

//! Lowers a [Build plan](crate::build_plan) into `n2`'s Build graph

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    rc::Rc,
};

use log::{debug, info};
use moonutil::{
    common::{DriverKind, TargetBackend},
    cond_expr::OptLevel,
    mooncakes::ModuleSource,
};
use n2::graph::{Build, BuildIns, BuildOuts, FileId, FileLoc, Graph as N2Graph};
use petgraph::Direction;

use crate::{
    build_lower::{
        artifact::LegacyLayout,
        compiler::{CmdlineAbstraction, MiDependency, PackageSource},
    },
    build_plan::{BuildActionSpec, BuildPlan, BuildTargetInfo, LinkCoreInfo},
    discover::{DiscoverResult, DiscoveredPackage},
    model::{BuildPlanNode, TargetKind},
    pkg_name::PackageFQNWithSource,
    pkg_solve::DepRelationship,
};

mod artifact;
mod compiler;

/// Knobs to tweak during build. Affects behaviors during lowering.
pub struct BuildOptions {
    pub main_module: Option<ModuleSource>,
    pub target_dir_root: PathBuf,
    // FIXME: This overlaps with `crate::build_plan::BuildEnvironment`
    pub target_backend: TargetBackend,
    pub opt_level: OptLevel,
    pub debug_symbols: bool,
    /// Only `Some` if we import standard library.
    pub stdlib_path: Option<PathBuf>,
}

/// An error that may be raised during build plan lowering
#[derive(thiserror::Error, Debug)]
pub enum LoweringError {
    #[error(
        "An error was reported by n2 (the build graph executor), \
        when lowering for package {package}, build node {node:?}"
    )]
    N2 {
        package: PackageFQNWithSource,
        node: BuildPlanNode,
        source: anyhow::Error,
    },
}

pub struct LoweringResult {
    /// The lowered n2 build graph.
    pub build_graph: N2Graph,

    /// The list of artifacts corresponding to the root input nodes.
    pub artifacts: Vec<Vec<PathBuf>>,
}

/// Lowers a [`BuildPlan`] into a n2 [Build Graph](n2::graph::Graph).
pub fn lower_build_plan(
    packages: &DiscoverResult,
    rel: &DepRelationship,
    build_plan: &BuildPlan,
    opt: &BuildOptions,
) -> Result<LoweringResult, LoweringError> {
    info!("Starting build plan lowering to n2 graph");
    debug!(
        "Build options: backend={:?}, opt_level={:?}, debug_symbols={}",
        opt.target_backend, opt.opt_level, opt.debug_symbols
    );

    let layout = LegacyLayout::new(opt.target_dir_root.to_owned(), opt.main_module.clone());
    let mut ctx = BuildPlanLowerContext {
        graph: N2Graph::default(),
        layout,
        rel,
        packages,
        build_plan,
        opt,
    };

    for node in build_plan.all_nodes() {
        debug!("Lowering build node: {:?}", node);
        ctx.lower_node(node)?;
    }

    let mut out_artifcts = Vec::with_capacity(build_plan.input_nodes().len());
    for n in build_plan.input_nodes() {
        let mut a = vec![];
        ctx.append_artifact_of(*n, &mut a);
        out_artifcts.push(a);
    }

    info!("Build plan lowering completed successfully");
    Ok(LoweringResult {
        build_graph: ctx.graph,
        artifacts: out_artifcts,
    })
}

/// Represents the essential information needed to construct an [`Build`] value
/// that cannot be derived fromthe build plan graph.
struct BuildCommand {
    /// The **extra** input files needed for this command, **in addition to**
    /// the artifacts of the build steps this command depends on.
    extra_inputs: Vec<PathBuf>,

    /// The command to execute.
    commandline: Vec<String>,
}

struct BuildPlanLowerContext<'a> {
    // What we're building
    graph: N2Graph,

    // folder layout
    layout: LegacyLayout,

    // External state
    packages: &'a DiscoverResult,
    rel: &'a DepRelationship,
    build_plan: &'a BuildPlan,
    opt: &'a BuildOptions,
}

impl<'a> BuildPlanLowerContext<'a> {
    /// Some nodes are no-op in n2 build graph. Early bailing.
    fn is_node_noop(&self, node: BuildPlanNode) -> bool {
        (!self.opt.target_backend.is_native())
            && matches!(node.action, crate::model::TargetAction::MakeExecutable)
    }

    fn lower_node(&mut self, node: BuildPlanNode) -> Result<(), LoweringError> {
        if self.is_node_noop(node) {
            return Ok(());
        }

        let spec = self
            .build_plan
            .get_spec(node)
            .expect("Node should be valid");
        let package = self.packages.get_package(node.target.package);

        // Lower the action to its commands. This step should be infallible.
        let cmd = match spec {
            BuildActionSpec::Check(info) => self.lower_check(node, package, info),
            BuildActionSpec::BuildMbt(info) => self.lower_build_mbt(node, package, info),
            BuildActionSpec::BuildC(_path_bufs) => todo!(),
            BuildActionSpec::LinkCore(info) => self.lower_link_core(node, package, info),
            BuildActionSpec::MakeExecutable { link_c_stubs: _ } => {
                // TODO: Native targets need another linking step
                panic!("Native make-executable not supported yet")
            }
            BuildActionSpec::GenerateMbti => todo!(),
            BuildActionSpec::Bundle => todo!(),
            BuildActionSpec::GenerateTestDriver(info) => {
                self.lower_gen_test_driver(node, package, info)
            }
        };

        // Collect n2 inputs and outputs.
        //
        // TODO: some of the inputs and outputs might be calculated twice,
        // once for the commandline and another here. Will this hurt perf?
        let mut ins = vec![];
        for n in self.build_plan.dependency_nodes(node) {
            self.append_artifact_of(n, &mut ins);
        }
        ins.extend(cmd.extra_inputs);
        let ins = build_ins(&mut self.graph, ins);

        let mut outs = vec![];
        self.append_artifact_of(node, &mut outs);
        let outs = build_outs(&mut self.graph, outs);

        // Construct n2 build node
        let mut build = Build::new(build_n2_fileloc(package.fqn.to_string()), ins, outs);
        build.cmdline = Some(
            shlex::try_join(cmd.commandline.iter().map(|x| x.as_str()))
                .expect("No `nul` should occur here"),
        );

        self.debug_print_command_and_files(node, spec, &build);
        self.lowered(build).map_err(|e| LoweringError::N2 {
            package: package.fqn.clone().into(),
            node,
            source: e,
        })
    }

    /// Append the output artifacts of the given node to the provided vector.
    fn append_artifact_of(&self, node: BuildPlanNode, out: &mut Vec<PathBuf>) {
        match node.action {
            crate::model::TargetAction::Check => {
                out.push(self.layout.mi_of_build_target(
                    self.packages,
                    &node.target,
                    self.opt.target_backend,
                ));
            }
            crate::model::TargetAction::Build => {
                out.push(self.layout.mi_of_build_target(
                    self.packages,
                    &node.target,
                    self.opt.target_backend,
                ));
                out.push(self.layout.core_of_build_target(
                    self.packages,
                    &node.target,
                    self.opt.target_backend,
                ));
            }
            crate::model::TargetAction::BuildCStubs => todo!("artifacts of build c stubs"),
            crate::model::TargetAction::LinkCore => {
                out.push(self.layout.linked_core_of_build_target(
                    self.packages,
                    &node.target,
                    self.opt.target_backend,
                    "todo: no native yet",
                ));
            }
            crate::model::TargetAction::MakeExecutable => {
                // No native yet means this is a no-op
            }
            crate::model::TargetAction::GenerateTestInfo => {
                out.push(self.layout.generated_test_driver(
                    self.packages,
                    &node.target,
                    self.opt.target_backend,
                ));
                out.push(self.layout.generated_test_driver_metadata(
                    self.packages,
                    &node.target,
                    self.opt.target_backend,
                ));
            }
        }
    }

    fn lowered(&mut self, build: Build) -> Result<(), anyhow::Error> {
        self.graph.add_build(build)
    }

    fn set_commons(&self, common: &mut compiler::BuildCommonArgs) {
        common.stdlib_core_file = self
            .opt
            .stdlib_path
            .as_ref()
            .map(|x| artifact::core_bundle_path(x, self.opt.target_backend).into());
    }

    fn lower_check(
        &self,
        node: BuildPlanNode,
        package: &DiscoveredPackage,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let mi_output =
            self.layout
                .mi_of_build_target(self.packages, &node.target, self.opt.target_backend);
        let mi_inputs = self.mi_inputs_of(node);

        let mut cmd = compiler::MooncCheck::new(
            &info.files,
            &mi_output,
            &mi_inputs,
            compiler::CompiledPackageName::new(&package.fqn, node.target),
            &package.root_path,
            self.opt.target_backend,
            node.target.kind,
        );
        self.set_commons(&mut cmd.common);

        // Determine whether the checked package is a main package.
        //
        // Black box tests does not include the source files of the original
        // package, while other kinds of package include those. Additionally,
        // no test drivers will be used in checking packages. Thus, black box
        // tests will definitely not contain a main function, while other
        // build targets will have the same kind of main function as the
        // original package.
        cmd.common.is_main = match node.target.kind {
            TargetKind::BlackboxTest => false,
            TargetKind::Source
            | TargetKind::WhiteboxTest
            | TargetKind::InlineTest
            | TargetKind::SubPackage => package.raw.is_main,
        };

        BuildCommand {
            extra_inputs: info.files.clone(),
            commandline: cmd.build_command("moonc"),
        }
    }

    fn lower_build_mbt(
        &self,
        node: BuildPlanNode,
        package: &DiscoveredPackage,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let core_output =
            self.layout
                .core_of_build_target(self.packages, &node.target, self.opt.target_backend);
        let mi_output =
            self.layout
                .mi_of_build_target(self.packages, &node.target, self.opt.target_backend);

        let mi_inputs = self.mi_inputs_of(node);

        let input_sources = match node.target.kind {
            TargetKind::Source | TargetKind::SubPackage => Cow::Borrowed(&info.files),
            TargetKind::WhiteboxTest | TargetKind::BlackboxTest | TargetKind::InlineTest => {
                let mut files = info.files.clone();
                files.push(self.layout.generated_test_driver(
                    self.packages,
                    &node.target,
                    self.opt.target_backend,
                ));
                Cow::Owned(files)
            }
        };

        let mut cmd = compiler::MooncBuildPackage::new(
            &input_sources,
            &core_output,
            &mi_output,
            &mi_inputs,
            compiler::CompiledPackageName::new(&package.fqn, node.target),
            &package.root_path,
            self.opt.target_backend,
            node.target.kind,
        );
        cmd.flags.no_opt = self.opt.opt_level == OptLevel::Debug;
        cmd.flags.symbols = self.opt.debug_symbols;
        self.set_commons(&mut cmd.common);

        // Determine whether the built package is a main package.
        //
        // Different from checking, building test packages will always include
        // the test driver files, which will include the main function.
        cmd.common.is_main = match node.target.kind {
            TargetKind::Source | TargetKind::SubPackage => package.raw.is_main,
            TargetKind::InlineTest | TargetKind::WhiteboxTest | TargetKind::BlackboxTest => true,
        };

        // TODO: a lot of knobs are not controlled here

        BuildCommand {
            extra_inputs: info.files.clone(),
            commandline: cmd.build_command("moonc"),
        }
    }

    fn mi_inputs_of(&self, node: BuildPlanNode) -> Vec<MiDependency<'_>> {
        self.rel
            .dep_graph
            .edges_directed(node.target, Direction::Outgoing)
            .map(|(_, it, w)| {
                let in_file =
                    self.layout
                        .mi_of_build_target(self.packages, &it, self.opt.target_backend);
                MiDependency::new(in_file, &w.short_alias)
            })
            .collect::<Vec<_>>()
    }

    fn lower_link_core(
        &mut self,
        node: BuildPlanNode,
        package: &DiscoveredPackage,
        info: &LinkCoreInfo,
    ) -> BuildCommand {
        let mut core_input_files = Vec::new();
        // Add core for the standard library
        if let Some(stdlib) = &self.opt.stdlib_path {
            // The two stdlib core files must be linked in the correct order,
            // in order to get the correct order of initialization.
            core_input_files.push(artifact::abort_core_path(stdlib, self.opt.target_backend));
            core_input_files.push(artifact::core_core_path(stdlib, self.opt.target_backend));
        }
        // Linked core targets
        for target in &info.linked_order {
            let core_path =
                self.layout
                    .core_of_build_target(self.packages, target, self.opt.target_backend);
            core_input_files.push(core_path);
        }

        let out_file = self.layout.linked_core_of_build_target(
            self.packages,
            &node.target,
            self.opt.target_backend,
            "todo: os not supported yet",
        );

        let package_sources = info
            .linked_order
            .iter()
            .map(|target| {
                let pkg = self.packages.get_package(target.package);
                PackageSource {
                    package_name: compiler::CompiledPackageName::new(&pkg.fqn, *target),
                    source_dir: pkg.root_path.as_path().into(),
                }
            })
            .collect::<Vec<_>>();

        let config_path = package.config_path();
        let mut cmd = compiler::MooncLinkCore::new(
            &core_input_files,
            &package.fqn,
            &out_file,
            &config_path,
            &package_sources,
            self.opt.target_backend,
        );
        cmd.flags.no_opt = self.opt.opt_level == OptLevel::Debug;
        cmd.flags.symbols = self.opt.debug_symbols;

        BuildCommand {
            extra_inputs: vec![],
            commandline: cmd.build_command("moonc"),
        }
    }

    fn lower_gen_test_driver(
        &mut self,
        node: BuildPlanNode,
        package: &DiscoveredPackage,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let output_driver =
            self.layout
                .generated_test_driver(self.packages, &node.target, self.opt.target_backend);
        let output_metadata = self.layout.generated_test_driver_metadata(
            self.packages,
            &node.target,
            self.opt.target_backend,
        );
        let driver_kind = match node.target.kind {
            TargetKind::Source => panic!("Source package cannot be a test driver"),
            TargetKind::WhiteboxTest => DriverKind::Whitebox,
            TargetKind::BlackboxTest => DriverKind::Blackbox,
            TargetKind::InlineTest => DriverKind::Internal,
            TargetKind::SubPackage => panic!("Sub-package cannot be a test driver"),
        };
        let pkg_full_name = package.fqn.to_string();
        let cmd = compiler::MoonGenTestDriver::new(
            &info.files,
            output_driver,
            output_metadata,
            self.opt.target_backend,
            &pkg_full_name,
            driver_kind,
        );

        BuildCommand {
            extra_inputs: info.files.clone(),
            commandline: cmd.build_command("moon"),
        }
    }

    /// **For debug use only.** Prints debug information about a specific build
    /// plan node, the n2 build it's mapped into, and the input and output files
    /// of it.
    #[doc(hidden)]
    fn debug_print_command_and_files(
        &mut self,
        node: BuildPlanNode,
        spec: &BuildActionSpec,
        build: &Build,
    ) {
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
                "lowered: {:?}\n spec {:?};\n into {:?};\n ins: {:?};\n outs: {:?}",
                node, spec, build.cmdline, in_files, out_files
            );
        }
    }
}

/// Create a [`n2::graph::BuildIns`] with all explicit input (because why not?).
fn build_ins(graph: &mut N2Graph, paths: impl IntoIterator<Item = impl AsRef<Path>>) -> BuildIns {
    // this might hint the vec with iterator size
    let file_ids: Vec<_> = paths
        .into_iter()
        .map(|x| register_file(graph, x.as_ref()))
        .collect();
    BuildIns {
        explicit: file_ids.len(),
        ids: file_ids,
        implicit: 0,
        order_only: 0,
    }
}

/// Create a [`n2::graph::BuildOuts`] with all explicit output.
fn build_outs(graph: &mut N2Graph, paths: impl IntoIterator<Item = impl AsRef<Path>>) -> BuildOuts {
    // this might hint the vec with iterator size
    let file_ids: Vec<_> = paths
        .into_iter()
        .map(|x| register_file(graph, x.as_ref()))
        .collect();
    BuildOuts {
        explicit: file_ids.len(),
        ids: file_ids,
    }
}

/// Create a dummy [`FileLoc`] for the given file name. This is a little bit
/// wasteful in terms of memory usage, but should do the job.
fn build_n2_fileloc(name: impl Into<PathBuf>) -> FileLoc {
    FileLoc {
        filename: Rc::new(name.into()),
        line: 0,
    }
}

fn register_file(graph: &mut N2Graph, path: &Path) -> FileId {
    // nah, n2 accepts strings but we're mainly working with `PathBuf`s, so
    // a lot of copying is happening here -- but shouldn't be perf bottleneck
    graph
        .files
        .id_from_canonical(path.to_string_lossy().into_owned())
}
