use super::*;
use crate::build_graph::compare_graphs;
use expect_test::expect_file;

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

    let check_js_wasm_graph = dir.join("check_js_wasm.jsonl");
    snap_dry_run_graph(
        &dir,
        [
            "check",
            "--target",
            "js,wasm",
            "--dry-run",
            "--serial",
            "--nostd",
            "--sort-input",
        ],
        &check_js_wasm_graph,
    );
    compare_graphs(
        &check_js_wasm_graph,
        expect_file!["./many_targets_check_js_wasm.jsonl.snap"],
    );

    let build_js_wasm_graph = dir.join("build_js_wasm.jsonl");
    snap_dry_run_graph(
        &dir,
        [
            "build",
            "--target",
            "js,wasm",
            "--dry-run",
            "--serial",
            "--nostd",
            "--sort-input",
        ],
        &build_js_wasm_graph,
    );
    compare_graphs(
        &build_js_wasm_graph,
        expect_file!["./many_targets_build_js_wasm.jsonl.snap"],
    );

    let bundle_js_wasm_graph = dir.join("bundle_js_wasm.jsonl");
    snap_dry_run_graph(
        &dir,
        [
            "bundle",
            "--target",
            "js,wasm",
            "--dry-run",
            "--serial",
            "--nostd",
            "--sort-input",
        ],
        &bundle_js_wasm_graph,
    );
    compare_graphs(
        &bundle_js_wasm_graph,
        expect_file!["./many_targets_bundle_js_wasm.jsonl.snap"],
    );

    let test_js_wasm_graph = dir.join("test_js_wasm.jsonl");
    snap_dry_run_graph(
        &dir,
        [
            "test",
            "--target",
            "js,wasm",
            "--dry-run",
            "--serial",
            "--nostd",
            "--sort-input",
        ],
        &test_js_wasm_graph,
    );
    compare_graphs(
        &test_js_wasm_graph,
        expect_file!["./many_targets_test_js_wasm.jsonl.snap"],
    );

    let test_js_wasm_filtered_graph = dir.join("test_js_wasm_filtered.jsonl");
    snap_dry_run_graph(
        &dir,
        [
            "test",
            "--target",
            "js,wasm",
            "--dry-run",
            "--serial",
            "--nostd",
            "--sort-input",
            "-p",
            "username/hello/lib",
            "--file",
            "hello.mbt",
            "-i",
            "0",
        ],
        &test_js_wasm_filtered_graph,
    );
    compare_graphs(
        &test_js_wasm_filtered_graph,
        expect_file!["./many_targets_test_js_wasm_filtered.jsonl.snap"],
    );

    // Normalize dylib outputs to linux style
    let replacement_fn = |s: &mut String| {
        *s = s.replace(".dylib", ".so");
    };

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use crate::build_graph::compare_graphs_with_replacements;

        let graph = dir.join("test_js_wasm_all.jsonl");
        snap_dry_run_graph(
            &dir,
            [
                "test",
                "--target",
                "js,wasm,all",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
            &graph,
        );
        compare_graphs_with_replacements(
            &graph,
            expect_file!["./many_targets_test_js_wasm_all.jsonl.snap"],
            replacement_fn,
        );
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use crate::build_graph::compare_graphs_with_replacements;

        let graph = dir.join("test_all.jsonl");
        snap_dry_run_graph(
            &dir,
            [
                "test",
                "--target",
                "all",
                "--dry-run",
                "--serial",
                "--nostd",
                "--sort-input",
            ],
            &graph,
        );
        compare_graphs_with_replacements(
            &graph,
            expect_file!["./many_targets_test_all.jsonl.snap"],
            replacement_fn,
        );
    }
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
