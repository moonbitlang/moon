use super::*;

#[test]
fn no_export_when_test() {
    let dir = TestDir::new("no_export_when_test.in");
    let build_graph = dir.join("build_graph.json");
    snap_dry_run_graph(&dir, ["test", "--dry-run"], &build_graph);
    compare_graphs(&build_graph, expect_file!["./build_graph.jsonl"]);

    let s = get_stdout(&dir, ["test"]);
    check(s, expect![[r#"
        Total tests: 1, passed: 1, failed: 0.
    "#]]);
}
