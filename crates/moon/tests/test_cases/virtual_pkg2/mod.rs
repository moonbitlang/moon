use super::*;

#[test]
fn implement_third_party1() {
    let dir = TestDir::new("virtual_pkg2.in/p");
    let check_graph = dir.join("check_graph.json");
    snap_dry_run_graph(&dir, ["check", ".", "--dry-run"], &check_graph);
    compare_graphs(&check_graph, expect_file!["./check_graph.jsonl"]);

    let s = get_stderr(&dir, ["check", "."]);
    check(
        s,
        expect![[r#"
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );
}

#[test]
fn implement_third_party2() {
    let dir = TestDir::new("virtual_pkg2.in/p");
    let build_graph = dir.join("build_graph.json");
    snap_dry_run_graph(&dir, ["build", "--dry-run"], &build_graph);
    compare_graphs(&build_graph, expect_file!["./build_graph.jsonl"]);

    let s = get_stderr(&dir, ["build"]);
    check(
        s,
        expect![[r#"
        Finished. moon: ran 2 tasks, now up to date
    "#]],
    );
}
