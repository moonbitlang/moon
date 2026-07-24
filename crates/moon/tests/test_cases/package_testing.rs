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
fn test_panic() {
    let dir = TestDir::new("panic.in");
    let data = moon_cmd(&dir)
        .args(["test"])
        .assert()
        .failure()
        .get_output()
        .stdout
        .to_owned();
    let out = String::from_utf8_lossy(&data).to_string();
    check(
        &out,
        expect![[r#"
            [username/hello] test lib/hello_wbtest.mbt:3 ("panic") failed: panic is expected
            Total tests: 2, passed: 1, failed: 1.
        "#]],
    );
}

#[test]
fn test_validate_import() {
    let dir = TestDir::new("validate_import.in");
    check(
        get_err_stderr(&dir, ["check"]),
        expect![[r#"
            Error: Failed to calculate build plan

            Caused by:
                0: Failed to solve package relationship
                1: Cannot find import 'mbt/core/set' in username/hello/main@0.1.0
        "#]],
    );
    check(
        get_err_stderr(&dir, ["build"]),
        expect![[r#"
            Error: Failed to solve package relationship

            Caused by:
                Cannot find import 'mbt/core/set' in username/hello/main@0.1.0
        "#]],
    );
    check(
        get_err_stderr(&dir, ["test"]),
        expect![[r#"
            Error: Failed to solve package relationship

            Caused by:
                Cannot find import 'mbt/core/set' in username/hello/main@0.1.0
        "#]],
    );
    check(
        get_err_stderr(&dir, ["bundle"]),
        expect![[r#"
            Error: Failed to solve package relationship

            Caused by:
                Cannot find import 'mbt/core/set' in username/hello/main@0.1.0
        "#]],
    );
}

#[test]
fn test_multi_process() {
    use std::thread;

    let dir = TestDir::new("test_multi_process");
    let path: PathBuf = dir.as_ref().into();

    let (num_threads, inner_loop) = (16, 10);
    let mut container = vec![];

    let success = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0));

    for _ in 0..num_threads {
        let path = path.clone();
        let success = success.clone();
        let work = thread::spawn(move || {
            for _ in 0..inner_loop {
                let _ = std::fs::OpenOptions::new()
                    .append(true)
                    .open(path.join("lib/hello.mbt"))
                    .unwrap()
                    .write(b"\n")
                    .unwrap();

                let output = moon_process_cmd(&path)
                    .arg("check")
                    .output()
                    .expect("Failed to execute command");

                if output.status.success() {
                    success.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                } else {
                    println!("moon output: {:?}", String::from_utf8(output.stdout));
                    let error_message = String::from_utf8_lossy(&output.stderr);
                    println!("{error_message}");
                }
            }
        });
        container.push(work);
    }

    for i in container {
        i.join().unwrap();
    }

    assert_eq!(
        success.load(std::sync::atomic::Ordering::SeqCst),
        num_threads * inner_loop
    );
}

#[test]
fn test_internal_package() {
    let dir = TestDir::new("internal_package.in");
    let output = get_err_stderr(&dir, ["check", "--sort-input"]);

    // Might need a better way
    assert!(
        output
            .to_lowercase()
            .contains("cannot import internal package")
    );
}

#[test]
fn test_nonexistent_package() {
    let dir = TestDir::new("nonexistent_package.in");
    check(
        get_err_stderr(&dir, ["check", "--sort-input"]),
        expect![[r#"
            Error: Failed to calculate build plan

            Caused by:
                0: Failed to solve package relationship
                1: Cannot find import 'username/hello/lib/b' in username/hello/main@0.1.0
        "#]],
    );
}

#[test]
fn moon_test_with_failure_json() {
    let dir = TestDir::new("test_with_failure_json");

    let output = get_err_stdout(&dir, ["test", "--test-failure-json"]);
    check(
        &output,
        // should keep in this format, it's used in ide test explorer
        expect![[r#"
            {"package":"username/hello/lib1","filename":"hello.mbt","index":"0","test_name":"test_1","message":"src/lib1/hello.mbt:7:3-7:24@username/hello FAILED: test_1 failed"}
            Total tests: 2, passed: 1, failed: 1.
        "#]],
    );
}

#[test]
fn test_moon_check_filter_package() {
    let dir = TestDir::new("test_check_filter.in");

    check(
        get_stdout(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "-p",
                "A",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./_build/wasm-gc/debug/check/A/A.whitebox_test.mi -pkg username/hello/A -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./A/hello.mbt ./A/test.mbt -o ./_build/wasm-gc/debug/check/A/A.mi -pkg username/hello/A -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./A/hello_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -include-doctests -o ./_build/wasm-gc/debug/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/A/A.mi:A -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "-p",
                "main",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./_build/wasm-gc/debug/check/lib2/lib2.mi -pkg username/hello/lib2 -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./lib/hello.mbt -o ./_build/wasm-gc/debug/check/lib/lib.mi -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./main/main.mbt -o ./_build/wasm-gc/debug/check/main/main.mi -pkg username/hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./_build/wasm-gc/debug/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i ./_build/wasm-gc/debug/check/main/main.mi:main -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );

    check(
        get_stdout(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "-p",
                "lib",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./_build/wasm-gc/debug/check/lib2/lib2.mi -pkg username/hello/lib2 -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./lib/hello.mbt -o ./_build/wasm-gc/debug/check/lib/lib.mi -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -o ./_build/wasm-gc/debug/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i ./_build/wasm-gc/debug/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );
}

#[test]
fn test_moon_check_package_with_patch() {
    let dir = TestDir::new("test_check_filter.in");

    // A has no deps
    check(
        get_stdout(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "-p",
                "A",
                "--patch-file",
                "/path/to/patch.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./_build/wasm-gc/debug/check/A/A.whitebox_test.mi -pkg username/hello/A -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -patch-file /path/to/patch.json ./A/hello.mbt ./A/test.mbt -o ./_build/wasm-gc/debug/check/A/A.mi -pkg username/hello/A -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./A/hello_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -include-doctests -o ./_build/wasm-gc/debug/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/A/A.mi:A -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "-p",
                "A",
                "--patch-file",
                "/path/to/patch_wbtest.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check -patch-file /path/to/patch_wbtest.json ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./_build/wasm-gc/debug/check/A/A.whitebox_test.mi -pkg username/hello/A -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./A/hello.mbt ./A/test.mbt -o ./_build/wasm-gc/debug/check/A/A.mi -pkg username/hello/A -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./A/hello_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -include-doctests -o ./_build/wasm-gc/debug/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/A/A.mi:A -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "-p",
                "A",
                "--patch-file",
                "/path/to/patch_test.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./A/hello.mbt ./A/test.mbt ./A/hello_wbtest.mbt -o ./_build/wasm-gc/debug/check/A/A.whitebox_test.mi -pkg username/hello/A -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -whitebox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./A/hello.mbt ./A/test.mbt -o ./_build/wasm-gc/debug/check/A/A.mi -pkg username/hello/A -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A:./A -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -patch-file /path/to/patch_test.json ./A/hello_test.mbt -doctest-only ./A/hello.mbt -doctest-only ./A/test.mbt -include-doctests -o ./_build/wasm-gc/debug/check/A/A.blackbox_test.mi -pkg username/hello/A_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/A/A.mi:A -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/A_blackbox_test:./A -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );

    // lib has dep lib2
    check(
        get_stdout(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "-p",
                "lib",
                "--patch-file",
                "/path/to/patch.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./_build/wasm-gc/debug/check/lib2/lib2.mi -pkg username/hello/lib2 -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -patch-file /path/to/patch.json ./lib/hello.mbt -o ./_build/wasm-gc/debug/check/lib/lib.mi -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -o ./_build/wasm-gc/debug/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i ./_build/wasm-gc/debug/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );
    check(
        get_stdout(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "-p",
                "lib",
                "--patch-file",
                "/path/to/patch_test.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./_build/wasm-gc/debug/check/lib2/lib2.mi -pkg username/hello/lib2 -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./lib/hello.mbt -o ./_build/wasm-gc/debug/check/lib/lib.mi -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -patch-file /path/to/patch_test.json -doctest-only ./lib/hello.mbt -include-doctests -o ./_build/wasm-gc/debug/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i ./_build/wasm-gc/debug/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );

    // main has dep lib
    check(
        get_stdout(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "-p",
                "main",
                "--patch-file",
                "/path/to/patch.json",
                "--dry-run",
                "--sort-input",
            ],
        ),
        expect![[r#"
            moonc check ./lib2/lib.mbt -o ./_build/wasm-gc/debug/check/lib2/lib2.mi -pkg username/hello/lib2 -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./lib/hello.mbt -o ./_build/wasm-gc/debug/check/lib/lib.mi -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -patch-file /path/to/patch.json ./main/main.mbt -o ./_build/wasm-gc/debug/check/main/main.mi -pkg username/hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./_build/wasm-gc/debug/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i ./_build/wasm-gc/debug/check/main/main.mi:main -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );
}

#[test]
fn test_no_mi_for_test_pkg() {
    let dir = TestDir::new("test_check_filter.in");

    get_stdout(
        &dir,
        ["test", "--target", "wasm-gc", "-p", "username/hello/A"],
    );

    // .mi should not be generated for test package
    let mi_path = dir.join("_build/wasm-gc/debug/test/A/A.internal_test.mi");
    assert!(!mi_path.exists());

    // .core should be generated for test package
    let core_path = dir.join("_build/wasm-gc/debug/test/A/A.internal_test.core");
    assert!(core_path.exists());
}

#[test]
fn test_render_diagnostic_in_patch_file() {
    let dir = TestDir::new("moon_test/patch");
    check(
        get_stderr(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "-p",
                "lib",
                "--patch-file",
                "./patch_test.json",
            ],
        ),
        expect![[r#"
            Warning: [0002]
               ╭─[ hello_2_test.mbt:2:6 ]
               │
             2 │  let unused_in_patch_test_json = 1;
               │      ────────────┬────────────  
               │                  ╰────────────── Warning (unused_value): Unused variable 'unused_in_patch_test_json'
            ───╯
        "#]],
    );
    check(
        get_stderr(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "-p",
                "lib",
                "--patch-file",
                "./patch_wbtest.json",
            ],
        ),
        expect![[r#"
            Warning: [0002]
               ╭─[ hello_1_wbtest.mbt:2:6 ]
               │
             2 │  let unused_in_patch_wbtest_json = 1;
               │      ─────────────┬─────────────  
               │                   ╰─────────────── Warning (unused_value): Unused variable 'unused_in_patch_wbtest_json'
            ───╯
        "#]],
    );
    check(
        get_stderr(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "-p",
                "lib",
                "--patch-file",
                "./patch.json",
            ],
        ),
        expect![[r#"
            Warning: [0002]
               ╭─[ hello_0.mbt:2:6 ]
               │
             2 │  let unused_in_patch_json = 1;
               │      ──────────┬─────────  
               │                ╰─────────── Warning (unused_value): Unused variable 'unused_in_patch_json'
            ───╯
        "#]],
    );

    // check --explain
    check(
        get_stderr(
            &dir,
            [
                "check",
                "--target",
                "wasm-gc",
                "-p",
                "lib",
                "--patch-file",
                "./patch_test.json",
                "--explain",
            ],
        ),
        expect![[r#"
            Warning: 
               ╭─[ hello_2_test.mbt:2:6 ]
               │
             2 │  let unused_in_patch_test_json = 1;
               │      ────────────┬────────────  
               │                  ╰────────────── Warning (unused_value): Unused variable 'unused_in_patch_test_json'
               │ 
               │ Help: # E0002
               │       
               │       Warning name: `unused_value`
               │       
               │       Unused variable.
               │       
               │       This variable is unused by any other part of your code, nor marked with `pub`
               │       visibility.
               │       
               │       Note that this warning might uncover other bugs in your code. For example, if
               │       there are two variables in your codebase that has similar name, you might just
               │       use the other variable by mistake.
               │       
               │       Specifically, if the variable is at the toplevel, and the body of the module
               │       contains side effects, the side effects will not happen.
               │       
               │       ## Erroneous example
               │       
               │       ```moonbit
               │       ///|
               │       let p : Int = {
               │         side_effect.val = 42
               │         42
               │       }
               │       
               │       ///|
               │       let side_effect : Ref[Int] = { val: 0 }
               │       
               │       ///|
               │       test {
               │         let x = 42
               │       
               │       }
               │       ```
               │       
               │       ## Suggestion
               │       
               │       There are multiple ways to fix this warning:
               │       
               │       - If the variable is indeed useless, you can remove the definition of the
               │         variable.
               │       - If this variable is at the toplevel (i.e., not local), and is part of the
               │         public API of your module, you can add the `pub` keyword to the variable.
               │         ```moonbit
               │       
               │         ///|
               │         pub let p = 42
               │         ```
               │       - If you made a typo in the variable name, you can rename the variable to the
               │         correct name at the use site.
               │       - If your code depends on the side-effect of the variable, you can wrap the
               │         side-effect in a `fn init` block.
               │         ```moonbit
               │       
               │         ///|
               │         let side_effect : Ref[Int] = { val: 0 }
               │       
               │         ///|
               │         fn init {
               │           side_effect.val = 42
               │         }
               │         ```
               │       
               │       There are some cases where you might want to keep the variable private and
               │       unused at the same time. In this case, you can call `ignore()` on the variable
               │       to force the use of it.
               │       
               │       ```moonbit
               │       
               │       ///|
               │       let p_unused : Int = 42
               │       
               │       ///|
               │       test {
               │         ignore(p_unused)
               │       }
               │       
               │       ///|
               │       fn main {
               │         let x = 42
               │         ignore(x)
               │       }
               │       ```
            ───╯
        "#]],
    );
}

#[test]
fn test_add_mi_if_self_not_set_in_test_imports() {
    let dir = TestDir::new("self-pkg-in-test-import.in");

    check(
        get_stdout(
            &dir,
            ["check", "--target", "wasm-gc", "--dry-run", "--sort-input"],
        ),
        expect![[r#"
            moonc check ./lib/hello.mbt -o ./_build/wasm-gc/debug/check/lib/lib.mi -pkg username/hello/lib -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib:./lib -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./main/main.mbt -o ./_build/wasm-gc/debug/check/main/main.mi -pkg username/hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./_build/wasm-gc/debug/check/main/main.blackbox_test.mi -pkg username/hello/main_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lib -i ./_build/wasm-gc/debug/check/main/main.mi:main -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./lib3/hello.mbt -o ./_build/wasm-gc/debug/check/lib3/lib3.mi -pkg username/hello/lib3 -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib3:./lib3 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./lib3/hello_test.mbt -doctest-only ./lib3/hello.mbt -include-doctests -o ./_build/wasm-gc/debug/check/lib3/lib3.blackbox_test.mi -pkg username/hello/lib3_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib3/lib3.mi:lib3 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib3_blackbox_test:./lib3 -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./lib2/hello.mbt -o ./_build/wasm-gc/debug/check/lib2/lib2.mi -pkg username/hello/lib2 -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2:./lib2 -target wasm-gc -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./lib2/hello_test.mbt -doctest-only ./lib2/hello.mbt -include-doctests -o ./_build/wasm-gc/debug/check/lib2/lib2.blackbox_test.mi -pkg username/hello/lib2_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib2/lib2.mi:lib2 -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib2_blackbox_test:./lib2 -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
            moonc check ./lib/hello_test.mbt -doctest-only ./lib/hello.mbt -include-doctests -o ./_build/wasm-gc/debug/check/lib/lib.blackbox_test.mi -pkg username/hello/lib_blackbox_test -pkg-type library -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/check/lib/lib.mi:lll -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path . -all-pkgs ./_build/wasm-gc/debug/check/all_pkgs.json
        "#]],
    );

    check(get_stdout(&dir, ["check"]), expect![""]);
    get_stdout(&dir, ["clean"]);
    check(get_stderr(&dir, ["check"]), expect![""]);

    check(
        get_stdout(&dir, ["test", "--no-parallelize", "--sort-input"]),
        expect![[r#"
            Hello, world! lib
            Hello, world! lib2
            Hello, world! lib3
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
}

#[test]
fn test_update_expect_failed_with_multiline_string() {
    let dir = TestDir::new("test_expect_with_multiline_string_content.in");
    let _ = get_stdout(&dir, ["test", "-u"]);
    check(
        read(dir.join("src").join("lib").join("hello.mbt")),
        expect![[r#"
            ///|
            test {
              inspect("\"abc\"", content=(#|"abc"
              ))
              inspect("\"abc\"", 
                content=(
                  #|"abc"
            )
              )
              inspect("\"abc\"", content=(
                #|"abc"

              ))
              inspect(
                "\"a\nb\nc\"",
                content=(
                  #|"a
                  #|b
                  #|c"

                ),
              )
            }
        "#]],
    );
}

#[test]
fn test_ambiguous_pkg() {
    let dir = TestDir::new("ambiguous_pkg.in");

    // FIXME: Improve error message
    let stderr = get_err_stderr(&dir, ["build"]);
    println!("{}", stderr);
    assert!(
        stderr.contains("Ambiguous package name") || stderr.contains("Duplicated package name")
    );
}

#[test]
#[ignore = "subpackage is not fully supported yet"]
fn test_sub_package() {
    let dir = TestDir::new("test_sub_package.in");

    check(
        get_stdout(&dir, ["test", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./dep/hello.mbt -o ./_build/wasm-gc/debug/test/dep/dep.core -pkg moon_new/dep -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -pkg-sources moon_new/dep:./dep -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt -o ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.core -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./dep2/hello.mbt -o ./_build/wasm-gc/debug/test/dep2/dep2.core -pkg moon_new/dep2 -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/hello.mbt -o ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.core -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/test/__generated_driver_for_whitebox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/test/__whitebox_test_info.json ./test/hello_wbtest.mbt --target wasm-gc --pkg-name moon_new/test --driver-kind whitebox
            moonc build-package ./test/hello.mbt ./test/hello_wbtest.mbt ./_build/wasm-gc/debug/test/test/__generated_driver_for_whitebox_test.mbt -o ./_build/wasm-gc/debug/test/test/test.whitebox_test.core -pkg moon_new/test -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -g -O0 -source-map -whitebox-test -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/dep2/dep2.core ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./_build/wasm-gc/debug/test/test/test.whitebox_test.core -main moon_new/test -o ./_build/wasm-gc/debug/test/test/test.whitebox_test.wasm -test-mode -pkg-config-path ./test/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/test:./test -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/test/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/test/__internal_test_info.json ./test/hello.mbt --target wasm-gc --pkg-name moon_new/test --driver-kind internal
            moonc build-package ./test/hello.mbt ./_build/wasm-gc/debug/test/test/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/test/test.internal_test.core -pkg moon_new/test -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/dep2/dep2.core ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./_build/wasm-gc/debug/test/test/test.internal_test.core -main moon_new/test -o ./_build/wasm-gc/debug/test/test/test.internal_test.wasm -test-mode -pkg-config-path ./test/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/test:./test -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moonc build-package ./test/hello.mbt -o ./_build/wasm-gc/debug/test/test/test.core -pkg moon_new/test -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/test/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/test/__blackbox_test_info.json ./test/hello_test.mbt --doctest-only ./test/hello.mbt --target wasm-gc --pkg-name moon_new/test --driver-kind blackbox
            moonc build-package ./test/hello_test.mbt ./_build/wasm-gc/debug/test/test/__generated_driver_for_blackbox_test.mbt -doctest-only ./test/hello.mbt -o ./_build/wasm-gc/debug/test/test/test.blackbox_test.core -pkg moon_new/test_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -i ./_build/wasm-gc/debug/test/test/test.mi:test -pkg-sources moon_new/test_blackbox_test:./test -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/dep2/dep2.core ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./_build/wasm-gc/debug/test/test/test.core ./_build/wasm-gc/debug/test/test/test.blackbox_test.core -main moon_new/test_blackbox_test -o ./_build/wasm-gc/debug/test/test/test.blackbox_test.wasm -test-mode -pkg-config-path ./test/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/test:./test -pkg-sources moon_new/test_blackbox_test:./test -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/sub_pkg_sub/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/sub_pkg_sub/__internal_test_info.json ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt --target wasm-gc --pkg-name moon_new/sub_pkg_sub --driver-kind internal
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt ./_build/wasm-gc/debug/test/sub_pkg_sub/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.internal_test.core -pkg moon_new/sub_pkg -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/dep/dep.core ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.internal_test.core -main moon_new/sub_pkg -o ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.internal_test.wasm -test-mode -pkg-config-path ./sub_pkg/moon.pkg.json -pkg-sources moon_new/dep:./dep -pkg-sources moon_new/sub_pkg_sub:./sub_pkg -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/sub_pkg_sub/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/sub_pkg_sub/__blackbox_test_info.json --doctest-only ./sub_pkg/111.mbt --doctest-only ./sub_pkg/dir/222.mbt --target wasm-gc --pkg-name moon_new/sub_pkg_sub --driver-kind blackbox
            moonc build-package ./_build/wasm-gc/debug/test/sub_pkg_sub/__generated_driver_for_blackbox_test.mbt -doctest-only ./sub_pkg/111.mbt -doctest-only ./sub_pkg/dir/222.mbt -o ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.blackbox_test.core -pkg moon_new/sub_pkg_sub_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/dep/dep.mi:dep -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg_sub -pkg-sources moon_new/sub_pkg_sub_blackbox_test:./sub_pkg -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/dep/dep.core ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.core ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.blackbox_test.core -main moon_new/sub_pkg_blackbox_test -o ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.blackbox_test.wasm -test-mode -pkg-config-path ./sub_pkg/moon.pkg.json -pkg-sources moon_new/dep:./dep -pkg-sources moon_new/sub_pkg_sub:./sub_pkg -pkg-sources moon_new/sub_pkg_sub_blackbox_test:./sub_pkg -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/sub_pkg/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/sub_pkg/__internal_test_info.json ./sub_pkg/111.mbt ./sub_pkg/hello.mbt --target wasm-gc --pkg-name moon_new/sub_pkg --driver-kind internal
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/hello.mbt ./_build/wasm-gc/debug/test/sub_pkg/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.internal_test.core -pkg moon_new/sub_pkg -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/dep2/dep2.core ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.internal_test.core -main moon_new/sub_pkg -o ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.internal_test.wasm -test-mode -pkg-config-path ./sub_pkg/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/sub_pkg/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/sub_pkg/__blackbox_test_info.json --doctest-only ./sub_pkg/111.mbt --doctest-only ./sub_pkg/hello.mbt --target wasm-gc --pkg-name moon_new/sub_pkg --driver-kind blackbox
            moonc build-package ./_build/wasm-gc/debug/test/sub_pkg/__generated_driver_for_blackbox_test.mbt -doctest-only ./sub_pkg/111.mbt -doctest-only ./sub_pkg/hello.mbt -o ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.blackbox_test.core -pkg moon_new/sub_pkg_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/dep2/dep2.mi:dep2 -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/sub_pkg_blackbox_test:./sub_pkg -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/dep2/dep2.core ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.blackbox_test.core -main moon_new/sub_pkg_blackbox_test -o ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.blackbox_test.wasm -test-mode -pkg-config-path ./sub_pkg/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/sub_pkg_blackbox_test:./sub_pkg -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/main/__internal_test_info.json ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind internal
            moonc build-package ./main/main.mbt ./_build/wasm-gc/debug/test/main/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/main/main.internal_test.core -pkg moon_new/main -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/dep2/dep2.core ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./_build/wasm-gc/debug/test/main/main.internal_test.core -main moon_new/main -o ./_build/wasm-gc/debug/test/main/main.internal_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/test/main/main.core -pkg moon_new/main -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/main/__blackbox_test_info.json --doctest-only ./main/main.mbt --target wasm-gc --pkg-name moon_new/main --driver-kind blackbox
            moonc build-package ./_build/wasm-gc/debug/test/main/__generated_driver_for_blackbox_test.mbt -doctest-only ./main/main.mbt -o ./_build/wasm-gc/debug/test/main/main.blackbox_test.core -pkg moon_new/main_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/main/main.mi:main -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/dep2/dep2.core ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./_build/wasm-gc/debug/test/main/main.core ./_build/wasm-gc/debug/test/main/main.blackbox_test.core -main moon_new/main_blackbox_test -o ./_build/wasm-gc/debug/test/main/main.blackbox_test.wasm -test-mode -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/main:./main -pkg-sources moon_new/main_blackbox_test:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__internal_test_info.json ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind internal
            moonc build-package ./lib/hello.mbt ./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/lib/lib.internal_test.core -pkg moon_new/lib -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/dep2/dep2.core ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./_build/wasm-gc/debug/test/lib/lib.internal_test.core -main moon_new/lib -o ./_build/wasm-gc/debug/test/lib/lib.internal_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/lib:./lib -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moonc build-package ./lib/hello.mbt -o ./_build/wasm-gc/debug/test/lib/lib.core -pkg moon_new/lib -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib:./lib -target wasm-gc -g -O0 -source-map -workspace-path .
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/lib/__blackbox_test_info.json --doctest-only ./lib/hello.mbt --target wasm-gc --pkg-name moon_new/lib --driver-kind blackbox
            moonc build-package ./_build/wasm-gc/debug/test/lib/__generated_driver_for_blackbox_test.mbt -doctest-only ./lib/hello.mbt -o ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core -pkg moon_new/lib_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/lib/lib.mi:lib -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/dep2/dep2.core ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./_build/wasm-gc/debug/test/lib/lib.core ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.core -main moon_new/lib_blackbox_test -o ./_build/wasm-gc/debug/test/lib/lib.blackbox_test.wasm -test-mode -pkg-config-path ./lib/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/lib:./lib -pkg-sources moon_new/lib_blackbox_test:./lib -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/dep2/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/dep2/__internal_test_info.json ./dep2/hello.mbt --target wasm-gc --pkg-name moon_new/dep2 --driver-kind internal
            moonc build-package ./dep2/hello.mbt ./_build/wasm-gc/debug/test/dep2/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/dep2/dep2.internal_test.core -pkg moon_new/dep2 -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./_build/wasm-gc/debug/test/dep2/dep2.internal_test.core -main moon_new/dep2 -o ./_build/wasm-gc/debug/test/dep2/dep2.internal_test.wasm -test-mode -pkg-config-path ./dep2/moon.pkg.json -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/dep2:./dep2 -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/dep2/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/dep2/__blackbox_test_info.json --doctest-only ./dep2/hello.mbt --target wasm-gc --pkg-name moon_new/dep2 --driver-kind blackbox
            moonc build-package ./_build/wasm-gc/debug/test/dep2/__generated_driver_for_blackbox_test.mbt -doctest-only ./dep2/hello.mbt -o ./_build/wasm-gc/debug/test/dep2/dep2.blackbox_test.core -pkg moon_new/dep2_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/dep2/dep2.mi:dep2 -i ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2_blackbox_test:./dep2 -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/sub_pkg/sub_pkg.core ./_build/wasm-gc/debug/test/dep2/dep2.core ./_build/wasm-gc/debug/test/dep2/dep2.blackbox_test.core -main moon_new/dep2_blackbox_test -o ./_build/wasm-gc/debug/test/dep2/dep2.blackbox_test.wasm -test-mode -pkg-config-path ./dep2/moon.pkg.json -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/dep2_blackbox_test:./dep2 -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/dep/__generated_driver_for_internal_test.mbt --output-metadata ./_build/wasm-gc/debug/test/dep/__internal_test_info.json ./dep/hello.mbt --target wasm-gc --pkg-name moon_new/dep --driver-kind internal
            moonc build-package ./dep/hello.mbt ./_build/wasm-gc/debug/test/dep/__generated_driver_for_internal_test.mbt -o ./_build/wasm-gc/debug/test/dep/dep.internal_test.core -pkg moon_new/dep -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -pkg-sources moon_new/dep:./dep -target wasm-gc -g -O0 -source-map -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/dep/dep.internal_test.core -main moon_new/dep -o ./_build/wasm-gc/debug/test/dep/dep.internal_test.wasm -test-mode -pkg-config-path ./dep/moon.pkg.json -pkg-sources moon_new/dep:./dep -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
            moon generate-test-driver --output-driver ./_build/wasm-gc/debug/test/dep/__generated_driver_for_blackbox_test.mbt --output-metadata ./_build/wasm-gc/debug/test/dep/__blackbox_test_info.json --doctest-only ./dep/hello.mbt --target wasm-gc --pkg-name moon_new/dep --driver-kind blackbox
            moonc build-package ./_build/wasm-gc/debug/test/dep/__generated_driver_for_blackbox_test.mbt -doctest-only ./dep/hello.mbt -o ./_build/wasm-gc/debug/test/dep/dep.blackbox_test.core -pkg moon_new/dep_blackbox_test -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/test/dep/dep.mi:dep -pkg-sources moon_new/dep_blackbox_test:./dep -target wasm-gc -g -O0 -source-map -blackbox-test -include-doctests -no-mi -test-mode -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/test/dep/dep.core ./_build/wasm-gc/debug/test/dep/dep.blackbox_test.core -main moon_new/dep_blackbox_test -o ./_build/wasm-gc/debug/test/dep/dep.blackbox_test.wasm -test-mode -pkg-config-path ./dep/moon.pkg.json -pkg-sources moon_new/dep:./dep -pkg-sources moon_new/dep_blackbox_test:./dep -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -exported_functions 'moonbit_test_driver_internal_execute,moonbit_test_driver_finish' -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["check", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc check ./dep/hello.mbt -o ./_build/wasm-gc/release/check/dep/dep.mi -pkg moon_new/dep -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -pkg-sources moon_new/dep:./dep -target wasm-gc -workspace-path .
            moonc check ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt -o ./_build/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc check ./dep2/hello.mbt -o ./_build/wasm-gc/release/check/dep2/dep2.mi -pkg moon_new/dep2 -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -workspace-path .
            moonc check ./sub_pkg/111.mbt ./sub_pkg/hello.mbt -o ./_build/wasm-gc/release/check/sub_pkg/sub_pkg.mi -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -workspace-path .
            moonc check ./test/hello.mbt ./test/hello_wbtest.mbt -o ./_build/wasm-gc/release/check/test/test.whitebox_test.mi -pkg moon_new/test -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -whitebox-test -workspace-path .
            moonc check ./test/hello.mbt -o ./_build/wasm-gc/release/check/test/test.mi -pkg moon_new/test -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/test:./test -target wasm-gc -workspace-path .
            moonc check ./test/hello_test.mbt -doctest-only ./test/hello.mbt -include-doctests -o ./_build/wasm-gc/release/check/test/test.blackbox_test.mi -pkg moon_new/test_blackbox_test -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -i ./_build/wasm-gc/release/check/test/test.mi:test -pkg-sources moon_new/test_blackbox_test:./test -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./sub_pkg/111.mbt -doctest-only ./sub_pkg/dir/222.mbt -include-doctests -o ./_build/wasm-gc/release/check/sub_pkg/sub_pkg_sub.blackbox_test.mi -pkg moon_new/sub_pkg_blackbox_test -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/dep/dep.mi:dep -i ./_build/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg_sub -pkg-sources moon_new/sub_pkg_blackbox_test:./sub_pkg -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./sub_pkg/111.mbt -doctest-only ./sub_pkg/hello.mbt -include-doctests -o ./_build/wasm-gc/release/check/sub_pkg/sub_pkg.blackbox_test.mi -pkg moon_new/sub_pkg_blackbox_test -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/dep2/dep2.mi:dep2 -i ./_build/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/sub_pkg_blackbox_test:./sub_pkg -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./main/main.mbt -o ./_build/wasm-gc/release/check/main/main.mi -pkg moon_new/main -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -workspace-path .
            moonc check -doctest-only ./main/main.mbt -include-doctests -o ./_build/wasm-gc/release/check/main/main.blackbox_test.mi -pkg moon_new/main_blackbox_test -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/main/main.mi:main -i ./_build/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main_blackbox_test:./main -target wasm-gc -blackbox-test -workspace-path .
            moonc check ./lib/hello.mbt -o ./_build/wasm-gc/release/check/lib/lib.mi -pkg moon_new/lib -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib:./lib -target wasm-gc -workspace-path .
            moonc check -doctest-only ./lib/hello.mbt -include-doctests -o ./_build/wasm-gc/release/check/lib/lib.blackbox_test.mi -pkg moon_new/lib_blackbox_test -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/lib/lib.mi:lib -i ./_build/wasm-gc/release/check/sub_pkg/sub_pkg.mi:sub_pkg -pkg-sources moon_new/lib_blackbox_test:./lib -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./dep2/hello.mbt -include-doctests -o ./_build/wasm-gc/release/check/dep2/dep2.blackbox_test.mi -pkg moon_new/dep2_blackbox_test -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/dep2/dep2.mi:dep2 -i ./_build/wasm-gc/release/check/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2_blackbox_test:./dep2 -target wasm-gc -blackbox-test -workspace-path .
            moonc check -doctest-only ./dep/hello.mbt -include-doctests -o ./_build/wasm-gc/release/check/dep/dep.blackbox_test.mi -pkg moon_new/dep_blackbox_test -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/release/check/dep/dep.mi:dep -pkg-sources moon_new/dep_blackbox_test:./dep -target wasm-gc -blackbox-test -workspace-path .
        "#]],
    );
    check(
        get_stdout(&dir, ["build", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./dep/hello.mbt -o ./_build/wasm-gc/debug/build/dep/dep.core -pkg moon_new/dep -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -pkg-sources moon_new/dep:./dep -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt -o ./_build/wasm-gc/debug/build/sub_pkg/sub_pkg_sub.core -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./dep2/hello.mbt -o ./_build/wasm-gc/debug/build/dep2/dep2.core -pkg moon_new/dep2 -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/hello.mbt -o ./_build/wasm-gc/debug/build/sub_pkg/sub_pkg.core -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg moon_new/main -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/dep2/dep2.core ./_build/wasm-gc/debug/build/sub_pkg/sub_pkg.core ./_build/wasm-gc/debug/build/main/main.core -main moon_new/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
        "#]],
    );
    check(
        get_stdout(&dir, ["run", "main", "--dry-run", "--sort-input"]),
        expect![[r#"
            moonc build-package ./dep/hello.mbt -o ./_build/wasm-gc/debug/build/dep/dep.core -pkg moon_new/dep -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -pkg-sources moon_new/dep:./dep -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/dir/222.mbt -o ./_build/wasm-gc/debug/build/sub_pkg/sub_pkg_sub.core -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/dep/dep.mi:dep -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./dep2/hello.mbt -o ./_build/wasm-gc/debug/build/dep2/dep2.core -pkg moon_new/dep2 -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/dep2:./dep2 -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./sub_pkg/111.mbt ./sub_pkg/hello.mbt -o ./_build/wasm-gc/debug/build/sub_pkg/sub_pkg.core -pkg moon_new/sub_pkg -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/dep2/dep2.mi:dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc build-package ./main/main.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg moon_new/main -is-main -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i ./_build/wasm-gc/debug/build/sub_pkg/sub_pkg_sub.mi:sub_pkg -pkg-sources moon_new/main:./main -target wasm-gc -g -O0 -source-map -workspace-path .
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/dep2/dep2.core ./_build/wasm-gc/debug/build/sub_pkg/sub_pkg.core ./_build/wasm-gc/debug/build/main/main.core -main moon_new/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources moon_new/dep2:./dep2 -pkg-sources moon_new/sub_pkg:./sub_pkg -pkg-sources moon_new/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
            moonrun ./_build/wasm-gc/debug/build/main/main.wasm --
        "#]],
    );
}

#[test]
fn test_in_main_pkg() {
    let dir = TestDir::new("test_in_main_pkg.in");

    check(
        get_stderr(&dir, ["check"]),
        expect![[r#"
            Warning: Main package `username/hello/main` uses blackbox-only test inputs (`_test.mbt` files) in package directory "$ROOT/main". Main packages will stop generating blackbox tests in a future release. Move public behavior into a non-main package and keep the main package as an entrypoint.
            Warning: [0002]
               ╭─[ $ROOT/lib/1_test.mbt:2:7 ]
               │
             2 │   let a = 1
               │       ┬  
               │       ╰── Warning (unused_value): Unused variable 'a'
            ───╯
        "#]],
    );

    check(
        get_stdout(
            &dir,
            ["test", "-p", "main", "--sort-input", "--no-parallelize"],
        ),
        expect![[r#"
            hello from lib pkg
            ------------------bb test in main pkg ------------------
            hello from lib pkg
            ------------------internal test in main pkg ------------------
            hello from lib pkg
            ------------------ wb test in main pkg ------------------
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );
    check(
        get_stdout(&dir, ["test", "--sort-input", "--no-parallelize"]),
        expect![[r#"
            ------------------bb test in lib pkg ------------------
            ------------------internal test in lib pkg ------------------
            ------------------ wb test in lib pkg ------------------
            hello from lib pkg
            ------------------bb test in main pkg ------------------
            hello from lib pkg
            ------------------internal test in main pkg ------------------
            hello from lib pkg
            ------------------ wb test in main pkg ------------------
            Total tests: 6, passed: 6, failed: 0.
        "#]],
    );
}

#[test]
fn moon_test_target_js_panic_with_sourcemap() {
    let dir = TestDir::new("moon_test_target_js_panic_with_sourcemap.in");

    let output = get_err_stdout(&dir, ["test", "--target", "js"]);

    // Extract first 4 lines + the last line (Total tests) as they should be consistent across Node.js versions
    let lines: Vec<&str> = output.lines().collect();
    let first_four_lines = lines.iter().take(4).cloned().collect::<Vec<_>>().join("\n");
    let last_line = lines.last().unwrap_or(&"");
    let filtered_output = format!("{}\n{}", first_four_lines, last_line);

    check(
        &filtered_output,
        // should keep in this format, it's used in ide test explorer
        expect![[r#"
            [username/hello] test lib/hello_test.mbt:1 ("hello") failed: Error
                at $panic ($ROOT/_build/js/debug/test/lib/lib.blackbox_test.js:16:9)
                at f ($ROOT/src/lib/hello_test.mbt:3:5)
                at moonbit_test_driver_internal_js_catch ($ROOT/src/lib/__generated_driver_for_blackbox_test.mbt:389:11)
            Total tests: 1, passed: 0, failed: 1."#]],
    );
}
