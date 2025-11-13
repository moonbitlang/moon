use expect_test::{expect, expect_file};

use crate::{
    TestDir,
    build_graph::compare_graphs,
    get_err_stderr, get_stdout, snap_dry_run_graph,
    util::{check, moon_bin},
};

#[test]
fn test_blackbox_test_core_override() {
    let dir = TestDir::new("blackbox_test_core_override.in");

    let graph = dir.join("out.jsonl");
    let output = snap_dry_run_graph(
        &dir,
        ["test", "--enable-coverage", "--dry-run", "--sort-input"],
        &graph,
    );
    compare_graphs(
        &graph,
        expect_file!["test_blackbox_test_core_override.jsonl.snap"],
    );

    let mut found = false;
    for line in output.lines() {
        // For the command compiling builtin's blackbox tests,
        if line.contains("moonc build-package") && line.contains("builtin_blackbox_test") {
            found = true;
            // it should not have the -enable-coverage flag
            assert!(
                !line.contains("-enable-coverage"),
                "Black box tests themselves should not contain coverage, since all they contain are tests of various kinds. {line}"
            );
            // and should not contain -coverage-package-override to itself
            assert!(
                !line.contains("-coverage-package-override=@self"),
                "Unexpected -coverage-package-override=@self found in the command: {line}"
            );
        }
    }
    assert!(found, "builtin's blackbox tests not found in the output");
}

