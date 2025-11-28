use super::*;

#[test]
fn test_run_md_test() {
    let dir = TestDir::new("run_md_test.in");

    check(
        get_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            Warning: [0002]
                ╭─[ $ROOT/src/lib/1.mbt.md:39:9 ]
                │
             39 │     let a = 1
                │         ┬  
                │         ╰── Warning (unused_value): Unused variable 'a'
            ────╯
            Finished. moon: ran 4 tasks, now up to date (1 warnings, 0 errors)
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
                "--sort-input",
                "-p",
                "lib",
                "-f",
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

    let _ = get_stdout(&dir, ["test", "--update", "--sort-input"]);

    check(
        get_stdout(&dir, ["test", "--sort-input"]),
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
        get_stdout(&dir, ["check", "--sort-input"]);
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
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
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
                          "path": "username/hello/lib",
                          "alias": "lib",
                          "fspath": "$ROOT/src/lib"
                        }
                      ],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/main/main.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "wasm-gc",
                  "opt_level": "release",
                  "source": "src"
                }"#]],
        );
    }
}
