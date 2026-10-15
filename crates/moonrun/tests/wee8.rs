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

#![cfg(feature = "wee8")]

use moonrun::{Engine, EngineConfig, RunOptions, RunOutcome};

#[test]
fn cli_runs_compiler_produced_moonbit() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = moon_test_util::test_dir::TestDir::from_case_root(
        manifest.join("tests/test_cases"),
        "test_wee8.in",
        false,
    );
    let moon = escargot::CargoBuild::new()
        .manifest_path(manifest.join("../moon/Cargo.toml"))
        .bin("moon")
        .current_release()
        .run()
        .expect("build moon");
    for (profile, flag) in [("debug", "--debug"), ("release", "--release")] {
        snapbox::cmd::Command::new(moon.path())
            .current_dir(&directory)
            .args(["build", "--target", "wasm", flag])
            .assert()
            .success();
        snapbox::cmd::Command::new(env!("CARGO_BIN_EXE_moonrun"))
            .arg(directory.join(format!("_build/wasm/{profile}/build/main/main.wasm")))
            .assert()
            .success()
            .stdout_eq("Hello, 世界 🦀\n42\n")
            .stderr_eq("");
    }
}

#[test]
fn cli_writes_from_grown_memory_and_exits() {
    let wasm = wat::parse_str(r#"(module
        (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
        (memory (export "memory") 1 2)
        (data (i32.const 16) "hello from wee8\0a")
        (func (export "_start")
            i32.const 0 i32.const 16 i32.store
            i32.const 4 i32.const 16 i32.store
            i32.const 1 i32.const 0 i32.const 1 i32.const 8 call $write
            if unreachable end
            i32.const 8 i32.load i32.const 16 i32.ne if unreachable end
            i32.const 1 memory.grow i32.const 1 i32.ne if unreachable end
            i32.const 65536 i32.const 0x0a77656e i32.store
            i32.const 0 i32.const 65536 i32.store
            i32.const 4 i32.const 4 i32.store
            i32.const 1 i32.const 0 i32.const 1 i32.const 8 call $write
            if unreachable end
            i32.const 7 call $exit
            unreachable))"#).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("hello.wasm");
    std::fs::write(&file, wasm).unwrap();
    snapbox::cmd::Command::new(env!("CARGO_BIN_EXE_moonrun"))
        .arg(file)
        .assert()
        .code(7)
        .stdout_eq("hello from wee8\nnew\n")
        .stderr_eq("");
}

#[test]
fn modules_reuse_compilation_with_fresh_memory_and_arguments() {
    let engine = Engine::default();
    let module = engine.compile("args.wasm", wat::parse_str(r#"(module
        (import "wasi_snapshot_preview1" "args_sizes_get" (func $sizes (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "args_get" (func $args (param i32 i32) (result i32)))
        (memory (export "memory") 1)
        (global $ran (mut i32) (i32.const 0))
        (func (export "_start")
            global.get $ran if unreachable end
            i32.const 1 global.set $ran
            i32.const 0 i32.const 4 call $sizes if unreachable end
            i32.const 0 i32.load i32.const 2 i32.ne if unreachable end
            i32.const 8 i32.const 32 call $args if unreachable end
            i32.const 12 i32.load i32.load8_u i32.const 120 i32.ne if unreachable end))"#).unwrap()).unwrap();
    for _ in 0..3 {
        assert_eq!(
            engine
                .run(&module, RunOptions::default().with_args(["x"]))
                .unwrap(),
            RunOutcome::Completed
        );
    }
    // The public Engine/Module contract allows a caller to choose execution threads.
    std::thread::spawn(move || {
        assert_eq!(
            engine
                .run(&module, RunOptions::default().with_args(["x"]))
                .unwrap(),
            RunOutcome::Completed
        );
    })
    .join()
    .unwrap();
}

#[test]
fn modules_without_start_export_complete() {
    let engine = Engine::default();
    for source in [
        "(module)",
        r#"(module (func (export "answer") (result i32) i32.const 42))"#,
        "(module (func $initialize) (start $initialize))",
    ] {
        let module = engine
            .compile("library.wasm", wat::parse_str(source).unwrap())
            .unwrap();
        assert_eq!(
            engine.run(&module, RunOptions::default()).unwrap(),
            RunOutcome::Completed
        );
    }
}

#[test]
fn guest_exit_and_guest_traps_preserve_run_outcomes() {
    let engine = Engine::default();
    for (source, expected) in [
        (
            r#"(module (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
            (func (export "_start") i32.const 0 call $exit unreachable))"#,
            RunOutcome::Exited(0),
        ),
        (
            r#"(module (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
            (func $start i32.const 9 call $exit unreachable) (start $start))"#,
            RunOutcome::Exited(9),
        ),
        (
            r#"(module (func $start unreachable) (start $start))"#,
            RunOutcome::Exited(1),
        ),
        (
            r#"(module (func (export "_start") unreachable))"#,
            RunOutcome::Exited(1),
        ),
    ] {
        let module = engine
            .compile("outcome.wasm", wat::parse_str(source).unwrap())
            .unwrap();
        assert_eq!(
            engine.run(&module, RunOptions::default()).unwrap(),
            expected
        );
    }
}

#[test]
fn unsupported_imports_and_start_memory_are_host_errors() {
    let engine = Engine::default();
    for (source, expected) in [
        (
            r#"(module (import "missing" "function" (func)))"#,
            "unsupported wee8 import missing::function",
        ),
        (
            r#"(module (import "wasi_snapshot_preview1" "proc_exit" (func (param i64))))"#,
            "incorrect signature",
        ),
        (
            r#"(module (import "wasi_snapshot_preview1" "fd_readdir"
                (func (param i32 i32 i32 i32 i32) (result i32))))"#,
            "incorrect signature",
        ),
        (
            r#"(module (import "wasi_snapshot_preview1" "path_open"
                (func (param i32 i32 i32 i32 i32 i64 i64 i32 i32))))"#,
            "incorrect signature",
        ),
        (
            r#"(module (import "host" "memory" (memory 1)))"#,
            "unsupported non-function import host::memory",
        ),
        (
            r#"(module
            (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func $start i32.const 1 i32.const 0 i32.const 0 i32.const 0 call $write drop)
            (start $start))"#,
            "unavailable during module start",
        ),
    ] {
        let module = engine
            .compile("imports.wasm", wat::parse_str(source).unwrap())
            .unwrap();
        let error = engine.run(&module, RunOptions::default()).unwrap_err();
        assert!(format!("{error:#}").contains(expected), "{error:#}");
    }
}

