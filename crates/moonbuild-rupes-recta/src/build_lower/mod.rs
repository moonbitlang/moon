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
    path::{Path, PathBuf},
    rc::Rc,
};

use log::{debug, info};
use moonutil::{
    common::{DriverKind, RunMode, TargetBackend},
    compiler_flags::{
        make_cc_command_pure, resolve_cc, CCConfigBuilder, CompilerPaths, OptLevel as CCOptLevel,
        OutputType as CCOutputType, CC,
    },
    cond_expr::OptLevel,
    mooncakes::{ModuleSource, CORE_MODULE},
    package::JsFormat,
};
use n2::graph::{Build, BuildIns, BuildOuts, FileId, FileLoc, Graph as N2Graph};
use petgraph::Direction;

use crate::{
    build_lower::{
        artifact::{LegacyLayout, LegacyLayoutBuilder},
        compiler::{CmdlineAbstraction, CompilationFlags, MiDependency, PackageSource},
    },
    build_plan::{BuildPlan, BuildTargetInfo, LinkCoreInfo, MakeExecutableInfo},
    discover::{DiscoverResult, DiscoveredPackage},
    model::{Artifacts, BuildPlanNode, BuildTarget, OperatingSystem, TargetKind},
    pkg_name::{OptionalPackageFQNWithSource, PackageFQN, PackagePath},
    pkg_solve::DepRelationship,
};

pub mod artifact;
mod compiler;

/// Knobs to tweak during build. Affects behaviors during lowering.
pub struct BuildOptions {
    pub main_module: Option<ModuleSource>,
    pub target_dir_root: PathBuf,
    // FIXME: This overlaps with `crate::build_plan::BuildEnvironment`
    pub target_backend: TargetBackend,
    pub os: OperatingSystem,
    pub opt_level: OptLevel,
    pub action: RunMode,
    pub debug_symbols: bool,
    /// Only `Some` if we import standard library.
    pub stdlib_path: Option<PathBuf>,
    pub runtime_dot_c_path: PathBuf,
    pub compiler_paths: CompilerPaths,
}

/// An error that may be raised during build plan lowering
#[derive(thiserror::Error, Debug)]
pub enum LoweringError {
    #[error(
        "An error was reported by n2 (the build graph executor), \
        when lowering for package {package}, build node {node:?}"
    )]
    N2 {
        package: OptionalPackageFQNWithSource,
        node: BuildPlanNode,
        source: anyhow::Error,
    },
}

pub struct LoweringResult {
    /// The lowered n2 build graph.
    pub build_graph: N2Graph,

    /// The list of artifacts corresponding to the root input nodes.
    pub artifacts: Vec<Artifacts>,
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

