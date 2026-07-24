use super::*;

#[test]
fn test_run_md_test() {
    let dir = TestDir::new("run_md_test.in");

    check(
        get_stderr(&dir, ["check", "--target", "wasm-gc", "--sort-input"]),
        expect![[r#"
            Warning: [0002]
                ╭─[ $ROOT/src/lib/1.mbt.md:39:9 ]
                │
             39 │     let a = 1
                │         ┬  
                │         ╰── Warning (unused_value): Unused variable 'a'
            ────╯
        "#]],
    );

    check(
        get_err_stdout(&dir, ["test", "--sort-input"]),
        expect![[r#"
            hello from hello_test.mbt
            fn in md test
            hello from hello_test.mbt
            Hello, world 1!
            Hello, world 3!
            ```moonbit
            fn main {
              println("Hello")
            }
            ```
            Hello, world 2!
            [username/hello] test lib/hello_test.mbt:10 ("inspect in bbtest") failed
            expect test failed at $ROOT/src/lib/hello_test.mbt:15:3-15:31
            Diff: (- expected, + actual)
            ----
            +inspect in bbtest
            ----

            [username/hello] test lib/1.mbt.md:26 (#2) failed
            expect test failed at $ROOT/src/lib/1.mbt.md:41:5-41:20
            Diff: (- expected, + actual)
            ----
            +4234
            ----

            [username/hello] test lib/1.mbt.md:49 (#3) failed
            expect test failed at $ROOT/src/lib/1.mbt.md:58:5-58:15
            Diff: (- expected, + actual)
            ----
            + all
            + wishes
            +
            + come
            + true
            ----

            Total tests: 7, passed: 4, failed: 3.
        "#]],
    );

    // test filter in md test
    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "wasm-gc",
                "--sort-input",
                "-p",
                "lib",
                "--file",
                "1.mbt.md",
                "-i",
                "1",
            ],
        ),
        expect![[r#"
            Hello, world 3!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    let _ = get_stdout(
        &dir,
        ["test", "--target", "wasm-gc", "--update", "--sort-input"],
    );

    check(
        get_stdout(&dir, ["test", "--target", "wasm-gc", "--sort-input"]),
        expect![[r#"
            hello from hello_test.mbt
            fn in md test
            hello from hello_test.mbt
            Hello, world 1!
            Hello, world 3!
            ```moonbit
            fn main {
              println("Hello")
            }
            ```
            Hello, world 2!
            Total tests: 7, passed: 7, failed: 0.
        "#]],
    );

    #[cfg(unix)]
    {
        get_stdout(&dir, ["check", "--target", "wasm-gc", "--sort-input"]);
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
                      "mbt-md-files": {
                        "$ROOT/src/lib/1.mbt.md": {
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
                        },
                        "$ROOT/src/lib/2.mbt.md": {
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
                        "$ROOT/src/lib/1.mbt.md",
                        "$ROOT/src/lib/2.mbt.md",
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
        );
    }
}
