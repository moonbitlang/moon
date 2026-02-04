use super::*;

#[test]
fn test_abort_override_links_impl() {
    let dir = TestDir::new("abort_override/abort_override.in");
    let run_graph = dir.join("run_graph.json");
    snap_dry_run_graph(
        &dir,
        ["run", "main", "--dry-run", "--sort-input"],
        &run_graph,
    );
    compare_graphs(&run_graph, expect_file!["./run_graph.jsonl.snap"]);

    let out = get_err_stdout(&dir, ["run", "main"]);
    assert!(out.contains("---myabort---"));
}
