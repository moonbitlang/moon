//! Lowers a [Build plan](crate::build_plan) into `n2`'s Build graph

use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

use moonutil::{common::TargetBackend, cond_expr::OptLevel, mooncakes::ModuleSource};
use n2::graph::{Build, BuildId, BuildIns, BuildOuts, FileId, FileLoc, Graph as N2Graph};

use crate::{
    build_lower::artifact::LegacyLayout,
    build_plan::{self, BuildPlan, BuildPlanNode},
    discover::{DiscoverResult, DiscoveredPackage},
};

mod artifact;
mod compiler;

/// Knobs to tweak during build. Affects behaviors during lowering.
pub struct BuildOptions {
    main_module: ModuleSource,
    target_dir_root: PathBuf,
    // FIXME: This overlaps with `crate::build_plan::BuildEnvironment`
    target_backend: TargetBackend,
    opt_level: OptLevel,
}

/// Lowers a [`BuildPlan`] into a n2 [Build Graph](n2::graph::Graph).
///
/// This function returns an [`anyhow::Result`], which is worse than optimal for
/// a library like this, but since `n2` uses it, we have no better choice.
pub fn lower_build_plan(
    packages: &DiscoverResult,
    build_plan: &BuildPlan,
    opt: &BuildOptions,
) -> anyhow::Result<N2Graph> {
    let layout = LegacyLayout::new(opt.target_dir_root.clone(), opt.main_module.clone());

    let mut graph = N2Graph::default();
    for node in build_plan.all_nodes() {
        lower_node(&mut graph, &layout, packages, build_plan, opt, node)?;
    }

    Ok(graph)
}

fn lower_node(
    graph: &mut N2Graph,
    layout: &LegacyLayout,
    packages: &DiscoverResult,
    build_plan: &BuildPlan,
    opt: &BuildOptions,
    node: BuildPlanNode,
) -> anyhow::Result<()> {
    let target = build_plan.get_spec(node).expect("Node should be valid");
    let package = packages.get_package(node.target.package);
    let base_dir = layout.package_dir(&package.fqn, opt.target_backend);

    match target {
        build_plan::BuildActionSpec::Check(path_bufs) => todo!(),
        build_plan::BuildActionSpec::BuildMbt(path_bufs) => {
            lower_build_mbt(graph, layout, node, package, opt, base_dir, path_bufs)
        }
        build_plan::BuildActionSpec::BuildC(path_bufs) => todo!(),
        build_plan::BuildActionSpec::LinkCore(build_targets) => todo!(),
        build_plan::BuildActionSpec::MakeExecutable(build_targets) => todo!(),
        build_plan::BuildActionSpec::GenerateMbti => todo!(),
    }
}

fn lower_build_mbt(
    graph: &mut N2Graph,
    layout: &LegacyLayout,
    node: BuildPlanNode,
    package: &DiscoveredPackage,
    opt: &BuildOptions,
    base_dir: PathBuf,
    path_bufs: &Vec<PathBuf>,
) -> anyhow::Result<()> {
    let core_output = base_dir.join(layout.pkg_core_basename(&package.fqn, node.target.kind));
    let mi_output = base_dir.join(layout.pkg_mi_basename(&package.fqn, node.target.kind));

    let ins = build_ins(graph, path_bufs);
    let outs = build_outs(graph, [&core_output, &mi_output]);

    let cmd = compiler::MooncBuildPackage::new(
        path_bufs.as_ref(),
        &core_output,
        &mi_output,
        &package.fqn,
        &package.root_path,
        opt.target_backend,
    );
    // TODO: a lot of knobs are not controlled here
    let mut args = vec!["moonc".into()]; // TODO: resolve to actual moonc
    cmd.to_args_legacy(&mut args);
    let build_cmdline =
        shlex::try_join(args.iter().map(|x| x.as_ref())).expect("No nul should be in here");

    let mut build = Build::new(build_n2_fileloc("build_mbt"), ins, outs);
    build.cmdline = Some(build_cmdline);
    graph.add_build(build)
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
