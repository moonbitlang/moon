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

#[test]
fn engines_share_the_full_wasi_surface() {
    let dir = tempfile::tempdir().unwrap();
    let stdio_wasm = dir.path().join("stdio.wasm");
    std::fs::write(
        &stdio_wasm,
        wat::parse_str(
            r#"(module
                (import "wasi_snapshot_preview1" "fd_read"
                    (func $fd_read (param i32 i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "fd_write"
                    (func $fd_write (param i32 i32 i32 i32) (result i32)))
                (memory (export "memory") 1)
                (func $ok (param $errno i32)
                    local.get $errno
                    if unreachable end)
                (func (export "_start")
                    i32.const 16 i32.const 32 i32.store
                    i32.const 20 i32.const 64 i32.store
                    i32.const 0 i32.const 16 i32.const 1 i32.const 24 call $fd_read call $ok
                    i32.const 20 i32.const 24 i32.load i32.store
                    i32.const 1 i32.const 16 i32.const 1 i32.const 28 call $fd_write call $ok))"#,
        )
        .unwrap(),
    )
    .unwrap();
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&stdio_wasm)
        .stdin("wasi stdio\n")
        .assert()
        .success()
        .stdout_eq("wasi stdio\n")
        .stderr_eq("");

    let surface_wasm = dir.path().join("surface.wasm");
    std::fs::write(
        &surface_wasm,
        wat::parse_str(
            r#"(module
                (import "wasi_snapshot_preview1" "args_get"
                    (func $args_get (param i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "args_sizes_get"
                    (func $args_sizes_get (param i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "environ_get"
                    (func $environ_get (param i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "environ_sizes_get"
                    (func $environ_sizes_get (param i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "random_get"
                    (func $random_get (param i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "fd_close"
                    (func $fd_close (param i32) (result i32)))
                (import "wasi_snapshot_preview1" "fd_prestat_get"
                    (func $fd_prestat_get (param i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "fd_prestat_dir_name"
                    (func $fd_prestat_dir_name (param i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "fd_readdir"
                    (func $fd_readdir (param i32 i32 i32 i64 i32) (result i32)))
                (import "wasi_snapshot_preview1" "fd_write"
                    (func $fd_write (param i32 i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "path_open"
                    (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "path_readlink"
                    (func $path_readlink (param i32 i32 i32 i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "path_rename"
                    (func $path_rename (param i32 i32 i32 i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "path_create_directory"
                    (func $path_create_directory (param i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "path_remove_directory"
                    (func $path_remove_directory (param i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "path_unlink_file"
                    (func $path_unlink_file (param i32 i32 i32) (result i32)))
                (import "wasi_snapshot_preview1" "proc_exit"
                    (func $proc_exit (param i32)))
                (memory (export "memory") 1)
                (data (i32.const 16) "dir")
                (data (i32.const 32) "dir/file.txt")
                (data (i32.const 48) "dir/renamed.txt")
                (data (i32.const 80) "payload")
                (func $ok (param $errno i32)
                    local.get $errno
                    if unreachable end)
                (func (export "_start")
                    i32.const 200 i32.const 204 call $args_sizes_get call $ok
                    i32.const 200 i32.load i32.const 2 i32.ne if unreachable end
                    i32.const 208 i32.const 256 call $args_get call $ok
                    i32.const 300 i32.const 304 call $environ_sizes_get call $ok
                    i32.const 300 i32.load if unreachable end
                    i32.const 8000 i32.const 12000 call $environ_get call $ok
                    i32.const 4096 i32.const 16 call $random_get call $ok

                    i32.const 3 i32.const 4200 call $fd_prestat_get call $ok
                    i32.const 3 i32.const 4210 i32.const 64 call $fd_prestat_dir_name call $ok
                    i32.const 3 i32.const 16 i32.const 3 call $path_create_directory call $ok

                    i32.const 3 i32.const 0 i32.const 32 i32.const 12 i32.const 1
                    i64.const 66 i64.const 0 i32.const 0 i32.const 140 call $path_open call $ok
                    i32.const 128 i32.const 80 i32.store
                    i32.const 132 i32.const 7 i32.store
                    i32.const 140 i32.load i32.const 128 i32.const 1 i32.const 148
                    call $fd_write call $ok
                    i32.const 140 i32.load call $fd_close call $ok

                    i32.const 3 i32.const 32 i32.const 12
                    i32.const 3 i32.const 48 i32.const 15 call $path_rename call $ok
                    i32.const 3 i32.const 0 i32.const 16 i32.const 3 i32.const 2
                    i64.const 16384 i64.const 0 i32.const 0 i32.const 144 call $path_open call $ok
                    i32.const 144 i32.load i32.const 5000 i32.const 1024 i64.const 0 i32.const 152
                    call $fd_readdir call $ok
                    i32.const 152 i32.load i32.eqz if unreachable end
                    i32.const 144 i32.load call $fd_close call $ok

                    i32.const 3 i32.const 48 i32.const 15 call $path_unlink_file call $ok
                    i32.const 3 i32.const 16 i32.const 3 call $path_remove_directory call $ok))"#,
        )
        .unwrap(),
    )
    .unwrap();
    let policy = dir.path().join("policy.json");
    std::fs::write(&policy, r#"{"fs":{"read":["."],"write":["."]}}"#).unwrap();
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(dir.path())
        .arg("--policy")
        .arg(&policy)
        .arg(&surface_wasm)
        .arg("--")
        .arg("alpha")
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");
    assert!(!dir.path().join("dir").exists());

    let exit_wasm = dir.path().join("exit.wasm");
    std::fs::write(
        &exit_wasm,
        wat::parse_str(
            r#"(module
                (import "wasi_snapshot_preview1" "proc_exit" (func $proc_exit (param i32)))
                (func (export "_start") i32.const 7 call $proc_exit))"#,
        )
        .unwrap(),
    )
    .unwrap();
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(exit_wasm)
        .assert()
        .code(7)
        .stdout_eq("")
        .stderr_eq("");
}

#[test]
fn i64_rights_and_directory_cookies_preserve_high_bits() {
    let engine = Engine::default();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("entry"), "content").unwrap();
    let module = engine
        .compile(
            "wide-arguments.wasm",
            wat::parse_str(
                r#"(module
        (import "wasi_snapshot_preview1" "path_open"
            (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_readdir"
            (func $read (param i32 i32 i32 i64 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "entry")
        (func $check (param $high i64)
            ;; Unknown high rights bits must reach the Host Domain and return
            ;; INVAL (28), rather than truncate to valid FD_READ rights.
            i32.const 3 i32.const 0 i32.const 0 i32.const 5 i32.const 0
            local.get $high i64.const 2 i64.or i64.const 0 i32.const 0 i32.const 16
            call $open i32.const 28 i32.ne if unreachable end
            i32.const 3 i32.const 0 i32.const 0 i32.const 5 i32.const 0
            i64.const 2 local.get $high i32.const 0 i32.const 16
            call $open i32.const 28 i32.ne if unreachable end
            ;; A high unsigned cookie is past the directory's end. Truncating
            ;; it to zero would return the entry again.
            i32.const 3 i32.const 128 i32.const 1024 local.get $high i32.const 32
            call $read if unreachable end
            i32.const 32 i32.load if unreachable end)
        (func (export "_start")
            i32.const 3 i32.const 128 i32.const 1024 i64.const 0 i32.const 32
            call $read if unreachable end
            i32.const 32 i32.load i32.eqz if unreachable end
            i64.const 0x100000000 call $check
            i64.const 0x8000000000000000 call $check
            i64.const -1 call $check))"#,
            )
            .unwrap(),
        )
        .unwrap();
    let options = RunOptions::default()
        .with_working_directory(moonrun::WorkingDirectory::Fixed(dir.path().to_path_buf()));
    assert_eq!(engine.run(&module, options).unwrap(), RunOutcome::Completed);
}

#[test]
fn descriptor_close_does_not_require_memory() {
    let engine = Engine::default();
    let module = engine
        .compile(
            "close.wasm",
            wat::parse_str(
                r#"(module
        (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
        (func $run (export "_start")
            i32.const -1 call $close i32.const 8 i32.ne if unreachable end)
        (start $run))"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        engine.run(&module, RunOptions::default()).unwrap(),
        RunOutcome::Completed
    );
}

#[cfg(unix)]
#[test]
fn readlink_copies_into_grown_memory() {
    let engine = Engine::default();
    let dir = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("target", dir.path().join("link")).unwrap();
    let module = engine
        .compile(
            "readlink.wasm",
            wat::parse_str(
                r#"(module
        (import "wasi_snapshot_preview1" "path_readlink"
            (func $readlink (param i32 i32 i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1 2)
        (data (i32.const 0) "link")
        (func (export "_start")
            i32.const 1 memory.grow drop
            i32.const 3 i32.const 0 i32.const 4 i32.const 65536 i32.const 6 i32.const 16
            call $readlink if unreachable end
            i32.const 16 i32.load i32.const 6 i32.ne if unreachable end
            i32.const 65536 i32.load i32.const 0x67726174 i32.ne if unreachable end
            i32.const 65540 i32.load16_u i32.const 0x7465 i32.ne if unreachable end))"#,
            )
            .unwrap(),
        )
        .unwrap();
    let options = RunOptions::default()
        .with_working_directory(moonrun::WorkingDirectory::Fixed(dir.path().to_path_buf()));
    assert_eq!(engine.run(&module, options).unwrap(), RunOutcome::Completed);
}
