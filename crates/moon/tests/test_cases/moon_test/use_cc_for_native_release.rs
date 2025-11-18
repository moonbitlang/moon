use crate::{TestDir, build_graph::compare_graphs_with_replacements, snap_dry_run_graph};
use expect_test::expect_file;

#[track_caller]
fn assert_dry_run_graph(
    dir: &TestDir,
    tmp_name: &str,
    args: &[&str],
    expected: expect_test::ExpectFile,
) {
    let graph = dir.join(tmp_name);
    snap_dry_run_graph(dir, args.iter().copied(), &graph);
    compare_graphs_with_replacements(&graph, expected, |s| {
        *s = s.replace(".dylib", ".so");
    });
}

#[cfg(unix)]
#[test]
fn test_use_cc_for_native_release() {
    let dir = TestDir::new("moon_test/hello_exec_fntest");
    // build
    {
        assert_dry_run_graph(
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
        // if --release is not specified, it should not use cc
        assert_dry_run_graph(
            &dir,
            "build_graph.jsonl",
            &["build", "--target", "native", "--sort-input", "--dry-run"],
            expect_file!["cc_for_native_release/build_graph.jsonl.snap"],
        );
        assert_dry_run_graph(
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
            expect_file!["cc_for_native_release/build_debug_graph.jsonl.snap"],
        );
    }

    // run
    {
        assert_dry_run_graph(
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
            expect_file!["cc_for_native_release/run_release_graph.jsonl.snap"],
        );
        // if --release is not specified, it should not use cc
        assert_dry_run_graph(
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
            expect_file!["cc_for_native_release/run_graph.jsonl.snap"],
        );
        assert_dry_run_graph(
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
            expect_file!["cc_for_native_release/run_debug_graph.jsonl.snap"],
        );
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

        // use tcc for debug test

        assert_dry_run_graph(
            &dir,
            "test_debug_graph.jsonl",
            &[
                "test",
                "--target",
                "native",
                "--debug",
                "--sort-input",
                "--dry-run",
            ],
            expect_file!["cc_for_native_release/test_debug_graph.jsonl.snap"],
        );
    }
}
