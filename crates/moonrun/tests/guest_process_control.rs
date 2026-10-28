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

#![cfg(feature = "v8")]

use moonrun::{
    Engine, ProcessError, ProcessState, RunOptions, RunOutcome, RunTermination, VirtualProcessTree,
};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn guest_process_control() {
    let Ok(case) = std::env::var("MOONRUN_GUEST_PROCESS_CASE") else {
        for case in [
            "identity",
            "kill_self",
            "pid_isolation",
            "child_wait",
            "wait_teardown",
            "sync_wait_teardown",
            "unread_wait",
        ] {
            snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "guest_process_control", "--nocapture"])
                .env("MOONRUN_GUEST_PROCESS_CASE", case)
                .timeout(Duration::from_secs(15))
                .assert()
                .success();
        }
        return;
    };
    eprintln!("guest process case: {case}");
    let engine = Engine::default();
    let tree = VirtualProcessTree::new(engine.clone()).unwrap();
    let module = engine
        .compile(
            "loop.wasm",
            wat::parse_str(r#"(module (func (export "_start") (loop br 0)))"#).unwrap(),
        )
        .unwrap();
    match case.as_str() {
        "kill_self" => {
            let module = engine
                .compile(
                    "self.wasm",
                    wat::parse_str(
                        r#"(module
                    (import "moonbitlang/async" "env_util/getpid" (func $pid (result i32)))
                    (import "moonbitlang/async" "process/kill" (func $kill (param i32)))
                    (func (export "_start") call $pid call $kill (loop br 0)))"#,
                    )
                    .unwrap(),
                )
                .unwrap();
            let pid = tree.spawn(None, &module, RunOptions::default()).unwrap();
            #[cfg(unix)]
            let expected = RunOutcome::KilledBySignal(libc::SIGKILL);
            #[cfg(windows)]
            let expected = RunOutcome::Exited(1);
            assert_eq!(tree.wait(pid).unwrap(), expected);
        }
        "pid_isolation" => {
            let other = VirtualProcessTree::new(engine.clone()).unwrap();
            let foreign = other.spawn(None, &module, RunOptions::default()).unwrap();
            let unrelated = tree.spawn(None, &module, RunOptions::default()).unwrap();
            // OS, foreign, unrelated PIDs and process-group selectors must
            // all be rejected. An accidental OS kill would kill this bounded
            // subprocess, rather than the parent test runner.
            for pid in [
                std::process::id() as i32,
                foreign.get(),
                unrelated.get(),
                0,
                -1,
            ] {
                let module = engine
                    .compile(
                        "foreign.wasm",
                        wat::parse_str(format!(
                            r#"(module
                        (import "moonbitlang/async" "process/kill" (func $kill (param i32)))
                        (func (export "_start") i32.const {pid} call $kill))"#
                        ))
                        .unwrap(),
                    )
                    .unwrap();
                let caller = tree.spawn(None, &module, RunOptions::default()).unwrap();
                let ProcessError::RunFailed(error) = tree.wait(caller).unwrap_err() else {
                    panic!("expected rejection by the guest process interface");
                };
                assert!(
                    format!("{error:#}").contains("PermissionDenied"),
                    "{error:#}"
                );
            }
            other.terminate(foreign, RunTermination::Exit(0)).unwrap();
            assert_eq!(other.wait(foreign).unwrap(), RunOutcome::Exited(0));
            tree.terminate(unrelated, RunTermination::Exit(0)).unwrap();
            assert_eq!(tree.wait(unrelated).unwrap(), RunOutcome::Exited(0));
        }
        "child_wait" | "wait_teardown" | "sync_wait_teardown" | "unread_wait" => {
            use std::io::Write;
            #[cfg(unix)]
            let status = -libc::SIGKILL;
            #[cfg(windows)]
            let status = 1;
            let body = match case.as_str() {
                    "child_wait" => {
                        r#"local.get $pid call $kill
                        local.get $job call $run
                        local.get $job call $err (if (then unreachable))
                        local.get $job call $ret i32.const EXPECTED_STATUS i32.ne (if (then unreachable))
                        i32.const 71 call $exit"#
                    }
                    "unread_wait" => {
                        r#"local.get $pid call $kill
                        local.get $job call $run
                        local.get $job call $err (if (then unreachable))
                        call $ready (loop br 0)"#
                    }
                    "sync_wait_teardown" => "call $ready local.get $job call $run unreachable",
                    _ => {
                        "call $poll call $init drop i32.const 1 local.get $job call $spawn drop call $ready (loop br 0)"
                    }
                }.replace("EXPECTED_STATUS", &status.to_string());
            let parent_module = engine.compile("parent.wasm", wat::parse_str(format!(r#"(module
                    (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
                    (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
                    (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
                    (import "moonbitlang/async" "process/kill" (func $kill (param i32)))
                    (import "moonbitlang/async" "fd_util/invalid_fd" (func $invalid (result i64)))
                    (import "moonbitlang/async" "thread_pool/make_wait_for_process_job" (func $make (param i64 i32) (result i64)))
                    (import "moonbitlang/async" "thread_pool/run_job" (func $run (param i64)))
                    (import "moonbitlang/async" "thread_pool/job_get_ret" (func $ret (param i64) (result i32)))
                    (import "moonbitlang/async" "thread_pool/job_get_err" (func $err (param i64) (result i32)))
                    (import "moonbitlang/async" "event_bus/create" (func $poll (result i64)))
                    (import "moonbitlang/async" "thread_pool/init_thread_pool" (func $init (param i64) (result i64)))
                    (import "moonbitlang/async" "thread_pool/spawn_worker" (func $spawn (param i32 i64) (result i64)))
                    (memory (export "memory") 1)
                    (func $ready
                        (drop (call $write (i32.const 1) (i32.const 0) (i32.const 1) (i32.const 8))))
                    (func (export "_start") (local $pid i32) (local $job i64)
                        i32.const 0 i32.const 16 i32.store
                        i32.const 4 i32.const 4 i32.store
                        (loop $input
                            (if (call $read (i32.const 0) (i32.const 0) (i32.const 1) (i32.const 8))
                                (then unreachable))
                            i32.const 8 i32.load i32.eqz br_if $input)
                        i32.const 16 i32.load local.set $pid
                        call $invalid local.get $pid call $make local.set $job
                        {body}))"#)).unwrap()).unwrap();
            let mut input = tempfile::NamedTempFile::new().unwrap();
            let marker = tempfile::NamedTempFile::new().unwrap();
            let diagnostics = tempfile::NamedTempFile::new().unwrap();
            let parent = tree
                .spawn(
                    None,
                    &parent_module,
                    RunOptions::default().with_stdio(
                        input.reopen().unwrap(),
                        marker.reopen().unwrap(),
                        diagnostics.reopen().unwrap(),
                    ),
                )
                .unwrap();
            let child = tree
                .spawn(Some(parent), &module, RunOptions::default())
                .unwrap();
            input.write_all(&child.get().to_le_bytes()).unwrap();
            #[cfg(unix)]
            let killed = RunOutcome::KilledBySignal(libc::SIGKILL);
            #[cfg(windows)]
            let killed = RunOutcome::Exited(1);
            if case == "child_wait" {
                assert_eq!(
                    tree.wait(parent).unwrap(),
                    RunOutcome::Exited(71),
                    "{}",
                    std::fs::read_to_string(diagnostics.path()).unwrap()
                );
                assert!(matches!(tree.wait(child), Err(ProcessError::NotFound(_))));
            } else {
                let deadline = Instant::now() + Duration::from_secs(5);
                while marker.as_file().metadata().unwrap().len() == 0 {
                    assert!(Instant::now() < deadline, "parent did not start waiting");
                    thread::yield_now();
                }
                tree.terminate(parent, RunTermination::Exit(7)).unwrap();
                assert_eq!(tree.wait(parent).unwrap(), RunOutcome::Exited(7));
                assert_eq!(tree.snapshot()[0].parent, None);
                if case == "unread_wait" {
                    assert_eq!(tree.wait(child).unwrap(), killed);
                } else {
                    assert_eq!(tree.snapshot()[0].state, ProcessState::Running);
                    tree.terminate(child, RunTermination::Exit(8)).unwrap();
                    assert_eq!(tree.wait(child).unwrap(), RunOutcome::Exited(8));
                }
            }
            assert!(tree.snapshot().is_empty());
        }
        "identity" => {
            let module = engine
                .compile(
                    "pid.wasm",
                    wat::parse_str(
                        r#"(module
                    (import "moonbitlang/async" "env_util/getpid" (func $pid (result i32)))
                    (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
                    (func (export "_start") call $pid call $exit))"#,
                    )
                    .unwrap(),
                )
                .unwrap();
            let pid = tree.spawn(None, &module, RunOptions::default()).unwrap();
            assert_eq!(tree.wait(pid).unwrap(), RunOutcome::Exited(pid.get()));
            assert_eq!(
                engine.run(&module, RunOptions::default()).unwrap(),
                RunOutcome::Exited(std::process::id() as i32)
            );
        }
        _ => unreachable!(),
    }
}
