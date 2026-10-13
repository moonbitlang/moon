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

use moonrun::{Engine, RunOptions, RunOutcome};
use std::sync::mpsc::sync_channel;

#[test]
fn concurrent_runs_isolate_standard_streams() {
    let engine = Engine::default();
    let bytes = wat::parse_str(r#"(module
        (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "_start")
            i32.const 0 i32.const 32 i32.store
            i32.const 4 i32.const 1 i32.store
            i32.const 0 i32.const 0 i32.const 1 i32.const 8 call $read
            if unreachable end
            i32.const 8 i32.load i32.const 1 i32.ne if unreachable end
            i32.const 1 i32.const 0 i32.const 1 i32.const 8 call $write
            if unreachable end
            i32.const 2 i32.const 0 i32.const 1 i32.const 8 call $write
            if unreachable end))"#).unwrap();
    let module = engine.compile("stdio.wasm", bytes).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut results = Vec::new();
    let runs: Vec<_> = [b'A', b'B']
        .into_iter()
        .map(|byte| {
            let (input, stdin) = sync_channel(1);
            let (stdout, output) = sync_channel(1);
            let (stderr, error) = sync_channel(1);
            input.send(vec![byte]).unwrap();
            drop(input);
            let options = RunOptions::default().with_stdio(stdin, stdout, stderr);
            results.push((byte, output, error));
            let engine = engine.clone();
            let module = module.clone();
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
    for (byte, output, error) in results {
        assert_eq!(output.into_iter().flatten().collect::<Vec<_>>(), [byte]);
        assert_eq!(error.into_iter().flatten().collect::<Vec<_>>(), [byte]);
    }
}
