use super::*;

fn assert_deny_warn_appended_to_custom_warn_list(stdout: &str, command_prefix: &str) {
    let mut checked_commands = 0;

    for command in stdout
        .lines()
        .filter(|line| line.starts_with(command_prefix))
    {
        let args = command.split_whitespace().collect::<Vec<_>>();
        let warn_lists = args
            .windows(2)
            .filter_map(|pair| (pair[0] == "-w").then_some(pair[1]))
            .collect::<Vec<_>>();

        assert_eq!(
            warn_lists.len(),
            1,
            "expected a single warning list in command:\n{command}"
        );
        let warn_list = warn_lists[0];
        assert!(
            warn_list.ends_with("@a") && warn_list.len() > "@a".len(),
            "deny warning token should be appended after custom warning list:\n{command}"
        );
        checked_commands += 1;
    }

    assert!(
        checked_commands > 0,
        "expected at least one `{command_prefix}` command in dry-run output:\n{stdout}"
    );
}

#[test]
fn test_warn_list_dry_run() {
    let dir = TestDir::new("warns/warn_list");

    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm-gc",
                "--sort-input",
                "--no-render",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib1/hello.mbt -w -1 -o ./_build/wasm-gc/debug/build/lib1/lib1.core -pkg username/hello/lib1 -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1:./lib1 -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./lib/hello.mbt -w -2 -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -w -1-2 -o ./_build/wasm-gc/debug/build/main/main.core -pkg username/hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -i ./_build/wasm-gc/debug/build/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/lib1/lib1.core ./_build/wasm-gc/debug/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib1:./lib1 -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm-gc",
                "--warn-list",
                "-29",
                "--sort-input",
                "--no-render",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib1/hello.mbt -w -1-29 -o ./_build/wasm-gc/debug/build/lib1/lib1.core -pkg username/hello/lib1 -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1:./lib1 -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./lib/hello.mbt -w -2-29 -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -w -1-2-29 -o ./_build/wasm-gc/debug/build/main/main.core -pkg username/hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -i ./_build/wasm-gc/debug/build/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/lib1/lib1.core ./_build/wasm-gc/debug/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib1:./lib1 -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
        "#]],
    );

    let build_deny_warn_stdout = get_stdout(
        &dir,
        [
            "build",
            "--target",
            "wasm-gc",
            "--deny-warn",
            "--sort-input",
            "--no-render",
            "--dry-run",
        ],
    );
    assert_deny_warn_appended_to_custom_warn_list(&build_deny_warn_stdout, "moonc build-package");

    check(
        get_stdout(&dir, ["test", "--target", "wasm-gc", "--sort-input"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "bundle",
                "--target",
                "wasm-gc",
                "--sort-input",
                "--no-render",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib1/hello.mbt -w -a -o ./_build/wasm-gc/release/bundle/lib1/lib1.core -pkg username/hello/lib1 -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1:./lib1 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/bundle/all_pkgs.json
            moonc build-package ./lib/hello.mbt -w -a -o ./_build/wasm-gc/release/bundle/lib/lib.core -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/bundle/all_pkgs.json
            moonc build-package ./main/main.mbt -w -a -o ./_build/wasm-gc/release/bundle/main/main.core -pkg username/hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/bundle/lib/lib.mi:lib -i ./_build/wasm-gc/release/bundle/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/bundle/all_pkgs.json
            moonc bundle-core ./_build/wasm-gc/release/bundle/lib/lib.core ./_build/wasm-gc/release/bundle/lib1/lib1.core ./_build/wasm-gc/release/bundle/main/main.core -o ./_build/wasm-gc/release/bundle/hello.core
        "#]],
    );

    // to cover `moon bundle` no work to do
    get_stdout(&dir, ["bundle", "--target", "wasm-gc", "--sort-input"]);

    check(
        get_stdout(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "--sort-input",
                "--no-render",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc check ./lib1/hello.mbt -w -1 -o ./_build/wasm-gc/debug/check/lib1/lib1.mi -pkg username/hello/lib1 -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1:./lib1 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./lib/hello.mbt -w -2 -o ./_build/wasm-gc/debug/check/lib/lib.mi -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./main/main.mbt -w -1-2 -o ./_build/wasm-gc/debug/check/main/main.mi -pkg username/hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i ./_build/wasm-gc/debug/check/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./main/main.mbt -include-doctests -w -1-2 -o ./_build/wasm-gc/debug/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i ./_build/wasm-gc/debug/check/lib1/lib1.mi:lib1 -i ./_build/wasm-gc/debug/check/main/main.mi:main -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./lib1/hello.mbt -include-doctests -w -1 -o ./_build/wasm-gc/debug/check/lib1/lib1.blackbox_test.mi -pkg username/hello/lib1_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1_blackbox_test:./lib1 -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./lib/hello_test.mbt -doctest-only ./lib/hello.mbt -include-doctests -w -2 -o ./_build/wasm-gc/debug/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "--warn-list",
                "-29",
                "--sort-input",
                "--no-render",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc check ./lib1/hello.mbt -w -1-29 -o ./_build/wasm-gc/debug/check/lib1/lib1.mi -pkg username/hello/lib1 -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1:./lib1 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./lib/hello.mbt -w -2-29 -o ./_build/wasm-gc/debug/check/lib/lib.mi -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./main/main.mbt -w -1-2-29 -o ./_build/wasm-gc/debug/check/main/main.mi -pkg username/hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i ./_build/wasm-gc/debug/check/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./main/main.mbt -include-doctests -w -1-2-29 -o ./_build/wasm-gc/debug/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i ./_build/wasm-gc/debug/check/lib1/lib1.mi:lib1 -i ./_build/wasm-gc/debug/check/main/main.mi:main -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./lib1/hello.mbt -include-doctests -w -1-29 -o ./_build/wasm-gc/debug/check/lib1/lib1.blackbox_test.mi -pkg username/hello/lib1_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1_blackbox_test:./lib1 -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./lib/hello_test.mbt -doctest-only ./lib/hello.mbt -include-doctests -w -2-29 -o ./_build/wasm-gc/debug/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );

    let check_deny_warn_stdout = get_stdout(
        &dir,
        [
            "check",
            "--target",
            "wasm-gc",
            "--deny-warn",
            "--sort-input",
            "--no-render",
            "--dry-run",
        ],
    );
    assert_deny_warn_appended_to_custom_warn_list(&check_deny_warn_stdout, "moonc check");
}

#[test]
fn test_warn_list_real_run() {
    let dir = TestDir::new("warns/warn_list");

    check(
        get_stderr(&dir, ["build", "--sort-input", "--no-render"]),
        expect![""],
    );

    check(
        get_stderr(&dir, ["test", "--sort-input"])
            .lines()
            .filter(|it| !it.starts_with("Blocking waiting for file lock"))
            .collect::<String>(),
        expect![""],
    );
    check(
        get_stdout(&dir, ["test", "--sort-input"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stderr(&dir, ["bundle", "--sort-input", "--no-render"]),
        expect![""],
    );

    // to cover `moon bundle` no work to do
    get_stdout(&dir, ["bundle", "--sort-input"]);

    check(
        get_stderr(&dir, ["check", "--sort-input", "--no-render"]),
        expect![""],
    );
}

#[test]
fn test_pkg_warn_list_does_not_report_generated_test_driver_warnings() {
    let dir = TestDir::new("warns/test_driver_warn_list");
    let assert = moon_cmd(&dir)
        .args(["test", "--sort-input"])
        .assert()
        .success();
    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("__generated_driver_for_"),
        "generated test driver diagnostics should not be reported:\n{stderr}"
    );
    assert!(
        !stderr.contains("missing_doc"),
        "package missing_doc warnings should not come from the generated test driver:\n{stderr}"
    );
    assert!(
        !stderr.contains("could not be rendered"),
        "filtered generated test driver warnings should not emit a generic render warning:\n{stderr}"
    );
    check(
        stdout.as_ref(),
        expect![[r#"
            Total tests: 2, passed: 2, failed: 0.
        "#]],
    );

    let deny_assert = moon_cmd(&dir)
        .args([
            "test",
            "--sort-input",
            "--target-dir",
            "warn-as-error-target",
            "--warn-list",
            "+a@a",
        ])
        .assert()
        .success();
    let deny_stderr = String::from_utf8_lossy(&deny_assert.get_output().stderr);
    assert!(
        !deny_stderr.contains("__generated_driver_for_"),
        "generated test driver warnings should not fail under +a@a:\n{deny_stderr}"
    );

    moon_cmd(&dir)
        .args([
            "bench",
            "--build-only",
            "--sort-input",
            "--target-dir",
            "bench-target",
            "--warn-list",
            "+a@a",
        ])
        .assert()
        .success();
}

#[test]
fn test_warn_list_alerts() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("NO_COLOR", "1") };
    let dir = TestDir::new("warns/warn_list_alerts");

    // don't set -w if it's empty string
    check(
        get_stdout(
            &dir,
            [
                "-C",
                "a",
                "check",
                "--target",
                "wasm-gc",
                "--sort-input",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc check ./b/hello.mbt -o ./_build/wasm-gc/debug/check/username/b/b.mi -pkg username/b -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/b:./b -target wasm-gc -workspace-path ./b -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./b/hello.mbt -include-doctests -o ./_build/wasm-gc/debug/check/username/b/b.blackbox_test.mi -pkg username/b_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/username/b/b.mi:b -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/b_blackbox_test:./b -target wasm-gc -blackbox-test -workspace-path ./b -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./a/main.mbt -o ./_build/wasm-gc/debug/check/username/a/a.mi -pkg username/a -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/username/b/b.mi:b -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/a:./a -target wasm-gc -workspace-path ./a -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./a/main.mbt -include-doctests -o ./_build/wasm-gc/debug/check/username/a/a.blackbox_test.mi -pkg username/a_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/username/a/a.mi:a -i ./_build/wasm-gc/debug/check/username/b/b.mi:b -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/a_blackbox_test:./a -target wasm-gc -blackbox-test -workspace-path ./a -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );

    check(
        get_stderr(
            &dir,
            ["-C", "a", "check", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            Warning: [0014]
               ╭─[ $ROOT/a/main.mbt:2:3 ]
               │
             2 │   @b.internal_one()
               │   ───────┬───────  
               │          ╰───────── Warning (alert_one): one
            ───╯
            Warning: [0014]
               ╭─[ $ROOT/a/main.mbt:3:3 ]
               │
             3 │   @b.internal_two()
               │   ───────┬───────  
               │          ╰───────── Warning (alert_two): two
            ───╯
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["-C", "a", "test", "--target", "wasm-gc", "--sort-input"],
        ),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}

#[test]
fn test_mod_level_warn_list_alerts() {
    let dir = TestDir::new("warns/mod_level");

    check(
        get_stdout(
            &dir,
            [
                "-C",
                "a",
                "check",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./b/hello.mbt -o ./_build/wasm-gc/debug/check/username/b/b.mi -pkg username/b -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/b:./b -target wasm-gc -workspace-path ./b -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./b/hello.mbt -include-doctests -o ./_build/wasm-gc/debug/check/username/b/b.blackbox_test.mi -pkg username/b_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/username/b/b.mi:b -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/b_blackbox_test:./b -target wasm-gc -blackbox-test -workspace-path ./b -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./a/main.mbt -w -alert_one-alert_two -o ./_build/wasm-gc/debug/check/username/a/a.mi -pkg username/a -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/username/b/b.mi:b -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/a:./a -target wasm-gc -workspace-path ./a -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./a/main.mbt -include-doctests -w -alert_one-alert_two -o ./_build/wasm-gc/debug/check/username/a/a.blackbox_test.mi -pkg username/a_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/username/a/a.mi:a -i ./_build/wasm-gc/debug/check/username/b/b.mi:b -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/a_blackbox_test:./a -target wasm-gc -blackbox-test -workspace-path ./a -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );
}

#[test]
fn test_deny_warn() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("NO_COLOR", "1") };
    let dir = TestDir::new("warns/deny_warn");

    check(
        get_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            Warning: [0002]
               ╭─[ $ROOT/lib/hello.mbt:4:7 ]
               │
             4 │   let a = 1;
               │       ┬  
               │       ╰── Warning (unused_value): Unused variable 'a'
            ───╯
            Warning: [0002]
                ╭─[ $ROOT/lib/hello.mbt:11:7 ]
                │
             11 │   let 中文 = 2
                │       ──┬─  
                │         ╰─── Warning (unused_value): Unused variable '中文'
            ────╯
            Warning: [0002]
                ╭─[ $ROOT/lib/hello.mbt:12:7 ]
                │
             12 │   let 🤣😭🤣😭🤣 = 2
                │       ────┬─────  
                │           ╰─────── Warning (unused_value): Unused variable '🤣😭🤣😭🤣'
            ────╯
            Warning: [0002]
               ╭─[ $ROOT/main/main.mbt:2:7 ]
               │
             2 │   let a = 0
               │       ┬  
               │       ╰── Warning (unused_value): Unused variable 'a'
            ───╯
        "#]],
    );

    check(
        get_err_stdout(&dir, ["check", "--deny-warn", "--sort-input"]),
        expect![""],
    );

    check(
        get_stderr(&dir, ["build", "--sort-input"]),
        expect![[r#"
            Warning: [0002]
               ╭─[ $ROOT/lib/hello.mbt:4:7 ]
               │
             4 │   let a = 1;
               │       ┬  
               │       ╰── Warning (unused_value): Unused variable 'a'
            ───╯
            Warning: [0002]
                ╭─[ $ROOT/lib/hello.mbt:11:7 ]
                │
             11 │   let 中文 = 2
                │       ──┬─  
                │         ╰─── Warning (unused_value): Unused variable '中文'
            ────╯
            Warning: [0002]
                ╭─[ $ROOT/lib/hello.mbt:12:7 ]
                │
             12 │   let 🤣😭🤣😭🤣 = 2
                │       ────┬─────  
                │           ╰─────── Warning (unused_value): Unused variable '🤣😭🤣😭🤣'
            ────╯
            Warning: [0002]
               ╭─[ $ROOT/main/main.mbt:2:7 ]
               │
             2 │   let a = 0
               │       ┬  
               │       ╰── Warning (unused_value): Unused variable 'a'
            ───╯
        "#]],
    );

    check(
        get_err_stdout(&dir, ["build", "--deny-warn", "--sort-input"]),
        expect![""],
    );
}
