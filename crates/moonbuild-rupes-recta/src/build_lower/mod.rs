//! Lowers a [Build plan](crate::build_plan) into `n2`'s Build graph

use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

use moonutil::{common::TargetBackend, cond_expr::OptLevel, mooncakes::ModuleSource};
use n2::graph::{Build, BuildIns, BuildOuts, FileId, FileLoc, Graph as N2Graph};

use crate::{
    build_lower::{
        artifact::LegacyLayout,
        compiler::{CmdlineAbstraction, PackageSource},
    },
    build_plan::{self, BuildPlan, BuildPlanNode},
    discover::{DiscoverResult, DiscoveredPackage},
    model::BuildTarget,
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
    build_plan: &BuildPlan,
    opt: &BuildOptions,
) -> Result<N2Graph, LoweringError> {
    let layout = LegacyLayout::new(opt.target_dir_root.to_owned(), opt.main_module.clone());
    let mut ctx = BuildPlanLowerContext {
        graph: N2Graph::default(),
        layout,
        packages,
        build_plan,
        opt,
    };

    for node in build_plan.all_nodes() {
        ctx.lower_node(node)?;
    }

    Ok(ctx.graph)
}

struct BuildPlanLowerContext<'a> {
    // What we're building
    graph: N2Graph,

    // folder layout
    layout: LegacyLayout,

    // External state
    packages: &'a DiscoverResult,
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
        let base_dir = self
            .layout
            .package_dir(&package.fqn, self.opt.target_backend);

        match target {
            build_plan::BuildActionSpec::Check(_path_bufs) => todo!(),
            build_plan::BuildActionSpec::BuildMbt(path_bufs) => {
                self.lower_build_mbt(node, package, base_dir, path_bufs)
            }
            build_plan::BuildActionSpec::BuildC(_path_bufs) => todo!(),
            build_plan::BuildActionSpec::LinkCore(core_inputs) => {
                self.lower_link_core(node, package, base_dir, core_inputs)
            }
            build_plan::BuildActionSpec::MakeExecutable(_build_targets) => {
                // TODO: Local targets need another linking step
                Ok(())
            }
            build_plan::BuildActionSpec::GenerateMbti => todo!(),
            build_plan::BuildActionSpec::Bundle => todo!(),
        }
    }

    fn lower_build_mbt(
        &mut self,
        node: BuildPlanNode,
        package: &DiscoveredPackage,
        base_dir: PathBuf,
        path_bufs: &Vec<PathBuf>,
    ) -> Result<(), LoweringError> {
        let core_output = base_dir.join(
            self.layout
                .pkg_core_basename(&package.fqn, node.target.kind),
        );
        let mi_output = base_dir.join(self.layout.pkg_mi_basename(&package.fqn, node.target.kind));

        let ins = build_ins(&mut self.graph, path_bufs);
        let outs = build_outs(&mut self.graph, [&core_output, &mi_output]);

        let mut cmd = compiler::MooncBuildPackage::new(
            path_bufs.as_ref(),
            &core_output,
            &mi_output,
            &package.fqn,
            &package.root_path,
            self.opt.target_backend,
        );
        cmd.flags.no_opt = self.opt.opt_level == OptLevel::Debug;
        cmd.flags.symbols = self.opt.debug_symbols;
        // TODO: a lot of knobs are not controlled here

        let mut build = Build::new(build_n2_fileloc("build_mbt"), ins, outs);
        build.cmdline = Some(build_cmdline("moonc".into(), &cmd)); // TODO: resolve moonc
        self.graph.add_build(build).map_err(LoweringError::N2)
    }

    fn lower_link_core(
        &mut self,
        node: BuildPlanNode,
        package: &DiscoveredPackage,
        base_dir: PathBuf,
        core_inputs: &[BuildTarget],
    ) -> Result<(), LoweringError> {
        let mut core_input_files = Vec::new();
        for target in core_inputs {
            let dep_pkg = self.packages.get_package(target.package);
            let base_path = self
                .layout
                .package_dir(&dep_pkg.fqn, self.opt.target_backend);
            let core_path =
                base_path.join(self.layout.pkg_core_basename(&dep_pkg.fqn, target.kind));
            core_input_files.push(core_path);
        }

        let out_file = base_dir.join(
            self.layout
                .pkg_core_basename(&package.fqn, node.target.kind),
        );

        let package_sources = core_inputs
            .iter()
            .map(|target| {
                let pkg = self.packages.get_package(target.package);
                PackageSource {
                    package_name: &pkg.fqn,
                    source_dir: &pkg.root_path,
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
        self.graph.add_build(build).map_err(LoweringError::N2)
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
