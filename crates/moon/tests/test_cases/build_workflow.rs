// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use super::*;

#[test]
fn test_moon_pkg() {
    let dir = TestDir::new("moon_pkg.in");
    check(
        get_stdout(&dir, ["check", "--target", "wasm-gc", "--dry-run"]),
        expect![[r#"
            moon tool exec --shell 'cat ./pkg/pkg.mbt > ./pkg/gen.txt'
              cwd: .
            moonc check ./pkg/pkg.mbt -w -unused_value-todo -o ./_build/wasm-gc/debug/check/pkg/pkg.mi -pkg user/mod/pkg -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources user/mod/pkg:./pkg -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./pkg/pkg_test.mbt -doctest-only ./pkg/pkg.mbt -include-doctests -w -unused_value-todo -o ./_build/wasm-gc/debug/check/pkg/pkg.blackbox_test.mi -pkg user/mod/pkg_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/pkg/pkg.mi:pkg -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources user/mod/pkg_blackbox_test:./pkg -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./main/main.mbt -o ./_build/wasm-gc/debug/check/main/main.mi -pkg user/mod/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/pkg/pkg.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources user/mod/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./_build/wasm-gc/debug/check/main/main.blackbox_test.mi -pkg user/mod/main_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/pkg/pkg.mi:lib -i ./_build/wasm-gc/debug/check/main/main.mi:main -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources user/mod/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--target", "wasm-gc", "--dry-run"]),
        expect![[r#"
            moon tool exec --shell 'cat ./pkg/pkg.mbt > ./pkg/gen.txt'
              cwd: .
            moonc build-package ./pkg/pkg.mbt -w -unused_value-todo -o ./_build/wasm-gc/debug/build/pkg/pkg.core -pkg user/mod/pkg -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources user/mod/pkg:./pkg -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/pkg/pkg.core -main user/mod/pkg -o ./_build/wasm-gc/debug/build/pkg/pkg.wasm -pkg-config-path ./pkg/moon.pkg -pkg-sources user/mod/pkg:./pkg -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg user/mod/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/pkg/pkg.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources user/mod/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/pkg/pkg.core ./_build/wasm-gc/debug/build/main/main.core -main user/mod/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg -pkg-sources user/mod/pkg:./pkg -pkg-sources user/mod/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
        "#]],
    );
}

#[test]
fn test_need_link() {
    let dir = TestDir::new("need_link.in");
    check(
        get_stdout(
            &dir,
            [
                "build",
                "--target",
                "wasm-gc",
                "--dry-run",
                "--nostd",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg username/hello/lib -pkg-type foreign_library -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg username/hello/main -pkg-type executable -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core ./_build/wasm-gc/debug/build/lib/lib.core -main username/hello/lib -o ./_build/wasm-gc/debug/build/lib/lib.wasm -pkg-config-path ./lib/moon.pkg.json -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map
        "#]],
    );
}

#[test]
fn test_build_summary_follows_user_log_level() {
    let dir = TestDir::new("moon_new/plain");
    let out = get_stderr(&dir, ["check"]);
    assert!(out.contains("now up to date"));

    assert_eq!(get_stderr(&dir, ["check", "--quiet"]), "");

    let out = get_stderr(&dir, ["check"]);
    assert!(out.contains("moon: no work to do"));

    let dir = TestDir::new("moon_new/plain");
    let out = get_stderr(&dir, ["build"]);
    assert!(out.contains("now up to date"));

    assert_eq!(get_stderr(&dir, ["build", "--quiet"]), "");

    let out = get_stderr(&dir, ["build"]);
    assert!(out.contains("moon: no work to do"));
}

#[test]
fn failed_binary_dependency_build_is_not_installed() {
    let dir = TestDir::new("build_binary_dep_failure.in");
    let assert = moon_cmd(&dir)
        .args([
            "tool",
            "build-binary-dep",
            "main",
            "--install-path",
            "installed",
        ])
        .assert()
        .failure();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);

    assert_eq!(stdout, "Failed with 0 warnings, 1 errors.\n");
    assert!(
        stderr.contains("Expr Type Mismatch"),
        "expected the compiler diagnostic, stderr: {stderr}"
    );
    assert!(
        stderr.contains("Error: failed when building project"),
        "expected the command failure context, stderr: {stderr}"
    );
    assert!(
        !dir.join("installed").exists(),
        "a failed binary dependency build must not install any artifact"
    );
}

#[test]
fn test_no_block_params() {
    let dir = TestDir::new("no_block_params.in");
    check(
        get_stdout(
            &dir,
            ["build", "--target", "wasm-gc", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg username/hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map -no-block-params
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "wasm"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm/debug/build/lib/lib.core -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm -g -O0 -workspace-path . -all-pkgs ./_build/wasm/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/wasm/debug/build/main/main.core -pkg username/hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm/release/bundle' -i ./_build/wasm/debug/build/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm -g -O0 -workspace-path . -all-pkgs ./_build/wasm/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm/release/bundle/core.core' ./_build/wasm/debug/build/lib/lib.core ./_build/wasm/debug/build/main/main.core -main username/hello/main -o ./_build/wasm/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm -g -O0 -wasi -no-block-params
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["build", "--dry-run", "--sort-input", "--target", "js"],
        ),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/js/debug/build/lib/lib.core -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/js/release/bundle' -i '$MOON_HOME/lib/core/_build/js/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc build-package ./main/main.mbt -o ./_build/js/debug/build/main/main.core -pkg username/hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/js/release/bundle' -i ./_build/js/debug/build/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/js/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/js/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/js/release/bundle/core.core' ./_build/js/debug/build/lib/lib.core ./_build/js/debug/build/main/main.core -main username/hello/main -o ./_build/js/debug/build/main/main.js -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/lib:./lib -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target js -g -O0 -source-map
        "#]],
    );
}

#[test]
fn test_check_failed_should_write_pkg_json() {
    let dir = TestDir::new("check_failed_should_write_pkg_json.in");
    moon_cmd(&dir).args(["check"]).assert().failure();

    let pkg_json = dir.join("_build/packages.json");
    assert!(pkg_json.exists());
}

#[test]
fn test_failed_to_fill_whole_buffer() {
    // TODO: Do we really need to test about database corruption?!

    let dir = TestDir::new("hello");
    check(
        get_stderr(&dir, ["check", "--target", "wasm-gc"]),
        expect![[r#"
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );

    // corrupt the DB intentionally
    let moon_db_path = dir.join("./_build/wasm-gc/debug/check/check.moon_db");
    if moon_db_path.exists() {
        std::fs::remove_file(&moon_db_path).unwrap();
    }
    std::fs::write(&moon_db_path, "").unwrap();

    let stderr = get_err_stderr(&dir, ["check", "--target", "wasm-gc"]);
    println!("stderr: {}", stderr);
    assert!(stderr.contains("failed to fill whole buffer"));
}

#[test]
fn test_trace_001() {
    let dir = TestDir::new("hello");
    let _ = get_stdout(&dir, ["build", "--trace"]);
    assert!(dir.join("trace.json").exists());
}

#[test]
fn no_main_just_init() {
    let dir = TestDir::new("no_main_just_init.in");
    get_stdout(&dir, ["build", "--target", "wasm-gc"]);
    let file = dir.join("_build/wasm-gc/debug/build/lib/lib.wasm");
    assert!(file.exists());

    let out = snapbox::cmd::Command::new(moonrun_bin())
        .current_dir(&dir)
        .args(["./_build/wasm-gc/debug/build/lib/lib.wasm"])
        .assert()
        .success()
        .get_output()
        .stdout
        .to_owned();

    check(
        std::str::from_utf8(&out).unwrap(),
        expect![[r#"
            I am in fn init { ... }
        "#]],
    );
}

#[test]
#[ignore = "platform-dependent behavior"]
fn test_strip_debug() {
    let dir = TestDir::new("strip_debug.in");

    check(
        get_stdout(&dir, ["build", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main moon_new/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--no-strip", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -g
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map -g
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main moon_new/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g -O0 -source-map -g
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--debug", "--strip", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -O0 -source-map
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map -O0 -source-map
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main moon_new/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g -O0 -source-map -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--debug", "--no-strip", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main moon_new/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--debug", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/build/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/build/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/build/lib/lib.core ./_build/wasm-gc/debug/build/main/main.core -main moon_new/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -target wasm-gc -g -O0 -source-map
        "#]],
    );

    check(
        get_stdout(&dir, ["test", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map
            moonc build-package ./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./_build/wasm-gc/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/test/main/main.mi:main -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/test/lib/lib.core ./_build/wasm-gc/debug/test/main/main.core ./_build/wasm-gc/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./_build/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/test/lib/lib.core ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./_build/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--debug", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map
            moonc build-package ./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./_build/wasm-gc/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/test/main/main.mi:main -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/test/lib/lib.core ./_build/wasm-gc/debug/test/main/main.core ./_build/wasm-gc/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./_build/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/test/lib/lib.core ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./_build/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--release", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/release/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/release/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/release/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -source-map
            moonc build-package ./_build/wasm-gc/release/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./_build/wasm-gc/release/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/release/test/main/main.mi:main -i ./_build/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/release/test/lib/lib.core ./_build/wasm-gc/release/test/main/main.core ./_build/wasm-gc/release/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./_build/wasm-gc/release/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/release/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/release/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./_build/wasm-gc/release/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./_build/wasm-gc/release/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/release/test/lib/lib.core ./_build/wasm-gc/release/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./_build/wasm-gc/release/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/release/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./_build/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/release/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -source-map -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/release/test/lib/lib.internal_test.core -main moon_new/lib -o ./_build/wasm-gc/release/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--release", "--no-strip", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/release/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/release/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/release/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/release/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -source-map
            moonc build-package ./_build/wasm-gc/release/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./_build/wasm-gc/release/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/release/test/main/main.mi:main -i ./_build/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -g -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/release/test/lib/lib.core ./_build/wasm-gc/release/test/main/main.core ./_build/wasm-gc/release/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./_build/wasm-gc/release/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/release/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/release/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./_build/wasm-gc/release/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./_build/wasm-gc/release/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/release/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/release/test/lib/lib.core ./_build/wasm-gc/release/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./_build/wasm-gc/release/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/release/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./_build/wasm-gc/release/test/lib/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/release/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -source-map -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/release/test/lib/lib.internal_test.core -main moon_new/lib -o ./_build/wasm-gc/release/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--debug", "--no-strip", "--dry-run"]),
        expect![[r#"
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/test/main/main.core -pkg moon_new/main -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map
            moonc build-package ./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./_build/wasm-gc/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/test/main/main.mi:main -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/test/lib/lib.core ./_build/wasm-gc/debug/test/main/main.core ./_build/wasm-gc/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./_build/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/test/lib/lib.core ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path $MOON_HOME/lib/core/_build/wasm-gc/release/bundle -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode
            moonc link-core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core $MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core ./_build/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./_build/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/lib:./lib -pkg-sources moonbitlang/core:$MOON_HOME/lib/core -exported_functions moonbit_test_driver_internal_execute,moonbit_test_driver_finish -target wasm-gc -g -O0 -source-map
        "#]],
    );
}

#[test]
fn test_exports_in_native_backend() {
    let dir = TestDir::new("native_exports.in");
    let _ = get_stdout(&dir, ["build", "--target", "native", "--release"]);
    assert!(
        !dir.join(BUILD_DIR)
            .join("native")
            .join("release")
            .join("build")
            .join("lib")
            .join("lib.c")
            .exists()
    );
    let lib2_c = read(
        dir.join(BUILD_DIR)
            .join("native")
            .join("release")
            .join("build")
            .join("lib2")
            .join("lib2.c"),
    );
    assert!(lib2_c.contains("_M0FP38username5hello4lib25hello()"));

    // alias not works
    let lib3_c = read(
        dir.join(BUILD_DIR)
            .join("native")
            .join("release")
            .join("build")
            .join("lib3")
            .join("lib3.c"),
    );
    assert!(lib3_c.contains("_M0FP38username5hello4lib35hello()"));
}

#[test]
fn test_diag_source_map_remaps_generated_sources() {
    // Real-world generated parser fixture: diagnostics from `parser.mbt` are
    // remapped to locations in `parser.mbty` via `parser.mbt.map.json`.
    let dir = TestDir::new("diag_loc_map.in");
    check(
        get_err_stderr(&dir, ["check"]),
        expect![[r#"
            Error: [4014]
                 ╭─[ $ROOT/parser.mbt:129:13 ]
                 │
             129 │       lhs + "x" + rhs
                 │             ─┬─  
                 │              ╰─── Expr Type Mismatch
                    has type : String
                    wanted   : Int
            ─────╯
            Error: failed when checking project
        "#]],
    );

    // Minimal reproducible fixture: a tiny DSL source is remapped from
    // generated `main.mbt` back to `toy.src`.
    let dir = TestDir::new("diag_loc_map_small.in");
    check(
        get_err_stderr(&dir, ["check"]),
        expect![[r#"
            Error: [4014]
               ╭─[ $ROOT/toy.src:2:7 ]
               │
             2 │ print "hello"
               │       ───┬───  
               │          ╰───── Expr Type Mismatch
                    has type : String
                    wanted   : Int
            ───╯
            Error: failed when checking project
        "#]],
    );
}

#[test]
fn test_dont_link_third_party() {
    let dir = TestDir::new("dont_link_third_party.in");

    check(
        get_stdout(
            &dir,
            ["build", "--target", "wasm-gc", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/main/main.core -main hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
        "#]],
    );
}

#[test]
fn test_no_warn_deps() {
    let dir = TestDir::new("no_warn_deps.in");
    let dir = dir.join("user.in");

    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Finished. moon: ran 5 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["check", "--deny-warn"]),
        expect![[r#"
            Finished. moon: ran 5 tasks, now up to date
        "#]],
    );
    check(
        get_stderr(&dir, ["build"]),
        expect![[r#"
            Finished. moon: ran 3 tasks, now up to date
        "#]],
    );
}
