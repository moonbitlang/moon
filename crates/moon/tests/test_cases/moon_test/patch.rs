use expect_test::{expect, expect_file};

use crate::{TestDir, build_graph::compare_graphs, get_stdout, snap_dry_run_graph, util::check};

#[test]
fn test_moon_test_patch() {
    let dir = TestDir::new("moon_test/patch");

    // Apply patch to normal build
    let graph_file = dir.join("dry_run_graph.jsonl");
    snap_dry_run_graph(
        &dir,
        [
            "test",
            "-p",
            "moon_new/lib",
            "--file",
            "hello_0.mbt",
            "--patch-file",
            "./patch.json",
            "--dry-run",
            "--sort-input",
            "--nostd",
        ],
        &graph_file,
    );
    compare_graphs(&graph_file, expect_file!["patch_dry_run_graph.jsonl.snap"]);
    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "--file",
                "hello_0.mbt",
                "--patch-file",
                "./patch.json",
            ],
        ),
        expect![[r#"
            hello from patch.json
            hello from lib/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // Apply patch to white box test
    let graph_file = dir.join("dry_run_wbtest_graph.jsonl");
    snap_dry_run_graph(
        &dir,
        [
            "test",
            "-p",
            "moon_new/lib",
            "--file",
            "hello_1_wbtest.mbt",
            "--patch-file",
            "./patch_wbtest.json",
            "--dry-run",
            "--sort-input",
            "--nostd",
        ],
        &graph_file,
    );
    compare_graphs(
        &graph_file,
        expect_file!["patch_wbtest_dry_run_graph.jsonl.snap"],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "--file",
                "hello_1_wbtest.mbt",
                "--patch-file",
                "./patch_wbtest.json",
            ],
        ),
        expect![[r#"
            hello from patch_wbtest.json
            hello from lib/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // Apply patch to black box test
    let graph_file = dir.join("dry_run_bbtest_graph.jsonl");
    snap_dry_run_graph(
        &dir,
        [
            "test",
            "-p",
            "moon_new/lib",
            "--file",
            "hello_2_test.mbt",
            "--patch-file",
            "./patch_test.json",
            "--dry-run",
            "--sort-input",
            "--nostd",
        ],
        &graph_file,
    );
    compare_graphs(
        &graph_file,
        expect_file!["patch_bbtest_dry_run_graph.jsonl.snap"],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib",
                "--file",
                "hello_2_test.mbt",
                "--patch-file",
                "./patch_test.json",
            ],
        ),
        expect![[r#"
            hello from patch_test.json
            hello from lib/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    // no _test.mbt and _wbtest.mbt in original package
    let graph_file = dir.join("dry_run_2_patch_graph.jsonl");
    snap_dry_run_graph(
        &dir,
        [
            "test",
            "-p",
            "moon_new/lib2",
            "--file",
            "hello_2_test.mbt",
            "--patch-file",
            "./2.patch_test.json",
            "--dry-run",
            "--sort-input",
            "--nostd",
        ],
        &graph_file,
    );
    compare_graphs(
        &graph_file,
        expect_file!["patch_2_bbtest_dry_run_graph.jsonl.snap"],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib2",
                "--file",
                "hello_2_test.mbt",
                "--patch-file",
                "./2.patch_test.json",
            ],
        ),
        expect![[r#"
            hello from 2.patch_test.json
            hello from lib2/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    let graph_file = dir.join("dry_run_2_wbpatch_graph.jsonl");
    snap_dry_run_graph(
        &dir,
        [
            "test",
            "-p",
            "moon_new/lib2",
            "--file",
            "hello_2_wbtest.mbt",
            "--patch-file",
            "./2.patch_wbtest.json",
            "--dry-run",
            "--sort-input",
            "--nostd",
        ],
        &graph_file,
    );
    compare_graphs(
        &graph_file,
        expect_file!["patch_2_wbtest_dry_run_graph.jsonl.snap"],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "moon_new/lib2",
                "--file",
                "hello_2_wbtest.mbt",
                "--patch-file",
                "./2.patch_wbtest.json",
            ],
        ),
        expect![[r#"
            hello from 2.patch_wbtest.json
            hello from lib2/hello.mbt
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}
