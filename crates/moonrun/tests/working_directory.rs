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
use std::{fs, path::Path};

fn options(dir: &Path) -> RunOptions {
    RunOptions::default().with_working_directory(WorkingDirectory::Fixed(dir.to_owned()))
}

fn execute(engine: &Engine, module: &Module, dir: &Path) -> anyhow::Result<RunOutcome> {
    engine.run(module, options(dir))
}

fn execute_empty_remove_dir(source: &str) -> anyhow::Result<RunOutcome> {
    let engine = Engine::default();
    let module = engine
        .compile("empty-remove-dir.wasm", wat::parse_str(source).unwrap())
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("keep.txt"), "keep").unwrap();

    let result = execute(&engine, &module, dir.path());
    assert_eq!(
        fs::read_to_string(dir.path().join("keep.txt")).unwrap(),
        "keep"
    );
    result
}

// The legacy V8 filesystem callback panics on host errors instead of returning them.
#[cfg(all(not(feature = "v8"), feature = "wasmtime"))]
#[test]
fn empty_legacy_remove_dir_does_not_delete_the_execution_directory() {
    let result = execute_empty_remove_dir(
        r#"(module
        (import "_" "" (global $empty (ref extern)))
        (import "__moonbit_fs_unstable" "remove_dir" (func $remove (param externref)))
        (func (export "_start") global.get $empty call $remove))"#,
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
    let outcome = execute_empty_remove_dir(
        r#"(module
        (import "_" "" (global $empty (ref extern)))
        (import "__moonbit_fs_unstable" "remove_dir_new" (func $remove (param externref) (result i32)))
        (func (export "_start")
            global.get $empty call $remove
            i32.const -1 i32.ne if unreachable end))"#,
    )
    .unwrap();
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
fn concurrent_runs_isolate_relative_files() {
    let engine = Engine::default();
    let bytes = wat::parse_str(r#"(module
        (import "_" "input.txt" (global $input (ref extern)))
        (import "_" "output.txt" (global $output (ref extern)))
        (import "__moonbit_fs_unstable" "write_string_to_file" (func $save (param externref externref)))
        (import "__moonbit_fs_unstable" "read_file_to_string" (func $load (param externref) (result externref)))
        (func (export "_start")
            global.get $output global.get $input call $load call $save))"#).unwrap();
    let module = engine.compile("relative-files.wasm", bytes).unwrap();
    let cwd = std::env::current_dir().unwrap();
    let directories: Vec<_> = (0..2).map(|_| tempfile::tempdir().unwrap()).collect();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let runs: Vec<_> = directories
        .iter()
        .zip([b'A', b'B'])
        .map(|(dir, byte)| {
            fs::write(dir.path().join("input.txt"), [byte]).unwrap();
            let engine = engine.clone();
            let module = module.clone();
            let options = options(dir.path());
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                engine.run(&module, options).unwrap()
            })
        })
        .collect();
    for run in runs {
        assert_eq!(run.join().unwrap(), RunOutcome::Completed);
    }
    for (dir, byte) in directories.iter().zip([b'A', b'B']) {
        assert_eq!(fs::read(dir.path().join("output.txt")).unwrap(), [byte]);
    }
    assert_eq!(std::env::current_dir().unwrap(), cwd);
}

#[test]
fn async_filesystem_jobs_use_the_fixed_directory() {
    let engine = Engine::default();
    let bytes = wat::parse_str(r#"(module
        (import "moonbitlang/async" "thread_pool/make_mkdir_job" (func $mkdir (param i32 i32 i32) (result i64)))
        (import "moonbitlang/async" "thread_pool/run_job" (func $run (param i64)))
        (import "moonbitlang/async" "thread_pool/job_get_err" (func $error (param i64) (result i32)))
        (import "moonbitlang/async" "thread_pool/free_job" (func $free (param i64)))
        (memory (export "memory") 1)
        (data (i32.const 16) "\64\00")
        (func (export "_start") (local $job i64)
            i32.const 16 i32.const 1 i32.const 448 call $mkdir local.set $job
            local.get $job call $run
            local.get $job call $error if unreachable end
            local.get $job call $free))"#).unwrap();
    let module = engine.compile("async-relative-path.wasm", bytes).unwrap();
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        execute(&engine, &module, dir.path()).unwrap(),
        RunOutcome::Completed
    );
    assert!(dir.path().join("d").is_dir());
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
    assert_eq!(
        execute(&engine, &module, dir.path()).unwrap(),
        RunOutcome::Completed
    );
    assert!(dir.path().join("db.sqlite").is_file());
}
