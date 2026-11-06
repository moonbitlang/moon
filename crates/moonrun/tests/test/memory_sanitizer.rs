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

use super::{TestDir, moon_cmd};

#[test]
fn test_moon_run_with_memory_sanitizer_imports() {
    let dir = TestDir::new("test_memory_sanitizer.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/main/main.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .success()
        .stdout_eq("ok\n");
}

#[test]
fn test_moon_run_memory_sanitizer_reports_demangled_alloc_stack() {
    let dir = TestDir::new("test_memory_sanitizer.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/duplicate_alloc/duplicate_alloc.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Error: moonbit:ffi/memory-sanitizer.register-object-alloc failed: object 2048 is already live with size 16
previous allocation stack:
    at @moonbit/ffi-memory-sanitizer-test/memory_sanitizer.host_register_object_alloc
    at @moonbit/ffi-memory-sanitizer-test/memory_sanitizer.register_object_alloc
    at @moonbit/ffi-memory-sanitizer-test/duplicate_alloc.allocate_once
    at __moonbit_main
...
"#]]);
}

#[test]
fn test_moon_run_memory_sanitizer_reports_double_free() {
    let dir = TestDir::new("test_memory_sanitizer.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/double_free/double_free.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Error: moonbit:ffi/memory-sanitizer.register-object-free failed: invalid object 4096
    at @moonbit/ffi-memory-sanitizer-test/memory_sanitizer.host_register_object_free ([..]0x[..])
    at @moonbit/ffi-memory-sanitizer-test/memory_sanitizer.register_object_free ([..]0x[..])
    at @moonbit/ffi-memory-sanitizer-test/double_free.free_twice ([..]0x[..])
    at __moonbit_main ([..]0x[..])
...
"#]]);
}

#[test]
fn test_moon_run_memory_sanitizer_reports_leaks() {
    let dir = TestDir::new("test_memory_sanitizer.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let wasm_file = dir.join("_build/wasm/debug/build/leak/leak.wasm");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm_file)
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Error: moonrun memory sanitizer detected 1 leaked object (16 bytes)
leaked object 8192 (16 bytes)
allocation stack:
    at @moonbit/ffi-memory-sanitizer-test/memory_sanitizer.host_register_object_alloc
    at @moonbit/ffi-memory-sanitizer-test/memory_sanitizer.register_object_alloc
    at @moonbit/ffi-memory-sanitizer-test/leak.leak_object
    at __moonbit_main
...
"#]]);
}
