use super::*;

#[test]
fn test_warn_list_dry_run() {
    let dir = TestDir::new("warns/warn_list");

    check(
        get_stdout(&dir, ["build", "--sort-input", "--no-render", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib1/hello.mbt -w -1 -o ./_build/wasm-gc/release/build/lib1/lib1.core -pkg username/hello/lib1 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1:./lib1 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./lib/hello.mbt -w -2 -o ./_build/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -w -1-2 -o ./_build/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/build/lib/lib.mi:lib -i ./_build/wasm-gc/release/build/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/release/build/lib/lib.core ./_build/wasm-gc/release/build/lib1/lib1.core ./_build/wasm-gc/release/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib1:./lib1 -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--warn-list",
                "-29",
                "--sort-input",
                "--no-render",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib1/hello.mbt -w -1-29 -o ./_build/wasm-gc/release/build/lib1/lib1.core -pkg username/hello/lib1 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1:./lib1 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./lib/hello.mbt -w -2-29 -o ./_build/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc build-package ./main/main.mbt -w -1-2-29 -o ./_build/wasm-gc/release/build/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/build/lib/lib.mi:lib -i ./_build/wasm-gc/release/build/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/release/build/lib/lib.core ./_build/wasm-gc/release/build/lib1/lib1.core ./_build/wasm-gc/release/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/release/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/lib1:./lib1 -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--sort-input"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout(&dir, ["bundle", "--sort-input", "--no-render", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib1/hello.mbt -w -a -o ./_build/wasm-gc/release/bundle/lib1/lib1.core -pkg username/hello/lib1 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1:./lib1 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/bundle/all_pkgs.json
            moonc build-package ./lib/hello.mbt -w -a -o ./_build/wasm-gc/release/bundle/lib/lib.core -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/bundle/all_pkgs.json
            moonc build-package ./main/main.mbt -w -a -o ./_build/wasm-gc/release/bundle/main/main.core -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/bundle/lib/lib.mi:lib -i ./_build/wasm-gc/release/bundle/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/bundle/all_pkgs.json
            moonc bundle-core ./_build/wasm-gc/release/bundle/lib/lib.core ./_build/wasm-gc/release/bundle/lib1/lib1.core ./_build/wasm-gc/release/bundle/main/main.core -o ./_build/wasm-gc/release/bundle/hello.core
        "#]],
    );

    // to cover `moon bundle` no work to do
    get_stdout(&dir, ["bundle", "--sort-input"]);

    check(
        get_stdout(&dir, ["check", "--sort-input", "--no-render", "--dry-run"]),
        expect![[r#"
            moonc check ./lib1/hello.mbt -w -1 -o ./_build/wasm-gc/release/check/lib1/lib1.mi -pkg username/hello/lib1 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1:./lib1 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib/hello.mbt -w -2 -o ./_build/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./main/main.mbt -w -1-2 -o ./_build/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i ./_build/wasm-gc/release/check/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./main/main.mbt -include-doctests -w -1-2 -o ./_build/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i ./_build/wasm-gc/release/check/lib1/lib1.mi:lib1 -i ./_build/wasm-gc/release/check/main/main.mi:main -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./lib1/hello.mbt -include-doctests -w -1 -o ./_build/wasm-gc/release/check/lib1/lib1.blackbox_test.mi -pkg username/hello/lib1_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1_blackbox_test:./lib1 -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib/hello_test.mbt -doctest-only ./lib/hello.mbt -include-doctests -w -2 -o ./_build/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "check",
                "--warn-list",
                "-29",
                "--sort-input",
                "--no-render",
                "--dry-run",
            ],
        ),
        expect![[r#"
            moonc check ./lib1/hello.mbt -w -1-29 -o ./_build/wasm-gc/release/check/lib1/lib1.mi -pkg username/hello/lib1 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1:./lib1 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib/hello.mbt -w -2-29 -o ./_build/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./main/main.mbt -w -1-2-29 -o ./_build/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i ./_build/wasm-gc/release/check/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./main/main.mbt -include-doctests -w -1-2-29 -o ./_build/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i ./_build/wasm-gc/release/check/lib1/lib1.mi:lib1 -i ./_build/wasm-gc/release/check/main/main.mi:main -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./lib1/hello.mbt -include-doctests -w -1-29 -o ./_build/wasm-gc/release/check/lib1/lib1.blackbox_test.mi -pkg username/hello/lib1_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib1/lib1.mi:lib1 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib1_blackbox_test:./lib1 -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib/hello_test.mbt -doctest-only ./lib/hello.mbt -include-doctests -w -2-29 -o ./_build/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );
}

#[test]
fn test_warn_list_real_run() {
    let dir = TestDir::new("warns/warn_list");

    check(
        get_stderr(&dir, ["build", "--sort-input", "--no-render"]),
        expect![[r#"
            Finished. moon: ran 4 tasks, now up to date
        "#]],
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
        expect![[r#"
            Finished. moon: ran 4 tasks, now up to date
        "#]],
    );

    // to cover `moon bundle` no work to do
    get_stdout(&dir, ["bundle", "--sort-input"]);

    check(
        get_stderr(&dir, ["check", "--sort-input", "--no-render"]),
        expect![[r#"
            Finished. moon: ran 6 tasks, now up to date
        "#]],
    );
}

#[test]
fn test_alert_list() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("NO_COLOR", "1") };
    let dir = TestDir::new("warns/alert_list");

    // don't set -alert & -w if it's empty string
    check(
        get_stdout(&dir, ["check", "--sort-input", "--dry-run"]),
        expect![[r#"
            moonc check ./lib/hello.mbt -w -2 -alert -alert_1-alert_2 -o ./_build/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./main/main.mbt -w -1-2 -alert -alert_1 -o ./_build/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./main/main.mbt -include-doctests -w -1-2 -alert -alert_1 -o ./_build/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i ./_build/wasm-gc/release/check/main/main.mi:main -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./lib2/hello.mbt -o ./_build/wasm-gc/release/check/lib2/lib2.mi -pkg username/hello/lib2 -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./lib2/hello.mbt -include-doctests -o ./_build/wasm-gc/release/check/lib2/lib2.blackbox_test.mi -pkg username/hello/lib2_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -w -2 -alert -alert_1-alert_2 -o ./_build/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
        "#]],
    );

    check(
        get_stderr(&dir, ["build", "--sort-input"]),
        expect![[r#"
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--sort-input"]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stderr(&dir, ["bundle", "--sort-input"]),
        expect![[r#"
            Finished. moon: ran 4 tasks, now up to date
        "#]],
    );

    check(
        get_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            Finished. moon: ran 6 tasks, now up to date
        "#]],
    );
}

#[test]
fn test_mod_level_warn_alert_list() {
    let dir = TestDir::new("warns/mod_level");

    check(
        get_stdout(&dir, ["check", "--dry-run"]),
        expect![[r#"
            moonc check ./lib/hello.mbt -w -1 -alert -alert_1 -o ./_build/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check ./main/main.mbt -w -1-2 -alert -alert_1-alert_2 -o ./_build/wasm-gc/release/check/main/main.mi -pkg username/hello/main -is-main -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./main/main.mbt -include-doctests -w -1-2 -alert -alert_1-alert_2 -o ./_build/wasm-gc/release/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i ./_build/wasm-gc/release/check/main/main.mi:main -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -w -1 -alert -alert_1 -o ./_build/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -std-path '$MOON_HOME/lib/core/target/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/release/check/all_pkgs.json
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
            Finished. moon: ran 4 tasks, now up to date (4 warnings, 0 errors)
        "#]],
    );

    check(
        get_err_stdout(&dir, ["check", "--deny-warn", "--sort-input"]),
        expect![[r#"
            failed: moonc check -error-format json -w @a $ROOT/lib/hello.mbt -o $ROOT/_build/wasm-gc/release/check/lib/lib.mi -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i $MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude -pkg-sources username/hello/lib:$ROOT/lib -target wasm-gc -workspace-path $ROOT -all-pkgs $ROOT/_build/wasm-gc/release/check/all_pkgs.json
        "#]],
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
            Finished. moon: ran 3 tasks, now up to date (4 warnings, 0 errors)
        "#]],
    );

    check(
        get_err_stdout(&dir, ["build", "--deny-warn", "--sort-input"]),
        expect![[r#"
            failed: moonc build-package -error-format json -w @a $ROOT/lib/hello.mbt -o $ROOT/_build/wasm-gc/release/build/lib/lib.core -pkg username/hello/lib -std-path $MOON_HOME/lib/core/target/wasm-gc/release/bundle -i $MOON_HOME/lib/core/target/wasm-gc/release/bundle/prelude/prelude.mi:prelude -pkg-sources username/hello/lib:$ROOT/lib -target wasm-gc -workspace-path $ROOT -all-pkgs $ROOT/_build/wasm-gc/release/build/all_pkgs.json
        "#]],
    );
}
