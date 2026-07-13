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
fn test_backtrace() {
    let dir = TestDir::new("backtrace.in");

    let out = get_err_stderr(&dir, ["run", "main"]);
    assert!(out.contains("main.foo"));
    assert!(out.contains("main.bar"));
    assert!(!out.contains("4main3foo"));
    assert!(!out.contains("4main3bar"));

    let out = get_err_stderr(&dir, ["run", "main", "--debug"]);
    assert!(out.contains("main.foo"));
    assert!(out.contains("main.bar"));
    assert!(!out.contains("4main3foo"));
    assert!(!out.contains("4main3bar"));
}

#[test]
fn bench2_test() {
    let dir = TestDir::new("bench2_test.in");
    moon_cmd(&dir)
        .args(["run", "main"])
        .assert()
        .success()
        .stdout_eq("ok[..]");
}

#[test]
fn cakenew_test() {
    let dir = TestDir::new("cakenew_test.in");
    moon_cmd(&dir)
        .args(["run", "main"])
        .assert()
        .success()
        .stdout_eq("Hello,[..]");
}

#[test]
fn capture_abort_test() {
    let dir = super::TestDir::new("capture_abort_test.in");
    moon_cmd(&dir)
        .args(["run", "main", "--nostd"])
        .assert()
        .failure();
}

#[test]
fn test_moon_run_with_cli_args() {
    let dir = TestDir::new("moon_run_with_cli_args.in");

    check(
        get_stdout(&dir, ["run", "main", "--dry-run"]),
        expect![[r#"
            moonc build-package ./main/exit_wasm_gc.mbt ./main/main_wasm.mbt -o ./_build/wasm-gc/debug/build/main/main.core -pkg username/hello/main -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/debug/debug.mi:debug' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources username/hello/main:./main -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/main/main.core -main username/hello/main -o ./_build/wasm-gc/debug/build/main/main.wasm -pkg-config-path ./main/moon.pkg.json -pkg-sources username/hello/main:./main -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
            moonrun ./_build/wasm-gc/debug/build/main/main.wasm --
        "#]],
    );

    assert_dry_run_graph(
        &dir,
        [
            "run",
            "main",
            "--dry-run",
            "--",
            "中文",
            "😄👍",
            "hello",
            "1242",
        ],
        expect_file!["./moon_run_with_cli_args_graph.jsonl"],
    );

    let s = get_stdout(
        &dir,
        [
            "run", "main", "--", "中文", "😄👍", "hello", "1242", "--flag",
        ],
    );
    let expected_args = "中文\n😄👍\nhello\n1242\n--flag\n";
    assert!(s.contains(expected_args));

    moon_cmd(&dir).args(["build"]).assert().success();
    let wasm_file = dir.join("_build/wasm-gc/debug/build/main/main.wasm");
    let stdout = moon_cmd(&dir)
        .arg("run")
        .arg(&wasm_file)
        .args(["--", "中文", "😄👍", "hello", "1242", "--flag"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let s = replace_dir(std::str::from_utf8(&stdout).unwrap(), &dir);
    assert!(s.contains(expected_args));

    let stdout = moon_cmd(&dir)
        .arg("run")
        .arg(&wasm_file)
        .args(["--dry-run", "--", "hello"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    check(
        replace_dir(std::str::from_utf8(&stdout).unwrap(), &dir),
        expect![[r#"
            moonrun ./_build/wasm-gc/debug/build/main/main.wasm -- hello
        "#]],
    );

    let wasm_named_package = dir.join("main.wasm");
    std::fs::create_dir_all(&wasm_named_package).unwrap();
    std::fs::write(
        wasm_named_package.join("moon.pkg.json"),
        r#"{
  "is-main": true
}
"#,
    )
    .unwrap();
    std::fs::write(
        wasm_named_package.join("main.mbt"),
        r#"fn main {
  println("package directory ending in wasm")
}
"#,
    )
    .unwrap();
    check(
        get_stdout(&dir, ["run", "main.wasm"]),
        expect![[r#"
            package directory ending in wasm
        "#]],
    );

    let s = get_stdout(
        &dir,
        [
            "run", "main", "--target", "js", "--", "中文", "😄👍", "hello", "1242", "--flag",
        ],
    );
    assert!(s.contains(expected_args));
}

#[test]
fn test_js() {
    let dir = TestDir::new("test_filter/test_filter");

    let output = get_stdout(
        &dir,
        [
            "test",
            "-p",
            "username/hello/lib",
            "--target",
            "js",
            "--sort-input",
            "--no-parallelize",
        ],
    );
    check(
        &output,
        expect![[r#"
            test A
            test hello_0
            test hello_1
            Total tests: 3, passed: 3, failed: 0.
        "#]],
    );

    let output = get_stdout(
        &dir,
        [
            "test",
            "-p",
            "username/hello/lib",
            "--file",
            "hello_wbtest.mbt",
            "-i",
            "1",
            "--target",
            "js",
            "--sort-input",
            "--no-parallelize",
        ],
    );
    check(
        &output,
        expect![[r#"
            test hello_1
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );
}
