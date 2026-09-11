use expect_test::{expect, expect_file};

use crate::{
    TestDir, build_graph, get_err_stderr, get_stdout, moon_cmd,
    util::{check, moon_bin},
};

#[test]
fn test_blackbox_test_core_override() {
    let dir = TestDir::new("blackbox_test_core_override.in");

    // Blackbox compilation must omit coverage instrumentation and the self override.
    build_graph::assert(
        moon_cmd(&dir).args([
            "test",
            "--target",
            "wasm-gc",
            "--enable-coverage",
            "--dry-run",
            "--sort-input",
        ]),
        expect_file!["test_blackbox_test_core_override.jsonl.snap"],
    )
    .stdout_eq(snapbox::str![[r#"
...
moonc build-package ./builtin/main_test.mbt ./_build/wasm-gc/debug/test/builtin/__generated_driver_for_blackbox_test.mbt -doctest-only ./builtin/main.mbt -o ./_build/wasm-gc/debug/test/builtin/builtin.blackbox_test.core -pkg moonbitlang/core/builtin_blackbox_test -pkg-type executable -i ./_build/wasm-gc/debug/test/builtin/builtin.mi:builtin -i ./_build/wasm-gc/debug/test/prelude/prelude.mi:prelude -pkg-sources moonbitlang/core/builtin_blackbox_test:./builtin -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path . -all-pkgs ./_build/wasm-gc/debug/test/all_pkgs.json
...
"#]]);
}

#[test]
fn test_blackbox_success() {
    let dir = TestDir::new("blackbox_success_test.in");

    build_graph::assert(
        moon_cmd(&dir).args([
            "test",
            "--target",
            "wasm-gc",
            "-p",
            "username/hello/A",
            "--file",
            "hello_test.mbt",
            "-i",
            "0",
            "--nostd",
            "--sort-input",
            "--dry-run",
        ]),
        expect_file!["test_blackbox_success_test.jsonl.snap"],
    );

    check(
        get_stdout(
            &dir,
            [
                "test",
                "--target",
                "wasm-gc",
                "-p",
                "username/hello/A",
                "--file",
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
        get_stdout(&dir, ["test", "--target", "wasm-gc"]),
        expect![[r#"
            output from A/hello.mbt!
            output from C/hello.mbt!
            output from D/hello.mbt!
            self.a: 33
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    build_graph::assert(
        moon_cmd(&dir).args(["check", "--target", "wasm-gc", "--sort-input", "--dry-run"]),
        expect_file!["test_blackbox_success_check.jsonl.snap"],
    );

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args(["check", "--target", "wasm-gc", "--sort-input"])
        .assert()
        .success();

    #[cfg(unix)]
    {
        use crate::util::replace_dir;

        let p = crate::scoped_packages_json_path(&dir, "wasm-gc", "debug");
        expect_file!["test_blackbox_success_packages.json.snap"]
            .assert_eq(&replace_dir(&std::fs::read_to_string(p).unwrap(), &dir));
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
    assert!(output.contains("Unused variable 'a'"));
    assert!(output.contains("Unused variable 'b'"));
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
   │                 ╰───────────── Warning (unused_package): Unused package 'username/hello/dir/lib'
───╯
    "#
            .trim()
        )
    );
}
