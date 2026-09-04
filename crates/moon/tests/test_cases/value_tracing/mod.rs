use super::*;

#[test]
fn test_tracing_value_for_test_block() {
    let dir = TestDir::new("tracing_value_for_test_block.in");
    assert_dry_run_graph(
        &dir,
        [
            "test",
            "--target",
            "wasm-gc",
            "--enable-value-tracing",
            "-p",
            "moon_new/lib1",
            "--dry-run",
        ],
        expect_file!["test_graph.jsonl.snap"],
    );

    let content = get_stdout(
        &dir,
        [
            "test",
            "--target",
            "wasm-gc",
            "-p",
            "moon_new/lib1",
            "--enable-value-tracing",
        ],
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
    // main.mbt in package
    assert_dry_run_graph(
        &dir,
        [
            "run",
            "--target",
            "wasm-gc",
            "./main/main.mbt",
            "--enable-value-tracing",
            "--dry-run",
        ],
        expect_file!["run_graph.jsonl.snap"],
    );

    let content = get_stdout(
        &dir,
        [
            "run",
            "--target",
            "wasm-gc",
            "./main/main.mbt",
            "--enable-value-tracing",
        ],
    )
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


            {"name": "a", "line": 2, "start_column": 7, "end_column": 8, "filepath": "$ROOT/main.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            1
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######


            {"name": "b", "line": 3, "start_column": 7, "end_column": 8, "filepath": "$ROOT/main.mbt", "value": "$placeholder"}
            ######MOONBIT_VALUE_TRACING_CONTENT_START######
            2
            ######MOONBIT_VALUE_TRACING_CONTENT_END######
            ######MOONBIT_VALUE_TRACING_END######


            {"name": "c", "line": 4, "start_column": 7, "end_column": 8, "filepath": "$ROOT/main.mbt", "value": "$placeholder"}
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
    let output = get_stdout(
        &dir,
        [
            "run",
            "--target",
            "wasm-gc",
            "./main.mbt",
            "--enable-value-tracing",
            "--dry-run",
        ],
    );

    check(
        collapse_core_import_args(&output, TargetBackend::WasmGC),
        expect![[r#"
            moonc build-package ./main.mbt -o ./_build/main.mbt/wasm-gc/debug/build/single/single.core -pkg moon/test/single -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/<imports>' -pkg-sources moon/test/single:. -target wasm-gc -g -O0 -source-map -enable-value-tracing -workspace-path . -all-pkgs ./_build/main.mbt/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/main.mbt/wasm-gc/debug/build/single/single.core -main moon/test/single -o ./_build/main.mbt/wasm-gc/debug/build/single/single.wasm -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
            '$MOONRUN_OVERRIDE' ./_build/main.mbt/wasm-gc/debug/build/single/single.wasm --
        "#]],
    );
}
