use super::*;

#[test]
fn test_mbtx_test_graph_compiles_source_with_driver() {
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("script.mbtx"),
        "import { \"moonbitlang/core/int\" }\ntest { assert_eq(@int.MAX_VALUE, 2147483647) }\n",
    )
    .unwrap();
    crate::build_graph::assert(
        moon_cmd(&dir).args(["test", "script.mbtx", "--target", "wasm", "--dry-run"]),
        expect_file!["single_file_test/standalone.jsonl"],
    );
}

#[test]
fn test_mbtx_tests_accept_optional_main() {
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("script.mbtx"),
        r#"import {
  "moonbitlang/core/int",
}

fn main { println("script main must not run") }

/// ```mbt test
/// assert_eq(@single.maximum(), 2147483647)
/// ```
pub fn maximum() -> Int { @int.MAX_VALUE }

test "maximum" {
  assert_eq(maximum(), 2147483647)
}
"#,
    )
    .unwrap();

    for with_main in [true, false] {
        if !with_main {
            let source = read(dir.join("script.mbtx"));
            std::fs::write(
                dir.join("script.mbtx"),
                source.replace("fn main { println(\"script main must not run\") }", ""),
            )
            .unwrap();
        }
        moon_cmd(&dir)
            .args(["check", "script.mbtx"])
            .assert()
            .success();
        for backend in ["wasm", "wasm-gc", "js", "native"] {
            moon_cmd(&dir)
                .args(["test", "script.mbtx", "--target", backend])
                .assert()
                .success()
                .stdout_eq("Total tests: 2, passed: 2, failed: 0.\n");
        }
        moon_cmd(&dir)
            .args(["test", "script.mbtx", "--doc-index", "0"])
            .assert()
            .success()
            .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");
        for command in ["build", "run"] {
            let result = moon_cmd(&dir).args([command, "script.mbtx"]).assert();
            if with_main {
                result.success();
            } else {
                result.failure().stderr_eq(snapbox::str![[r#"
Error: [4067] Missing main function in the main package.
...
"#]]);
            }
        }
    }
}

#[test]
fn test_mbtx_tests_filter_fail_and_update() {
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("script.mbtx"),
        r#"fn main { println("script main must not run") }

test "selected" { assert_eq(1 + 1, 2) }
test "update" { inspect(2, content="1") }
"#,
    )
    .unwrap();

    moon_cmd(&dir)
        .args(["test", "script.mbtx", "--filter", "selected"])
        .assert()
        .success()
        .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");

    moon_cmd(&dir)
        .args(["test", "script.mbtx", "--index", "1"])
        .assert()
        .failure()
        .stdout_eq(snapbox::str![[r#"
[moon/test] test single/script.mbtx:4 ("update") failed
expect test failed at [..]/script.mbtx:4:17-4:40
Diff: (- expected, + actual)
----
-1
+2
----

Total tests: 1, passed: 0, failed: 1.

"#]]);

    moon_cmd(&dir)
        .args(["test", "script.mbtx", "--index", "1", "--update"])
        .assert()
        .success()
        .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");
    expect![[r#"
        fn main { println("script main must not run") }

        test "selected" { assert_eq(1 + 1, 2) }
        test "update" { inspect(2, content=(
          #|2
        )) }
    "#]]
    .assert_eq(&read(dir.join("script.mbtx")));

    moon_cmd(&dir)
        .args(["test", "script.mbtx"])
        .assert()
        .success()
        .stdout_eq("Total tests: 2, passed: 2, failed: 0.\n");
}

#[test]
fn test_mbtx_explicit_selection_inside_project() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("moon.mod"), "name = \"test/project\"\n").unwrap();
    std::fs::write(dir.join("moon.pkg"), "\n").unwrap();
    std::fs::write(
        dir.join("control.mbt"),
        "test \"package\" { println(\"package test ran\") }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("script.mbtx"),
        "fn main { println(\"script main must not run\") }\ntest \"script\" { println(\"script test ran\") }\n",
    )
    .unwrap();
    std::fs::write(dir.join("ignored.mbtx"), "not valid MoonBit").unwrap();

    // Package discovery continues to exclude standalone scripts.
    for args in [vec!["test"], vec!["test", "."], vec!["test", "control.mbt"]] {
        moon_cmd(&dir)
            .args(args)
            .assert()
            .success()
            .stdout_eq("package test ran\nTotal tests: 1, passed: 1, failed: 0.\n");
    }

    moon_cmd(&dir)
        .args(["test", "script.mbtx"])
        .assert()
        .success()
        .stdout_eq("script test ran\nTotal tests: 1, passed: 1, failed: 0.\n");

    // Explicit scripts do not load the surrounding project's manifests.
    std::fs::write(dir.join("moon.mod"), "not a valid manifest").unwrap();
    moon_cmd(&dir)
        .args(["test", "script.mbtx"])
        .assert()
        .success()
        .stdout_eq("script test ran\nTotal tests: 1, passed: 1, failed: 0.\n");

    for paths in [["script.mbtx", "."], ["script.mbtx", "ignored.mbtx"]] {
        moon_cmd(&dir)
            .arg("test")
            .args(paths)
            .assert()
            .failure()
            .stderr_eq("Error: standalone `.mbtx` `moon test` expects exactly one `PATH`\n");
    }
}

#[test]
fn test_single_mbt_tests_do_not_require_main() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("test.mbt"), "test { assert_eq(1, 1) }\n").unwrap();
    std::fs::write(
        dir.join("test.mbt.md"),
        "```mbt test\nassert_eq(1, 1)\n```\n",
    )
    .unwrap();
    for file in ["test.mbt", "test.mbt.md"] {
        moon_cmd(&dir)
            .args(["test", file])
            .assert()
            .success()
            .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");
    }
}