#[test]
fn test_blackbox_success() {
    let dir = TestDir::new("blackbox_success_test.in");

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "-f",
                "hello_test.mbt",
                "-i",
                "0",
                "--nostd",
                "--sort-input",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc build-package ./D/hello.mbt -o ./target/wasm-gc/debug/test/D/D.core -pkg username/hello/D -pkg-sources username/hello/D:./D -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./B/hello.mbt -o ./target/wasm-gc/debug/test/B/B.core -pkg username/hello/B -pkg-sources username/hello/B:./B -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/A/__generated_driver_for_whitebox_test.mbt --output-metadata ./target/wasm-gc/debug/test/A/__whitebox_test_info.json ./A/hello_wbtest.mbt --target wasm-gc --pkg-name username/hello/A --driver-kind whitebox
            moonc build-package ./A/hello.mbt ./A/hello_wbtest.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_whitebox_test.mbt -o ./target/wasm-gc/debug/test/A/A.whitebox_test.core -pkg username/hello/A -is-main -i ./target/wasm-gc/debug/test/B/B.mi:B -i ./target/wasm-gc/debug/test/D/D.mi:D -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/B/B.core ./target/wasm-gc/debug/test/D/D.core ./target/wasm-gc/debug/test/A/A.whitebox_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.whitebox_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/B:./B -pkg-sources username/hello/D:./D -pkg-sources username/hello/A:./A -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt --output-metadata ./target/wasm-gc/debug/test/A/__internal_test_info.json ./A/hello.mbt --target wasm-gc --pkg-name username/hello/A --driver-kind internal
            moonc build-package ./A/hello.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_internal_test.mbt -o ./target/wasm-gc/debug/test/A/A.internal_test.core -pkg username/hello/A -is-main -i ./target/wasm-gc/debug/test/D/D.mi:D -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/D/D.core ./target/wasm-gc/debug/test/A/A.internal_test.core -main username/hello/A -o ./target/wasm-gc/debug/test/A/A.internal_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/D:./D -pkg-sources username/hello/A:./A -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moonc build-package ./C/hello.mbt -o ./target/wasm-gc/debug/test/C/C.core -pkg username/hello/C -pkg-sources username/hello/C:./C -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./A/hello.mbt -o ./target/wasm-gc/debug/test/A/A.core -pkg username/hello/A -i ./target/wasm-gc/debug/test/D/D.mi:D -pkg-sources username/hello/A:./A -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./target/wasm-gc/debug/test/A/__generated_driver_for_blackbox_test.mbt --output-metadata ./target/wasm-gc/debug/test/A/__blackbox_test_info.json ./A/hello_test.mbt --doctest-only ./A/hello.mbt --target wasm-gc --pkg-name username/hello/A --driver-kind blackbox
            moonc build-package ./A/hello_test.mbt ./target/wasm-gc/debug/test/A/__generated_driver_for_blackbox_test.mbt -doctest-only ./A/hello.mbt -o ./target/wasm-gc/debug/test/A/A.blackbox_test.core -pkg username/hello/A_blackbox_test -is-main -i ./target/wasm-gc/debug/test/A/A.mi:A -i ./target/wasm-gc/debug/test/C/C.mi:C -i ./target/wasm-gc/debug/test/D/D.mi:D -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core ./target/wasm-gc/debug/test/C/C.core ./target/wasm-gc/debug/test/D/D.core ./target/wasm-gc/debug/test/A/A.core ./target/wasm-gc/debug/test/A/A.blackbox_test.core -main username/hello/A_blackbox_test -o ./target/wasm-gc/debug/test/A/A.blackbox_test.wasm -test-mode -pkg-config-path ./A/moon.pkg.json -pkg-sources username/hello/C:./C -pkg-sources username/hello/D:./D -pkg-sources username/hello/A:./A -pkg-sources username/hello/A_blackbox_test:./A -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "-p",
                "username/hello/A",
                "-f",
                "hello_test.mbt",
                "-i",
                "0",
            ],
        ),
        expect![[r#"
            output from A/hello.mbt!
            output from C/hello.mbt!
            output from D/hello.mbt!
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["test"]),
        expect![[r#"
            output from A/hello.mbt!
            output from C/hello.mbt!
            output from D/hello.mbt!
            self.a: 33
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["check", "--sort-input", "--dry-run"]),
        expect![[r#"
            moonc check ./main/main.mbt -o ./target/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path .
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./target/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/main/main.mi:main -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./D/hello.mbt -o ./target/wasm-gc/release/check/D/D.mi -pkg username/hello/D -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/D:./D -target wasm-gc -workspace-path .
            moonc check -doctest-only ./D/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/D/D.blackbox_test.mi -pkg username/hello/D_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/D/D.mi:D -pkg-sources username/hello/D_blackbox_test:./D -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./C/hello.mbt -o ./target/wasm-gc/release/check/C/C.mi -pkg username/hello/C -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/C:./C -target wasm-gc -workspace-path .
            moonc check -doctest-only ./C/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/C/C.blackbox_test.mi -pkg username/hello/C_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/C/C.mi:C -pkg-sources username/hello/C_blackbox_test:./C -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./B/hello.mbt -o ./target/wasm-gc/release/check/B/B.mi -pkg username/hello/B -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -pkg-sources username/hello/B:./B -target wasm-gc -workspace-path .
            moonc check -doctest-only ./B/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/B/B.blackbox_test.mi -pkg username/hello/B_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/B/B.mi:B -pkg-sources username/hello/B_blackbox_test:./B -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./A/hello.mbt ./A/hello_wbtest.mbt -o ./target/wasm-gc/release/check/A/A.whitebox_test.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/B/B.mi:B -i ./target/wasm-gc/release/check/D/D.mi:D -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path .
            moonc check ./A/hello.mbt -o ./target/wasm-gc/release/check/A/A.mi -pkg username/hello/A -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/D/D.mi:D -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path .
            moonc check ./A/hello_test.mbt -doctest-only ./A/hello.mbt -include-doctests -o ./target/wasm-gc/release/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i ./target/wasm-gc/release/check/A/A.mi:A -i ./target/wasm-gc/release/check/C/C.mi:C -i ./target/wasm-gc/release/check/D/D.mi:D -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path .
        "#]],
    );

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        use crate::util::replace_dir;

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
                      "root-path": "$ROOT/A",
                      "root": "username/hello",
                      "rel": "A",
                      "files": {
                        "$ROOT/A/hello.mbt": {
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
                      "wbtest-files": {
                        "$ROOT/A/hello_wbtest.mbt": {
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
                      "test-files": {
                        "$ROOT/A/hello_test.mbt": {
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
                          "path": "username/hello/D",
                          "alias": "D",
                          "fspath": "$ROOT/D"
                        }
                      ],
                      "wbtest-deps": [
                        {
                          "path": "username/hello/B",
                          "alias": "B",
                          "fspath": "$ROOT/B"
                        }
                      ],
                      "test-deps": [
                        {
                          "path": "username/hello/C",
                          "alias": "C",
                          "fspath": "$ROOT/C"
                        }
                      ],
                      "artifact": "$ROOT/target/wasm-gc/release/check/A/A.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/B",
                      "root": "username/hello",
                      "rel": "B",
                      "files": {
                        "$ROOT/B/hello.mbt": {
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
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/B/B.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/C",
                      "root": "username/hello",
                      "rel": "C",
                      "files": {
                        "$ROOT/C/hello.mbt": {
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
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/C/C.mi"
                    },
                    {
                      "is-main": false,
                      "is-third-party": false,
                      "root-path": "$ROOT/D",
                      "root": "username/hello",
                      "rel": "D",
                      "files": {
                        "$ROOT/D/hello.mbt": {
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
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/D/D.mi"
                    },
                    {
                      "is-main": true,
                      "is-third-party": false,
                      "root-path": "$ROOT/main",
                      "root": "username/hello",
                      "rel": "main",
                      "files": {
                        "$ROOT/main/main.mbt": {
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
                      "deps": [],
                      "wbtest-deps": [],
                      "test-deps": [],
                      "artifact": "$ROOT/target/wasm-gc/release/check/main/main.mi"
                    }
                  ],
                  "deps": [],
                  "backend": "wasm-gc",
                  "opt_level": "release",
                  "source": null
                }"#]],
        );
    }
}

#[test]
fn test_blackbox_failed() {
    let dir = TestDir::new("blackbox_failed_test.in");

    let output = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .arg("test")
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();

    let output = String::from_utf8_lossy(&output);
    // bbtest can not use private function in bbtest_import
    assert!(output.contains("Value _private_hello not found in package `A`"));
    // bbtest_import could no be used in _wbtest.mbt
    assert!(output.contains("Package \"C\" not found in the loaded packages."));

    let output = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check"])
        .assert()
        .failure()
        .get_output()
        .stderr
        .to_owned();

    let output = String::from_utf8_lossy(&output);
    assert!(output.contains("Warning: Unused variable 'a'"));
    assert!(output.contains("Warning: Unused variable 'b'"));
    assert!(output.contains("Value _private_hello not found in package `A`"));
    assert!(output.contains("Package \"C\" not found in the loaded packages."));
}

#[test]
fn test_blackbox_dedup_alias() {
    let dir = TestDir::new("blackbox_test_dedup_alias.in");
    let output = get_err_stderr(&dir, ["test"]);
    println!("{}", output);
    assert!(output.contains(
        "Duplicate alias `lib` at \"$ROOT/lib/moon.pkg.json\". \"test-import\" will automatically add \"import\" and current package as dependency so you don't need to add it manually. If you're test-importing a dependency with the same default alias as your current package, considering give it a different alias than the current package."
    ));
    assert!(
        output.contains(
            r#"
Error: [4021]
   ╭─[ $ROOT/lib/hello_test.mbt:3:3 ]
   │
 3 │   @lib.hello()
   │   ─────┬────  
   │        ╰────── Value hello not found in package `lib`.
───╯
Warning: [0029]
   ╭─[ $ROOT/lib/moon.pkg.json:3:5 ]
   │
 3 │     "username/hello/dir/lib"
   │     ────────────┬───────────  
   │                 ╰───────────── Warning: Unused package 'username/hello/dir/lib'
───╯
    "#
            .trim()
        )
    );
}
