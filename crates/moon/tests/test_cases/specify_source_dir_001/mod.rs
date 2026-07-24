use super::*;

#[test]
fn test_specify_source_dir_001() {
    let dir = TestDir::new("specify_source_dir_001.in");
    assert_dry_run_graph(
        &dir,
        ["check", "--target", "wasm-gc", "--dry-run", "--sort-input"],
        expect_file!["check_graph.jsonl.snap"],
    );
    assert_dry_run_graph(
        &dir,
        ["build", "--target", "wasm-gc", "--dry-run", "--sort-input"],
        expect_file!["build_graph.jsonl.snap"],
    );
    assert_dry_run_graph(
        &dir,
        ["test", "--target", "wasm-gc", "--dry-run", "--sort-input"],
        expect_file!["test_graph.jsonl.snap"],
    );
    check(
        get_stderr(&dir, ["check", "--target", "wasm-gc", "--sort-input"]),
        expect![[r#"
            Finished. moon: ran 4 tasks, now up to date
        "#]],
    );
    #[cfg(unix)]
    {
        let p = dir.join("_build/packages.json");
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
                      "artifact": "$ROOT/_build/wasm-gc/debug/check/lib/lib.mi",
                      "check-command": [
                        "check",
                        "-error-format",
                        "json",
                        "$ROOT/src/lib/hello.mbt",
                        "-o",
                        "$ROOT/_build/wasm-gc/debug/check/lib/lib.mi",
                        "-pkg",
                        "username/hello/lib",
                        "-pkg-type",
                        "library",
                        "-std-path",
                        "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle",
                        "-i",
                        "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude",
                        "-pkg-sources",
                        "username/hello/lib:$ROOT/src/lib",
                        "-target",
                        "wasm-gc",
                        "-workspace-path",
                        "$ROOT",
                        "-all-pkgs",
                        "$ROOT/_build/wasm-gc/debug/check/all_pkgs.json"
                      ],
                      "wbtest-check-command": null,
                      "test-check-command": [
                        "check",
                        "-error-format",
                        "json",
                        "$ROOT/src/lib/hello_test.mbt",
                        "-doctest-only",
                        "$ROOT/src/lib/hello.mbt",
                        "-include-doctests",
                        "-o",
                        "$ROOT/_build/wasm-gc/debug/check/lib/lib.blackbox_test.mi",
                        "-pkg",
                        "username/hello/lib_blackbox_test",
                        "-pkg-type",
                        "library",
                        "-std-path",
                        "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle",
                        "-i",
                        "$ROOT/_build/wasm-gc/debug/check/lib/lib.mi:lib",
                        "-i",
                        "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude",
                        "-pkg-sources",
                        "username/hello/lib_blackbox_test:$ROOT/src/lib",
                        "-target",
                        "wasm-gc",
                        "-blackbox-test",
                        "-workspace-path",
                        "$ROOT",
                        "-all-pkgs",
                        "$ROOT/_build/wasm-gc/debug/check/all_pkgs.json"
                      ],
                      "supported-targets": [
                        "Wasm",
                        "WasmGC",
                        "Js",
                        "Native",
                        "LLVM"
                      ]
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
                      "artifact": "$ROOT/_build/wasm-gc/debug/check/main/main.mi",
                      "check-command": [
                        "check",
                        "-error-format",
                        "json",
                        "$ROOT/src/main/main.mbt",
                        "-o",
                        "$ROOT/_build/wasm-gc/debug/check/main/main.mi",
                        "-pkg",
                        "username/hello/main",
                        "-pkg-type",
                        "executable",
                        "-std-path",
                        "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle",
                        "-i",
                        "$ROOT/_build/wasm-gc/debug/check/lib/lib.mi:lib",
                        "-i",
                        "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude",
                        "-pkg-sources",
                        "username/hello/main:$ROOT/src/main",
                        "-target",
                        "wasm-gc",
                        "-workspace-path",
                        "$ROOT",
                        "-all-pkgs",
                        "$ROOT/_build/wasm-gc/debug/check/all_pkgs.json"
                      ],
                      "wbtest-check-command": null,
                      "test-check-command": [
                        "check",
                        "-error-format",
                        "json",
                        "-doctest-only",
                        "$ROOT/src/main/main.mbt",
                        "-include-doctests",
                        "-o",
                        "$ROOT/_build/wasm-gc/debug/check/main/main.blackbox_test.mi",
                        "-pkg",
                        "username/hello/main_blackbox_test",
                        "-pkg-type",
                        "library",
                        "-std-path",
                        "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle",
                        "-i",
                        "$ROOT/_build/wasm-gc/debug/check/lib/lib.mi:lib",
                        "-i",
                        "$ROOT/_build/wasm-gc/debug/check/main/main.mi:main",
                        "-i",
                        "$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude",
                        "-pkg-sources",
                        "username/hello/main_blackbox_test:$ROOT/src/main",
                        "-target",
                        "wasm-gc",
                        "-blackbox-test",
                        "-workspace-path",
                        "$ROOT",
                        "-all-pkgs",
                        "$ROOT/_build/wasm-gc/debug/check/all_pkgs.json"
                      ],
                      "supported-targets": [
                        "Wasm",
                        "WasmGC",
                        "Js",
                        "Native",
                        "LLVM"
                      ]
                    }
                  ],
                  "deps": [],
                  "backend": "wasm-gc",
                  "opt_level": "debug",
                  "source": "src"
                }"#]],
        )
    }
    check(
        get_stderr(&dir, ["build", "--target", "wasm-gc"]),
        expect![[r#"
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--target", "wasm-gc"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "--target", "wasm-gc", "./src/main"]),
        expect![[r#"
            Hello, world!
        "#]],
    );
}

#[test]
fn test_specify_source_dir_root_path_reports_missing_pkg() {
    let dir = TestDir::new("specify_source_dir_001.in");

    let stderr = get_err_stderr(&dir, ["test", ".", "--dry-run"]);
    assert!(
        stderr.contains("does not contain `moon.pkg` or `moon.pkg.json`, so it is not a package"),
        "stderr: {stderr}"
    );
}
