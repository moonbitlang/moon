use crate::{TestDir, build_graph::compare_graphs, get_stdout, snap_dry_run_graph, util::check};
use expect_test::{expect, expect_file};

#[test]
fn test_moonbitlang_x() {
    let dir = TestDir::new("test_moonbitlang_x");
    get_stdout(&dir, ["update"]);
    get_stdout(&dir, ["install"]);

    let build_snap_file = dir.join("build_dry_run.jsonl");
    snap_dry_run_graph(
        &dir,
        ["build", "--dry-run", "--sort-input"],
        &build_snap_file,
    );
    compare_graphs(
        &build_snap_file,
        expect_file!["moonbitlang_x_build_dry_run.jsonl.snap"],
    );

    let test_snap_file = dir.join("test_dry_run.jsonl");
    snap_dry_run_graph(&dir, ["test", "--dry-run", "--sort-input"], &test_snap_file);
    compare_graphs(
        &test_snap_file,
        expect_file!["moonbitlang_x_test_dry_run.jsonl.snap"],
    );

    check(
        get_stdout(&dir, ["run", "src/main"]),
        expect![[r#"
            Some(123)
        "#]],
    );
}
