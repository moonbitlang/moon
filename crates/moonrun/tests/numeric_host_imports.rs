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

#[test]
fn synchronous_jobs_reuse_constructors_and_result_getters_without_a_pool() {
    let engine = moonrun::Engine::default();
    let module = engine
        .compile(
            "synchronous-jobs.wasm",
            wat::parse_str(
                r#"(module
                    (import "moonbitlang/async" "thread_pool/make_sleep_job"
                        (func $sleep (param i32) (result i64)))
                    (import "moonbitlang/async" "thread_pool/make_statx_job"
                        (func $stat (param i32 i32 i32 i32 i64 i32) (result i64)))
                    (import "moonbitlang/async" "fd_util/invalid_fd"
                        (func $invalid (result i64)))
                    (import "moonbitlang/async" "thread_pool/run_job"
                        (func $run (param i64)))
                    (import "moonbitlang/async" "thread_pool/job_get_ret"
                        (func $ret (param i64) (result i32)))
                    (import "moonbitlang/async" "thread_pool/job_get_err"
                        (func $err (param i64) (result i32)))
                    (import "moonbitlang/async" "thread_pool/get_stat_result"
                        (func $copy (param i64 i32 i32)))
                    (import "moonbitlang/async" "thread_pool/free_job"
                        (func $free (param i64)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) ".\00")
                    ;; Scalar jobs can run during instantiation without guest memory.
                    (func $init (local $job i64)
                        (local.set $job (call $sleep (i32.const 0)))
                        (call $run (local.get $job))
                        (if (call $err (local.get $job)) (then unreachable))
                        (if (call $ret (local.get $job)) (then unreachable))
                        (call $free (local.get $job)))
                    (start $init)
                    (func (export "_start") (local $job i64)
                        (local.set $job (call $stat
                            (i32.const 0) (i32.const 1) (i32.const 1)
                            (i32.const 16) (call $invalid) (i32.const 1)))
                        (call $run (local.get $job))
                        (if (call $err (local.get $job)) (then unreachable))
                        (if (call $ret (local.get $job)) (then unreachable))
                        ;; Results stay host-owned until copied into the current memory.
                        (drop (memory.grow (i32.const 1)))
                        (call $copy (local.get $job) (i32.const 65536) (i32.const 16))
                        (if (i32.ne (i32.load (i32.const 65536)) (i32.const 16))
                            (then unreachable))
                        (if (i32.ne (i32.load (i32.const 65540)) (i32.const 1))
                            (then unreachable))
                        (if (i64.ne (i64.load (i32.const 65544)) (i64.const 2))
                            (then unreachable))
                        (call $free (local.get $job))
                        ;; Invalid requests use the ordinary job error and copy-out path.
                        (local.set $job (call $stat
                            (i32.const 0) (i32.const 1) (i32.const 256)
                            (i32.const 16) (call $invalid) (i32.const 1)))
                        (call $run (local.get $job))
                        (if (i32.eqz (call $err (local.get $job))) (then unreachable))
                        (if (i32.ne (call $ret (local.get $job)) (i32.const -1))
                            (then unreachable))
                        (i32.store (i32.const 65536) (i32.const 123))
                        (call $copy (local.get $job) (i32.const 65536) (i32.const 16))
                        (if (i32.ne (i32.load (i32.const 65536)) (i32.const 123))
                            (then unreachable))
                        (call $free (local.get $job))))"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        engine.run(&module, moonrun::RunOptions::default()).unwrap(),
        moonrun::RunOutcome::Completed
    );
}