    let layout = LegacyLayoutBuilder::default()
        .target_base_dir(opt.target_dir_root.to_owned())
        .main_module(opt.main_module.clone())
        .opt_level(opt.opt_level)
        .run_mode(opt.action)
        .build()
        .expect("Failed to build legacy layout");

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
        out_artifcts.push(Artifacts {
            node: *n,
            artifacts: a,
        });
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
        (!self.opt.target_backend.is_native()) && matches!(node, BuildPlanNode::MakeExecutable(_))
    }

    fn get_package(&self, target: BuildTarget) -> &DiscoveredPackage {
        self.packages.get_package(target.package)
    }

    fn lower_node(&mut self, node: BuildPlanNode) -> Result<(), LoweringError> {
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
            BuildPlanNode::BuildCStubs(_target) => todo!(),
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
            BuildPlanNode::GenerateMbti(_target) => todo!(),
            BuildPlanNode::Bundle(_module_id) => todo!(),
            BuildPlanNode::GenerateTestInfo(target) => {
                let info = self
                    .build_plan
                    .get_build_target_info(&target)
                    .expect("Build target info should be present for GenerateTestInfo nodes");
                self.lower_gen_test_driver(node, target, info)
            }
            BuildPlanNode::BuildRuntimeLib => self.lower_compile_runtime(),
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
        let fqn = node
            .extract_target()
            .map(|x| self.get_package(x).fqn.clone());
        let mut build = Build::new(
            build_n2_fileloc(
                fqn.as_ref()
                    .map_or_else(|| "no_package".into(), |x| x.to_string()),
            ),
            ins,
            outs,
        );
        build.cmdline = Some(
            shlex::try_join(cmd.commandline.iter().map(|x| x.as_str()))
                .expect("No `nul` should occur here"),
        );

        self.debug_print_command_and_files(node, &build);
        self.lowered(build).map_err(|e| LoweringError::N2 {
            package: fqn.into(),
            node,
            source: e,
        })
    }

    /// Append the output artifacts of the given node to the provided vector.
    fn append_artifact_of(&self, node: BuildPlanNode, out: &mut Vec<PathBuf>) {
        match node {
            BuildPlanNode::Check(target) => {
                out.push(self.layout.mi_of_build_target(
                    self.packages,
                    &target,
                    self.opt.target_backend,
                ));
            }
            BuildPlanNode::BuildCore(target) => {
                out.push(self.layout.mi_of_build_target(
                    self.packages,
                    &target,
                    self.opt.target_backend,
                ));
                out.push(self.layout.core_of_build_target(
                    self.packages,
                    &target,
                    self.opt.target_backend,
                ));
            }
            BuildPlanNode::BuildCStubs(_target) => todo!("artifacts of build c stubs"),
            BuildPlanNode::LinkCore(target) => {
                out.push(self.layout.linked_core_of_build_target(
                    self.packages,
                    &target,
                    self.opt.target_backend,
                    self.opt.os,
                ));
            }
            BuildPlanNode::MakeExecutable(target) => {
                out.push(self.layout.executable_of_build_target(
                    self.packages,
                    &target,
                    self.opt.target_backend,
                    self.opt.os,
                    true,
                ))
            }
            BuildPlanNode::GenerateTestInfo(target) => {
                out.push(self.layout.generated_test_driver(
                    self.packages,
                    &target,
                    self.opt.target_backend,
                ));
                out.push(self.layout.generated_test_driver_metadata(
                    self.packages,
                    &target,
                    self.opt.target_backend,
                ));
            }
            BuildPlanNode::Bundle(_module_id) => {
                todo!()
            }
            BuildPlanNode::BuildRuntimeLib => {
                out.push(
                    self.layout
                        .runtime_output_path(self.opt.target_backend, self.opt.os),
                );
            }
            BuildPlanNode::GenerateMbti(_target) => todo!(),
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

    fn set_flags(&self, flags: &mut CompilationFlags) {
        flags.no_opt = self.opt.opt_level == OptLevel::Debug;
        flags.symbols = self.opt.debug_symbols;
        flags.source_map = self.opt.debug_symbols
            && matches!(
                self.opt.target_backend,
                TargetBackend::Js | TargetBackend::WasmGC
            );
    }

    fn lower_check(
        &self,
        node: BuildPlanNode,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
        let mi_output =
            self.layout
                .mi_of_build_target(self.packages, &target, self.opt.target_backend);
        let mi_inputs = self.mi_inputs_of(node, target);

        // Collect files iterator once so we can pass slices and extra inputs
        let files_vec = info.files().map(|x| x.to_owned()).collect::<Vec<_>>();

        let mut cmd = compiler::MooncCheck::new(
            &files_vec,
            &mi_output,
            &mi_inputs,
            compiler::CompiledPackageName::new(&package.fqn, target.kind),
            &package.root_path,
            self.opt.target_backend,
            target.kind,
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
        cmd.common.is_main = match target.kind {
            TargetKind::BlackboxTest => false,
            TargetKind::Source
            | TargetKind::WhiteboxTest
            | TargetKind::InlineTest
            | TargetKind::SubPackage => package.raw.is_main,
        };

        BuildCommand {
            extra_inputs: files_vec.clone(),
            commandline: cmd.build_command("moonc"),
        }
    }

    fn lower_build_mbt(
        &self,
        node: BuildPlanNode,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
        let core_output =
            self.layout
                .core_of_build_target(self.packages, &target, self.opt.target_backend);
        let mi_output =
            self.layout
                .mi_of_build_target(self.packages, &target, self.opt.target_backend);

        let mi_inputs = self.mi_inputs_of(node, target);

        let mut files = info.files().map(|x| x.to_owned()).collect::<Vec<_>>();
        match target.kind {
            TargetKind::Source | TargetKind::SubPackage => {}
            TargetKind::WhiteboxTest | TargetKind::BlackboxTest | TargetKind::InlineTest => {
                files.push(self.layout.generated_test_driver(
                    self.packages,
                    &target,
                    self.opt.target_backend,
                ));
            }
        };

        let mut cmd = compiler::MooncBuildPackage::new(
            &files,
            &core_output,
            &mi_output,
            &mi_inputs,
            compiler::CompiledPackageName::new(&package.fqn, target.kind),
            &package.root_path,
            self.opt.target_backend,
            target.kind,
        );
        cmd.flags.no_opt = self.opt.opt_level == OptLevel::Debug;
        cmd.flags.symbols = self.opt.debug_symbols;
        self.set_commons(&mut cmd.common);

        // Determine whether the built package is a main package.
        //
        // Different from checking, building test packages will always include
        // the test driver files, which will include the main function.
        cmd.common.is_main = match target.kind {
            TargetKind::Source | TargetKind::SubPackage => package.raw.is_main,
            TargetKind::InlineTest | TargetKind::WhiteboxTest | TargetKind::BlackboxTest => true,
        };

        // TODO: a lot of knobs are not controlled here

        BuildCommand {
            commandline: cmd.build_command("moonc"),
            extra_inputs: files,
        }
    }

    fn mi_inputs_of(&self, _node: BuildPlanNode, target: BuildTarget) -> Vec<MiDependency<'_>> {
        self.rel
            .dep_graph
            .edges_directed(target, Direction::Outgoing)
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
        _node: BuildPlanNode,
        target: BuildTarget,
        info: &LinkCoreInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
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
            &target,
            self.opt.target_backend,
            self.opt.os,
        );

        let core_fqn = PackageFQN::new(CORE_MODULE.clone(), PackagePath::empty());
        let package_sources = info
            .linked_order
            .iter()
            .map(|target| {
                let pkg = self.packages.get_package(target.package);
                PackageSource {
                    package_name: compiler::CompiledPackageName::new(&pkg.fqn, target.kind),
                    source_dir: pkg.root_path.as_path().into(),
                }
            })
            .chain(self.opt.stdlib_path.as_ref().map(|p| PackageSource {
                package_name: compiler::CompiledPackageName::new(&core_fqn, TargetKind::Source),
                source_dir: p.into(),
            }))
            .collect::<Vec<_>>();

        let config_path = package.config_path();
        let mut cmd = compiler::MooncLinkCore::new(
            &core_input_files,
            compiler::CompiledPackageName {
                fqn: &package.fqn,
                kind: target.kind,
            },
            &out_file,
            &config_path,
            &package_sources,
            self.opt.target_backend,
            target.kind.is_test(),
        );
        self.set_flags(&mut cmd.flags);

        // JS format settings
        if self.opt.target_backend == TargetBackend::Js {
            if package.raw.force_link {
                cmd.js_format = Some(JsFormat::default());
            } else if let Some(link) = package.raw.link.as_ref().and_then(|x| x.js.as_ref()) {
                cmd.js_format = Some(link.format.unwrap_or_default());
            }
        }

        BuildCommand {
            extra_inputs: vec![],
            commandline: cmd.build_command("moonc"),
        }
    }

    fn lower_make_exe(&mut self, target: BuildTarget, info: &MakeExecutableInfo) -> BuildCommand {
        assert!(
            self.opt.target_backend.is_native(),
            "Non-native make-executable should be already matched and should not be here"
        );

        let _package = self.get_package(target);

        // Two things needs to be done here:
        // - compile the program (if needed)
        // - link with runtime library & artifacts of other C stubs
        // let cc_cmd = make_cc_command_pure(cc, config, user_cc_flags, src, dest_dir, dest, paths);

        let mut sources = vec![];
        // C artifact path
        self.append_artifact_of(BuildPlanNode::LinkCore(target), &mut sources);
        // Runtime path
        self.append_artifact_of(BuildPlanNode::BuildRuntimeLib, &mut sources);
        // C stubs to link
        for &stub_tgt in &info.link_c_stubs {
            self.append_artifact_of(BuildPlanNode::BuildCStubs(stub_tgt), &mut sources);
        }

        let opt_level = match self.opt.opt_level {
            OptLevel::Release => CCOptLevel::Speed,
            OptLevel::Debug => CCOptLevel::Debug,
        };
        let config = CCConfigBuilder::default()
            .no_sys_header(true)
            .output_ty(CCOutputType::Executable) // TODO: support compiling to library
            .opt_level(opt_level)
            .debug_info(self.opt.opt_level == OptLevel::Debug)
            .link_moonbitrun(true) // TODO: support `tcc run`
            .define_use_shared_runtime_macro(false)
            .build()
            .expect("Failed to build CC configuration for executable");
        let cc_cmd = make_cc_command_pure::<&'static str>(
            resolve_cc(CC::default(), None),
            config,
            &[], // TODO: support native cc flags
            sources.iter().map(|x| x.display().to_string()),
            &self.opt.target_dir_root.display().to_string(),
            &self
                .layout
                .executable_of_build_target(
                    self.packages,
                    &target,
                    self.opt.target_backend,
                    self.opt.os,
                    true,
                )
                .display()
                .to_string(),
            &self.opt.compiler_paths,
        );

        BuildCommand {
            extra_inputs: vec![],
            commandline: cc_cmd,
        }
    }

    fn lower_gen_test_driver(
        &mut self,
        _node: BuildPlanNode,
        target: BuildTarget,
        info: &BuildTargetInfo,
    ) -> BuildCommand {
        let package = self.get_package(target);
        let output_driver =
            self.layout
                .generated_test_driver(self.packages, &target, self.opt.target_backend);
        let output_metadata = self.layout.generated_test_driver_metadata(
            self.packages,
            &target,
            self.opt.target_backend,
        );
        let driver_kind = match target.kind {
            TargetKind::Source => panic!("Source package cannot be a test driver"),
            TargetKind::WhiteboxTest => DriverKind::Whitebox,
            TargetKind::BlackboxTest => DriverKind::Blackbox,
            TargetKind::InlineTest => DriverKind::Internal,
            TargetKind::SubPackage => panic!("Sub-package cannot be a test driver"),
        };
        let pkg_full_name = package.fqn.to_string();
        let files_vec = info.files().map(|x| x.to_owned()).collect::<Vec<_>>();

        let cmd = compiler::MoonGenTestDriver::new(
            &files_vec,
            output_driver,
            output_metadata,
            self.opt.target_backend,
            &pkg_full_name,
            driver_kind,
        );

        BuildCommand {
            commandline: cmd.build_command("moon"),
            extra_inputs: files_vec,
        }
    }

    fn lower_compile_runtime(&mut self) -> BuildCommand {
        let artifact_path = self
            .layout
            .runtime_output_path(self.opt.target_backend, self.opt.os);

        // TODO: this part might need more simplification?
        let runtime_c_path = self.opt.runtime_dot_c_path.clone();
        let cc_cmd = make_cc_command_pure::<&'static str>(
            resolve_cc(CC::default(), None),
            CCConfigBuilder::default()
                .no_sys_header(true)
                .output_ty(CCOutputType::Object)
                .opt_level(CCOptLevel::Speed)
                .debug_info(true)
                // always link moonbitrun in this mode
                .link_moonbitrun(true)
                .define_use_shared_runtime_macro(false)
                .build()
                .expect("Failed to build CC configuration for runtime"),
            &[],
            [runtime_c_path.display().to_string()],
            &self.opt.target_dir_root.display().to_string(),
            &artifact_path.display().to_string(),
            &self.opt.compiler_paths,
        );

        BuildCommand {
            extra_inputs: vec![runtime_c_path],
            commandline: cc_cmd,
        }
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

/// Create a [`n2::graph::BuildIns`] with all explicit input (because why not?).
pub(crate) fn build_ins(
    graph: &mut N2Graph,
    paths: impl IntoIterator<Item = impl AsRef<Path>>,
) -> BuildIns {
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
pub(crate) fn build_outs(
    graph: &mut N2Graph,
    paths: impl IntoIterator<Item = impl AsRef<Path>>,
) -> BuildOuts {
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

pub(crate) fn build_phony_out(
    graph: &mut N2Graph,
    paths: impl IntoIterator<Item = impl AsRef<Path>>,
) -> BuildOuts {
    let file_ids: Vec<_> = paths
        .into_iter()
        .map(|x| register_file(graph, x.as_ref()))
        .collect();
    BuildOuts {
        explicit: 0,
        ids: file_ids,
    }
}

/// Create a dummy [`FileLoc`] for the given file name. This is a little bit
/// wasteful in terms of memory usage, but should do the job.
pub(crate) fn build_n2_fileloc(name: impl Into<PathBuf>) -> FileLoc {
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
