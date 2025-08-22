use std::{path::Path, process::Command};

use moonbuild_rupes_recta::model::Artifacts;

/// Print what would be executed in a dry-run.
///
/// This is a helper function that prints the build commands from a build graph.
pub fn print_dry_run(
    build_graph: &n2::graph::Graph,
    artifacts: &[Artifacts],
    source_dir: &Path,
    target_dir: &Path,
) {
    let default_files = artifacts
        .iter()
        .flat_map(|art| {
            art.artifacts
                .iter()
                .flat_map(|file| build_graph.files.lookup(&file.to_string_lossy()))
        })
        .collect::<Vec<_>>();
    moonbuild::dry_run::print_build_commands(build_graph, &default_files, source_dir, target_dir);
}

/// Print a command as it would be executed, with the proper escaping.
pub fn dry_print_command(cmd: &Command) {
    let args = std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|x| x.to_string_lossy())
        .collect::<Vec<_>>();
    let cmd = shlex::try_join(args.iter().map(|x| &**x)).expect("null in args, should not happen");
    println!("{cmd}");
}