#[test]
fn async_resource_spawn_requires_reserved_absence_sentinel() {
    #[cfg(unix)]
    let (imports, inputs, flags) = (
        r#"(import "moonbitlang/async" "process/make_argv_array/unix"
                (func $argv (param i32) (result i64)))
            (import "moonbitlang/async" "thread_pool/spawn_job/unix"
                (func $spawn (param i32 i32 i64 i64 i64 i64 i64) (result i64)))"#,
        "i32.const 0 call $argv i32.const 0 call $env",
        "",
    );
    #[cfg(windows)]
    let (imports, inputs, flags) = (
        r#"(import "moonbitlang/async" "thread_pool/spawn_job/windows"
                (func $spawn (param i32 i32 i64 i64 i64 i64 i32) (result i64)))"#,
        "i32.const 0 call $env",
        "i32.const 0",
    );
    let engine = moonrun::Engine::default();
    // Construct and free the job without running it: only handle decoding is tested.
    for zero_stream in [None, Some(0), Some(1), Some(2)] {
        let stdio = (0..3)
            .map(|index| {
                if zero_stream == Some(index) {
                    "i64.const 0"
                } else {
                    "call $invalid"
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let module = engine
            .compile(
                "spawn-stdio-handles.wasm",
                wat::parse_str(format!(
                    r#"(module
                        {imports}
                        (import "moonbitlang/async" "process/make_env"
                            (func $env (param i32) (result i64)))
                        (import "moonbitlang/async" "fd_util/invalid_fd"
                            (func $invalid (result i64)))
                        (import "moonbitlang/async" "event_bus/create"
                            (func $poll (result i64)))
                        (import "moonbitlang/async" "thread_pool/init_thread_pool"
                            (func $init (param i64) (result i64)))
                        (import "moonbitlang/async" "thread_pool/free_job"
                            (func $free (param i64)))
                        (memory (export "memory") 1)
                        (func (export "_start")
                            call $poll call $init drop
                            i32.const 0 i32.const 0 {inputs} {stdio} {flags}
                            call $spawn call $free))"#
                ))
                .unwrap(),
            )
            .unwrap();
        let result = engine.run(&module, moonrun::RunOptions::default());
        if zero_stream.is_some() {
            let error = result.unwrap_err();
            assert!(format!("{error:#}").contains("Badf"), "{error:#}");
        } else {
            assert_eq!(result.unwrap(), moonrun::RunOutcome::Completed);
        }
    }
}

#[cfg(unix)]
#[test]
fn async_resource_process_result_rejects_zero_handle() {
    let engine = moonrun::Engine::default();
    let module = engine
        .compile(
            "process-result-handles.wasm",
            wat::parse_str(format!(
                r#"(module
                    (import "moonbitlang/async" "process/get_process_result"
                        (func $result (param i64 i32 i32) (result i32)))
                    (import "moonbitlang/async" "fd_util/invalid_fd"
                        (func $invalid (result i64)))
                    (import "moonbitlang/async" "os_error/get_errno"
                        (func $errno (result i32)))
                    (memory (export "memory") 1)
                    (func (export "_start")
                        (if (i32.ne (call $result (i64.const 0) (i32.const {pid}) (i32.const 0))
                                    (i32.const -1)) (then unreachable))
                        (if (i32.ne (call $errno) (i32.const {badf})) (then unreachable))
                        (if (i32.ne (call $result (call $invalid) (i32.const {pid}) (i32.const 0))
                                    (i32.const -1)) (then unreachable))
                        (if (i32.ne (call $errno) (i32.const {child})) (then unreachable))))"#,
                // The host process is not its own child; the sentinel selects waitpid.
                pid = std::process::id(),
                badf = libc::EBADF,
                child = libc::ECHILD,
            ))
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        engine.run(&module, moonrun::RunOptions::default()).unwrap(),
        moonrun::RunOutcome::Completed
    );
}

#[test]
fn current_exe_imports_report_missing_origin_to_guest() {
    let engine = moonrun::Engine::default();
    for import in ["env/current_exe", "env/current_exe_dir"] {
        let module = engine
            .compile(
                "diagnostic-name.wasm",
                wat::parse_str(format!(
                    r#"(module
                        (import "moonbitlang/core" "{import}"
                            (func $path (result i64)))
                        (import "moonbitlang/async" "os_error/get_errno"
                            (func $errno (result i32)))
                        (import "moonbitlang/sqlite" "sqlite3_null_handle"
                            (func $null (result i64)))
                        (import "moonbitlang/async" "c_buffer/is_null"
                            (func $is_null (param i64) (result i32)))
                        (memory (export "memory") 1)
                        (func (export "_start") (local $buffer i64)
                            (if (call $errno) (then unreachable))
                            (local.set $buffer (call $path))
                            (if (i64.ne (local.get $buffer) (call $null)) (then unreachable))
                            (if (i32.eqz (call $is_null (local.get $buffer))) (then unreachable))
                            (if (i32.eqz (call $errno)) (then unreachable))))"#
                ))
                .unwrap(),
            )
            .unwrap();
        assert_eq!(
            engine.run(&module, moonrun::RunOptions::default()).unwrap(),
            moonrun::RunOutcome::Completed,
            "{import} must return a null buffer and set errno"
        );
    }
}

