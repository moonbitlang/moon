use super::*;

#[cfg(unix)]
#[test]
fn test_mbtx_symlink_preserves_script_semantics() {
    let dir = TestDir::new_empty();
    std::fs::create_dir(dir.join("sources")).unwrap();
    std::fs::write(dir.join("sources/source.mbt"), "test { assert_eq(1, 1) }\n").unwrap();
    std::os::unix::fs::symlink("sources/source.mbt", dir.join("alias.mbtx")).unwrap();

    moon_cmd(&dir)
        .args(["test", "alias.mbtx"])
        .assert()
        .failure()
        .stderr_eq("Error: [4067] Missing main function in the main package.\n");

    std::fs::write(
        dir.join("sources/source.mbt"),
        "----\nimport { \"moonbitlang/core/int\", }\n----\nfn main { println(\"script main ran\") }\ntest { assert_eq(@int.MAX_VALUE, 2147483647) }\n",
    )
    .unwrap();
    moon_cmd(&dir)
        .args(["test", "alias.mbtx"])
        .assert()
        .success()
        .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");

    for command in ["build", "check"] {
        moon_cmd(&dir)
            .args([command, "alias.mbtx"])
            .assert()
            .success();
    }
    moon_cmd(&dir)
        .args(["run", "alias.mbtx"])
        .assert()
        .success()
        .stdout_eq("script main ran\n");
}

#[cfg(unix)]
#[test]
fn test_old_extension_symlinks_require_project() {
    let dir = TestDir::new_empty();
    std::fs::create_dir(dir.join("sources")).unwrap();
    std::fs::write(
        dir.join("sources/source.mbtx"),
        "fn main { println(1) }\ntest { assert_eq(1, 1) }\n",
    )
    .unwrap();
    for alias in ["alias.mbt", "alias.mbt.md"] {
        std::os::unix::fs::symlink("sources/source.mbtx", dir.join(alias)).unwrap();
        for command in ["check", "run", "test"] {
            moon_cmd(&dir)
                .env(MOON_WORK_ENV, "auto")
                .args([command, alias])
                .assert()
                .failure()
                .stdout_eq("")
                .stderr_eq(snapbox::str![[r#"
Error: not in a Moon project (no moon.mod, moon.mod.json, or moon.work found starting from [..] or its ancestors)

"#]]);
            assert!(!dir.join("_build").exists());
            assert!(!dir.join(".mooncakes").exists());
            assert!(!dir.join("sources/_build").exists());
            assert!(!dir.join("sources/.mooncakes").exists());
        }
    }
}

#[test]
fn test_mbtx_requires_main() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("script.mbtx"), "test { assert_eq(1, 1) }\n").unwrap();
    moon_cmd(&dir)
        .args(["test", "script.mbtx"])
        .assert()
        .failure()
        .stderr_eq("Error: [4067] Missing main function in the main package.\n");
}

#[test]
fn test_mbtx_runs_tests_without_running_main() {
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("script.mbtx"),
        r#"----
import {
  "moonbitlang/core/int",
}
----

fn main { println("script main must not run") }

/// ```mbt check
/// test {
/// assert_eq(@single.maximum(), 2147483647)
/// }
/// ```
pub fn maximum() -> Int { @int.MAX_VALUE }

test "maximum" {
  assert_eq(maximum(), 2147483647)
}
"#,
    )
    .unwrap();

    for backend in ["wasm", "wasm-gc", "js", "native"] {
        moon_cmd(&dir)
            .args(["test", "script.mbtx", "--target", backend])
            .assert()
            .success()
            .stdout_eq("Total tests: 2, passed: 2, failed: 0.\n");
    }

    moon_cmd(&dir)
        .args(["test", "script.mbtx", "--target", "all"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
Total tests: 2, passed: 2, failed: 0. [wasm]
Total tests: 2, passed: 2, failed: 0. [wasm-gc]
Total tests: 2, passed: 2, failed: 0. [js]
Total tests: 2, passed: 2, failed: 0. [native]

"#]]);

    moon_cmd(&dir)
        .args(["test", "script.mbtx", "--target", "js,wasm"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
Total tests: 2, passed: 2, failed: 0. [wasm]
Total tests: 2, passed: 2, failed: 0. [js]

"#]]);
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
        .args(["test", "script.mbtx", "--target", "wasm,js", "--update"])
        .assert()
        .failure()
        .stderr_eq("Error: cannot update test on multiple targets\n");

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
fn test_mbtx_directory_selects_package() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("moon.mod"), "name = \"test/project\"\n").unwrap();
    for package in ["foo.mbtx", "other"] {
        std::fs::create_dir(dir.join(package)).unwrap();
        std::fs::write(dir.join(package).join("moon.pkg"), "\n").unwrap();
        std::fs::write(
            dir.join(package).join("test.mbt"),
            "test { assert_eq(1, 1) }\n",
        )
        .unwrap();
    }

    // The shared script selector must leave package directories to each command.
    for command in ["build", "check", "test"] {
        moon_cmd(&dir)
            .args([command, "foo.mbtx"])
            .assert()
            .success();
        moon_cmd(&dir)
            .args([command, "foo.mbtx", "other"])
            .assert()
            .success();
    }

    moon_cmd(&dir)
        .args(["test", "foo.mbtx"])
        .assert()
        .success()
        .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");
}

#[test]
fn test_project_mbt_and_markdown_tests_do_not_require_main() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("moon.mod"), "name = \"test/project\"\n").unwrap();
    std::fs::write(dir.join("moon.pkg"), "\n").unwrap();
    std::fs::write(dir.join("test.mbt"), "test { assert_eq(1, 1) }\n").unwrap();
    std::fs::write(
        dir.join("test.mbt.md"),
        "```mbt check\ntest { assert_eq(1, 1) }\n```\n",
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

#[test]
fn test_old_extensions_select_project_package_and_tests() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("moon.mod"), "name = \"test/project\"\n").unwrap();
    std::fs::create_dir(dir.join("app")).unwrap();
    std::fs::write(dir.join("app/moon.pkg"), "pkgtype(kind: \"executable\")\n").unwrap();
    std::fs::write(
        dir.join("app/main.mbt"),
        "fn main { println(value()) }\ntest \"selected\" { assert_eq(value(), 42) }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app/helper.mbt"),
        "pub fn value() -> Int { 42 }\ntest \"other\" { println(\"other file test\") }\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("app/readme.mbt.md"),
        "---\nmoonbit:\n  deps: {}\n  backend: js\n---\n```mbt check\ntest { assert_eq(@app.value(), 42) }\n```\n",
    )
    .unwrap();

    for file in ["app/main.mbt", "app/readme.mbt.md"] {
        for path in [PathBuf::from(file), dir.join(file)] {
            moon_cmd(&dir)
                .arg("run")
                .arg(&path)
                .assert()
                .success()
                .stdout_eq("42\n");
            moon_cmd(&dir)
                .arg("test")
                .arg(&path)
                .assert()
                .success()
                .stdout_eq("Total tests: 1, passed: 1, failed: 0.\n");
        }
    }

    // Run keeps finding the project from the input path, even from another project.
    let caller = TestDir::new_empty();
    std::fs::write(caller.join("moon.mod"), "name = \"test/caller\"\n").unwrap();
    moon_cmd(&caller)
        .arg("run")
        .arg(dir.join("app/main.mbt"))
        .assert()
        .success()
        .stdout_eq("42\n");

    std::fs::write(dir.join("app/moon.pkg"), "\n").unwrap();
    std::fs::write(
        dir.join("app/main.mbt"),
        "test { assert_eq(value(), 42) }\n",
    )
    .unwrap();
    for file in ["app/main.mbt", "app/readme.mbt.md"] {
        moon_cmd(&dir)
            .args(["run", file])
            .assert()
            .failure()
            .stderr_eq(format!("Error: `{file}` is not a main package\n"));
    }
}

