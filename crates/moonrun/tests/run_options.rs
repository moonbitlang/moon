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

#![cfg(unix)]

use moonrun::{Engine, Module, RunOptions, RunOutcome, WorkingDirectory};
use std::{
    fs::{self, File},
    path::Path,
};

fn options(dir: &Path) -> RunOptions {
    RunOptions::default()
        .with_working_directory(WorkingDirectory::Fixed(dir.to_owned()))
        .with_stdio(
            File::open(dir.join("stdin")).unwrap(),
            File::create(dir.join("stdout")).unwrap(),
            File::create(dir.join("stderr")).unwrap(),
        )
}

fn execute(engine: &Engine, module: &Module, dir: &Path) -> anyhow::Result<RunOutcome> {
    engine.run(module, options(dir))
}

// The legacy V8 filesystem callback panics on host errors instead of returning them.
#[cfg(all(not(feature = "v8"), feature = "wasmtime"))]
#[test]
fn empty_legacy_remove_dir_does_not_delete_the_execution_directory() {
    let engine = Engine::default();
    let module = engine
        .compile(
            "empty-remove-dir.wasm",
            wat::parse_str(
                r#"(module
                (import "_" "" (global $empty (ref extern)))
                (import "__moonbit_fs_unstable" "remove_dir" (func $remove (param externref)))
                (func (export "_start") global.get $empty call $remove))"#,
            )
            .unwrap(),
        )
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("stdin"), []).unwrap();
    fs::write(dir.path().join("keep.txt"), "keep").unwrap();

    let result = execute(&engine, &module, dir.path());
    assert_eq!(
        fs::read_to_string(dir.path().join("keep.txt")).unwrap(),
        "keep"
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Failed to remove directory")
    );
}

#[test]
fn empty_status_remove_dir_does_not_delete_the_execution_directory() {
    let engine = Engine::default();
    let module = engine
        .compile(
            "empty-remove-dir-new.wasm",
            wat::parse_str(
                r#"(module
                (import "_" "" (global $empty (ref extern)))
                (import "__moonbit_fs_unstable" "remove_dir_new" (func $remove (param externref) (result i32)))
                (func (export "_start")
                    global.get $empty call $remove
                    i32.const -1 i32.ne if unreachable end))"#,
            )
            .unwrap(),
        )
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("stdin"), []).unwrap();
    fs::write(dir.path().join("keep.txt"), "keep").unwrap();

    let outcome = execute(&engine, &module, dir.path()).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("keep.txt")).unwrap(),
        "keep"
    );
    assert_eq!(outcome, RunOutcome::Completed);
}

#[test]
fn relative_fixed_root_is_canonicalized_before_guest_execution() {
    let cwd = std::env::current_dir().unwrap();
    let directory = tempfile::tempdir_in(&cwd).unwrap();
    let relative_root = directory.path().strip_prefix(&cwd).unwrap().to_owned();
    let expected = dunce::canonicalize(directory.path()).unwrap();
    let source = format!(
        r#"(module
        (import "_" {expected:?} (global $expected (ref extern)))
        (import "__moonbit_fs_unstable" "current_dir" (func $cwd (result externref)))
        (import "wasm:js-string" "equals" (func $equals (param externref externref) (result i32)))
        (func (export "_start")
            call $cwd global.get $expected call $equals
            i32.eqz if unreachable end))"#,
        expected = expected.to_str().unwrap(),
    );
    let engine = Engine::default();
    let module = engine
        .compile("relative-root.wasm", wat::parse_str(source).unwrap())
        .unwrap();

    assert_eq!(
        engine
            .run(
                &module,
                RunOptions::default()
                    .with_working_directory(WorkingDirectory::Fixed(relative_root)),
            )
            .unwrap(),
        RunOutcome::Completed
    );
    assert_eq!(std::env::current_dir().unwrap(), cwd);
}

#[test]
fn fixed_root_must_be_an_existing_directory() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("file");
    fs::write(&file, []).unwrap();
    let engine = Engine::default();
    let module = engine
        .compile(
            "invalid-root.wasm",
            wat::parse_str(r#"(module (func (export "_start")))"#).unwrap(),
        )
        .unwrap();

    for root in [directory.path().join("missing"), file] {
        assert!(
            engine
                .run(
                    &module,
                    RunOptions::default().with_working_directory(WorkingDirectory::Fixed(root)),
                )
                .is_err()
        );
    }
}

