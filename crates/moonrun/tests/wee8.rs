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
fn guest_exit_traps_and_exceptions_preserve_run_outcomes() {
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
        (
            r#"(module
            (import "exception" "throw" (func $throw))
            (import "exception" "tag" (tag $tag))
            (func (export "_start") call $throw))"#,
            RunOutcome::Exited(1),
        ),
        (
            r#"(module
            (import "exception" "tag" (tag $tag))
            (func $start throw $tag) (start $start))"#,
            RunOutcome::Exited(1),
        ),
        (
            // The callback must throw the exact imported tag, even when the
            // function import precedes it in the module's declaration order.
            r#"(module
            (import "exception" "throw" (func $throw))
            (import "exception" "tag" (tag $tag))
            (func (export "_start")
                block $caught
                    try_table (catch $tag $caught) call $throw end
                    unreachable
                end))"#,
            RunOutcome::Completed,
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
            "unsupported import host::memory",
        ),
        (
            r#"(module
            (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func $start i32.const 1 i32.const 0 i32.const 0 i32.const 0 call $write drop)
            (start $start))"#,
            "unavailable during module start",
        ),
        (
            r#"(module (import "exception" "tag" (tag (param i32))))"#,
            "incorrect signature for exception::tag",
        ),
        (
            r#"(module (import "exception" "throw" (func)))"#,
            "missing `exception.tag` import",
        ),
        (
            r#"(module (import "exception" "tag" (tag)) (import "exception" "tag" (tag)))"#,
            "duplicate `exception.tag` import",
        ),
        (
            r#"(module (import "exception" "tag" (tag)) (import "exception" "throw" (func (param i32))))"#,
            "incorrect signature for exception::throw",
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
fn proposals_and_unsupported_options_fail_before_execution() {
    let engine = Engine::default();
    let error = engine
        .compile(
            "unsupported.wasm",
            wat::parse_str(r#"(module (memory (export "memory") 1 1 shared))"#).unwrap(),
        )
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("shared memory is unsupported in C API"),
        "{error:#}"
    );
    let bytes = wat::parse_str(r#"(module (func (export "_start")))"#).unwrap();
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

#[test]
fn gc_modules_reuse_compilation_with_fresh_guest_references() {
    let engine = Engine::default();
    let module = engine
        .compile(
            "gc.wasm",
            wat::parse_str(
                r#"(module
        (type $cell (struct (field (mut i32))))
        (type $items (array (mut (ref null $cell))))
        (global $saved (mut (ref null $items)) (ref.null $items))
        (func (export "_start")
            global.get $saved ref.is_null i32.eqz if unreachable end
            i32.const 42 struct.new $cell i32.const 2 array.new $items global.set $saved
            global.get $saved i32.const 1 array.get $items struct.get $cell 0
            i32.const 42 i32.ne if unreachable end))"#,
            )
            .unwrap(),
        )
        .unwrap();
    for _ in 0..3 {
        assert_eq!(
            engine.run(&module, RunOptions::default()).unwrap(),
            RunOutcome::Completed
        );
    }
}

#[test]
fn unsupported_gc_boundary_types_fail_during_compilation() {
    let engine = Engine::default();
    for source in [
        r#"(module (type $s (struct)) (func (export "f") (result (ref $s)) struct.new $s))"#,
        r#"(module (type $a (array i32)) (global (export "g") (ref null $a) (ref.null $a)))"#,
        r#"(module (func (export "f") (result externref) i32.const 1 ref.i31 extern.convert_any))"#,
    ] {
        let error = engine
            .compile("boundary.wasm", wat::parse_str(source).unwrap())
            .unwrap_err();
        assert!(format!("{error:#}").contains("C API"), "{error:#}");
    }
}

#[test]
fn core_strings_preserve_utf16_and_snapshot_builder_contents() {
    let engine = Engine::default();
    let module = engine.compile("core-strings.wasm", wat::parse_str(r#"(module
        (import "__moonbit_fs_unstable" "begin_create_string" (func $new (result externref)))
        (import "__moonbit_fs_unstable" "string_append_char" (func $append (param externref i32)))
        (import "__moonbit_fs_unstable" "finish_create_string" (func $finish (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "begin_read_string" (func $reader (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "string_read_char" (func $read (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "finish_read_string" (func $done (param externref)))
        (func (export "_start") (local $b externref) (local $s externref) (local $r externref)
            call $new local.set $b
            local.get $b i32.const 0xd800 call $append
            local.get $b i32.const 0xd83e call $append
            local.get $b i32.const 0xdd80 call $append
            local.get $b call $finish local.set $s
            local.get $b i32.const 65 call $append
            local.get $s call $reader local.set $r
            local.get $r call $read i32.const 0xd800 i32.ne if unreachable end
            local.get $r call $read i32.const 0xd83e i32.ne if unreachable end
            local.get $r call $read i32.const 0xdd80 i32.ne if unreachable end
            local.get $r call $read i32.const -1 i32.ne if unreachable end
            local.get $r call $read i32.const -1 i32.ne if unreachable end
            local.get $r call $done
            local.get $s call $reader call $read i32.const 0xd800 i32.ne if unreachable end))"#).unwrap()).unwrap();
    assert_eq!(
        engine.run(&module, RunOptions::default()).unwrap(),
        RunOutcome::Completed
    );
}

#[test]
fn core_environment_reads_runtime_values() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("core-env.wasm");
    let append_key = "MOONRUN_WEE8_CORE_VALUE"
        .encode_utf16()
        .map(|unit| format!("local.get $builder i32.const {unit} call $append"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&file, wat::parse_str(format!(r#"(module
        (import "__moonbit_fs_unstable" "begin_create_string" (func $new (result externref)))
        (import "__moonbit_fs_unstable" "string_append_char" (func $append (param externref i32)))
        (import "__moonbit_fs_unstable" "finish_create_string" (func $finish (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "get_env_var" (func $get (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "get_env_var_exists" (func $exists (param externref) (result i32)))
        (import "__moonbit_fs_unstable" "begin_read_string" (func $reader (param externref) (result externref)))
        (import "__moonbit_fs_unstable" "string_read_char" (func $read (param externref) (result i32)))
        (func (export "_start") (local $builder externref) (local $key externref) (local $r externref)
            call $new local.set $builder
            {append_key}
            local.get $builder call $finish local.set $key
            local.get $key call $exists i32.const 1 i32.ne if unreachable end
            local.get $key call $get call $reader local.set $r
            local.get $r call $read i32.const 955 i32.ne if unreachable end
            local.get $r call $read i32.const 0xd83e i32.ne if unreachable end
            local.get $r call $read i32.const 0xdd80 i32.ne if unreachable end
            local.get $r call $read i32.const -1 i32.ne if unreachable end
            local.get $builder i32.const 88 call $append
            local.get $builder call $finish local.set $key
            local.get $key call $exists if unreachable end
            local.get $key call $get call $reader call $read i32.const -1 i32.ne if unreachable end))"#)).unwrap()).unwrap();
    snapbox::cmd::Command::new(env!("CARGO_BIN_EXE_moonrun"))
        .env("MOONRUN_WEE8_CORE_VALUE", "λ🦀")
        .env_remove("MOONRUN_WEE8_CORE_VALUEX")
        .arg(&file)
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");

    // Provisioned Env must win over the process environment, just as it does
    // in the other adapters. Do not read std::env directly in this boundary.
    let policy = directory.path().join("env.toml");
    std::fs::write(&policy, "[env.set]\nMOONRUN_WEE8_CORE_VALUE = \"λ🦀\"\n").unwrap();
    snapbox::cmd::Command::new(env!("CARGO_BIN_EXE_moonrun"))
        .env("MOONRUN_WEE8_CORE_VALUE", "wrong ambient value")
        .args(["--policy", policy.to_str().unwrap()])
        .arg(file)
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");
}

#[test]
fn core_string_imports_reject_invalid_signatures_and_reference_types() {
    let engine = Engine::default();
    for source in [
        r#"(module (import "__moonbit_fs_unstable" "begin_create_string" (func (result i32))))"#,
        r#"(module
            (import "__moonbit_fs_unstable" "begin_read_string" (func $read (param externref) (result externref)))
            (func (export "_start") ref.null extern call $read drop))"#,
        r#"(module
            (import "__moonbit_fs_unstable" "begin_create_string" (func $new (result externref)))
            (import "__moonbit_fs_unstable" "begin_read_string" (func $read (param externref) (result externref)))
            (func (export "_start") call $new call $read drop))"#,
    ] {
        let module = engine
            .compile("invalid-core-string.wasm", wat::parse_str(source).unwrap())
            .unwrap();
        let error = engine.run(&module, RunOptions::default()).unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("incorrect signature") || message.contains("invalid host reference"),
            "{message}"
        );
    }
}

#[test]
fn legacy_host_imports_reject_bad_signatures_and_payloads() {
    let engine = Engine::default();
    for (source, expected) in [
        (
            r#"(module (import "__moonbit_time_unstable" "now" (func (result i32))))"#,
            "incorrect signature",
        ),
        (
            r#"(module
            (import "__moonbit_fs_unstable" "begin_read_byte_array" (func $read (param externref) (result externref)))
            (func (export "_start") ref.null extern call $read drop))"#,
            "invalid host reference",
        ),
        (
            r#"(module
            (import "__moonbit_fs_unstable" "begin_create_byte_array" (func $new (result externref)))
            (import "__moonbit_time_unstable" "instant_elapsed_as_secs_f64" (func $elapsed (param externref) (result f64)))
            (func (export "_start") call $new call $elapsed drop))"#,
            "invalid host reference",
        ),
        (
            r#"(module
            (import "__moonbit_fs_unstable" "args_get" (func $args (result externref)))
            (import "__moonbit_fs_unstable" "array_get" (func $get (param externref i32) (result externref)))
            (func (export "_start") call $args i32.const -1 call $get drop))"#,
            "array index out of bounds",
        ),
        (
            r#"(module
            (import "__moonbit_rand_unstable" "stdrng_seed_from_u64" (func $new (param i32) (result externref)))
            (import "__moonbit_rand_unstable" "stdrng_gen_range" (func $next (param externref i32) (result i32)))
            (func (export "_start") i32.const 1 call $new i32.const 0 call $next drop))"#,
            "random range upper bound must be positive",
        ),
    ] {
        let module = engine
            .compile("invalid-host-value.wasm", wat::parse_str(source).unwrap())
            .unwrap();
        let error = engine.run(&module, RunOptions::default()).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains(expected), "{message}");
    }
}
