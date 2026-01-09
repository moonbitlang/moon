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

    check(
        get_stdout(
            &dir,
            ["run", "./main.mbt", "--enable-value-tracing", "--dry-run"],
        ),
        expect![[r#"
            moonc build-package ./main.mbt -o ./_build/wasm-gc/debug/build/single/single.core -pkg moon/test/single -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.mi:abort' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/array/array.mi:array' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/bench/bench.mi:bench' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/bigint/bigint.mi:bigint' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/bool/bool.mi:bool' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/buffer/buffer.mi:buffer' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/builtin/builtin.mi:builtin' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/byte/byte.mi:byte' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/bytes/bytes.mi:bytes' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/char/char.mi:char' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/cmp/cmp.mi:cmp' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/coverage/coverage.mi:coverage' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/debug/debug.mi:debug' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/deque/deque.mi:deque' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/double/double.mi:double' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/env/env.mi:env' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/error/error.mi:error' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/float/float.mi:float' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/hashmap/hashmap.mi:hashmap' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/hashset/hashset.mi:hashset' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/immut/array/array.mi:immut/array' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/immut/hashmap/hashmap.mi:immut/hashmap' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/immut/hashset/hashset.mi:immut/hashset' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/immut/list/list.mi:immut/list' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/immut/priority_queue/priority_queue.mi:immut/priority_queue' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/immut/sorted_map/sorted_map.mi:immut/sorted_map' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/immut/sorted_set/sorted_set.mi:immut/sorted_set' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/int/int.mi:int' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/int16/int16.mi:int16' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/int64/int64.mi:int64' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/json/json.mi:json' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/list/list.mi:list' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/math/math.mi:math' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/option/option.mi:option' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/priority_queue/priority_queue.mi:priority_queue' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/queue/queue.mi:queue' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/quickcheck/quickcheck.mi:quickcheck' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/random/random.mi:random' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/ref/ref.mi:ref' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/string/regex/regex.mi:regex' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/result/result.mi:result' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/set/set.mi:set' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/sorted_map/sorted_map.mi:sorted_map' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/sorted_set/sorted_set.mi:sorted_set' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/quickcheck/splitmix/splitmix.mi:splitmix' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/strconv/strconv.mi:strconv' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/string/string.mi:string' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/test/test.mi:test' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/tuple/tuple.mi:tuple' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/uint/uint.mi:uint' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/uint16/uint16.mi:uint16' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/uint64/uint64.mi:uint64' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/unit/unit.mi:unit' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/encoding/utf16/utf16.mi:utf16' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/encoding/utf8/utf8.mi:utf8' -pkg-sources moon/test/single:. -target wasm-gc -O0 -enable-value-tracing -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/single/single.core -main moon/test/single -o ./_build/wasm-gc/debug/build/single/single.wasm -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -O0
            moonrun ./_build/wasm-gc/debug/build/single/single.wasm --
        "#]],
    );
}
