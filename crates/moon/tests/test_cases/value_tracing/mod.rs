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

#[test]
fn test_tracing_value_for_main_func() {
    let dir = TestDir::new("tracing_value.in");
    let run_graph = dir.join("run_graph.jsonl");
    // main.mbt in package
    snap_dry_run_graph(
        &dir,
        [
            "run",
            "./main/main.mbt",
            "--enable-value-tracing",
            "--dry-run",
        ],
        &run_graph,
    );
    compare_graphs(&run_graph, expect_file!["run_graph.jsonl.snap"]);

    let content = get_stdout(&dir, ["run", "./main/main.mbt", "--enable-value-tracing"])
        .split("######MOONBIT_VALUE_TRACING_START######")
        .filter(|l| !(l.contains("__generated_driver_for")))
        .collect::<Vec<_>>()
        .join("\n");
    check(
        content,
        expect![[r#"
            Hello, world!


            {"name": "a", "line": 3, "start_column": 7, "end_column": 8, "filepath": "$ROOT/main/main.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            1
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######


            {"name": "b", "line": 4, "start_column": 7, "end_column": 8, "filepath": "$ROOT/main/main.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            2
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######


            {"name": "c", "line": 5, "start_column": 7, "end_column": 8, "filepath": "$ROOT/main/main.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            3
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######
            3
        "#]],
    );
}

#[test]
fn test_tracing_value_for_single_file() {
    // single file
    let dir = TestDir::new("tracing_value.in");

    let content = get_stdout(&dir, ["run", "./main.mbt", "--enable-value-tracing"])
        .split("######MOONBIT_VALUE_TRACING_START######")
        .filter(|l| !(l.contains("__generated_driver_for")))
        .collect::<Vec<_>>()
        .join("\n");
    check(
        content,
        expect![[r#"


          {"name": "a", "line": 2, "start_column": 7, "end_column": 8, "filepath": "moon/run/single/main.mbt", "value": "$placeholder"}
          ######MOONBIT_VALUE_TRACING_CONTENT_START######
          1
          ######MOONBIT_VALUE_TRACING_CONTENT_END######
          ######MOONBIT_VALUE_TRACING_END######


          {"name": "b", "line": 3, "start_column": 7, "end_column": 8, "filepath": "moon/run/single/main.mbt", "value": "$placeholder"}
          ######MOONBIT_VALUE_TRACING_CONTENT_START######
          2
          ######MOONBIT_VALUE_TRACING_CONTENT_END######
          ######MOONBIT_VALUE_TRACING_END######


          {"name": "c", "line": 4, "start_column": 7, "end_column": 8, "filepath": "moon/run/single/main.mbt", "value": "$placeholder"}
          ######MOONBIT_VALUE_TRACING_CONTENT_START######
          3
          ######MOONBIT_VALUE_TRACING_CONTENT_END######
          ######MOONBIT_VALUE_TRACING_END######
          3
      "#]],
    );
}

#[test]
fn test_tracing_value_for_single_file_dry_run() {
    // single file
    let dir = TestDir::new("tracing_value.in");

    check(
        get_stdout(
            &dir,
            ["run", "./main.mbt", "--enable-value-tracing", "--dry-run"],
        ),
        expect![[r#"
            moonc build-package $ROOT/main.mbt -o $ROOT/target/main.core -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -is-main -pkg moon/run/single -g -O0 -source-map -target wasm-gc -enable-value-tracing
            moonc link-core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core $ROOT/target/main.core -o $ROOT/target/main.wasm -pkg-sources moon/run/single:$ROOT -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -g -O0 -source-map -target wasm-gc
            moonrun $ROOT/target/main.wasm
        "#]],
    );
}
