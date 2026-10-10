use super::*;

mod n2_compaction;

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
fn test_many_targets_sync_dependencies_once() {
    let top_dir = TestDir::new("targets/sync_once");
    for command in ["check", "build", "test", "bench", "bundle"] {
        let stderr = get_stderr(&top_dir.join("app"), [command, "--target", "all"]);

        assert_eq!(
            stderr.matches("`bin-deps` is deprecated").count(),
            1,
            "explicit multi-target {command} should sync dependencies once, got:\n{stderr}"
        );
    }
}

#[test]
fn test_targets_share_prebuild_execution_state() {
    let dir = TestDir::new("targets/shared_n2_db");

    moon_cmd(&dir)
        .args(["check", "--target", "wasm"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![""])
        .stderr_eq(snapbox::str![[r#"
Finished. moon: ran 3 tasks, now up to date

"#]]);

    assert!(dir.join("_build/.moon_db").is_file());

    moon_cmd(&dir)
        .args(["build", "--target", "js", "--release"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![""])
        .stderr_eq(snapbox::str![[r#"
Finished. moon: ran 1 task, now up to date

"#]]);

    moon_cmd(&dir)
        .args(["check", "--target", "js"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![""])
        .stderr_eq(snapbox::str![[r#"
Finished. moon: ran 2 tasks, now up to date

"#]]);

    moon_cmd(&dir)
        .args(["check", "--target", "wasm,js"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![""])
        .stderr_eq(snapbox::str![[r#"
Finished. moon: no work to do

"#]]);
}

#[test]
fn progress_identifies_backend_and_prebuild_plan_kinds() {
    for (target, backend_plan_kinds) in [("wasm", &["wasm"][..]), ("wasm,js", &["wasm", "js"][..])]
    {
        let dir = TestDir::new("targets/shared_n2_db");

        moon_cmd(&dir)
            .args(["check", "--target", target, "-j1", "--trace", "--quiet"])
            .assert()
            .success();

        let trace = std::fs::read_to_string(dir.join("trace.json")).unwrap();
        let events: serde_json::Value = serde_json::from_str(&trace).unwrap();
        let descriptions = events
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["name"] == "n2.task" && event["ph"] == "B")
            .filter_map(|event| event["args"]["desc"].as_str())
            .map(|description| description.trim_matches('"'))
            .collect::<Vec<_>>();

        for plan_kind in backend_plan_kinds {
            assert!(descriptions.iter().any(|description| {
                description.starts_with("check ")
                    && description.ends_with(&format!("({plan_kind})"))
            }));
        }
        assert!(descriptions.iter().any(|description| {
            description.starts_with("run script ")
                && description.ends_with("generated.txt (prebuild)")
        }));
    }
}

#[test]
fn multi_target_tests_wait_for_every_backend_to_build() {
    let dir = TestDir::new("targets/build_barrier");

    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .args(["test", "--target", "wasm,js", "--serial"])
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_DEP_CACHE", "off")
        .current_dir(&dir)
        .assert()
        .code(1);
}

#[test]
fn test_many_targets_auto_update_001() {
    let dir = TestDir::new("targets/auto_update");
    let _ = get_stdout(
        &dir,
        ["test", "--target", "wasm-gc", "-u", "--no-parallelize"],
    );
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