#[test]
fn nullable_imports_use_runtime_null_handle() {
    let engine = moonrun::Engine::default();
    let module = engine
        .compile(
            "nullable-handles.wasm",
            wat::parse_str(
                r#"(module
                    (import "moonbitlang/sqlite" "sqlite3_null_handle" (func $null (result i64)))
                    (import "moonbitlang/async" "c_buffer/is_null" (func $buffer_null (param i64) (result i32)))
                    (import "moonbitlang/async" "c_buffer/free" (func $free_buffer (param i64)))
                    (import "moonbitlang/async" "socket/if_indextoname" (func $ifname (param i32) (result i64)))
                    (import "moonbitlang/async" "socket/addrinfo_is_null" (func $addrinfo_null (param i64) (result i32)))
                    (import "moonbitlang/async" "socket/addrinfo_get_next" (func $next (param i64) (result i64)))
                    (import "moonbitlang/async" "socket/addrinfo_addr_size" (func $size (param i64) (result i32)))
                    (import "moonbitlang/async" "socket/addrinfo_free" (func $free_addrinfo (param i64)))
                    (import "moonbitlang/async" "tls/connection/free" (func $free_tls (param i64)))
                    (memory (export "memory") 1)
                    (func (export "_start")
                        (if (i32.eqz (call $buffer_null (call $null))) (then unreachable))
                        (if (i32.eqz (call $addrinfo_null (call $null))) (then unreachable))
                        (if (call $buffer_null (i64.const 0)) (then unreachable))
                        (if (call $addrinfo_null (i64.const 0)) (then unreachable))
                        (if (i64.ne (call $ifname (i32.const 0)) (call $null)) (then unreachable))
                        (if (i64.ne (call $next (call $null)) (call $null)) (then unreachable))
                        (if (call $size (call $null)) (then unreachable))
                        (call $free_buffer (call $null))
                        (call $free_addrinfo (call $null))
                        (call $free_tls (call $null))))"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        engine.run(&module, moonrun::RunOptions::default()).unwrap(),
        moonrun::RunOutcome::Completed
    );
}

#[test]
fn tls_free_rejects_zero_handle() {
    let engine = moonrun::Engine::default();
    let module = engine
        .compile(
            "invalid-tls-handle.wasm",
            wat::parse_str(
                r#"(module
                    (import "moonbitlang/async" "tls/connection/free" (func $free (param i64)))
                    (memory (export "memory") 1)
                    (func (export "_start") (call $free (i64.const 0))))"#,
            )
            .unwrap(),
        )
        .unwrap();
    let error = engine
        .run(&module, moonrun::RunOptions::default())
        .unwrap_err();
    assert!(format!("{error:#}").contains("Badf"), "{error:#}");
}

