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

use super::{TestDir, moon_cmd};

#[test]
fn engine_backends_run_compiler_generated_tests() {
    let dir = TestDir::new("test_driver.in");
    for target in ["wasm", "wasm-gc"] {
        for profile in ["debug", "release"] {
            moon_cmd()
                .current_dir(&dir)
                .args(["test", "--target", target, "--build-only"])
                .arg(format!("--{profile}"))
                .assert()
                .success();

            snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
                .args(["--no-stack-trace", "--test-args"])
                .arg("driver_test.mbt:0-3")
                .arg(dir.join(&format!(
                    "_build/{target}/{profile}/test/test_driver.blackbox_test.wasm"
                )))
                .assert()
                .success()
                .stdout_eq(snapbox::str![[r#"
----- BEGIN MOON TEST RESULT -----
{"type":"start","file":"driver_test.mbt","index":0}
----- END MOON TEST RESULT -----
----- BEGIN MOON TEST RESULT -----
{"type":"result","file":"driver_test.mbt","index":0,"message":""}
----- END MOON TEST RESULT -----
----- BEGIN MOON TEST RESULT -----
{"type":"start","file":"driver_test.mbt","index":1}
----- END MOON TEST RESULT -----
----- BEGIN MOON TEST RESULT -----
{"type":"result","file":"driver_test.mbt","index":1,"message":"Error"}
----- END MOON TEST RESULT -----
----- BEGIN MOON TEST RESULT -----
{"type":"start","file":"driver_test.mbt","index":2}
----- END MOON TEST RESULT -----
----- BEGIN MOON TEST RESULT -----
{"type":"result","file":"driver_test.mbt","index":2,"message":""}
----- END MOON TEST RESULT -----

"#]])
                .stderr_eq(if target == "wasm" {
                    "expected test failure\n"
                } else {
                    ""
                });
        }
    }
}

#[test]
fn engine_backends_run_both_wasm_profiles_and_js_strings() {
    let dir = TestDir::new("test_engine_backend.in");

    for target in ["wasm", "wasm-gc"] {
        moon_cmd()
            .current_dir(&dir)
            .args(["build", "--target", target])
            .assert()
            .success();
    }
    moon_cmd()
        .current_dir(&dir)
        .args(["test", "js_string", "--target", "wasm-gc", "--build-only"])
        .assert()
        .success();

    let engine = moonrun::Engine::default();
    let plain_wasm = dir.join("_build/wasm/debug/build/plain/plain.wasm");
    let plain_module = engine.load_file(&plain_wasm).unwrap();
    std::fs::remove_file(plain_wasm).unwrap();
    assert_eq!(
        engine
            .run(&plain_module, moonrun::RunOptions::default())
            .unwrap(),
        moonrun::RunOutcome::Completed
    );
    assert_eq!(
        engine
            .run(&plain_module, moonrun::RunOptions::default())
            .unwrap(),
        moonrun::RunOutcome::Completed
    );

    let exit_wasm = dir.join("_build/wasm-gc/debug/build/exit/exit.wasm");
    assert_eq!(
        engine
            .run_file(&exit_wasm, moonrun::RunOptions::default())
            .unwrap(),
        moonrun::RunOutcome::Exited(7)
    );

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(dir.join("_build/wasm-gc/debug/build/js_string/js_string.wasm"))
        .assert()
        .success()
        .stdout_eq("hello from imported constants\n11\nfalse\n")
        .stderr_eq("");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(dir.join("_build/wasm-gc/debug/build/time/time.wasm"))
        .assert()
        .success()
        .stdout_eq("true\ntrue\n")
        .stderr_eq("");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg("--test-args")
        .arg("main_wbtest.mbt:0-1")
        .arg(dir.join("_build/wasm-gc/debug/test/js_string/js_string.whitebox_test.wasm"))
        .assert()
        .success()
        .stdout_eq(concat!(
            "----- BEGIN MOON TEST RESULT -----\n",
            r#"{"type":"start","file":"main_wbtest.mbt","index":0}"#,
            "\n----- END MOON TEST RESULT -----\n",
            "----- BEGIN MOON TEST RESULT -----\n",
            r#"{"type":"result","file":"main_wbtest.mbt","index":0,"message":""}"#,
            "\n----- END MOON TEST RESULT -----\n",
        ))
        .stderr_eq("");
}

#[test]
fn engine_backends_run_compiler_generated_ffi_bytes() {
    let dir = TestDir::new("test_engine_backend.in");

    moon_cmd()
        .current_dir(&dir)
        .env("MOONC_INTERNAL_PARAMS", "use_ffi_bytes=1|")
        .args(["build", "ffi_bytes", "--target", "wasm-gc"])
        .assert()
        .success();

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(dir.join("_build/wasm-gc/debug/build/ffi_bytes/ffi_bytes.wasm"))
        .assert()
        .success()
        .stdout_eq("A\n")
        .stderr_eq("");
}

#[test]
fn engine_backends_share_the_sqlite_wasm_adapter() {
    let wasm = wat::parse_str(
        r#"(module
            (import "moonbitlang/sqlite" "sqlite3_null_handle"
                (func $null_handle (result i64)))
            (import "moonbitlang/sqlite" "sqlite3_open_v2"
                (func $open (param i32 i32 i32 i32 i64) (result i32)))
            (import "moonbitlang/sqlite" "sqlite3_close"
                (func $close (param i64) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 16) ":memory:")
            (func (export "_start") (local $null i64)
                call $null_handle
                local.set $null
                i32.const 16
                i32.const 8
                i32.const 8
                i32.const 6
                local.get $null
                call $open
                if unreachable end
                i32.const 8
                i64.load
                call $close
                if unreachable end))"#,
    )
    .unwrap();
    let engine = moonrun::Engine::default();
    let module = engine.compile("sqlite-adapter.wasm", wasm).unwrap();

    assert_eq!(
        engine.run(&module, moonrun::RunOptions::default()).unwrap(),
        moonrun::RunOutcome::Completed
    );
}

#[test]
fn engine_backends_share_the_js_byte_adapter() {
    let wasm = wat::parse_str(
        r#"(module
            (import "ffi-bytes" "memory" (memory 1))
            (import "ffi-bytes" "new"
                (func $new (param i32) (result (ref extern))))
            (import "ffi-bytes" "set"
                (func $set (param externref i32 i32)))
            (import "ffi-bytes" "get"
                (func $get (param externref i32) (result i32)))
            (import "ffi-bytes" "length"
                (func $length (param externref) (result i32)))
            (import "ffi-bytes" "asString"
                (func $as_string (param externref i32 i32) (result (ref extern))))
            (func (export "_start") (local $bytes externref)
                i32.const 4
                call $new
                local.set $bytes
                local.get $bytes i32.const 0 i32.const 300 call $set
                local.get $bytes i32.const 0 call $get
                i32.const 44
                i32.ne
                if unreachable end
                local.get $bytes i32.const 0 i32.const 65 call $set
                local.get $bytes i32.const 1 i32.const 0 call $set
                local.get $bytes i32.const 2 i32.const 66 call $set
                local.get $bytes i32.const 3 i32.const 0 call $set
                local.get $bytes
                call $length
                i32.const 4
                i32.ne
                if unreachable end
                local.get $bytes
                i32.const 0
                i32.const 4
                call $as_string
                drop))"#,
    )
    .unwrap();
    let wasm_file = tempfile::Builder::new()
        .prefix("ffi-bytes.")
        .suffix(".wasm")
        .tempfile()
        .unwrap();
    std::fs::write(wasm_file.path(), wasm).unwrap();
    let output = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(wasm_file.path())
        .assert()
        .success();

    output.stdout_eq("");
}

#[test]
fn engine_backends_share_the_memory_sanitizer_adapter() {
    let wasm = wat::parse_str(
        r#"(module
            (import "moonbit:ffi/memory-sanitizer" "register-object-alloc"
                (func $alloc (param i32 i32)))
            (import "moonbit:ffi/memory-sanitizer" "register-object-free"
                (func $free (param i32)))
            (import "moonbit:ffi/memory-sanitizer" "object-is-valid"
                (func $valid (param i32) (result i32)))
            (func (export "_start")
                i32.const 16
                i32.const 32
                call $alloc
                i32.const 32
                call $valid
                i32.eqz
                if unreachable end
                i32.const 32
                call $free
                i32.const 32
                call $valid
                if unreachable end))"#,
    )
    .unwrap();
    let engine = moonrun::Engine::default();
    let module = engine
        .compile("memory-sanitizer-adapter.wasm", wasm)
        .unwrap();

    assert_eq!(
        engine.run(&module, moonrun::RunOptions::default()).unwrap(),
        moonrun::RunOutcome::Completed
    );
}

#[test]
fn engine_backends_share_the_async_host_adapter() {
    let wasm = wat::parse_str(
        r#"(module
            (import "moonbitlang/async" "time/get_ms_since_epoch"
                (func $now (result i64)))
            (import "moonbitlang/async" "random/fill"
                (func $random_fill (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "_start")
                call $now
                i64.eqz
                if unreachable end
                i32.const 16
                i32.const 32
                call $random_fill
                if unreachable end))"#,
    )
    .unwrap();
    let engine = moonrun::Engine::default();
    let module = engine.compile("async-adapter.wasm", wasm).unwrap();

    assert_eq!(
        engine.run(&module, moonrun::RunOptions::default()).unwrap(),
        moonrun::RunOutcome::Completed
    );
}
