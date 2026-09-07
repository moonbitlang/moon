// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

#[test]
fn engine_backend_reuses_compiled_modules() {
    let engine = moonrun::Engine::default();
    let plain = engine
        .compile(
            "plain.wasm",
            wat::parse_str(r#"(module (func (export "_start")))"#).unwrap(),
        )
        .unwrap();
    for _ in 0..2 {
        assert_eq!(
            engine.run(&plain, moonrun::RunOptions::default()).unwrap(),
            moonrun::RunOutcome::Completed
        );
    }
}

#[test]
fn engine_backends_support_legacy_spectest_read_char() {
    let wasm = tempfile::Builder::new()
        .prefix("spectest-read-char.")
        .suffix(".wasm")
        .tempfile()
        .unwrap();
    std::fs::write(
        wasm.path(),
        wat::parse_str(
            r#"(module
                (import "spectest" "read_char" (func $read_char (result i32)))
                (import "spectest" "print_char" (func $print_char (param i32)))
                (func (export "_start")
                    call $read_char
                    call $print_char
                    call $read_char
                    i32.const -1
                    i32.ne
                    if unreachable end))"#,
        )
        .unwrap(),
    )
    .unwrap();

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(wasm.path())
        .stdin("λ")
        .assert()
        .success()
        .stdout_eq("λ")
        .stderr_eq("");
}

#[test]
fn engine_backends_support_js_string_builtins() {
    let engine = moonrun::Engine::default();
    let js_strings = engine
        .compile(
            "js-strings.wasm",
            wat::parse_str(
                r#"(module
                    (type $chars (array (mut i16)))
                    (import "_" "hello" (global $hello (ref extern)))
                    (import "_" " world" (global $world (ref extern)))
                    (import "_" "ell" (global $ell (ref extern)))
                    (import "wasm:js-string" "cast"
                        (func $cast (param externref) (result (ref extern))))
                    (import "wasm:js-string" "test"
                        (func $test (param externref) (result i32)))
                    (import "wasm:js-string" "fromCharCode"
                        (func $from_char_code (param i32) (result (ref extern))))
                    (import "wasm:js-string" "fromCodePoint"
                        (func $from_code_point (param i32) (result (ref extern))))
                    (import "wasm:js-string" "length"
                        (func $length (param externref) (result i32)))
                    (import "wasm:js-string" "charCodeAt"
                        (func $char_code_at (param externref i32) (result i32)))
                    (import "wasm:js-string" "codePointAt"
                        (func $code_point_at (param externref i32) (result i32)))
                    (import "wasm:js-string" "equals"
                        (func $equals (param externref externref) (result i32)))
                    (import "wasm:js-string" "concat"
                        (func $concat (param externref externref) (result (ref extern))))
                    (import "wasm:js-string" "substring"
                        (func $substring (param externref i32 i32) (result (ref extern))))
                    (import "wasm:js-string" "compare"
                        (func $compare (param externref externref) (result i32)))
                    (import "wasm:js-string" "fromCharCodeArray"
                        (func $from_chars
                            (param (ref null $chars) i32 i32)
                            (result (ref extern))))
                    (import "wasm:js-string" "intoCharCodeArray"
                        (func $into_chars
                            (param externref (ref null $chars) i32)
                            (result i32)))
                    (func (export "_start")
                        (local $array (ref $chars))
                        (local $string externref)
                        global.get $hello
                        call $cast
                        call $length
                        i32.const 5
                        i32.ne
                        if unreachable end

                        global.get $hello
                        call $test
                        i32.const 1
                        i32.ne
                        if unreachable end
                        ref.null extern
                        call $test
                        if unreachable end

                        i32.const 65
                        call $from_char_code
                        i32.const 0
                        call $char_code_at
                        i32.const 65
                        i32.ne
                        if unreachable end

                        i32.const 128516
                        call $from_code_point
                        local.set $string
                        local.get $string
                        call $length
                        i32.const 2
                        i32.ne
                        if unreachable end
                        local.get $string
                        i32.const 0
                        call $code_point_at
                        i32.const 128516
                        i32.ne
                        if unreachable end

                        global.get $hello
                        global.get $world
                        call $concat
                        call $length
                        i32.const 11
                        i32.ne
                        if unreachable end

                        global.get $hello
                        i32.const 1
                        call $char_code_at
                        i32.const 101
                        i32.ne
                        if unreachable end

                        global.get $hello
                        i32.const 1
                        i32.const 4
                        call $substring
                        global.get $ell
                        call $equals
                        i32.eqz
                        if unreachable end

                        global.get $hello
                        ref.null extern
                        call $equals
                        if unreachable end
                        ref.null extern
                        ref.null extern
                        call $equals
                        i32.eqz
                        if unreachable end

                        global.get $hello
                        global.get $world
                        call $compare
                        i32.const 1
                        i32.ne
                        if unreachable end

                        i32.const 6
                        array.new_default $chars
                        local.set $array
                        global.get $hello
                        local.get $array
                        i32.const 1
                        call $into_chars
                        i32.const 5
                        i32.ne
                        if unreachable end
                        local.get $array
                        i32.const 1
                        i32.const 6
                        call $from_chars
                        global.get $hello
                        call $equals
                        i32.eqz
                        if unreachable end))"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        engine
            .run(&js_strings, moonrun::RunOptions::default())
            .unwrap(),
        moonrun::RunOutcome::Completed
    );
}