#[test]
fn engines_reuse_worker_for_async_and_sqlite_jobs() {
    let wasm = tempfile::Builder::new()
        .prefix("worker-completion-pipe.")
        .suffix(".wasm")
        .tempfile()
        .unwrap();
    std::fs::write(
        wasm.path(),
        wat::parse_str(
            r#"(module
                (import "moonbitlang/async" "fd_util/pipe"
                    (func $pipe (param i32 i32 i32 i32) (result i32)))
                (import "moonbitlang/async" "fd_util/close_fd"
                    (func $close (param i64) (result i32)))
                (import "moonbitlang/async" "thread_pool/make_sleep_job"
                    (func $sleep_job (param i32) (result i64)))
                (import "moonbitlang/async" "thread_pool/make_read_job"
                    (func $read_job (param i64 i32 i64) (result i64)))
                (import "moonbitlang/async" "thread_pool/run_job"
                    (func $run (param i64)))
                (import "moonbitlang/async" "thread_pool/get_read_result"
                    (func $read_result (param i64 i32 i32 i32)))
                (import "moonbitlang/async" "thread_pool/job_get_ret"
                    (func $ret (param i64) (result i32)))
                (import "moonbitlang/async" "thread_pool/job_get_err"
                    (func $err (param i64) (result i32)))
                (import "moonbitlang/async" "thread_pool/free_job"
                    (func $free_job (param i64)))
                (import "moonbitlang/async" "thread_pool/spawn_worker_with_pipe"
                    (func $spawn (param i32 i64 i64) (result i64)))
                (import "moonbitlang/async" "thread_pool/wake_worker"
                    (func $wake (param i64 i32 i64)))
                (import "moonbitlang/async" "thread_pool/free_worker"
                    (func $free_worker (param i64)))
                (import "moonbitlang/sqlite" "sqlite3_make_open_job"
                    (func $open (param i32 i32 i32) (result i64)))
                (import "moonbitlang/sqlite" "sqlite3_take_job_handle"
                    (func $take (param i64) (result i64)))
                (import "moonbitlang/sqlite" "sqlite3_close"
                    (func $close_db (param i64) (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 64) ":memory:")
                (func $receive (param $expected i32) (param $job i64)
                    (local $read i64)
                    (local.set $read (call $read_job (i64.load (i32.const 0))
                        (i32.const 4) (i64.const -1)))
                    (call $run (local.get $read))
                    (if (i32.ne (call $ret (local.get $read)) (i32.const 4))
                        (then unreachable))
                    (call $read_result (local.get $read) (i32.const 16)
                        (i32.const 0) (i32.const 4))
                    (if (i32.ne (i32.load (i32.const 16)) (local.get $expected))
                        (then unreachable))
                    ;; No event bus or private async completion drain was used.
                    (if (call $err (local.get $job)) (then unreachable))
                    (if (call $ret (local.get $job)) (then unreachable))
                    (call $free_job (local.get $read)))
                (func (export "_start")
                    (local $worker i64)
                    (local $job i64)
                    ;; Blocking reader, async writer (also exercises Windows OVERLAPPED).
                    (if (call $pipe (i32.const 0) (i32.const 2)
                        (i32.const 0) (i32.const 1)) (then unreachable))
                    (local.set $job (call $sleep_job (i32.const 0)))
                    (local.set $worker (call $spawn (i32.const 17)
                        (local.get $job) (i64.load (i32.const 8))))
                    ;; Closing guest reachability must not close the Worker's writer.
                    (if (call $close (i64.load (i32.const 8))) (then unreachable))
                    (call $receive (i32.const 17) (local.get $job))
                    (call $free_job (local.get $job))
                    ;; Reuse the same Worker for a different Host Domain's Job.
                    (local.set $job (call $open (i32.const 64) (i32.const 8) (i32.const 6)))
                    (call $wake (local.get $worker) (i32.const 23) (local.get $job))
                    (call $receive (i32.const 23) (local.get $job))
                    (if (call $close_db (call $take (local.get $job))) (then unreachable))
                    (call $free_job (local.get $job))
                    (call $free_worker (local.get $worker))
                    ;; The last writer is released when the Worker is freed.
                    (local.set $job (call $read_job (i64.load (i32.const 0))
                        (i32.const 1) (i64.const -1)))
                    (call $run (local.get $job))
                    (if (call $ret (local.get $job)) (then unreachable))
                    (call $free_job (local.get $job))
                    (if (call $close (i64.load (i32.const 0))) (then unreachable))))"#,
        )
        .unwrap(),
    )
    .unwrap();

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .arg(wasm.path())
        .env("MOONBIT_ASYNC_CHECK_FD_LEAK", "1")
        .timeout(std::time::Duration::from_secs(10))
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");
}

