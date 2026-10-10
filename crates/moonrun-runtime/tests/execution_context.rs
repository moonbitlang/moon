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

use moonrun_runtime::{Engine, ExecutionContext, RunOptions, RunOutcome};
use std::fs::{self, File};

#[test]
fn concurrent_contexts_own_stdio_and_relative_files() {
    let engine = Engine::default();
    let module = engine.compile("stdio-and-files.wasm", wat::parse_str(r#"(module
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
            ;; Write and read the same relative filename in each context.
            i32.const 120 call $string
            i32.const 32 i32.load8_u call $string call $save
            i32.const 32 i32.const 120 call $string call $load i32.const 0 call $char i32.store8
            i32.const 1 i32.const 0 i32.const 1 i32.const 8 call $write
            if unreachable end
            i32.const 2 i32.const 0 i32.const 1 i32.const 8 call $write
            if unreachable end))"#).unwrap()).unwrap();
    let cwd = std::env::current_dir().unwrap();
    let directories: Vec<_> = (0..2).map(|_| tempfile::tempdir().unwrap()).collect();
    let runs: Vec<_> = directories
        .iter()
        .zip([b'A', b'B'])
        .map(|(dir, byte)| {
            fs::write(dir.path().join("stdin"), [byte]).unwrap();
            let context = ExecutionContext::new(
                dir.path(),
                File::open(dir.path().join("stdin")).unwrap(),
                File::create(dir.path().join("stdout")).unwrap(),
                File::create(dir.path().join("stderr")).unwrap(),
            )
            .unwrap();
            let engine = engine.clone();
            let module = module.clone();
            std::thread::spawn(move || {
                engine
                    .run_in_context(&module, RunOptions::default(), context)
                    .unwrap()
            })
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

#[cfg(all(not(feature = "v8"), feature = "wasmtime"))]
#[test]
fn cancelling_one_context_leaves_other_runs_usable() {
    use moonrun_runtime::{EngineConfig, RunControl};
    use std::time::{Duration, Instant};
    let engine = Engine::new(EngineConfig::default().with_epoch_interruption(true));
    let looping = engine
        .compile(
            "loop.wasm",
            wat::parse_str("(module (func (export \"_start\") (loop br 0)))").unwrap(),
        )
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let control = RunControl::default();
    let context = ExecutionContext::new(
        dir.path(),
        tempfile::tempfile().unwrap(),
        tempfile::tempfile().unwrap(),
        tempfile::tempfile().unwrap(),
    )
    .unwrap()
    .with_control(control.clone());
    let run_engine = engine.clone();
    let run = std::thread::spawn(move || {
        run_engine.run_in_context(&looping, RunOptions::default(), context)
    });
    // Also covers cancellation before the instance enters guest code.
    control.cancel();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !run.is_finished() {
        assert!(
            Instant::now() < deadline,
            "guest cancellation did not complete"
        );
        engine.increment_epoch();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        run.join()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("cancelled")
    );
    let empty = engine
        .compile(
            "empty.wasm",
            wat::parse_str("(module (func (export \"_start\")))").unwrap(),
        )
        .unwrap();
    assert_eq!(
        engine.run(&empty, RunOptions::default()).unwrap(),
        RunOutcome::Completed
    );
}
