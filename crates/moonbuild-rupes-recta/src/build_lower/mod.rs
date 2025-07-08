//! Lowers a [Build plan](crate::build_plan) into `n2`'s Build graph

use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

use log::{debug, info, trace};
use moonutil::{common::TargetBackend, cond_expr::OptLevel, mooncakes::ModuleSource};
use n2::graph::{Build, BuildIns, BuildOuts, FileId, FileLoc, Graph as N2Graph};
use petgraph::Direction;

use crate::{
    build_lower::{
        artifact::LegacyLayout,
        compiler::{CmdlineAbstraction, MiDependency, PackageSource},
    },
    build_plan::{self, BuildPlan, BuildPlanNode},
    discover::{DiscoverResult, DiscoveredPackage},
    model::BuildTarget,
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
}

/// An error that may be raised during build plan lowering
#[derive(thiserror::Error, Debug)]
pub enum LoweringError {
    #[error("An error was reported by n2 (the build graph executor): {0}")]
    N2(anyhow::Error),
}

/// Lowers a [`BuildPlan`] into a n2 [Build Graph](n2::graph::Graph).
pub fn lower_build_plan(
    packages: &DiscoverResult,
    rel: &DepRelationship,
    build_plan: &BuildPlan,
    opt: &BuildOptions,
) -> Result<N2Graph, LoweringError> {
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

    info!("Build plan lowering completed successfully");
    Ok(ctx.graph)
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
    fn lower_node(&mut self, node: BuildPlanNode) -> Result<(), LoweringError> {
        let target = self
            .build_plan
            .get_spec(node)
            .expect("Node should be valid");
        let package = self.packages.get_package(node.target.package);

        trace!(
            "Lowering {} action for package {}",
            match target {
                build_plan::BuildActionSpec::Check(_) => "Check",
                build_plan::BuildActionSpec::BuildMbt(_) => "BuildMbt",
                build_plan::BuildActionSpec::BuildC(_) => "BuildC",
                build_plan::BuildActionSpec::LinkCore(_) => "LinkCore",
                build_plan::BuildActionSpec::MakeExecutable(_) => "MakeExecutable",
                build_plan::BuildActionSpec::GenerateMbti => "GenerateMbti",
                build_plan::BuildActionSpec::Bundle => "Bundle",
            },
            package.fqn
        );

        match target {
            build_plan::BuildActionSpec::Check(_path_bufs) => todo!(),
            build_plan::BuildActionSpec::BuildMbt(path_bufs) => {
                self.lower_build_mbt(node, package, path_bufs)
            }
            build_plan::BuildActionSpec::BuildC(_path_bufs) => todo!(),
            build_plan::BuildActionSpec::LinkCore(core_inputs) => {
                self.lower_link_core(node, package, core_inputs)
            }
            build_plan::BuildActionSpec::MakeExecutable(_build_targets) => {
                // TODO: Local targets need another linking step
                Ok(())
            }
            build_plan::BuildActionSpec::GenerateMbti => todo!(),
            build_plan::BuildActionSpec::Bundle => todo!(),
        }
    }

    fn lowered(&mut self, node: BuildPlanNode, build: Build) -> Result<(), LoweringError> {
        // Debug the lowered build
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
        self.graph.add_build(build).map_err(LoweringError::N2)
    }

    fn lower_build_mbt(
        &mut self,
        node: BuildPlanNode,
        package: &DiscoveredPackage,
        path_bufs: &Vec<PathBuf>,
    ) -> Result<(), LoweringError> {
        let core_output =
            self.layout
                .core_of_build_target(self.packages, &node.target, self.opt.target_backend);
        let mi_output =
            self.layout
                .mi_of_build_target(self.packages, &node.target, self.opt.target_backend);

        let mi_inputs = self
            .rel
            .dep_graph
            .edges_directed(node.target, Direction::Incoming)
            .map(|(it, _, w)| {
                let in_file =
                    self.layout
                        .mi_of_build_target(self.packages, &it, self.opt.target_backend);
                MiDependency::new(in_file, &w.short_alias)
            })
            .collect::<Vec<_>>();

        let ins = build_ins(
            &mut self.graph,
            path_bufs
                .iter()
                .map(|x| x.as_path())
                .chain(mi_inputs.iter().map(|x| x.path.as_ref())),
        );
        let outs = build_outs(&mut self.graph, [&core_output, &mi_output]);

        let mut cmd = compiler::MooncBuildPackage::new(
            path_bufs.as_ref(),
            &core_output,
            &mi_output,
            &mi_inputs,
            &package.fqn,
            &package.root_path,
            self.opt.target_backend,
        );
        cmd.flags.no_opt = self.opt.opt_level == OptLevel::Debug;
        cmd.flags.symbols = self.opt.debug_symbols;
        // TODO: a lot of knobs are not controlled here

        let mut build = Build::new(build_n2_fileloc("build_mbt"), ins, outs);
        build.cmdline = Some(build_cmdline("moonc".into(), &cmd)); // TODO: resolve moonc
        self.lowered(node, build)
    }

    fn lower_link_core(
        &mut self,
        node: BuildPlanNode,
        package: &DiscoveredPackage,
        core_inputs: &[BuildTarget],
    ) -> Result<(), LoweringError> {
        let mut core_input_files = Vec::new();
        for target in core_inputs {
            let core_path =
                self.layout
                    .core_of_build_target(self.packages, target, self.opt.target_backend);
            core_input_files.push(core_path);
        }

        let out_file =
            self.layout
                .core_of_build_target(self.packages, &node.target, self.opt.target_backend);

        let package_sources = core_inputs
            .iter()
            .map(|target| {
                let pkg = self.packages.get_package(target.package);
                PackageSource {
                    package_name: &pkg.fqn,
                    source_dir: pkg.root_path.as_path().into(),
                }
            })
            .collect::<Vec<_>>();

        let ins = build_ins(&mut self.graph, &core_input_files);
        let outs = build_outs(&mut self.graph, [&out_file]);
        let loc = build_n2_fileloc("link_core");

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

        let mut build = Build::new(loc, ins, outs);
        build.cmdline = Some(build_cmdline("moonc".into(), &cmd));
        self.lowered(node, build)
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

fn build_cmdline(moonc: String, cmdline: &dyn CmdlineAbstraction) -> String {
    let mut args = vec![moonc];
    cmdline.to_args(&mut args);
    shlex::try_join(args.iter().map(|x| x.as_ref())).expect("No nul should be in here")
}