#[test]
fn engine_backends_support_test_driver_string_bridge() {
    let engine = moonrun::Engine::default();
    let driver = engine
        .compile(
            "test-driver.wasm",
            wat::parse_str(
                r#"(module
                    (import "__moonbit_fs_unstable" "begin_read_string"
                        (func $begin_read_string (param externref) (result externref)))
                    (import "__moonbit_fs_unstable" "string_read_char"
                        (func $string_read_char (param externref) (result i32)))
                    (import "__moonbit_fs_unstable" "finish_read_string"
                        (func $finish_read_string (param externref)))
                    (func (export "moonbit_test_driver_internal_execute")
                        (param $file externref) (param i32)
                        (local $reader externref)
                        (local $length i32)
                        local.get $file
                        call $begin_read_string
                        local.set $reader

                        local.get $reader
                        call $string_read_char
                        i32.const 109
                        i32.ne
                        if unreachable end
                        i32.const 1
                        local.set $length

                        block $done
                            loop $next
                                local.get $reader
                                call $string_read_char
                                i32.const -1
                                i32.eq
                                br_if $done
                                local.get $length
                                i32.const 1
                                i32.add
                                local.set $length
                                br $next
                            end
                        end
                        local.get $reader
                        call $finish_read_string

                        local.get $length
                        i32.const 13
                        i32.ne
                        if unreachable end)
                    (func (export "moonbit_test_driver_finish")))"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        engine
            .run(
                &driver,
                moonrun::RunOptions::default().with_test_args(
                    r#"{"package":"moon/core","file_and_index":[["main_test.mbt",[{"start":0,"end":1}]]]}"#,
                ),
            )
            .unwrap(),
        moonrun::RunOutcome::Completed
    );
}

#[test]
fn engine_backends_support_repeated_imports() {
    let engine = moonrun::Engine::default();
    let module = engine
        .compile(
            "repeated-imports.wasm",
            wat::parse_str(
                r#"(module
                    (import "__moonbit_sys_unstable" "is_windows"
                        (func $is_windows_a (result i32)))
                    (import "__moonbit_sys_unstable" "is_windows"
                        (func $is_windows_b (result i32)))
                    (func (export "_start")
                        call $is_windows_a
                        call $is_windows_b
                        i32.ne
                        if unreachable end))"#,
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(
        engine.run(&module, moonrun::RunOptions::default()).unwrap(),
        moonrun::RunOutcome::Completed
    );
}

#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
#[test]
fn wasmtime_rejects_unregistered_imports() {
    let engine = moonrun::Engine::default();
    for (name, import, expected) in [
        (
            "missing-wasi-import.wasm",
            r#"(import "wasi_snapshot_preview1" "not_registered" (func))"#,
            "unknown import: `wasi_snapshot_preview1::not_registered`",
        ),
        (
            "unsupported-poll-oneoff.wasm",
            r#"(import "wasi_snapshot_preview1" "poll_oneoff"
                (func (param i32 i32 i32 i32) (result i32)))"#,
            "unknown import: `wasi_snapshot_preview1::poll_oneoff`",
        ),
        (
            "unsupported-legacy-string-hook.wasm",
            r#"(import "moonbit" "string_to_js_string" (func (param i32)))"#,
            "unsupported Wasmtime import `moonbit.string_to_js_string`",
        ),
        (
            "unsupported-console-elog.wasm",
            r#"(import "console" "elog" (func (param externref)))"#,
            "unsupported Wasmtime import `console.elog`",
        ),
    ] {
        let module = engine
            .compile(name, wat::parse_str(format!("(module {import})")).unwrap())
            .unwrap();
        let error = engine
            .run(&module, moonrun::RunOptions::default())
            .unwrap_err();
        let error = format!("{error:#}");
        assert!(error.contains(expected), "{name}: {error}");
    }
}

#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
#[test]
fn wasmtime_preserves_instantiation_context_for_start_host_errors() {
    let engine = moonrun::Engine::default();
    let module = engine
        .compile(
            "start-host-error.wasm",
            wat::parse_str(
                r#"(module
                    (import "_" "filename" (global $filename (ref extern)))
                    (import "__moonbit_fs_unstable" "begin_read_string"
                        (func $begin_read_string (param externref) (result externref)))
                    (import "console" "log" (func $log (param (ref extern))))
                    (func $module_start
                        global.get $filename
                        call $begin_read_string
                        ref.as_non_null
                        call $log)
                    (start $module_start))"#,
            )
            .unwrap(),
        )
        .unwrap();

    let error = engine
        .run(&module, moonrun::RunOptions::default())
        .unwrap_err();
    let error = format!("{error:#}");
    assert!(
        error.contains("failed to instantiate `start-host-error.wasm`")
            && error.contains("externref is not a JS string")
            && error.contains("at module_start"),
        "{error}"
    );
}