#[test]
fn concurrent_runs_isolate_stdio_and_relative_files() {
    let engine = Engine::default();
    let bytes = wat::parse_str(r#"(module
        (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
        (import "wasm:js-string" "fromCodePoint" (func $string (param i32) (result (ref extern))))
        (import "wasm:js-string" "charCodeAt" (func $char (param externref i32) (result i32)))
        (import "__moonbit_fs_unstable" "write_string_to_file" (func $save (param externref externref)))
        (import "__moonbit_fs_unstable" "read_file_to_string" (func $load (param externref) (result externref)))
        (memory (export "memory") 1)
        (func (export "_start")
            ;; A one-byte iovec, followed by a result slot.
            i32.const 0 i32.const 32 i32.store
            i32.const 4 i32.const 1 i32.store
            i32.const 0 i32.const 0 i32.const 1 i32.const 8 call $read
            if unreachable end
            ;; Write and read the same relative filename in each run.
            i32.const 120 call $string
            i32.const 32 i32.load8_u call $string call $save
            i32.const 32 i32.const 120 call $string call $load i32.const 0 call $char i32.store8
            i32.const 1 i32.const 0 i32.const 1 i32.const 8 call $write
            if unreachable end
            i32.const 2 i32.const 0 i32.const 1 i32.const 8 call $write
            if unreachable end))"#).unwrap();
    let module = engine.compile("context.wasm", &bytes).unwrap();
    let cwd = std::env::current_dir().unwrap();
    let directories: Vec<_> = (0..2).map(|_| tempfile::tempdir().unwrap()).collect();
    let runs: Vec<_> = directories
        .iter()
        .zip([b'A', b'B'])
        .map(|(dir, byte)| {
            fs::write(dir.path().join("stdin"), [byte]).unwrap();
            let engine = engine.clone();
            let module = module.clone();
            let dir = dir.path().to_owned();
            std::thread::spawn(move || execute(&engine, &module, &dir).unwrap())
        })
        .collect();
    for run in runs {
        assert_eq!(run.join().unwrap(), RunOutcome::Completed);
    }
    for (dir, byte) in directories.iter().zip([b'A', b'B']) {
        for filename in ["stdout", "stderr", "x"] {
            assert_eq!(fs::read(dir.path().join(filename)).unwrap(), [byte]);
        }
    }
    assert_eq!(std::env::current_dir().unwrap(), cwd);
}

#[test]
fn relative_sqlite_database_uses_the_execution_directory() {
    let bytes = wat::parse_str(
        r#"(module
        (import "moonbitlang/sqlite" "sqlite3_null_handle" (func $null (result i64)))
        (import "moonbitlang/sqlite" "sqlite3_open_v2" (func $open (param i32 i32 i32 i32 i64) (result i32)))
        (import "moonbitlang/sqlite" "sqlite3_close" (func $close (param i64) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 16) "db.sqlite")
        (func (export "_start")
            i32.const 16 i32.const 9 i32.const 64 i32.const 6 call $null call $open
            if unreachable end
            i32.const 64 i64.load call $close if unreachable end))"#,
    )
    .unwrap();
    let engine = Engine::default();
    let module = engine.compile("sqlite.wasm", &bytes).unwrap();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("stdin"), []).unwrap();
    assert_eq!(
        execute(&engine, &module, dir.path()).unwrap(),
        RunOutcome::Completed
    );
    assert!(dir.path().join("db.sqlite").is_file());
}

#[test]
fn options_preserve_exit_and_guest_failure_handling() {
    let engine = Engine::default();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("stdin"), []).unwrap();
    for (source, expected) in [
        (
            r#"(module
            (import "__moonbit_sys_unstable" "exit" (func $exit (param i32)))
            (func (export "_start") i32.const 17 call $exit))"#,
            RunOutcome::Exited(17),
        ),
        (
            r#"(module (func (export "_start") unreachable))"#,
            RunOutcome::Exited(1),
        ),
        (
            r#"(module (func $start unreachable) (start $start))"#,
            RunOutcome::Exited(1),
        ),
        (
            r#"(module (func (export "_start")))"#,
            RunOutcome::Completed,
        ),
    ] {
        assert_eq!(
            execute(
                &engine,
                &engine
                    .compile("outcome.wasm", wat::parse_str(source).unwrap())
                    .unwrap(),
                dir.path()
            )
            .unwrap(),
            expected
        );
    }
}
