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

#[cfg(unix)]
#[test]
fn test_moon_run_single_file_dry_run() {
    let dir = TestDir::new("run_single_mbt_file.in");

    let mut output = get_stdout_with_envs(
        &dir,
        ["run", "a/b/single.mbt", "--target", "native", "--dry-run"],
        [("MOONBIT_NEW_NATIVE", "0")],
    )
    // Normalize clang-only warnings to keep snapshots portable across macOS/Linux.
    .replace(" -Wno-unused-value", "");
    crate::util::normalize_host_archiver(&mut output);
    check(
        collapse_core_import_args(&output, TargetBackend::Native),
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/native/debug/build/single/single.core -pkg moon/test/single -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/native/release/bundle' -i '$MOON_HOME/lib/core/<imports>' -pkg-sources moon/test/single:. -target native -g -O0 -workspace-path . -all-pkgs ./_build/native/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/native/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/native/release/bundle/core.core' ./_build/native/debug/build/single/single.core -main moon/test/single -o ./_build/native/debug/build/single/single.c -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target native -g -O0
            cc -o ./_build/native/debug/build/runtime-utf.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 -DMOONBIT_ALLOW_STACKTRACE '-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC' '$MOON_HOME/lib/runtime/utf.c'
            cc -o ./_build/native/debug/build/runtime-sync_io.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 -DMOONBIT_ALLOW_STACKTRACE '-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC' '$MOON_HOME/lib/runtime/sync_io.c'
            cc -o ./_build/native/debug/build/runtime-runtime.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 -DMOONBIT_ALLOW_STACKTRACE '-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC' '$MOON_HOME/lib/runtime/runtime.c'
            cc -o ./_build/native/debug/build/runtime-env.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 -DMOONBIT_ALLOW_STACKTRACE '-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC' '$MOON_HOME/lib/runtime/env.c'
            cc -o ./_build/native/debug/build/runtime-backtrace.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 -DMOONBIT_ALLOW_STACKTRACE '-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC' '$MOON_HOME/lib/runtime/backtrace.c'
            [ARCHIVER] [CREATE_ARGS] ./_build/native/debug/build/libruntime[FINGER_PRINT].a ./_build/native/debug/build/runtime-backtrace.o ./_build/native/debug/build/runtime-env.o ./_build/native/debug/build/runtime-runtime.o ./_build/native/debug/build/runtime-sync_io.o ./_build/native/debug/build/runtime-utf.o
            cc -o ./_build/native/debug/build/single/single.exe '-I$MOON_HOME/include' -g -fwrapv -fno-strict-aliasing -Og '$MOON_HOME/lib/libmoonbitrun.o' ./_build/native/debug/build/single/single.c ./_build/native/debug/build/libruntime[FINGER_PRINT].a -lm '$MOON_HOME/lib/libbacktrace.a'
            ./_build/native/debug/build/single/single.exe
        "#]],
    );

    let mut output = get_stdout(
        &dir,
        [
            "run",
            "a/b/single.mbt",
            "--target",
            "native",
            "--dry-run",
            "--release",
        ],
    )
    // Normalize clang-only warnings to keep snapshots portable across macOS/Linux.
    .replace(" -Wno-unused-value", "");
    crate::util::normalize_host_archiver(&mut output);
    check(
        collapse_core_import_args(&output, TargetBackend::Native),
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/native/release/build/single/single.core -pkg moon/test/single -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/native/release/bundle' -i '$MOON_HOME/lib/core/<imports>' -pkg-sources moon/test/single:. -target native -workspace-path . -all-pkgs ./_build/native/release/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/native/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/native/release/bundle/core.core' ./_build/native/release/build/single/single.core -main moon/test/single -o ./_build/native/release/build/single/single.c -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target native
            cc -o ./_build/native/release/build/runtime-utf.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 -DMOONBIT_USE_SIMDUTF '-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC' '$MOON_HOME/lib/runtime/utf.c'
            cc -o ./_build/native/release/build/runtime-sync_io.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 -DMOONBIT_USE_SIMDUTF '-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC' '$MOON_HOME/lib/runtime/sync_io.c'
            cc -o ./_build/native/release/build/runtime-runtime.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 -DMOONBIT_USE_SIMDUTF '-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC' '$MOON_HOME/lib/runtime/runtime.c'
            cc -o ./_build/native/release/build/runtime-env.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 -DMOONBIT_USE_SIMDUTF '-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC' '$MOON_HOME/lib/runtime/env.c'
            cc -o ./_build/native/release/build/runtime-backtrace.o '-I$MOON_HOME/include' -g -c -fwrapv -fno-strict-aliasing -O2 -DMOONBIT_USE_SIMDUTF '-DMOONBIT_ALLOCATOR=MOONBIT_ALLOCATOR_MIMALLOC' '$MOON_HOME/lib/runtime/backtrace.c'
            [ARCHIVER] [CREATE_ARGS] ./_build/native/release/build/libruntime[FINGER_PRINT].a ./_build/native/release/build/runtime-backtrace.o ./_build/native/release/build/runtime-env.o ./_build/native/release/build/runtime-runtime.o ./_build/native/release/build/runtime-sync_io.o ./_build/native/release/build/runtime-utf.o '$MOON_HOME/lib/moonbit_simdutf.o' '$MOON_HOME/lib/simdutf.o'
            cc -o ./_build/native/release/build/single/single.exe '-I$MOON_HOME/include' -fwrapv -fno-strict-aliasing -O2 '$MOON_HOME/lib/libmoonbitrun.o' ./_build/native/release/build/single/single.c ./_build/native/release/build/libruntime[FINGER_PRINT].a -lm '$MOON_HOME/lib/libbacktrace.a'
            ./_build/native/release/build/single/single.exe
        "#]],
    );

    let output = get_stdout(
        &dir,
        [
            "run",
            "a/b/single.mbt",
            "--target",
            "js",
            "--build-only",
            "--dry-run",
        ],
    );
    check(
        collapse_core_import_args(&output, TargetBackend::Js),
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/js/debug/build/single/single.core -pkg moon/test/single -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/js/release/bundle' -i '$MOON_HOME/lib/core/<imports>' -pkg-sources moon/test/single:. -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/js/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/js/release/bundle/core.core' ./_build/js/debug/build/single/single.core -main moon/test/single -o ./_build/js/debug/build/single/single.js -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target js -g -O0 -source-map
            node --enable-source-maps ./_build/js/debug/build/single/single.js
        "#]],
    );

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "wasm-gc", "--dry-run"],
    );
    check(
        collapse_core_import_args(&output, TargetBackend::WasmGC),
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/wasm-gc/debug/build/single/single.core -pkg moon/test/single -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle' -i '$MOON_HOME/lib/core/<imports>' -pkg-sources moon/test/single:. -target wasm-gc -g -O0 -source-map -workspace-path . -all-pkgs ./_build/wasm-gc/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/wasm-gc/release/bundle/core.core' ./_build/wasm-gc/debug/build/single/single.core -main moon/test/single -o ./_build/wasm-gc/debug/build/single/single.wasm -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target wasm-gc -g -O0 -source-map
            '$MOONRUN_OVERRIDE' ./_build/wasm-gc/debug/build/single/single.wasm --
        "#]],
    );

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "js", "--dry-run"],
    );
    check(
        collapse_core_import_args(&output, TargetBackend::Js),
        expect![[r#"
            moonc build-package ./single.mbt -o ./_build/js/debug/build/single/single.core -pkg moon/test/single -pkg-type executable -std-path '$MOON_HOME/lib/core/_build/js/release/bundle' -i '$MOON_HOME/lib/core/<imports>' -pkg-sources moon/test/single:. -target js -g -O0 -source-map -workspace-path . -all-pkgs ./_build/js/debug/build/all_pkgs.json
            moonc link-core '$MOON_HOME/lib/core/_build/js/release/bundle/abort/abort.core' '$MOON_HOME/lib/core/_build/js/release/bundle/core.core' ./_build/js/debug/build/single/single.core -main moon/test/single -o ./_build/js/debug/build/single/single.js -pkg-config-path ./moon.pkg.json -pkg-sources moon/test/single:. -pkg-sources 'moonbitlang/core:$MOON_HOME/lib/core' -target js -g -O0 -source-map
            node --enable-source-maps ./_build/js/debug/build/single/single.js
        "#]],
    );

    let output = get_stdout(
        &dir,
        ["run", "a/b/single.mbt", "--target", "js", "--build-only"],
    );
    check(
        &output,
        expect![[r#"
            {"artifacts_path":["$ROOT/a/b/_build/js/debug/build/single/single.js"]}
        "#]],
    );
    assert!(
        dir.join("a/b/_build/js/debug/build/single/single.js")
            .exists()
    );
}

#[test]
fn test_moon_run_wasm_policy_is_only_forwarded_to_wasm_backends() {
    let dir = TestDir::new("run_single_mbt_file.in");

    let wasm_output = get_stdout(
        &dir,
        [
            "run",
            "a/b/single.mbt",
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
            "a/b/single.mbt",
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
            "a/b/single.mbt",
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
fn test_moon_run_single_mbt_file() {
    let dir = TestDir::new("run_single_mbt_file.in");

    let output = get_stdout(&dir, ["run", "a/b/single.mbt"]);
    check(
        &output,
        expect![[r#"
        I am OK
    "#]],
    );

    let output = get_stdout(&dir.join("a").join("b").join("c"), ["run", "../single.mbt"]);
    check(
        &output,
        expect![[r#"
            I am OK
            "#]],
    );

    let output = get_stdout(
        &dir.join("a").join("b"),
        ["run", "single.mbt", "--target", "js"],
    );
    check(
        &output,
        expect![[r#"
        I am OK
        "#]],
    );

    let output = get_stdout(
        &dir.join("a").join("b"),
        ["run", "single.mbt", "--target", "native"],
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
#[ignore = "There's conflict between base64 in core and base64 in x"]
fn moon_check_and_test_single_file() {
    let dir = TestDir::new("moon_test_single_file.in");
    let single_mbt = dir.join("single.mbt").display().to_string();
    let single_mbt_md = dir.join("111.mbt.md").display().to_string();

    // .mbt
    {
        // rel path
        check(
            get_stdout(&dir, ["test", "single.mbt", "-i", "0"]),
            expect![[r#"
                ------------------ 11111111 ------------------
                Total tests: 1, passed: 1, failed: 0.
            "#]],
        );
        check(
            get_err_stdout(&dir, ["test", "single.mbt", "-i", "1"]),
            expect![[r#"
                [moon/test] test single/single.mbt:12 (#1) failed
                expect test failed at $ROOT/single.mbt:13:3-13:18
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
            get_stderr(&dir, ["check", "single.mbt"]),
            expect![[r#"
                Warning: [0002]
                   ╭─[ $ROOT/single.mbt:8:7 ]
                   │
                 8 │   let single_mbt = 1
                   │       ─────┬────  
                   │            ╰────── Warning (unused_value): Unused variable 'single_mbt'
                ───╯
                Finished. moon: ran 2 tasks, now up to date (1 warnings, 0 errors)
            "#]],
        );
        // abs path
        check(
            get_stderr(&dir, ["check", &single_mbt]),
            expect![[r#"
                Warning: [0002]
                   ╭─[ $ROOT/single.mbt:8:7 ]
                   │
                 8 │   let single_mbt = 1
                   │       ─────┬────  
                   │            ╰────── Warning (unused_value): Unused variable 'single_mbt'
                ───╯
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
                [moon/test] test single/111.mbt.md:27 (#1) failed
                expect test failed at $ROOT/111.mbt.md:34:5-34:20
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
            snapbox::str!(
                r#"
Warning: [0002]
    ╭─[ $ROOT/111.mbt.md:28:9 ]
    │
 28 │     let single_mbt_md = 1
    │         ──────┬──────  
    │               ╰──────── Warning (unused_value): Unused variable 'single_mbt_md'
────╯
..."#
            )
        );

        // abs path
        check(
            get_stderr(&dir, ["check", &single_mbt_md]),
            expect![[r#"
                Warning: [0002]
                    ╭─[ $ROOT/111.mbt.md:28:9 ]
                    │
                 28 │     let single_mbt_md = 1
                    │         ──────┬──────  
                    │               ╰──────── Warning (unused_value): Unused variable 'single_mbt_md'
                ────╯
                Finished. moon: ran 1 task, now up to date (1 warnings, 0 errors)
            "#]],
        );
    }

    // check single file (with or without main func)
    {
        let with_main = dir.join("with_main.mbt").display().to_string();
        snapbox::assert_data_eq!(
            get_stderr(&dir, ["check", &with_main]),
            snapbox::str![[r#"
Warning: [0002]
   ╭─[ $ROOT/with_main.mbt:2:7 ]
   │
 2 │   let with_main = 1
   │       ────┬────  
   │           ╰────── Warning (unused_value): Unused variable 'with_main'
───╯
...
"#]],
        );
        let without_main = dir.join("without_main.mbt").display().to_string();
        snapbox::assert_data_eq!(
            get_stderr(&dir, ["check", &without_main]),
            snapbox::str![[r#"
Warning: [0001]
   ╭─[ $ROOT/without_main.mbt:1:4 ]
   │
 1 │ fn func() -> Unit {
   │    ──┬─  
   │      ╰─── Warning (unused_value): Unused function 'func'
───╯
Warning: [0002]
   ╭─[ $ROOT/without_main.mbt:2:7 ]
   │
 2 │   let without_main = 1
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
        dir.join("hello.mbt"),
        r#"fn main {
  println("hello")
}
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("test.mbt"),
        r#"test "x" {
  assert_true(true)
}
"#,
    )
    .unwrap();

    let check_result = moon_cmd(&dir)
        .env(MOON_NO_WORKSPACE, "1")
        .args(["check", "hello.mbt", "--target", "wasm-gc"])
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
    let packages_selector = standalone_packages_selector_path(dir, "hello.mbt");
    let scoped_pkg_json =
        standalone_scoped_packages_json_path(dir, "wasm-gc", "debug", "hello.mbt");
    let packages_selector: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(packages_selector).unwrap()).unwrap();
    assert_eq!(
        packages_selector,
        serde_json::json!({
            "backend": "wasm-gc",
            "opt_level": "debug"
        })
    );
    let packages_index: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(packages_index_path(dir)).unwrap()).unwrap();
    assert_eq!(packages_index, serde_json::json!(["wasm-gc"]));
    let scoped_pkg_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(scoped_pkg_json).unwrap()).unwrap();
    assert_eq!(scoped_pkg_json["backend"], serde_json::json!("wasm-gc"));
    assert_eq!(scoped_pkg_json["opt_level"], serde_json::json!("debug"));

    check(
        get_stdout_with_envs(&dir, ["test", "test.mbt"], [(MOON_NO_WORKSPACE, "1")]),
        expect![[r#"
            Total tests: 1, passed: 1, failed: 0.
        "#]],
    );

    check(
        get_stdout_with_envs(&dir, ["run", "hello.mbt"], [(MOON_NO_WORKSPACE, "1")]),
        expect![[r#"
            hello
        "#]],
    );

    check(
        get_stdout_with_envs(&dir, ["run", "hello.mbt"], [(MOON_WORK_ENV, "off")]),
        expect![[r#"
            hello
        "#]],
    );
}
