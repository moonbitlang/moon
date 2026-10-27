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

use moonrun::{Engine, RunOptions, RunTermination, termination_channel};

#[cfg(not(feature = "v8"))]
#[test]
fn unsupported_backends_reject_external_termination_and_disconnect() {
    let engine = Engine::default();
    let module = engine
        .compile("empty.wasm", wat::parse_str("(module)").unwrap())
        .unwrap();
    let (terminator, receiver) = termination_channel();
    let error = engine
        .run(
            &module,
            RunOptions::default().with_termination_receiver(receiver),
        )
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("does not support external Run termination")
    );
    assert!(!terminator.terminate(RunTermination::Exit(1)));
}

#[cfg(feature = "v8")]
mod v8 {
    use super::*;
    use moonrun::{Module, RunOutcome, RunTerminator};
    use std::fs::File;
    use std::sync::{Arc, Barrier};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    #[test]
    fn requests_before_execution_skip_guest_and_errors_disconnect_the_receiver() {
        let engine = Engine::default();
        let module = engine
            .compile(
                "unreachable.wasm",
                wat::parse_str("(module (func (export \"_start\") unreachable))").unwrap(),
            )
            .unwrap();
        let (terminator, receiver) = termination_channel();
        assert!(terminator.terminate(RunTermination::Exit(7)));
        assert!(!terminator.terminate(RunTermination::Exit(8)));
        assert_eq!(
            engine
                .run(
                    &module,
                    RunOptions::default().with_termination_receiver(receiver)
                )
                .unwrap(),
            RunOutcome::Exited(7)
        );
        assert!(!terminator.terminate(RunTermination::Exit(9)));

        let (terminator, receiver) = termination_channel();
        assert!(
            engine
                .run(
                    &module,
                    RunOptions::default()
                        .with_termination_receiver(receiver)
                        .with_test_args("{")
                )
                .is_err()
        );
        assert!(!terminator.terminate(RunTermination::Exit(1)));

        let (terminator, receiver) = termination_channel();
        drop(RunOptions::default().with_termination_receiver(receiver));
        assert!(!terminator.terminate(RunTermination::Exit(1)));
    }

    #[test]
    fn concurrent_completion_and_termination_have_one_final_outcome() {
        let engine = Engine::default();
        let module = engine
            .compile(
                "return.wasm",
                wat::parse_str("(module (func (export \"_start\")))").unwrap(),
            )
            .unwrap();
        for _ in 0..20 {
            let (terminator, receiver) = termination_channel();
            let barrier = Arc::new(Barrier::new(2));
            let ready = Arc::clone(&barrier);
            let engine = engine.clone();
            let module = module.clone();
            let run = thread::spawn(move || {
                ready.wait();
                engine
                    .run(
                        &module,
                        RunOptions::default().with_termination_receiver(receiver),
                    )
                    .unwrap()
            });
            barrier.wait();
            let accepted = terminator.terminate(RunTermination::Exit(8));
            let outcome = run.join().unwrap();
            assert_eq!(
                outcome,
                if accepted {
                    RunOutcome::Exited(8)
                } else {
                    RunOutcome::Completed
                }
            );
            assert!(!terminator.terminate(RunTermination::Exit(9)));
        }
    }

    fn spawn(engine: &Engine, module: &Module) -> (RunTerminator, JoinHandle<RunOutcome>, File) {
        let marker = tempfile::tempfile().unwrap();
        let (terminator, receiver) = termination_channel();
        let options = RunOptions::default()
            .with_termination_receiver(receiver)
            .with_stdio(
                tempfile::tempfile().unwrap(),
                marker.try_clone().unwrap(),
                tempfile::tempfile().unwrap(),
            );
        let engine = engine.clone();
        let module = module.clone();
        let run = thread::spawn(move || engine.run(&module, options).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        while marker.metadata().unwrap().len() == 0 {
            assert!(
                !run.is_finished(),
                "guest exited before signalling readiness"
            );
            assert!(Instant::now() < deadline, "guest did not start");
            thread::sleep(Duration::from_millis(1));
        }
        (terminator, run, marker)
    }

    #[test]
    fn external_termination() {
        let Ok(case) = std::env::var("MOONRUN_TERMINATION_CASE") else {
            // A broken engine interrupt must fail with a timeout, never leave
            // an infinite guest loop in the test runner or the test process.
            for case in ["loop", "start", "catch", "imports", "workers"] {
                snapbox::cmd::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "v8::external_termination", "--nocapture"])
                    .env("MOONRUN_TERMINATION_CASE", case)
                    .timeout(Duration::from_secs(15))
                    .assert()
                    .success();
            }
            return;
        };
        eprintln!("termination case: {case}");

