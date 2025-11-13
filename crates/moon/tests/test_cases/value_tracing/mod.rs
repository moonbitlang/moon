use super::*;

use crate::build_graph::compare_graphs;

#[test]
fn test_tracing_value_for_test_block() {
    let dir = TestDir::new("tracing_value_for_test_block.in");
    let test_graph = dir.join("test_graph.jsonl");
    snap_dry_run_graph(
        &dir,
        [
            "test",
            "--enable-value-tracing",
            "-p",
            "moon_new/lib1",
            "--dry-run",
        ],
        &test_graph,
    );
    compare_graphs(&test_graph, expect_file!["test_graph.jsonl.snap"]);

    let content = get_stdout(
        &dir,
        ["test", "-p", "moon_new/lib1", "--enable-value-tracing"],
    )
    .split("######MOONBIT_VALUE_TRACING_START######")
    .filter(|l| !(l.contains("__generated_driver_for")))
    .collect::<Vec<_>>()
    .join("\n");
    check(
        content,
        expect![[r#"


            {"name": "a", "line": 6, "start_column": 7, "end_column": 8, "filepath": "$ROOT/lib1/hello.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            1
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######


            {"name": "b", "line": 7, "start_column": 7, "end_column": 8, "filepath": "$ROOT/lib1/hello.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            2
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######


            {"name": "c", "line": 8, "start_column": 7, "end_column": 8, "filepath": "$ROOT/lib1/hello.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            3
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######
            Hello, world!
        "#]],
    );
}
