use expect_test::ExpectFile;
use moonbuild_debug::graph::ENV_VAR;

use crate::{TestDir, build_graph::compare_graphs_with_replacements, get_stdout_with_envs};

mod cc_flags;
mod parallel_msvc;
mod tcc_run;
mod test_filter;

#[track_caller]
pub(super) fn assert_native_backend_graph(
    dir: &TestDir,
    tmp_name: &str,
    args: &[&str],
    envs: &[(&str, &str)],
    expected: ExpectFile,
) {
    let graph = dir.join(tmp_name);
    let mut env_pairs: Vec<(String, String)> = envs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect();
    env_pairs.push((ENV_VAR.to_string(), graph.to_string_lossy().into_owned()));
    get_stdout_with_envs(dir, args.iter().copied(), env_pairs);
    compare_graphs_with_replacements(&graph, expected, |s| {
        // Normalize clang-only warnings to keep snapshots portable across macOS/Linux.
        *s = s.replace(" -Wno-unused-value", "");
        *s = s.replace(".dylib", ".so");
    });
}

#[track_caller]
pub(super) fn assert_native_backend_graph_no_env(
    dir: &TestDir,
    tmp_name: &str,
    args: &[&str],
    expected: ExpectFile,
) {
    assert_native_backend_graph(dir, tmp_name, args, &[], expected);
}
