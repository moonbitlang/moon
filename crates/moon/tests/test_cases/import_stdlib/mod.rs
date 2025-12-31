use super::*;

#[test]
fn test_import_stdlib() {
    let dir = TestDir::new("import_stdlib/import_stdlib.in");

    let check = get_stderr(&dir, ["check"]);
    expect![[r#"
        Finished. moon: ran 4 tasks, now up to date
    "#]]
    .assert_eq(&check);

    let build = get_stderr(&dir, ["build"]);
    expect![[r#"
        Finished. moon: ran 2 tasks, now up to date
    "#]]
    .assert_eq(&build);

    let test = get_stderr(&dir, ["test"]);
    expect![""].assert_eq(&test);

    let run = get_stdout(&dir, ["run", "cmd/main"]);
    expect![[r#"
        1
    "#]]
    .assert_eq(&run);
}

#[test]
fn test_import_stdlib_dry_run() {
    let dir = TestDir::new("import_stdlib/import_stdlib.in");

    let check_graph = dir.join("check_graph.jsonl");
    snap_dry_run_graph(&dir, ["check", "--dry-run"], &check_graph);

    compare_graphs(&check_graph, expect_file!["./check_graph.jsonl.snap"]);

    let build_graph = dir.join("build_graph.jsonl");
    snap_dry_run_graph(&dir, ["build", "--dry-run"], &build_graph);
    compare_graphs(&build_graph, expect_file!["./build_graph.jsonl.snap"]);

    let test_graph = dir.join("test_graph.jsonl");
    snap_dry_run_graph(&dir, ["test", "--dry-run"], &test_graph);
    compare_graphs(&test_graph, expect_file!["./test_graph.jsonl.snap"]);

    let run_graph = dir.join("run_graph.jsonl");
    snap_dry_run_graph(&dir, ["run", "cmd/main", "--dry-run"], &run_graph);
    compare_graphs(&run_graph, expect_file!["./run_graph.jsonl.snap"]);
}