#[test]
fn test_old_extensions_without_package_do_not_run_standalone() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("moon.mod"), "name = \"test/project\"\n").unwrap();
    std::fs::create_dir(dir.join("orphan")).unwrap();
    for file in ["script.mbt", "script.mbt.md"] {
        std::fs::write(dir.join("orphan").join(file), "fn main { println(1) }\n").unwrap();
        for command in ["run", "test"] {
            for mode in [None, Some("--dry-run"), Some("--build-only")] {
                moon_cmd(&dir)
                    .arg(command)
                    .arg(dir.join("orphan").join(file))
                    .args(mode)
                    .assert()
                    .failure()
                    .stdout_eq("")
                    .stderr_eq(snapbox::str![[r#"
Error: Cannot find package to build based on input path `[..]/orphan`.
Hint: The provided path `[..]/orphan` is inside the packages directory of module `test/project` at `[..]`, but the directory itself does not contain `moon.pkg` or `moon.pkg.json`, so it is not a package.

"#]]);
                assert!(!dir.join("orphan/_build").exists());
                assert!(!dir.join("orphan/.mooncakes").exists());
            }
        }
    }
}

#[test]
fn test_run_mbtx_directory_selects_package() {
    let dir = TestDir::new_empty();
    std::fs::write(dir.join("moon.mod"), "name = \"test/project\"\n").unwrap();
    std::fs::create_dir(dir.join("app.mbtx")).unwrap();
    std::fs::write(
        dir.join("app.mbtx/moon.pkg"),
        "pkgtype(kind: \"executable\")\n",
    )
    .unwrap();
    std::fs::write(dir.join("app.mbtx/main.mbt"), "fn main { println(42) }\n").unwrap();
    moon_cmd(&dir)
        .args(["run", "app.mbtx"])
        .assert()
        .success()
        .stdout_eq("42\n");
    assert!(!dir.join("app.mbtx/_build").exists());
}
