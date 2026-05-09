use crate::{TestDir, build_graph::compare_graphs_with_replacements, snap_dry_run_graph};
use expect_test::expect_file;

fn normalize_native_graph(graph: &mut String) {
    // Normalize clang-only warnings to keep snapshots portable across macOS/Linux.
    *graph = graph.replace(" -Wno-unused-value", "");
    *graph = graph.replace(".dylib", ".so");
}

#[track_caller]
fn dry_run_graph(dir: &TestDir, tmp_name: &str, args: &[&str]) -> String {
    let graph = dir.join(tmp_name);
    snap_dry_run_graph(dir, args.iter().copied(), &graph);
    let mut graph = std::fs::read_to_string(graph).expect("dry-run graph should be readable");
    normalize_native_graph(&mut graph);
    graph
}

#[track_caller]
fn assert_dry_run_graph(
    dir: &TestDir,
    tmp_name: &str,
    args: &[&str],
    expected: expect_test::ExpectFile,
) -> String {
    let graph = dir.join(tmp_name);
    snap_dry_run_graph(dir, args.iter().copied(), &graph);
    compare_graphs_with_replacements(&graph, expected, normalize_native_graph);
    let mut graph = std::fs::read_to_string(graph).expect("dry-run graph should be readable");
    normalize_native_graph(&mut graph);
    graph
}

#[cfg(unix)]
#[test]
fn test_use_cc_for_native_release() {
    let dir = TestDir::new("moon_test/hello_exec_fntest");
    // build
    let (build_release_graph, build_graph) = {
        let build_release_graph = assert_dry_run_graph(
            &dir,
            "build_release_graph.jsonl",
            &[
                "build",
                "--target",
                "native",
                "--release",
                "--sort-input",
                "--dry-run",
            ],
            expect_file!["cc_for_native_release/build_release_graph.jsonl.snap"],
        );
        let build_graph = assert_dry_run_graph(
            &dir,
            "build_graph.jsonl",
            &["build", "--target", "native", "--sort-input", "--dry-run"],
            expect_file!["cc_for_native_release/build_graph.jsonl.snap"],
        );

        let build_debug_graph = dry_run_graph(
            &dir,
            "build_debug_graph.jsonl",
            &[
                "build",
                "--target",
                "native",
                "--debug",
                "--sort-input",
                "--dry-run",
            ],
        );
        assert_eq!(build_debug_graph, build_graph);
        (build_release_graph, build_graph)
    };

    // run
    {
        let run_release_graph = dry_run_graph(
            &dir,
            "run_release_graph.jsonl",
            &[
                "run",
                "main",
                "--target",
                "native",
                "--release",
                "--sort-input",
                "--dry-run",
            ],
        );
        assert_eq!(run_release_graph, build_release_graph);

        let run_graph = dry_run_graph(
            &dir,
            "run_graph.jsonl",
            &[
                "run",
                "main",
                "--target",
                "native",
                "--sort-input",
                "--dry-run",
            ],
        );
        assert_eq!(run_graph, build_graph);

        let run_debug_graph = dry_run_graph(
            &dir,
            "run_debug_graph.jsonl",
            &[
                "run",
                "main",
                "--target",
                "native",
                "--debug",
                "--sort-input",
                "--dry-run",
            ],
        );
        assert_eq!(run_debug_graph, build_graph);
    }

    // test
    {
        assert_dry_run_graph(
            &dir,
            "test_release_graph.jsonl",
            &[
                "test",
                "--target",
                "native",
                "--release",
                "--sort-input",
                "--dry-run",
            ],
            expect_file!["cc_for_native_release/test_release_graph.jsonl.snap"],
        );

        // TODO: use tcc for debug test
        // assert_dry_run_graph(
        //     &dir,
        //     "test_debug_graph.jsonl",
        //     &[
        //         "test",
        //         "--target",
        //         "native",
        //         "--debug",
        //         "--sort-input",
        //         "--dry-run",
        //     ],
        //     expect_file!["cc_for_native_release/test_debug_graph.jsonl.snap"],
        // );
    }
}
