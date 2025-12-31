use crate::build_graph::compare_graphs;

use super::*;

#[test]
fn test_specify_source_dir_001() {
    let dir = TestDir::new("specify_source_dir_001.in");
    let check_graph = dir.join("check_graph.jsonl");
    snap_dry_run_graph(&dir, ["check", "--dry-run", "--sort-input"], &check_graph);
    compare_graphs(&check_graph, expect_file!["check_graph.jsonl.snap"]);
    let build_graph = dir.join("build_graph.jsonl");
    snap_dry_run_graph(&dir, ["build", "--dry-run", "--sort-input"], &build_graph);
    compare_graphs(&build_graph, expect_file!["build_graph.jsonl.snap"]);
    let test_graph = dir.join("test_graph.jsonl");
    snap_dry_run_graph(&dir, ["test", "--dry-run", "--sort-input"], &test_graph);
    compare_graphs(&test_graph, expect_file!["test_graph.jsonl.snap"]);
    check(
        get_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            Finished. moon: ran 4 tasks, now up to date
        "#]],
    );
    #[cfg(unix)]
    {
        let p = dir.join("target/packages.json");
        check(
            replace_dir(&std::fs::read_to_string(p).unwrap(), &dir),
            expect![[r#"
                {
                  "source_dir": "$ROOT",
                  "name": "username/hello",
                  "packages": [
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/src/lib",
                      "root": "username/hello",
                      "rel": "lib",
                      "files": {
                        "$ROOT/src/lib/hello.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {
                        "$ROOT/src/lib/hello_test.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$MOON_HOME/lib/core/prelude"
                        }
                      ],
                      "wbtest-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$MOON_HOME/lib/core/prelude"
                        }
                      ],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$MOON_HOME/lib/core/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/lib/lib.mi"
                    },
                    {
                      "is-main": true,
                      "is-third-party": false,
                      "root-path": "$ROOT/src/main",
                      "root": "username/hello",
                      "rel": "main",
                      "files": {
                        "$ROOT/src/main/main.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$MOON_HOME/lib/core/prelude"
                        },
                        {
                          "path": "username/hello/lib",
                          "alias": "lib",
                          "fspath": "$ROOT/src/lib"
                        }
                      ],
                      "wbtest-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$MOON_HOME/lib/core/prelude"
                        }
                      ],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$MOON_HOME/lib/core/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/main/main.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "wasm-gc",
                  "opt_level": "release",
                  "source": "src"
                }"#]],
        )
    }
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );
    check(
        get_stdout(&dir, ["test"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "./src/main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}
