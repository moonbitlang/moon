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

    let graph_1 = dir.join("test.jsonl");
    let _output_1 = snap_dry_run_graph(
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
        &graph_1,
    );
    compare_graphs(
        &graph_1,
        expect_file!["test_blackbox_success_test.jsonl.snap"],
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

    let graph_2 = dir.join("check.jsonl");
    let _output_2 = snap_dry_run_graph(&dir, ["check", "--sort-input", "--dry-run"], &graph_2);
    compare_graphs(
        &graph_2,
        expect_file!["test_blackbox_success_check.jsonl.snap"],
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
