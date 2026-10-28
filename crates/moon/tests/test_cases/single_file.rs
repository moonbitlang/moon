// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use super::*;

#[cfg(unix)]
#[test]
fn test_moon_run_single_file_dry_run() {
    let dir = TestDir::new("run_single_mbtx_file.in");

    let mut output = get_stdout_with_envs(
        &dir,
        ["run", "a/b/single.mbtx", "--target", "native", "--dry-run"],
        [("MOONBIT_NEW_NATIVE", "0")],
    )
    // Normalize clang-only warnings to keep snapshots portable across macOS/Linux.
    .replace(" -Wno-unused-value", "")
    .replace("$MOON_TOOLCHAIN_ROOT", "$MOON_HOME");
    crate::util::normalize_host_archiver(&mut output);
    // macOS has an additional symbol-bundle action. Check that boundary
    // separately so the remaining dry-run snapshot stays shared with Linux.
    let dsym_commands = output
        .lines()
        .filter(|line| line.contains("dsymutil"))
        .collect::<Vec<_>>();
    assert_eq!(dsym_commands.len(), usize::from(cfg!(target_os = "macos")));
    if let Some(command) = dsym_commands.first() {
        assert!(command.ends_with("./_build/single.mbtx/native/debug/build/single/single.exe"));
        output = output
            .lines()
            .filter(|line| !line.contains("dsymutil"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
    }
    expect_file!["single_file/native_debug.snap"].assert_eq(&output);

    let mut output = get_stdout(
        &dir,
        [
            "run",
            "a/b/single.mbtx",
            "--target",
            "native",
            "--dry-run",
            "--release",
        ],
    )
    // Normalize clang-only warnings to keep snapshots portable across macOS/Linux.
    .replace(" -Wno-unused-value", "")
    .replace("$MOON_TOOLCHAIN_ROOT", "$MOON_HOME");
    crate::util::normalize_host_archiver(&mut output);
    expect_file!["single_file/native_release.snap"].assert_eq(&output);

    for build_only in [None, Some("--build-only")] {
        let output = get_stdout(
            &dir,
            ["run", "a/b/single.mbtx", "--target", "js", "--dry-run"]
                .into_iter()
                .chain(build_only),
        );
        check(
            output.replace("$MOON_TOOLCHAIN_ROOT", "$MOON_HOME"),
            expect![[r#"
                moonc build-package ./single.mbtx -ignore-import-declaration -w +core_package_not_imported@core_package_not_imported -o ./_build/single.mbtx/js/debug/build/single/single.core -pkg moon/test/single -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/js/release/bundle' -i '$MOON_HOME/lib/core/_build/js/release/bundle/prelude/prelude.mi:prelude' -pkg-sources moon/test/single:. -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/single.mbtx/js/debug/build/all_pkgs.json
                moonc link-core '$MOON_HOME/lib/core/_build/js/release/bundle/core.core' ./_build/single.mbtx/js/debug/build/single/single.core -main moon/test/single -o ./_build/single.mbtx/js/debug/build/single/single.js -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target js -g -O0 -source-map
                node --enable-source-maps ./_build/single.mbtx/js/debug/build/single/single.js
            "#]],
        );
    }

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbtx", "--target", "wasm-gc", "--dry-run"],
    );
    check(
        output.replace("$MOON_TOOLCHAIN_ROOT", "$MOON_HOME"),
        expect![[r#"
            moonc build-package ./single.mbtx -ignore-import-declaration -w +core_package_not_imported@core_package_not_imported -o ./_build/single.mbtx/wasm-gc/debug/build/single/single.core -pkg moon/test/single -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/prelude/prelude.mi:prelude' -pkg-sources moon/test/single:. -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/single.mbtx/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/single.mbtx/wasm-gc/debug/build/single/single.core -main moon/test/single -o ./_build/single.mbtx/wasm-gc/debug/build/single/single.wasm -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
            '$MOONRUN_OVERRIDE' ./_build/single.mbtx/wasm-gc/debug/build/single/single.wasm --
        "#]],
    );

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbtx", "--target", "js", "--build-only"],
    );
    check(
        &output,
        expect![[r#"
            {"artifacts_path":["$ROOT/a/b/_build/single.mbtx/js/debug/build/single/single.js"]}
        "#]],
    );
    assert!(
        dir.join("a/b/_build/single.mbtx/js/debug/build/single/single.js")
            .exists()
    );
}

#[test]
fn test_moon_run_wasm_policy_is_only_forwarded_to_wasm_backends() {
    let dir = TestDir::new("run_single_mbtx_file.in");

    let wasm_output = get_stdout(
        &dir,
        [
            "run",
            "a/b/single.mbtx",
            "--target",
            "wasm",
            "--wasm-policy",
            "moonrun-policy.json",
            "--dry-run",
        ],
    );
    assert!(
        wasm_output
            .lines()
            .last()
            .is_some_and(|line| line.contains("--policy moonrun-policy.json")),
        "expected Wasm run command to forward the policy:\n{wasm_output}"
    );

    let wasm_gc_output = get_stdout(
        &dir,
        [
            "run",
            "a/b/single.mbtx",
            "--target",
            "wasm-gc",
            "--wasm-policy",
            "wasm-gc-policy.json",
            "--dry-run",
        ],
    );
    assert!(
        wasm_gc_output
            .lines()
            .last()
            .is_some_and(|line| line.contains("single.wasm")),
        "expected the WasmGC run command to use moonrun:\n{wasm_gc_output}"
    );
    assert!(
        wasm_gc_output
            .lines()
            .last()
            .is_some_and(|line| line.contains("--policy wasm-gc-policy.json")),
        "expected the WasmGC run command to forward the policy:\n{wasm_gc_output}"
    );

    let js_output = get_stdout(
        &dir,
        [
            "run",
            "a/b/single.mbtx",
            "--target",
            "js",
            "--wasm-policy",
            "missing-policy.json",
            "--dry-run",
        ],
    );
    assert!(
        js_output
            .lines()
            .last()
            .is_some_and(|line| line.contains("single.js")),
        "expected the JavaScript run command to remain unchanged:\n{js_output}"
    );
    assert!(
        !js_output.contains("missing-policy.json"),
        "expected the JavaScript backend to ignore the policy:\n{js_output}"
    );
}

#[test]
fn test_moon_run_single_mbtx_file() {
    let dir = TestDir::new("run_single_mbtx_file.in");

    let output = get_stdout(&dir, ["run", "a/b/single.mbtx"]);
    check(
        &output,
        expect![[r#"
        I am OK
    "#]],
    );

    let output = get_stdout(
        &dir.join("a").join("b").join("c"),
        ["run", "../single.mbtx"],
    );
    check(
        &output,
        expect![[r#"
            I am OK
            "#]],
    );

    let output = get_stdout(
        &dir.join("a").join("b"),
        ["run", "single.mbtx", "--target", "js"],
    );
    check(
        &output,
        expect![[r#"
        I am OK
        "#]],
    );

    let output = get_stdout(
        &dir.join("a").join("b"),
        ["run", "single.mbtx", "--target", "native"],
    );
    // cl have other output
    assert!(output.contains("I am OK"));
}

#[test]
fn test_moon_run_single_mbt_file_inside_a_pkg() {
    let dir = TestDir::new("run_single_mbt_file_inside_pkg.in");

    let output = get_stdout(&dir, ["run", "main/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(&dir, ["run", "lib/main_in_lib/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );

    let output = get_stdout(&dir.join("lib"), ["run", "../main/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(&dir.join("lib"), ["run", "main_in_lib/main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );

    let output = get_stdout(
        &dir.join("lib").join("main_in_lib"),
        ["run", "../../main/main.mbt"],
    );
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(&dir.join("lib").join("main_in_lib"), ["run", "main.mbt"]);
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );

    let output = get_stdout(
        &dir.join("lib").join("main_in_lib"),
        ["run", "../../main/main.mbt", "--target", "native"],
    );
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            root main
        "#]],
    );
    let output = get_stdout(
        &dir.join("lib").join("main_in_lib"),
        ["run", "main.mbt", "--target", "native"],
    );
    check(
        &output,
        expect![[r#"
            Hello, world!!!
            main in lib
        "#]],
    );
}

#[test]
fn moon_check_and_test_single_file() {
    let dir = TestDir::new("moon_test_single_file.in");
    let single_mbt = dir.join("single.mbtx").display().to_string();
    let single_mbt_md = dir.join("111.mbt.md").display().to_string();

    // .mbtx
    {
        // rel path
        check(
            get_stdout(&dir, ["test", "single.mbtx", "-i", "0"]),
            expect![[r#"
                ------------------ 11111111 ------------------
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );
        check(
            get_err_stdout(&dir, ["test", "single.mbtx", "-i", "1"]),
            expect![[r#"
                [moon/test] test single/single.mbtx:14 (#1) failed
                expect test failed at $ROOT/single.mbtx:15:3-15:18
                Diff: (- expected, + actual)
                ----
                +234523
                ----

                Total tests: 1, passed: 0, failed: 1.
            "#]],
        );
        // abs path
        check(
            get_stdout(&dir, ["test", &single_mbt, "-i", "0"]),
            expect![[r#"
                ------------------ 11111111 ------------------
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );
        let s = get_stdout(&dir, ["test", &single_mbt, "-i", "1", "-u"]);
        let exp = r#"
------------------ 22222222 ------------------
Total tests: 1, passed: 1, failed: 0.
"#
        .trim();
        assert!(
            s.contains(exp),
            "output did not contain expected updated test output"
        ); // FIXME: this is because different versions have different output during update expect

        check(
            get_stderr(&dir, ["check", "single.mbtx"]),
            expect![[r#"
                Warning: [0002]
                    ╭─[ $ROOT/single.mbtx:10:7 ]
                    │
                 10 │   let single_mbt = 1
                    │       ─────┬────  
                    │            ╰────── Warning (unused_value): Unused variable 'single_mbt'
                ────╯
                Finished. moon: ran 2 tasks, now up to date (1 warnings, 0 errors)
            "#]],
        );
        // abs path
        check(
            get_stderr(&dir, ["check", &single_mbt]),
            expect![[r#"
                Warning: [0002]
                    ╭─[ $ROOT/single.mbtx:10:7 ]
                    │
                 10 │   let single_mbt = 1
                    │       ─────┬────  
                    │            ╰────── Warning (unused_value): Unused variable 'single_mbt'
                ────╯
                Finished. moon: ran 1 task, now up to date (1 warnings, 0 errors)
            "#]],
        );
    }

    // .mbt.md
    {
        check(
            get_stdout(&dir, ["test", "222.mbt.md"]),
            expect![[r#"
                222
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );

        // rel path
        let s = get_stdout(&dir, ["test", "111.mbt.md", "-i", "0"]);
        assert!(
            s.contains("111"),
            "output did not contain expected test output"
        );

        check(
            get_err_stdout(&dir, ["test", "111.mbt.md", "-i", "1"]),
            expect![[r#"
                [moon/test] test single/111.mbt.md:28 (#1) failed
                expect test failed at $ROOT/111.mbt.md:35:5-35:20
                Diff: (- expected, + actual)
                ----
                +234523
                ----

                Total tests: 1, passed: 0, failed: 1.
            "#]],
        );
        // abs path
        check(
            get_stdout(&dir, ["test", &single_mbt_md, "-i", "0"]),
            expect![[r#"
                111
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );

        let s = get_stdout(&dir, ["test", &single_mbt_md, "-i", "1", "-u"]);
        assert!(
            s.contains("222"),
            "output did not contain expected updated test output"
        );
        assert!(
            s.contains("Total tests: 1, passed: 1, failed: 0."),
            "output did not contain expected updated test output"
        );

        // rel path
        snapbox::assert_data_eq!(
            get_stderr(&dir, ["check", "111.mbt.md"]),
            snapbox::str![[r#"
Warning: [0002]
    ╭─[ $ROOT/111.mbt.md:29:9 ]
    │
 29 │     let single_mbt_md = 1
    │         ──────┬──────  
    │               ╰──────── Warning (unused_value): Unused variable 'single_mbt_md'
────╯
...
"#]]
        );

        // abs path
        check(
            get_stderr(&dir, ["check", &single_mbt_md]),
            expect![[r#"
                Warning: [0002]
                    ╭─[ $ROOT/111.mbt.md:29:9 ]
                    │
                 29 │     let single_mbt_md = 1
                    │         ──────┬──────  
                    │               ╰──────── Warning (unused_value): Unused variable 'single_mbt_md'
                ────╯
                Finished. moon: ran 1 task, now up to date (1 warnings, 0 errors)
            "#]],
        );
    }

    // check single file (script with main or Markdown library without main)
    {
        let with_main = dir.join("with_main.mbtx").display().to_string();
        snapbox::assert_data_eq!(
            get_stderr(&dir, ["check", &with_main]),
            snapbox::str![[r#"
Warning: [0002]
   ╭─[ $ROOT/with_main.mbtx:2:7 ]
   │
 2 │   let with_main = 1
   │       ────┬────  
   │           ╰────── Warning (unused_value): Unused variable 'with_main'
───╯
...
"#]],
        );
        let without_main = dir.join("without_main.mbt.md").display().to_string();
        snapbox::assert_data_eq!(
            get_stderr(&dir, ["check", &without_main]),
            snapbox::str![[r#"
Warning: [0001]
   ╭─[ $ROOT/without_main.mbt.md:2:4 ]
   │
 2 │ fn func() -> Unit {
   │    ──┬─  
   │      ╰─── Warning (unused_value): Unused function 'func'
───╯
Warning: [0002]
   ╭─[ $ROOT/without_main.mbt.md:3:7 ]
   │
 3 │   let without_main = 1
   │       ──────┬─────  
   │             ╰─────── Warning (unused_value): Unused variable 'without_main'
───╯
...
"#]],
        );
    }
}

/// Test that single-file commands properly report errors for non-existent files
/// instead of panicking (issue #1192)
#[test]
fn test_single_file_nonexistent_path_error() {
    // Use temp_dir for cross-platform compatibility
    let temp_dir = std::env::temp_dir();
    let nonexistent_path = std::env::temp_dir()
        .join("nonexistent_file_12345.mbt")
        .display()
        .to_string();

    // Test moon check with non-existent file outside any project
    // Should fail gracefully (exit != 101 which is Rust panic code)
    let check_result = moon_cmd(&temp_dir)
        .args(["check", &nonexistent_path])
        .assert()
        .failure();
    // Verify it's not a panic (exit code 101)
    assert_ne!(
        check_result.get_output().status.code(),
        Some(101),
        "moon check should not panic for non-existent file"
    );

    // Test moon test with non-existent file outside any project
    let test_result = moon_cmd(&temp_dir)
        .args(["test", &nonexistent_path])
        .assert()
        .failure();
    // Verify it's not a panic (exit code 101)
    assert_ne!(
        test_result.get_output().status.code(),
        Some(101),
        "moon test should not panic for non-existent file"
    );

    // Test moon run with non-existent file outside any project
    let run_result = moon_cmd(&temp_dir)
        .args(["run", &nonexistent_path])
        .assert()
        .failure();
    // Verify it's not a panic (exit code 101)
    assert_ne!(
        run_result.get_output().status.code(),
        Some(101),
        "moon run should not panic for non-existent file"
    );
}

#[test]
fn test_single_file_commands_work_with_workspace_disabled() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    std::fs::write(
        dir.join("hello.mbtx"),
        r#"fn main {
  println("hello")
}
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("test.mbtx"),
        r#"fn main {}
test "x" {
  assert_true(true)
}
"#,
    )
    .unwrap();

    let check_result = moon_cmd(&dir)
        .env(MOON_NO_WORKSPACE, "1")
        .args(["check", "hello.mbtx", "--target", "wasm-gc"])
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();
    check(
        String::from_utf8(check_result).unwrap(),
        expect![[r#"
            Warning: `MOON_NO_WORKSPACE` is deprecated. Use `MOON_WORK=off` to disable workspace mode.
            Finished. moon: ran 2 tasks, now up to date
        "#]],
    );
    assert!(!packages_selector_path(dir).exists());
    let target_dir = standalone_target_dir(dir, "hello.mbtx");
    let packages_selector = target_packages_selector_path(&target_dir);
    let scoped_pkg_json = target_scoped_packages_json_path(&target_dir, "wasm-gc", "debug");
    let packages_selector: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(packages_selector).unwrap()).unwrap();
    assert_eq!(
        packages_selector,
        serde_json::json!({
            "backend": "wasm-gc",
            "opt_level": "debug"
        })
    );
    let scoped_pkg_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(scoped_pkg_json).unwrap()).unwrap();
    assert_eq!(scoped_pkg_json["backend"], serde_json::json!("wasm-gc"));
    assert_eq!(scoped_pkg_json["opt_level"], serde_json::json!("debug"));
    let packages_index: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(target_packages_index_path(&target_dir)).unwrap(),
    )
    .unwrap();
    assert_eq!(packages_index, serde_json::json!(["wasm-gc"]));

    check(
        get_stdout_with_envs(&dir, ["test", "test.mbtx"], [(MOON_NO_WORKSPACE, "1")]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout_with_envs(&dir, ["run", "hello.mbtx"], [(MOON_NO_WORKSPACE, "1")]),
        expect![[r#"
            hello
        "#]],
    );

    check(
        get_stdout_with_envs(&dir, ["run", "hello.mbtx"], [(MOON_WORK_ENV, "off")]),
        expect![[r#"
            hello
        "#]],
    );
}

#[test]
fn test_single_files_use_isolated_target_directories() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let source = r#"fn main {
  println("hello")
}
"#;
    std::fs::write(dir.join("main.mbtx"), source).unwrap();
    std::fs::write(
        dir.join("main.mbt.md"),
        "```mbt check\nfn main { println(\"hello\") }\n```\n",
    )
    .unwrap();

    for filename in ["main.mbtx", "main.mbt.md"] {
        moon_cmd(&dir)
            .env(MOON_WORK_ENV, "off")
            .args(["check", filename, "--target", "wasm-gc"])
            .assert()
            .success();

        let target_dir = standalone_target_dir(dir, filename);
        assert!(target_dir.join(".moon_db").is_file());
        assert!(target_packages_selector_path(&target_dir).is_file());
    }

    assert!(!dir.join("_build/.moon_db").exists());
    assert_ne!(
        standalone_target_dir(dir, "main.mbtx"),
        standalone_target_dir(dir, "main.mbt.md")
    );
}

#[test]
fn test_single_file_check_accepts_multiple_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    let target_dir = standalone_target_dir(dir, "hello.mbtx");
    for (header, body, explicit_import) in [
        ("", "println(1)", None),
        ("import {}\n", "println(1)", None),
        ("----\npermission()\n----\n", "println(1)", None),
        (
            "import { \"moonbitlang/core/int\" @integer }\n",
            "println(@integer.MAX_VALUE)",
            Some(("moonbitlang/core/int", "integer")),
        ),
    ] {
        std::fs::write(
            dir.join("hello.mbtx"),
            format!("{header}fn main {{ {body} }}\n"),
        )
        .unwrap();

        moon_cmd(&dir)
            .env(MOON_WORK_ENV, "off")
            .args(["check", "hello.mbtx", "--target", "wasm-gc,js"])
            .assert()
            .success();

        for backend in ["wasm-gc", "js"] {
            let metadata: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(target_scoped_packages_json_path(
                    &target_dir,
                    backend,
                    "debug",
                ))
                .unwrap(),
            )
            .unwrap();
            let script = metadata["packages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|pkg| pkg["root"] == "moon/test" && pkg["rel"] == "single")
                .unwrap();
            assert_eq!(script["is-main"], true);
            let mut imports = script["deps"]
                .as_array()
                .unwrap()
                .iter()
                .map(|dep| {
                    (
                        dep["path"].as_str().unwrap(),
                        dep["alias"].as_str().unwrap(),
                    )
                })
                .collect::<Vec<_>>();
            imports.sort();
            let mut expected = vec![("moonbitlang/core/prelude", "prelude")];
            expected.extend(explicit_import);
            expected.sort();
            assert_eq!(imports, expected, "only the ordinary prelude is implicit");
        }
    }
    let packages_index: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(target_packages_index_path(&target_dir)).unwrap(),
    )
    .unwrap();
    assert_eq!(packages_index, serde_json::json!(["wasm-gc", "js"]));
    let selector: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(target_packages_selector_path(&target_dir)).unwrap(),
    )
    .unwrap();
    assert_eq!(selector["backend"], "js");
}
