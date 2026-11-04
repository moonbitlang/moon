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

//! Stdio grants authorize acquired-handle operations independently of path policy.

use moonrun::{Engine, RunOptions, RunOutcome};

const STDIN_SIZE: &str = r#"(module
    (import "moonbitlang/async" "stdio/get_stdio_handle" (func $stdio (param i32) (result i64)))
    (import "moonbitlang/async" "thread_pool/make_fstatx_job" (func $stat (param i64 i32 i32) (result i64)))
    (import "moonbitlang/async" "thread_pool/run_job" (func $run (param i64)))
    (import "moonbitlang/async" "thread_pool/job_get_err" (func $err (param i64) (result i32)))
    (import "moonbitlang/async" "thread_pool/get_stat_result" (func $result (param i64 i32 i32)))
    (import "moonbitlang/async" "thread_pool/free_job" (func $free (param i64)))
    (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
    (memory (export "memory") 1)
    (func (export "_start") (local $job i64)
        ;; Request FILE_SIZE, beyond the kind-only initialization query.
        i32.const 0 call $stdio i32.const 2 i32.const 16 call $stat local.set $job
        local.get $job call $run
        local.get $job call $err if local.get $job call $err call $exit end
        local.get $job i32.const 0 i32.const 16 call $result
        local.get $job call $free
        i32.const 4 i32.load i32.const 2 i32.ne if unreachable end
        i32.const 8 i64.load i64.const 3 i64.ne if unreachable end))"#;

#[test]
fn bound_stdin_metadata_ignores_path_policy() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let policy = root.path().join("policy.json");
    std::fs::write(&input, b"abc").unwrap();
    std::fs::write(&policy, "{}").unwrap();
    let engine = Engine::default();
    let module = engine
        .compile("stdin-size.wasm", wat::parse_str(STDIN_SIZE).unwrap())
        .unwrap();
    let options = RunOptions::default()
        .with_policy_file(policy)
        .unwrap()
        .with_stdio(
            std::fs::File::open(input).unwrap(),
            tempfile::tempfile().unwrap(),
            tempfile::tempfile().unwrap(),
        );
    assert_eq!(engine.run(&module, options).unwrap(), RunOutcome::Completed);
}

#[test]
fn inherited_stdin_metadata_ignores_path_policy() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let policy = root.path().join("policy.toml");
    let wasm = root.path().join("stdin-size.wasm");
    std::fs::write(&input, b"abc").unwrap();
    std::fs::write(&policy, "[fs]\nread = [\"*\"]\nwrite = [\"*\"]\n").unwrap();
    std::fs::write(&wasm, wat::parse_str(STDIN_SIZE).unwrap()).unwrap();
    // Supply a real inherited file, as shell redirection does. Snapbox's
    // Command replaces stdin with a pipe, so capture this process directly.
    let output = std::process::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg("--policy")
        .arg(policy)
        .arg(wasm)
        .stdin(std::fs::File::open(input).unwrap())
        .output()
        .unwrap();
    snapbox::cmd::OutputAssert::new(output)
        .success()
        .stdout_eq("")
        .stderr_eq("");
}

#[test]
fn bound_stdout_exclusive_lock_ignores_path_policy() {
    let root = tempfile::tempdir().unwrap();
    let policy = root.path().join("policy.json");
    std::fs::write(&policy, "{}").unwrap();
    let engine = Engine::default();
    let module = engine
        .compile(
            "stdout-lock.wasm",
            wat::parse_str(
                r#"(module
        (import "moonbitlang/async" "stdio/get_stdio_handle" (func $stdio (param i32) (result i64)))
        (import "moonbitlang/async" "fs/try_lock_file" (func $lock (param i64 i32) (result i32)))
        (import "moonbitlang/async" "fs/unlock_file" (func $unlock (param i64) (result i32)))
        (func (export "_start") (local $stdout i64)
            i32.const 1 call $stdio local.set $stdout
            local.get $stdout i32.const 1 call $lock if unreachable end
            local.get $stdout call $unlock if unreachable end))"#,
            )
            .unwrap(),
        )
        .unwrap();
    let options = RunOptions::default()
        .with_policy_file(policy)
        .unwrap()
        .with_stdio(
            tempfile::tempfile().unwrap(),
            tempfile::tempfile().unwrap(),
            tempfile::tempfile().unwrap(),
        );
    assert_eq!(engine.run(&module, options).unwrap(), RunOutcome::Completed);
}