#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
#[test]
fn wasmtime_reports_invalid_byte_handles_as_host_errors() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("invalid-bytes.bin");
    let path = output
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let engine = moonrun::Engine::default();
    let module = engine
        .compile(
            "invalid-byte-handle.wasm",
            wat::parse_str(format!(
                r#"(module
                    (import "_" "{path}" (global $path (ref extern)))
                    (import "_" "not bytes" (global $not_bytes (ref extern)))
                    (import "__moonbit_fs_unstable" "write_bytes_to_file"
                        (func $write (param externref externref)))
                    (func (export "_start")
                        global.get $path
                        global.get $not_bytes
                        call $write))"#,
            ))
            .unwrap(),
        )
        .unwrap();

    let error = engine
        .run(&module, moonrun::RunOptions::default())
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("unexpected externref host value"),
        "{error:#}"
    );
    assert!(!output.exists());
}

#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
#[test]
fn wasmtime_authorizes_before_reading_byte_handles() {
    let directory = tempfile::tempdir().unwrap();
    let policy = directory.path().join("policy.toml");
    std::fs::write(&policy, "").unwrap();
    let wasm = directory.path().join("denied-invalid-byte-handle.wasm");
    std::fs::write(
        &wasm,
        wat::parse_str(
            r#"(module
                (import "_" "denied.bin" (global $path (ref extern)))
                (import "_" "not bytes" (global $not_bytes (ref extern)))
                (import "__moonbit_fs_unstable" "write_bytes_to_file_new"
                    (func $write (param externref externref) (result i32)))
                (import "__moonbit_fs_unstable" "get_error_message"
                    (func $get_error_message (result externref)))
                (import "console" "log" (func $log (param (ref extern))))
                (func (export "_start")
                    global.get $path
                    global.get $not_bytes
                    call $write
                    drop
                    call $get_error_message
                    ref.as_non_null
                    call $log))"#,
        )
        .unwrap(),
    )
    .unwrap();

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(&wasm)
        .arg("--policy")
        .arg(policy)
        .assert()
        .success()
        .stdout_eq("Permission denied: denied.bin\n")
        .stderr_eq("Sandbox policy blocked file write: \"denied.bin\"\n");
}

#[cfg(all(feature = "wasmtime", not(feature = "v8")))]
#[test]
fn wasmtime_preserves_host_adapter_errors_from_exported_calls() {
    let engine = moonrun::Engine::default();
    let cases = [
        (
            "run-host-error.wasm",
            r#"(module
                (import "_" "filename" (global $filename (ref extern)))
                (import "__moonbit_fs_unstable" "begin_read_string"
                    (func $begin_read_string (param externref) (result externref)))
                (import "console" "log" (func $log (param (ref extern))))
                (func (export "_start")
                    global.get $filename
                    call $begin_read_string
                    ref.as_non_null
                    call $log))"#,
            moonrun::RunOptions::default(),
            None,
        ),
        (
            "test-execute-host-error.wasm",
            r#"(module
                (import "__moonbit_fs_unstable" "begin_read_string"
                    (func $begin_read_string (param externref) (result externref)))
                (import "console" "log" (func $log (param (ref extern))))
                (func (export "moonbit_test_driver_internal_execute")
                    (param $file externref) (param i32)
                    local.get $file
                    call $begin_read_string
                    ref.as_non_null
                    call $log)
                (func (export "moonbit_test_driver_finish")))"#,
            moonrun::RunOptions::default().with_test_args(
                r#"{"package":"moon/core","file_and_index":[["main_test.mbt",[{"start":0,"end":1}]]]}"#,
            ),
            Some("failed to execute a MoonBit test"),
        ),
        (
            "test-finish-host-error.wasm",
            r#"(module
                (import "_" "filename" (global $filename (ref extern)))
                (import "__moonbit_fs_unstable" "begin_read_string"
                    (func $begin_read_string (param externref) (result externref)))
                (import "console" "log" (func $log (param (ref extern))))
                (func (export "moonbit_test_driver_internal_execute")
                    (param externref i32))
                (func (export "moonbit_test_driver_finish")
                    global.get $filename
                    call $begin_read_string
                    ref.as_non_null
                    call $log))"#,
            moonrun::RunOptions::default()
                .with_test_args(r#"{"package":"moon/core","file_and_index":[]}"#),
            Some("failed to finish the MoonBit test driver"),
        ),
    ];

    for (name, source, options, expected) in cases {
        let module = engine
            .compile(name, wat::parse_str(source).unwrap())
            .unwrap();
        let error = engine.run(&module, options).unwrap_err();
        let error = format!("{error:#}");
        assert!(
            expected.is_none_or(|context| error.contains(context))
                && error.contains("externref is not a JS string"),
            "{error}"
        );
    }
}

#[test]
fn engine_backends_share_the_wasip1_adapter() {
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
    std::fs::write(&policy, "{}").unwrap();
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
