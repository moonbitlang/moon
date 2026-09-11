use super::*;

#[test]
fn test_abort_override_links_impl() {
    let dir = TestDir::new("abort_override/abort_override.in");
    build_graph::assert(
        moon_cmd(&dir).args([
            "run",
            "--target",
            "wasm-gc",
            "main",
            "--dry-run",
            "--sort-input",
        ]),
        expect_file!["./run_graph.jsonl.snap"],
    );

    let out = get_err_stdout(&dir, ["run", "main"]);
    assert!(out.contains("---myabort---"));
}