#[test]
fn async_imports_without_memory_work_during_instantiation_and_execution() {
    let engine = moonrun::Engine::default();
    let module = engine
        .compile(
            "async-no-memory.wasm",
            wat::parse_str(
                r#"(module
        (import "moonbitlang/async" "runtime/get_platform" (func $platform (result i32)))
        (import "moonbitlang/async" "c_buffer/new" (func $new (param i32) (result i64)))
        (import "moonbitlang/async" "c_buffer/free" (func $free (param i64)))
        (import "moonbitlang/async" "stdio/get_stdio_handle" (func $stdio (param i32) (result i64)))
        (import "moonbitlang/async" "stdio/kind_of_fd" (func $kind (param i64) (result i32)))
        (func $initialize
            call $platform drop i32.const 8 call $new call $free
            i32.const 1 call $stdio call $kind i32.const 0 i32.lt_s if unreachable end)
        (start $initialize)
        (func (export "_start") call $initialize))"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        engine.run(&module, moonrun::RunOptions::default()).unwrap(),
        moonrun::RunOutcome::Completed
    );
}

#[test]
fn async_termination_interrupts_without_guest_memory() {
    let engine = moonrun::Engine::default();
    for (import, outcome) in [
        ("runtime/exit", moonrun::RunOutcome::Exited(7)),
        (
            "runtime/terminate_process_by_signal",
            moonrun::RunOutcome::KilledBySignal(7),
        ),
    ] {
        for start in ["(start $run)", "(export \"_start\" (func $run))"] {
            let module = engine
                .compile(
                    "async-termination.wasm",
                    wat::parse_str(format!(
                        r#"(module
                (import "moonbitlang/async" "{import}" (func $terminate (param i32)))
                (func $run i32.const 7 call $terminate unreachable)
                {start})"#
                    ))
                    .unwrap(),
                )
                .unwrap();
            assert_eq!(
                engine.run(&module, moonrun::RunOptions::default()).unwrap(),
                outcome,
                "{import}: {start}"
            );
        }
    }
}

#[test]
fn async_memory_is_reacquired_after_growth() {
    let engine = moonrun::Engine::default();
    let module = engine
        .compile(
            "async-memory-growth.wasm",
            wat::parse_str(
                r#"(module
        (import "moonbitlang/async" "c_buffer/new" (func $new (param i32) (result i64)))
        (import "moonbitlang/async" "c_buffer/free" (func $free (param i64)))
        (import "moonbitlang/async" "c_buffer/blit_to_c" (func $to (param i64 i32 i32 i32 i32)))
        (import "moonbitlang/async" "c_buffer/blit_from_c" (func $from (param i64 i32 i32 i32 i32)))
        (memory (export "memory") 1)
        (func (export "_start") (local $buffer i64)
            i32.const 1 call $new local.set $buffer
            i32.const 0 i32.const 42 i32.store8
            local.get $buffer i32.const 0 i32.const 0 i32.const 0 i32.const 1 call $to
            i32.const 1 memory.grow i32.const 1 i32.ne if unreachable end
            local.get $buffer i32.const 0 i32.const 65536 i32.const 0 i32.const 1 call $from
            i32.const 65536 i32.load8_u i32.const 42 i32.ne if unreachable end
            i32.const 65536 i32.const 99 i32.store8
            local.get $buffer i32.const 0 i32.const 65536 i32.const 0 i32.const 1 call $to
            local.get $buffer i32.const 0 i32.const 0 i32.const 0 i32.const 1 call $from
            i32.const 0 i32.load8_u i32.const 99 i32.ne if unreachable end
            local.get $buffer call $free))"#,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        engine.run(&module, moonrun::RunOptions::default()).unwrap(),
        moonrun::RunOutcome::Completed
    );
}

#[test]
fn async_memory_access_rejects_missing_memory_and_invalid_ranges() {
    let engine = moonrun::Engine::default();
    for memory in ["", "(memory (export \"memory\") 1)"] {
        let module = engine.compile("async-invalid-memory.wasm", wat::parse_str(format!(r#"(module
            (import "moonbitlang/async" "c_buffer/new" (func $new (param i32) (result i64)))
            (import "moonbitlang/async" "c_buffer/blit_to_c" (func $to (param i64 i32 i32 i32 i32)))
            {memory}
            (func (export "_start")
                i32.const 1 call $new i32.const 0 i32.const 65536 i32.const 0 i32.const 1 call $to))"#)).unwrap()).unwrap();
        let error = engine
            .run(&module, moonrun::RunOptions::default())
            .unwrap_err();
        assert!(format!("{error:#}").contains("Fault"), "{error:#}");
    }
}