#[test]
fn wasi_reports_bad_guest_addresses_without_panicking() {
    let engine = Engine::default();
    let module = engine.compile("bounds.wasm", wat::parse_str(r#"(module
        (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "_start")
            i32.const 1 i32.const -1 i32.const 1 i32.const 0 call $write
            i32.eqz if unreachable end))"#).unwrap()).unwrap();
    assert_eq!(
        engine.run(&module, RunOptions::default()).unwrap(),
        RunOutcome::Completed
    );
}

#[test]
fn proposals_and_unsupported_options_fail_before_execution() {
    let engine = Engine::default();
    for source in [
        "(module (global externref (ref.null extern)))",
        "(module (type (struct (field i32))))",
        "(module (memory 1 1 shared))",
        "(module (tag (param i32)))",
    ] {
        let error = engine
            .compile("unsupported.wasm", wat::parse_str(source).unwrap())
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("supported numeric Wasm subset"),
            "{error:#}"
        );
    }
    let bytes = wat::parse_str(r#"(module (func (export "_start")))"#).unwrap();
    let module = engine.compile("test.wasm", &bytes).unwrap();
    assert!(
        engine
            .run(
                &module,
                RunOptions::default().with_test_args(r#"{"package":"test","file_and_index":[]}"#)
            )
            .unwrap_err()
            .to_string()
            .contains("test driver")
    );
    assert!(
        Engine::new(EngineConfig::default().with_stack_size(512))
            .compile("stack.wasm", bytes)
            .is_err()
    );
}

#[test]
fn cli_start_exit_is_silent() {
    let wasm = wat::parse_str(
        r#"(module
        (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
        (func $start i32.const 0 call $exit unreachable)
        (start $start))"#,
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("start-exit.wasm");
    std::fs::write(&file, wasm).unwrap();
    snapbox::cmd::Command::new(env!("CARGO_BIN_EXE_moonrun"))
        .arg(file)
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");
}
