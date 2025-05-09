use super::*;
#[test]
fn test_dummy_core() {
    let test_dir = TestDir::new("dummy_core");
    let dir = dunce::canonicalize(test_dir.as_ref()).unwrap();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        let p = dir.join("target/packages.json");
        check(
            replace_dir(&std::fs::read_to_string(p).unwrap(), &dir),
            expect![[r#"
                {
                  "source_dir": "$ROOT",
                  "name": "moonbitlang/core",
                  "packages": [
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/0",
                      "root": "moonbitlang/core",
                      "rel": "0",
                      "files": {
                        "$ROOT/0/lib.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/0/y.js.mbt": {
                          "backend": [
                            "Js"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y.wasm.mbt": {
                          "backend": [
                            "Wasm"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "wbtest-files": {
                        "$ROOT/0/y_wbtest.js.mbt": {
                          "backend": [
                            "Js"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y_wbtest.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/0/y_wbtest.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y_wbtest.wasm.mbt": {
                          "backend": [
                            "Wasm"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/0/0.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/1",
                      "root": "moonbitlang/core",
                      "rel": "1",
                      "files": {
                        "$ROOT/1/lib.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/1/x.js.mbt": {
                          "backend": [
                            "Js"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/1/x.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/1/x.wasm.mbt": {
                          "backend": [
                            "Wasm"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "wbtest-files": {
                        "$ROOT/1/x_wbtest.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/1/1.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/2",
                      "root": "moonbitlang/core",
                      "rel": "2",
                      "files": {
                        "$ROOT/2/lib.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/1",
                          "alias": "1",
                          "fspath": "$ROOT/1"
                        }
                      ],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/2/2.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/char",
                      "root": "moonbitlang/core",
                      "rel": "char",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/coverage",
                          "alias": "coverage",
                          "fspath": "$ROOT/coverage"
                        }
                      ],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/char/char.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/coverage",
                      "root": "moonbitlang/core",
                      "rel": "coverage",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/coverage/coverage.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/iter",
                      "root": "moonbitlang/core",
                      "rel": "iter",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/coverage",
                          "alias": "coverage",
                          "fspath": "$ROOT/coverage"
                        }
                      ],
                      "wbtest-deps": [
                        {
                          "path": "moonbitlang/core/char",
                          "alias": "char",
                          "fspath": "$ROOT/char"
                        }
                      ],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/iter/iter.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/prelude",
                      "root": "moonbitlang/core",
                      "rel": "prelude",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/prelude/prelude.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "wasm-gc",
                  "opt_level": "release",
                  "source": null
                }"#]],
        );
    }
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--target", "js", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        let p = dir.join("target/packages.json");
        check(
            replace_dir(&std::fs::read_to_string(p).unwrap(), &dir),
            expect![[r#"
                {
                  "source_dir": "$ROOT",
                  "name": "moonbitlang/core",
                  "packages": [
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/0",
                      "root": "moonbitlang/core",
                      "rel": "0",
                      "files": {
                        "$ROOT/0/lib.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/0/y.js.mbt": {
                          "backend": [
                            "Js"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y.wasm.mbt": {
                          "backend": [
                            "Wasm"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "wbtest-files": {
                        "$ROOT/0/y_wbtest.js.mbt": {
                          "backend": [
                            "Js"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y_wbtest.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/0/y_wbtest.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/0/y_wbtest.wasm.mbt": {
                          "backend": [
                            "Wasm"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/0/0.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/1",
                      "root": "moonbitlang/core",
                      "rel": "1",
                      "files": {
                        "$ROOT/1/lib.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        },
                        "$ROOT/1/x.js.mbt": {
                          "backend": [
                            "Js"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/1/x.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        },
                        "$ROOT/1/x.wasm.mbt": {
                          "backend": [
                            "Wasm"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "wbtest-files": {
                        "$ROOT/1/x_wbtest.wasm-gc.mbt": {
                          "backend": [
                            "WasmGC"
                          ],
                          "optlevel": [
                            "Release",
                            "Debug"
                          ]
                        }
                      },
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/1/1.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/2",
                      "root": "moonbitlang/core",
                      "rel": "2",
                      "files": {
                        "$ROOT/2/lib.mbt": {
                          "backend": [
                            "Wasm",
                            "WasmGC",
                            "Js",
                            "Native",
                            "LLVM"
                          ],
                          "optlevel": [
                            "Debug",
                            "Release"
                          ]
                        }
                      },
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/1",
                          "alias": "1",
                          "fspath": "$ROOT/1"
                        }
                      ],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/2/2.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/char",
                      "root": "moonbitlang/core",
                      "rel": "char",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/coverage",
                          "alias": "coverage",
                          "fspath": "$ROOT/coverage"
                        }
                      ],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/char/char.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/coverage",
                      "root": "moonbitlang/core",
                      "rel": "coverage",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/coverage/coverage.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/iter",
                      "root": "moonbitlang/core",
                      "rel": "iter",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [
                        {
                          "path": "moonbitlang/core/coverage",
                          "alias": "coverage",
                          "fspath": "$ROOT/coverage"
                        }
                      ],
                      "wbtest-deps": [
                        {
                          "path": "moonbitlang/core/char",
                          "alias": "char",
                          "fspath": "$ROOT/char"
                        }
                      ],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/iter/iter.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/prelude",
                      "root": "moonbitlang/core",
                      "rel": "prelude",
                      "files": {},
                      "wbtest-files": {},
                      "test-files": {},
                      "mbt-md-files": {},
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [
                        {
                          "path": "moonbitlang/core/prelude",
                          "alias": "prelude",
                          "fspath": "$ROOT/prelude"
                        }
                      ],
                      "artifact": "$ROOT/target/js/release/check/prelude/prelude.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "js",
                  "opt_level": "release",
                  "source": null
                }"#]],
        );
    };

    check(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check -o ./target/wasm-gc/release/check/prelude/prelude.mi -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/coverage/coverage.mi -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/iter/iter.mi -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/char/char.mi -pkg moonbitlang/core/char -i ./target/wasm-gc/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc check ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/check/1/1.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc check ./2/lib.mbt -o ./target/wasm-gc/release/check/2/2.mi -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/check/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc check ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_wbtest.wasm-gc.mbt -o ./target/wasm-gc/release/check/1/1.whitebox_test.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -whitebox-test
            moonc check ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm-gc.mbt -o ./target/wasm-gc/release/check/0/0.whitebox_test.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -whitebox-test
            moonc check ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/check/0/0.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["check", "--dry-run", "--target", "wasm", "--sort-input"],
        ),
        expect![[r#"
            moonc check -o ./target/wasm/release/check/prelude/prelude.mi -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm
            moonc check -o ./target/wasm/release/check/coverage/coverage.mi -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm
            moonc check -o ./target/wasm/release/check/iter/iter.mi -pkg moonbitlang/core/iter -i ./target/wasm/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm
            moonc check -o ./target/wasm/release/check/char/char.mi -pkg moonbitlang/core/char -i ./target/wasm/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm
            moonc check ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/check/1/1.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc check ./2/lib.mbt -o ./target/wasm/release/check/2/2.mi -pkg moonbitlang/core/2 -i ./target/wasm/release/check/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc check ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/check/1/1.whitebox_test.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm -whitebox-test
            moonc check ./0/lib.mbt ./0/y.wasm.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm.mbt -o ./target/wasm/release/check/0/0.whitebox_test.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm -whitebox-test
            moonc check ./0/lib.mbt ./0/y.wasm.mbt -o ./target/wasm/release/check/0/0.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["check", "--dry-run", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            moonc check -o ./target/wasm-gc/release/check/prelude/prelude.mi -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/coverage/coverage.mi -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/iter/iter.mi -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc check -o ./target/wasm-gc/release/check/char/char.mi -pkg moonbitlang/core/char -i ./target/wasm-gc/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc check ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/check/1/1.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc check ./2/lib.mbt -o ./target/wasm-gc/release/check/2/2.mi -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/check/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc check ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_wbtest.wasm-gc.mbt -o ./target/wasm-gc/release/check/1/1.whitebox_test.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -whitebox-test
            moonc check ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm-gc.mbt -o ./target/wasm-gc/release/check/0/0.whitebox_test.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -whitebox-test
            moonc check ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/check/0/0.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["check", "--dry-run", "--target", "js", "--sort-input"],
        ),
        expect![[r#"
            moonc check -o ./target/js/release/check/prelude/prelude.mi -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target js
            moonc check -o ./target/js/release/check/coverage/coverage.mi -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target js
            moonc check -o ./target/js/release/check/iter/iter.mi -pkg moonbitlang/core/iter -i ./target/js/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target js
            moonc check -o ./target/js/release/check/char/char.mi -pkg moonbitlang/core/char -i ./target/js/release/check/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target js
            moonc check ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/check/1/1.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js
            moonc check ./2/lib.mbt -o ./target/js/release/check/2/2.mi -pkg moonbitlang/core/2 -i ./target/js/release/check/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc check ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/check/1/1.whitebox_test.mi -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js -whitebox-test
            moonc check ./0/lib.mbt ./0/y.js.mbt ./0/y_wbtest.js.mbt ./0/y_wbtest.mbt -o ./target/js/release/check/0/0.whitebox_test.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js -whitebox-test
            moonc check ./0/lib.mbt ./0/y.js.mbt -o ./target/js/release/check/0/0.mi -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js
        "#]],
    );

    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/build/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/build/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/build/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/1/1.core ./target/wasm-gc/release/build/2/2.core -main moonbitlang/core/2 -o ./target/wasm-gc/release/build/2/2.wasm -pkg-config-path ./2/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/1/1.core -main moonbitlang/core/1 -o ./target/wasm-gc/release/build/1/1.wasm -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/build/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/0/0.core -main moonbitlang/core/0 -o ./target/wasm-gc/release/build/0/0.wasm -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--target", "wasm", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/build/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc build-package ./2/lib.mbt -o ./target/wasm/release/build/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm/release/build/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc link-core ./target/wasm/release/build/1/1.core ./target/wasm/release/build/2/2.core -main moonbitlang/core/2 -o ./target/wasm/release/build/2/2.wasm -pkg-config-path ./2/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc link-core ./target/wasm/release/build/1/1.core -main moonbitlang/core/1 -o ./target/wasm/release/build/1/1.wasm -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt -o ./target/wasm/release/build/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm
            moonc link-core ./target/wasm/release/build/0/0.core -main moonbitlang/core/0 -o ./target/wasm/release/build/0/0.wasm -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -target wasm
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/build/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/build/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/build/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/1/1.core ./target/wasm-gc/release/build/2/2.core -main moonbitlang/core/2 -o ./target/wasm-gc/release/build/2/2.wasm -pkg-config-path ./2/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/1/1.core -main moonbitlang/core/1 -o ./target/wasm-gc/release/build/1/1.wasm -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/build/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc link-core ./target/wasm-gc/release/build/0/0.core -main moonbitlang/core/0 -o ./target/wasm-gc/release/build/0/0.wasm -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--target", "js", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/build/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js
            moonc build-package ./2/lib.mbt -o ./target/js/release/build/2/2.core -pkg moonbitlang/core/2 -i ./target/js/release/build/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc link-core ./target/js/release/build/1/1.core ./target/js/release/build/2/2.core -main moonbitlang/core/2 -o ./target/js/release/build/2/2.js -pkg-config-path ./2/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc link-core ./target/js/release/build/1/1.core -main moonbitlang/core/1 -o ./target/js/release/build/1/1.js -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -target js
            moonc build-package ./0/lib.mbt ./0/y.js.mbt -o ./target/js/release/build/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js
            moonc link-core ./target/js/release/build/0/0.core -main moonbitlang/core/0 -o ./target/js/release/build/0/0.js -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -target js
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_wbtest.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/1/1.whitebox_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -O0 -whitebox-test -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/1/1.whitebox_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.whitebox_test.wasm -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm-gc.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/0/0.whitebox_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g -O0 -whitebox-test -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/0/0.whitebox_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.whitebox_test.wasm -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["test", "--dry-run", "--target", "wasm", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target wasm --driver-kind whitebox --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt ./target/wasm/debug/test/1/__generated_driver_for_whitebox_test.mbt -o ./target/wasm/debug/test/1/1.whitebox_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm -g -O0 -whitebox-test -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/1/1.whitebox_test.core -main moonbitlang/core/1 -o ./target/wasm/debug/test/1/1.whitebox_test.wasm -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target wasm --driver-kind whitebox --mode test
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm.mbt ./target/wasm/debug/test/0/__generated_driver_for_whitebox_test.mbt -o ./target/wasm/debug/test/0/0.whitebox_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm -g -O0 -whitebox-test -no-mi -test-mode
            moonc link-core ./target/wasm/debug/test/0/0.whitebox_test.core -main moonbitlang/core/0 -o ./target/wasm/debug/test/0/0.whitebox_test.wasm -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm -g -O0
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["test", "--dry-run", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_wbtest.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/1/1.whitebox_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -O0 -whitebox-test -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/1/1.whitebox_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.whitebox_test.wasm -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target wasm-gc --driver-kind whitebox --mode test
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm-gc.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/0/0.whitebox_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g -O0 -whitebox-test -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/0/0.whitebox_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.whitebox_test.wasm -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["test", "--dry-run", "--target", "js", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target js --driver-kind whitebox --mode test
            moonc build-package ./1/lib.mbt ./1/x.js.mbt ./target/js/debug/test/1/__generated_driver_for_whitebox_test.mbt -o ./target/js/debug/test/1/1.whitebox_test.core -pkg moonbitlang/core/1 -is-main -pkg-sources moonbitlang/core/1:./1 -target js -g -O0 -whitebox-test -no-mi -test-mode
            moonc link-core ./target/js/debug/test/1/1.whitebox_test.core -main moonbitlang/core/1 -o ./target/js/debug/test/1/1.whitebox_test.js -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target js --driver-kind whitebox --mode test
            moonc build-package ./0/lib.mbt ./0/y.js.mbt ./0/y_wbtest.js.mbt ./0/y_wbtest.mbt ./target/js/debug/test/0/__generated_driver_for_whitebox_test.mbt -o ./target/js/debug/test/0/0.whitebox_test.core -pkg moonbitlang/core/0 -is-main -pkg-sources moonbitlang/core/0:./0 -target js -g -O0 -whitebox-test -no-mi -test-mode
            moonc link-core ./target/js/debug/test/0/0.whitebox_test.core -main moonbitlang/core/0 -o ./target/js/debug/test/0/0.whitebox_test.js -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -js-format cjs -no-dts -target js -g -O0
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["test", "--dry-run", "--enable-coverage", "--sort-input"],
        ),
        expect![[r#"
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/1 --sort-input --target wasm-gc --driver-kind whitebox --enable-coverage --mode test
            moonc build-package -o ./target/wasm-gc/debug/test/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc -g -O0 -enable-coverage -coverage-package-override=@self
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt ./1/x_wbtest.wasm-gc.mbt ./target/wasm-gc/debug/test/1/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/1/1.whitebox_test.core -pkg moonbitlang/core/1 -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/1:./1 -target wasm-gc -g -O0 -enable-coverage -whitebox-test -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/1/1.whitebox_test.core -main moonbitlang/core/1 -o ./target/wasm-gc/debug/test/1/1.whitebox_test.wasm -test-mode -pkg-config-path ./1/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/1:./1 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
            moon generate-test-driver --source-dir . --target-dir ./target --package moonbitlang/core/0 --sort-input --target wasm-gc --driver-kind whitebox --enable-coverage --mode test
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt ./0/y_wbtest.mbt ./0/y_wbtest.wasm-gc.mbt ./target/wasm-gc/debug/test/0/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/0/0.whitebox_test.core -pkg moonbitlang/core/0 -is-main -i ./target/wasm-gc/debug/test/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/0:./0 -target wasm-gc -g -O0 -enable-coverage -whitebox-test -no-mi -test-mode
            moonc link-core ./target/wasm-gc/debug/test/coverage/coverage.core ./target/wasm-gc/debug/test/0/0.whitebox_test.core -main moonbitlang/core/0 -o ./target/wasm-gc/debug/test/0/0.whitebox_test.wasm -test-mode -pkg-config-path ./0/moon.pkg.json -pkg-sources moonbitlang/core/coverage:./coverage -pkg-sources moonbitlang/core/0:./0 -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0
        "#]],
    );
    check(
        get_stdout(&dir, ["bundle", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc
            moonc bundle-core ./target/wasm-gc/release/bundle/0/0.core ./target/wasm-gc/release/bundle/1/1.core ./target/wasm-gc/release/bundle/2/2.core ./target/wasm-gc/release/bundle/coverage/coverage.core ./target/wasm-gc/release/bundle/char/char.core ./target/wasm-gc/release/bundle/iter/iter.core ./target/wasm-gc/release/bundle/prelude/prelude.core -o ./target/wasm-gc/release/bundle/core.core
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["bundle", "--dry-run", "--target", "wasm", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc build-package -o ./target/wasm/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt -o ./target/wasm/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm
            moonc build-package ./2/lib.mbt -o ./target/wasm/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc build-package -o ./target/wasm/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm
            moonc build-package -o ./target/wasm/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm
            moonc build-package -o ./target/wasm/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm
            moonc bundle-core ./target/wasm/release/bundle/0/0.core ./target/wasm/release/bundle/1/1.core ./target/wasm/release/bundle/2/2.core ./target/wasm/release/bundle/coverage/coverage.core ./target/wasm/release/bundle/char/char.core ./target/wasm/release/bundle/iter/iter.core ./target/wasm/release/bundle/prelude/prelude.core -o ./target/wasm/release/bundle/core.core
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["bundle", "--dry-run", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc
            moonc bundle-core ./target/wasm-gc/release/bundle/0/0.core ./target/wasm-gc/release/bundle/1/1.core ./target/wasm-gc/release/bundle/2/2.core ./target/wasm-gc/release/bundle/coverage/coverage.core ./target/wasm-gc/release/bundle/char/char.core ./target/wasm-gc/release/bundle/iter/iter.core ./target/wasm-gc/release/bundle/prelude/prelude.core -o ./target/wasm-gc/release/bundle/core.core
        "#]],
    );
    check(
        get_stdout(
            &dir,
            ["bundle", "--dry-run", "--target", "js", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js
            moonc build-package -o ./target/js/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target js
            moonc build-package ./0/lib.mbt ./0/y.js.mbt -o ./target/js/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js
            moonc build-package ./2/lib.mbt -o ./target/js/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/js/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc build-package -o ./target/js/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/js/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target js
            moonc build-package -o ./target/js/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/js/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target js
            moonc build-package -o ./target/js/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target js
            moonc bundle-core ./target/js/release/bundle/0/0.core ./target/js/release/bundle/1/1.core ./target/js/release/bundle/2/2.core ./target/js/release/bundle/coverage/coverage.core ./target/js/release/bundle/char/char.core ./target/js/release/bundle/iter/iter.core ./target/js/release/bundle/prelude/prelude.core -o ./target/js/release/bundle/core.core
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "bundle",
                "--target",
                "all",
                "--dry-run",
                "--sort-input",
                "--serial",
            ],
        ),
        expect![[r#"
            moonc build-package ./1/lib.mbt ./1/x.wasm.mbt -o ./target/wasm/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm
            moonc build-package -o ./target/wasm/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm
            moonc build-package ./0/lib.mbt ./0/y.wasm.mbt -o ./target/wasm/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm
            moonc build-package ./2/lib.mbt -o ./target/wasm/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm
            moonc build-package -o ./target/wasm/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm
            moonc build-package -o ./target/wasm/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm
            moonc build-package -o ./target/wasm/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm
            moonc bundle-core ./target/wasm/release/bundle/0/0.core ./target/wasm/release/bundle/1/1.core ./target/wasm/release/bundle/2/2.core ./target/wasm/release/bundle/coverage/coverage.core ./target/wasm/release/bundle/char/char.core ./target/wasm/release/bundle/iter/iter.core ./target/wasm/release/bundle/prelude/prelude.core -o ./target/wasm/release/bundle/core.core
            moonc build-package ./1/lib.mbt ./1/x.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target wasm-gc
            moonc build-package ./0/lib.mbt ./0/y.wasm-gc.mbt -o ./target/wasm-gc/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target wasm-gc
            moonc build-package ./2/lib.mbt -o ./target/wasm-gc/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/wasm-gc/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/wasm-gc/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target wasm-gc
            moonc build-package -o ./target/wasm-gc/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target wasm-gc
            moonc bundle-core ./target/wasm-gc/release/bundle/0/0.core ./target/wasm-gc/release/bundle/1/1.core ./target/wasm-gc/release/bundle/2/2.core ./target/wasm-gc/release/bundle/coverage/coverage.core ./target/wasm-gc/release/bundle/char/char.core ./target/wasm-gc/release/bundle/iter/iter.core ./target/wasm-gc/release/bundle/prelude/prelude.core -o ./target/wasm-gc/release/bundle/core.core
            moonc build-package ./1/lib.mbt ./1/x.js.mbt -o ./target/js/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target js
            moonc build-package -o ./target/js/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target js
            moonc build-package ./0/lib.mbt ./0/y.js.mbt -o ./target/js/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target js
            moonc build-package ./2/lib.mbt -o ./target/js/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/js/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target js
            moonc build-package -o ./target/js/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/js/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target js
            moonc build-package -o ./target/js/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/js/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target js
            moonc build-package -o ./target/js/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target js
            moonc bundle-core ./target/js/release/bundle/0/0.core ./target/js/release/bundle/1/1.core ./target/js/release/bundle/2/2.core ./target/js/release/bundle/coverage/coverage.core ./target/js/release/bundle/char/char.core ./target/js/release/bundle/iter/iter.core ./target/js/release/bundle/prelude/prelude.core -o ./target/js/release/bundle/core.core
            moonc build-package ./1/lib.mbt -o ./target/native/release/bundle/1/1.core -pkg moonbitlang/core/1 -pkg-sources moonbitlang/core/1:./1 -target native
            moonc build-package -o ./target/native/release/bundle/coverage/coverage.core -pkg moonbitlang/core/coverage -pkg-sources moonbitlang/core/coverage:./coverage -target native
            moonc build-package ./0/lib.mbt -o ./target/native/release/bundle/0/0.core -pkg moonbitlang/core/0 -pkg-sources moonbitlang/core/0:./0 -target native
            moonc build-package ./2/lib.mbt -o ./target/native/release/bundle/2/2.core -pkg moonbitlang/core/2 -i ./target/native/release/bundle/1/1.mi:1 -pkg-sources moonbitlang/core/2:./2 -target native
            moonc build-package -o ./target/native/release/bundle/char/char.core -pkg moonbitlang/core/char -i ./target/native/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/char:./char -target native
            moonc build-package -o ./target/native/release/bundle/iter/iter.core -pkg moonbitlang/core/iter -i ./target/native/release/bundle/coverage/coverage.mi:coverage -pkg-sources moonbitlang/core/iter:./iter -target native
            moonc build-package -o ./target/native/release/bundle/prelude/prelude.core -pkg moonbitlang/core/prelude -pkg-sources moonbitlang/core/prelude:./prelude -target native
            moonc bundle-core ./target/native/release/bundle/0/0.core ./target/native/release/bundle/1/1.core ./target/native/release/bundle/2/2.core ./target/native/release/bundle/coverage/coverage.core ./target/native/release/bundle/char/char.core ./target/native/release/bundle/iter/iter.core ./target/native/release/bundle/prelude/prelude.core -o ./target/native/release/bundle/core.core
        "#]],
    );
}
