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

//! Exercise the same shared registrations through each selected engine.

use moonrun::{Engine, RunOptions, RunOutcome};

#[test]
fn arguments_and_memory_are_fresh_across_calls_and_runs() {
    let engine = Engine::default();
    let module = engine.compile("args.wasm", wat::parse_str(r#"(module
        ;; Deliberately reorder and duplicate imports: the C API uses module order.
        (import "wasi_snapshot_preview1" "args_get" (func $args (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "args_sizes_get" (func $sizes (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "args_sizes_get" (func $sizes_again (param i32 i32) (result i32)))
        (memory (export "memory") 1 2)
        (func (export "_start")
            i32.const 0 i32.load if unreachable end
            i32.const 0 i32.const 4 call $sizes if unreachable end
            i32.const 0 i32.load i32.const 2 i32.ne if unreachable end
            i32.const 1 memory.grow i32.const 1 i32.ne if unreachable end
            i32.const 65536 i32.const 65540 call $sizes_again if unreachable end
            i32.const 65536 i32.load i32.const 2 i32.ne if unreachable end
            i32.const 65544 i32.const 65560 call $args if unreachable end
            i32.const 65548 i32.load i32.load8_u i32.const 120 i32.ne if unreachable end
            ;; Negative Wasm i32 addresses must remain large unsigned addresses.
            i32.const -1 i32.const 4 call $sizes i32.const 21 i32.ne if unreachable end
            i32.const -1 i32.const 32 call $args i32.const 21 i32.ne if unreachable end))"#).unwrap()).unwrap();
    for _ in 0..2 {
        assert_eq!(
            engine
                .run(&module, RunOptions::default().with_args(["x"]))
                .unwrap(),
            RunOutcome::Completed
        );
    }
}

#[test]
fn write_reacquires_memory_and_reports_errno() {
    let engine = Engine::default();
    let module = engine.compile("write.wasm", wat::parse_str(r#"(module
        (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1 2)
        (func (export "_start")
            ;; Zero iovecs writes no output but still validates/stores nwritten.
            i32.const 1 i32.const 0 i32.const 0 i32.const 8 call $write if unreachable end
            i32.const 8 i32.load if unreachable end
            i32.const 1 memory.grow drop
            i32.const 1 i32.const 65536 i32.const 0 i32.const 65540 call $write if unreachable end
            i32.const 65540 i32.load if unreachable end
            i32.const 65536 i32.const 65552 i32.store
            i32.const 65540 i32.const 1 i32.store
            i32.const 65552 i32.const 65 i32.store8
            i32.const 1 i32.const 65536 i32.const 1 i32.const 65544 call $write if unreachable end
            i32.const 65544 i32.load i32.const 1 i32.ne if unreachable end
            i32.const 1 i32.const 0 i32.const 0 i32.const -1 call $write
            i32.const 21 i32.ne if unreachable end))"#).unwrap()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("stdout");
    let options = RunOptions::default().with_stdio(
        tempfile::tempfile().unwrap(),
        std::fs::File::create(&output).unwrap(),
        tempfile::tempfile().unwrap(),
    );
    assert_eq!(engine.run(&module, options).unwrap(), RunOutcome::Completed);
    assert_eq!(std::fs::read(output).unwrap(), b"A");
}

#[test]
fn exit_without_memory_unwinds_start_sections_and_exported_calls() {
    let engine = Engine::default();
    for export in ["(export \"_start\")", ""] {
        for code in [0, 9] {
            let start = if export.is_empty() {
                "(start $run)"
            } else {
                ""
            };
            let module = engine
                .compile(
                    "exit.wasm",
                    wat::parse_str(format!(
                        r#"(module
                (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
                (func $run {export} i32.const {code} call $exit unreachable) {start})"#
                    ))
                    .unwrap(),
                )
                .unwrap();
            assert_eq!(
                engine.run(&module, RunOptions::default()).unwrap(),
                RunOutcome::Exited(code)
            );
        }
    }
}
