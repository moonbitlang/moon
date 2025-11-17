use expect_test::{expect, expect_file};

use crate::{TestDir, build_graph::compare_graphs, get_stdout, snap_dry_run_graph, util::check};

#[test]
fn test_moon_test_release() {
    let dir = TestDir::new("test_release");

    let graph_file = dir.join("dry_run_graph.jsonl");
    snap_dry_run_graph(&dir, ["test", "--dry-run", "--sort-input"], &graph_file);
    compare_graphs(&graph_file, expect_file!["dry_run_graph.jsonl.snap"]);

    let graph_file = dir.join("release_dry_run_graph.jsonl");
    snap_dry_run_graph(
        &dir,
        ["test", "--release", "--dry-run", "--sort-input"],
        &graph_file,
    );
    compare_graphs(
        &graph_file,
        expect_file!["release_dry_run_graph.jsonl.snap"],
    );

    check(
        get_stdout(
            &dir,
            ["test", "--release", "--sort-input", "--no-parallelize"],
        ),
        expect![[r#"
            test A
            test hello_0
            test hello_1
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}
