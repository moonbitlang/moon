use super::*;

#[test]
fn no_export_when_test() {
    let dir = TestDir::new("no_export_when_test.in");
    build_graph::assert(
        moon_cmd(&dir).args(["test", "--target", "wasm-gc", "--dry-run"]),
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
