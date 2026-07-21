use super::*;

#[test]
fn no_export_when_test() {
    let dir = TestDir::new("no_export_when_test.in");
    assert_dry_run_graph(
        &dir,
        ["test", "--target", "wasm-gc", "--dry-run"],
        expect_file!["./build_graph.jsonl"],
    );

    let s = get_stdout(&dir, ["test", "--target", "wasm-gc"]);
    check(
        s,
        expect![[r#"
        Total tests: 1, passed: 1, failed: 0.
    "#]],
    );
}