        let (imports, setup, body) = match case.as_str() {
            "loop" | "start" => ("", "", "(loop br 0)"),
            "catch" => (
                "",
                "",
                "(block $caught (try_table (catch_all $caught) (loop br 0)) return) i32.const 88 call $ready",
            ),
            "imports" => (
                r#"(import "__moonbit_fs_unstable" "begin_create_string" (func $builder (result externref)))
                    (import "__moonbit_fs_unstable" "string_append_char" (func $append (param externref i32)))
                    (import "__moonbit_fs_unstable" "finish_create_string" (func $finish (param externref) (result externref)))"#,
                "",
                "(loop call $builder local.tee $value i32.const 65 call $append local.get $value call $finish drop br 0)",
            ),
            "workers" => (
                r#"(import "moonbitlang/async" "event_bus/create" (func $poll (result i64)))
                    (import "moonbitlang/async" "thread_pool/init_thread_pool" (func $init (param i64) (result i64)))
                    (import "moonbitlang/async" "fd_util/pipe" (func $pipe (param i32 i32 i32 i32) (result i32)))
                    (import "moonbitlang/async" "thread_pool/make_read_job" (func $read (param i64 i32 i64) (result i64)))
                    (import "moonbitlang/async" "thread_pool/spawn_worker" (func $spawn (param i32 i64) (result i64)))"#,
                // Keep the writer open so teardown must cancel the blocking read.
                // The test-only sleep Job uses uncancellable Sleep on Windows.
                r#"call $poll call $init drop
                    (if (call $pipe (i32.const 0) (i32.const 2)
                        (i32.const 0) (i32.const 0)) (then unreachable))
                    (drop (call $spawn (i32.const 1)
                        (call $read (i64.load (i32.const 0)) (i32.const 1) (i64.const -1))))"#,
                "(loop br 0)",
            ),
            _ => panic!("unknown case: {case}"),
        };
        let entry = if case == "start" {
            ""
        } else {
            "(export \"_start\")"
        };
        let start = if case == "start" {
            "(start $entry)"
        } else {
            ""
        };
        let engine = Engine::default();
        let module = engine
            .compile(
                "loop.wasm",
                wat::parse_str(format!(
                    r#"(module
            (import "spectest" "print_char" (func $ready (param i32)))
            {imports}
            (memory (export "memory") 1)
            (func $entry {entry} (local $value externref)
                {setup} i32.const 82 call $ready {body})
            {start})"#
                ))
                .unwrap(),
            )
            .unwrap();
        let (first, first_run, first_marker) = spawn(&engine, &module);
        let (second, second_run, second_marker) = spawn(&engine, &module);
        thread::sleep(Duration::from_millis(20));
        assert!(first.terminate(RunTermination::KilledBySignal(9)));
        assert_eq!(first_run.join().unwrap(), RunOutcome::KilledBySignal(9));
        assert!(!first.terminate(RunTermination::Exit(1)));
        thread::sleep(Duration::from_millis(20));
        assert!(
            !second_run.is_finished(),
            "termination affected another Run"
        );
        assert!(second.terminate(RunTermination::Exit(23)));
        assert_eq!(second_run.join().unwrap(), RunOutcome::Exited(23));
        assert_eq!(first_marker.metadata().unwrap().len(), 1);
        assert_eq!(second_marker.metadata().unwrap().len(), 1);

        let plain = engine
            .compile("next.wasm", wat::parse_str("(module)").unwrap())
            .unwrap();
        assert_eq!(
            engine.run(&plain, RunOptions::default()).unwrap(),
            RunOutcome::Completed
        );
    }
}
