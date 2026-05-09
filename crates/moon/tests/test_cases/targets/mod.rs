use super::*;

#[test]
fn test_many_targets() {
    let dir = TestDir::new("targets/many_targets");
    check(
        get_stdout(&dir, ["test", "--target", "all", "--serial"]),
        expect![[r#"
            Total tests: 2, passed: 2, failed: 0. [wasm]
            Total tests: 2, passed: 2, failed: 0. [wasm-gc]
            Total tests: 2, passed: 2, failed: 0. [js]
            Total tests: 2, passed: 2, failed: 0. [native]
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--target", "js,wasm", "--serial"]),
        expect![[r#"
            Total tests: 2, passed: 2, failed: 0. [wasm]
            Total tests: 2, passed: 2, failed: 0. [js]
        "#]],
    );
}

#[test]
fn test_many_targets_auto_update_001() {
    let dir = TestDir::new("targets/auto_update");
    let _ = get_stdout(&dir, ["test", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.wasm.mbt")),
        expect![[r#"
            test {
              inspect("wasm")
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.wasm-gc.mbt")),
        expect![[r#"
            test {
              inspect("wasm-gc", content=(#|wasm-gc
              ))
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.js.mbt")),
        expect![[r#"
            test {
              inspect("js")
            }
        "#]],
    );

    check(
        read(dir.join("lib").join("x.native.mbt")),
        expect![[r#"
                test {
                  inspect("native")
                }
            "#]],
    );
}

#[test]
fn test_many_targets_auto_update_002() {
    let dir = TestDir::new("targets/auto_update");
    let _ = get_stdout(&dir, ["test", "--target", "js", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.wasm.mbt")),
        expect![[r#"
            test {
              inspect("wasm")
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.wasm-gc.mbt")),
        expect![[r#"
            test {
              inspect("wasm-gc")
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.js.mbt")),
        expect![[r#"
            test {
              inspect("js", content=(#|js
              ))
            }
        "#]],
    );

    check(
        read(dir.join("lib").join("x.native.mbt")),
        expect![[r#"
            test {
              inspect("native")
            }
            "#]],
    );

    let _ = get_stdout(
        &dir,
        ["test", "--target", "native", "-u", "--no-parallelize"],
    );
    check(
        read(dir.join("lib").join("x.native.mbt")),
        expect![[r#"
            test {
              inspect("native", content=(#|native
              ))
            }
        "#]],
    );
}

#[test]
fn test_many_targets_auto_update_003() {
    let dir = TestDir::new("targets/auto_update");
    let _ = get_stdout(&dir, ["test", "--target", "wasm", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.wasm.mbt")),
        expect![[r#"
            test {
              inspect("wasm", content=(#|wasm
              ))
            }
        "#]],
    );
    check(
        read(dir.join("lib").join("x.wasm-gc.mbt")),
        expect![[r#"
            test {
              inspect("wasm-gc")
            }
        "#]],
    );
    let _ = get_stdout(&dir, ["test", "--target", "js", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.js.mbt")),
        expect![[r#"
            test {
              inspect("js", content=(#|js
              ))
            }
        "#]],
    );
}

#[test]
fn test_many_targets_auto_update_004() {
    let dir = TestDir::new("targets/auto_update");
    let _ = get_stdout(&dir, ["test", "--target", "wasm", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.wasm.mbt")),
        expect![[r#"
            test {
              inspect("wasm", content=(#|wasm
              ))
            }
        "#]],
    );
    let _ = get_stdout(
        &dir,
        ["test", "--target", "wasm-gc", "-u", "--no-parallelize"],
    );
    check(
        read(dir.join("lib").join("x.wasm-gc.mbt")),
        expect![[r#"
            test {
              inspect("wasm-gc", content=(#|wasm-gc
              ))
            }
        "#]],
    );
    let _ = get_stdout(&dir, ["test", "--target", "js", "-u", "--no-parallelize"]);
    check(
        read(dir.join("lib").join("x.js.mbt")),
        expect![[r#"
            test {
              inspect("js", content=(#|js
              ))
            }
        "#]],
    );
}

#[test]
fn test_many_targets_expect_failed() {
    let dir = TestDir::new("targets/expect_failed");
    check(
        get_err_stdout(
            &dir,
            ["test", "--target", "all", "--serial", "--sort-input"],
        ),
        expect![[r#"
            [username/hello] test lib/x.wasm.mbt:1 (#0) failed
            expect test failed at $ROOT/lib/x.wasm.mbt:2:3-2:31
            Diff: (- expected, + actual)
            ----
            -0
            +wasm
            ----

            Total tests: 1, passed: 0, failed: 1. [wasm]
            [username/hello] test lib/x.wasm-gc.mbt:1 (#0) failed
            expect test failed at $ROOT/lib/x.wasm-gc.mbt:2:3-2:34
            Diff: (- expected, + actual)
            ----
            -1
            +wasm-gc
            ----

            Total tests: 1, passed: 0, failed: 1. [wasm-gc]
            [username/hello] test lib/x.js.mbt:1 (#0) failed
            expect test failed at $ROOT/lib/x.js.mbt:2:3-2:29
            Diff: (- expected, + actual)
            ----
            -2
            +js
            ----

            Total tests: 1, passed: 0, failed: 1. [js]
            [username/hello] test lib/x.native.mbt:1 (#0) failed
            expect test failed at $ROOT/lib/x.native.mbt:2:3-2:33
            Diff: (- expected, + actual)
            ----
            -3
            +native
            ----

            Total tests: 1, passed: 0, failed: 1. [native]
        "#]],
    );
    check(
        get_err_stdout(
            &dir,
            ["test", "--target", "js,wasm", "--sort-input", "--serial"],
        ),
        expect![[r#"
            [username/hello] test lib/x.wasm.mbt:1 (#0) failed
            expect test failed at $ROOT/lib/x.wasm.mbt:2:3-2:31
            Diff: (- expected, + actual)
            ----
            -0
            +wasm
            ----

            Total tests: 1, passed: 0, failed: 1. [wasm]
            [username/hello] test lib/x.js.mbt:1 (#0) failed
            expect test failed at $ROOT/lib/x.js.mbt:2:3-2:29
            Diff: (- expected, + actual)
            ----
            -2
            +js
            ----

            Total tests: 1, passed: 0, failed: 1. [js]
        "#]],
    );

    check(
        get_err_stdout(
            &dir,
            [
                "test",
                "--target",
                "js,wasm,native",
                "--sort-input",
                "--serial",
            ],
        ),
        expect![[r#"
            [username/hello] test lib/x.wasm.mbt:1 (#0) failed
            expect test failed at $ROOT/lib/x.wasm.mbt:2:3-2:31
            Diff: (- expected, + actual)
            ----
            -0
            +wasm
            ----

            Total tests: 1, passed: 0, failed: 1. [wasm]
            [username/hello] test lib/x.js.mbt:1 (#0) failed
            expect test failed at $ROOT/lib/x.js.mbt:2:3-2:29
            Diff: (- expected, + actual)
            ----
            -2
            +js
            ----

            Total tests: 1, passed: 0, failed: 1. [js]
            [username/hello] test lib/x.native.mbt:1 (#0) failed
            expect test failed at $ROOT/lib/x.native.mbt:2:3-2:33
            Diff: (- expected, + actual)
            ----
            -3
            +native
            ----

            Total tests: 1, passed: 0, failed: 1. [native]
        "#]],
    );
}
