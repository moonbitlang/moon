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

//! The same Runtime restrictions must apply across filesystem import families.

#[cfg(feature = "v8")]
#[path = "common/test_init.rs"]
mod test_init;

use moonrun::{Engine, RunOptions, RunOutcome, WorkingDirectory};

#[test]
fn filesystem_policy_applies_to_legacy_async_and_wasi_imports() {
    let engine = Engine::default();
    for (policy, read_allowed, write_allowed) in [
        (None, true, true),
        (Some(""), false, false),
        (Some("[fs]\nread = [\".\"]\n"), true, false),
        (Some("[fs]\nread = [\".\"]\nwrite = [\".\"]\n"), true, true),
        (
            Some("[fs]\nread = [\".\", { glob = \"!file\" }]\nwrite = [\".\"]\n"),
            false,
            true,
        ),
        (
            Some("[fs]\nread = [\".\"]\nwrite = [\".\", { glob = \"!{file,async,wasi}\" }]\n"),
            true,
            false,
        ),
        (Some("[fs]\nread = [{ glob = \"f*\" }]\n"), true, false),
        (
            Some("[fs]\nread = [{ glob = \"*.txt\" }]\nwrite = [{ glob = \"*.txt\" }]\n"),
            false,
            false,
        ),
        (
            Some(
                "[fs]\nread = [{ glob = \"file\" }]\nwrite = [{ glob = \"{file,async,wasi}\" }]\n",
            ),
            true,
            true,
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("file"), b"keep").unwrap();
        let module = engine.compile("filesystem-authority.wasm", wat::parse_str(format!(r#"(module
            (import "_" "file" (global $file (ref extern)))
            (import "__moonbit_fs_unstable" "read_file_to_bytes_new" (func $legacy_read (param externref) (result i32)))
            (import "__moonbit_fs_unstable" "remove_file_new" (func $legacy_remove (param externref) (result i32)))
            (import "moonbitlang/async" "thread_pool/make_open_job" (func $async_open (param i32 i32 i32 i32 i32 i32 i32) (result i64)))
            (import "moonbitlang/async" "thread_pool/open_job_get_fd" (func $async_fd (param i64) (result i64)))
            (import "moonbitlang/async" "fd_util/close_fd" (func $async_close (param i64) (result i32)))
            (import "moonbitlang/async" "thread_pool/make_mkdir_job" (func $async_mkdir (param i32 i32 i32) (result i64)))
            (import "moonbitlang/async" "thread_pool/run_job" (func $run (param i64)))
            (import "moonbitlang/async" "thread_pool/job_get_err" (func $err (param i64) (result i32)))
            (import "moonbitlang/async" "thread_pool/free_job" (func $free (param i64)))
            (import "wasi_snapshot_preview1" "path_open" (func $wasi_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_close" (func $wasi_close (param i32) (result i32)))
            (import "wasi_snapshot_preview1" "path_create_directory" (func $wasi_mkdir (param i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 0) "filewasi")
            (data (i32.const 32) "f\00i\00l\00e\00a\00s\00y\00n\00c\00")
            (func (export "_start") (local $job i64)
                ;; Legacy status results, native errno, and WASI errno remain distinct.
                global.get $file call $legacy_read i32.const {legacy_read} i32.ne if unreachable end
                i32.const 32 i32.const 4 i32.const 0 i32.const 0 i32.const 0 i32.const 0 i32.const 0
                call $async_open local.set $job
                local.get $job call $run
                local.get $job call $err i32.const {async_read} i32.ne if unreachable end
                i32.const {read_allowed} if
                    local.get $job call $async_fd call $async_close if unreachable end
                end
                local.get $job call $free
                i32.const 3 i32.const 0 i32.const 0 i32.const 4 i32.const 0 i64.const 2 i64.const 0 i32.const 0 i32.const 128
                call $wasi_open i32.const {wasi_read} i32.ne if unreachable end
                i32.const {read_allowed} if
                    i32.const 128 i32.load call $wasi_close if unreachable end
                end
                i32.const 40 i32.const 5 i32.const 448 call $async_mkdir local.set $job
                local.get $job call $run
                local.get $job call $err i32.const {async_write} i32.ne if unreachable end
                local.get $job call $free
                i32.const 3 i32.const 4 i32.const 4 call $wasi_mkdir
                i32.const {wasi_write} i32.ne if unreachable end
                global.get $file call $legacy_remove i32.const {legacy_write} i32.ne if unreachable end))"#,
            legacy_read = if read_allowed { 0 } else { -1 },
            legacy_write = if write_allowed { 0 } else { -1 },
            async_read = if read_allowed { 0 } else { access_errno() },
            async_write = if write_allowed { 0 } else { access_errno() },
            wasi_read = if read_allowed { 0 } else { 2 },
            wasi_write = if write_allowed { 0 } else { 2 },
            read_allowed = i32::from(read_allowed),
        )).unwrap()).unwrap();
        let mut options = RunOptions::default()
            .with_working_directory(WorkingDirectory::Fixed(directory.path().to_owned()));
        if let Some(policy) = policy {
            let path = directory.path().join("policy.toml");
            std::fs::write(&path, policy).unwrap();
            options = options.with_policy_file(path).unwrap();
        }
        assert_eq!(
            engine.run(&module, options).unwrap(),
            RunOutcome::Completed,
            "policy={policy:?}"
        );
        assert_eq!(directory.path().join("async").is_dir(), write_allowed);
        assert_eq!(directory.path().join("wasi").is_dir(), write_allowed);
        assert_eq!(directory.path().join("file").exists(), !write_allowed);
    }
}

fn access_errno() -> i32 {
    #[cfg(unix)]
    {
        libc::EACCES
    }
    #[cfg(windows)]
    {
        windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED as i32
    }
}

#[test]
fn bound_stdio_is_an_explicit_grant_even_when_paths_are_denied() {
    let engine = Engine::default();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input");
    let output = directory.path().join("output");
    let error = directory.path().join("error");
    let policy = directory.path().join("policy.json");
    std::fs::write(&input, b"abc").unwrap();
    std::fs::write(&policy, "{}").unwrap();
    let module = engine.compile("stdio-grant.wasm", wat::parse_str(format!(r#"(module
        (import "__moonbit_io_unstable" "read_char" (func $char_read (result i32)))
        (import "__moonbit_io_unstable" "write_char" (func $char_write (param i32 i32)))
        (import "moonbitlang/async" "stdio/get_stdio_handle" (func $stdio (param i32) (result i64)))
        (import "moonbitlang/async" "stdio/kind_of_fd" (func $kind (param i64) (result i32)))
        (import "moonbitlang/async" "thread_pool/make_fstatx_job" (func $fstatx (param i64 i32 i32) (result i64)))
        (import "moonbitlang/async" "thread_pool/get_stat_result" (func $stat_result (param i64 i32 i32)))
        (import "moonbitlang/async" "fd_util/pipe" (func $pipe (param i32 i32 i32 i32) (result i32)))
        (import "moonbitlang/async" "thread_pool/make_read_job" (func $async_read (param i64 i32 i64) (result i64)))
        (import "moonbitlang/async" "thread_pool/get_read_result" (func $read_result (param i64 i32 i32 i32)))
        (import "moonbitlang/async" "thread_pool/make_write_job" (func $async_write (param i64 i32 i32 i32 i64) (result i64)))
        (import "moonbitlang/async" "thread_pool/run_job" (func $run (param i64)))
        (import "moonbitlang/async" "thread_pool/job_get_err" (func $err (param i64) (result i32)))
        (import "moonbitlang/async" "thread_pool/job_get_ret" (func $ret (param i64) (result i32)))
        (import "moonbitlang/async" "thread_pool/free_job" (func $free (param i64)))
        (import "moonbitlang/async" "fd_util/close_fd" (func $close (param i64) (result i32)))
        (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (global $FILE_KIND i32 (i32.const 0x0001))
        (global $FILE_SIZE i32 (i32.const 0x0002))
        (global $REGULAR_FILE i64 (i64.const 1))
        (global $PIPE i64 (i64.const 5))
        (global $ACCESS_DENIED i32 (i32.const {denied}))
        (data (i32.const 64) "input")
        (func $check_kind (param $fd i64) (param $expected i64) (local $job i64)
            ;; The scalar and generic APIs must classify the same acquired resource.
            local.get $fd call $kind i64.extend_i32_s local.get $expected i64.ne if unreachable end
            local.get $fd global.get $FILE_KIND i32.const 16 call $fstatx local.set $job
            local.get $job call $run
            local.get $job call $err if unreachable end
            local.get $job i32.const 128 i32.const 16 call $stat_result
            i32.const 128 i32.load i32.const 16 i32.ne if unreachable end
            i32.const 132 i32.load global.get $FILE_KIND i32.ne if unreachable end
            i32.const 136 i64.load local.get $expected i64.ne if unreachable end
            local.get $job call $free)
        (func $check_metadata_denied (param $fd i64) (param $mask i32) (local $job i64)
            local.get $fd local.get $mask i32.const 24 call $fstatx local.set $job
            local.get $job call $run
            local.get $job call $err global.get $ACCESS_DENIED i32.ne if unreachable end
            local.get $job call $free)
        (func $check_stdio (param $id i32) (local $fd i64)
            local.get $id call $stdio local.set $fd
            local.get $fd global.get $REGULAR_FILE call $check_kind)
        (func (export "_start") (local $job i64)
            ;; Core initializes all three streams through a synchronous fstatx job.
            i32.const 0 call $check_stdio
            i32.const 1 call $check_stdio
            i32.const 2 call $check_stdio

            ;; Acquired anonymous resources have the same classification grant.
            i32.const 256 i32.const 2 i32.const 0 i32.const 0 call $pipe if unreachable end
            i32.const 256 i64.load global.get $PIPE call $check_kind
            i32.const 264 i64.load global.get $PIPE call $check_kind
            i32.const 256 i64.load global.get $FILE_SIZE call $check_metadata_denied
            i32.const 256 i64.load call $close if unreachable end
            i32.const 264 i64.load call $close if unreachable end

            ;; Each ABI consumes the next byte from the same granted file.
            call $char_read i32.const 97 i32.ne if unreachable end
            i32.const 1 i32.const 97 call $char_write
            i32.const 0 i32.const 32 i32.store
            i32.const 4 i32.const 1 i32.store
            i32.const 0 i32.const 0 i32.const 1 i32.const 8 call $read if unreachable end
            i32.const 8 i32.load i32.const 1 i32.ne if unreachable end
            i32.const 32 i32.load8_u i32.const 98 i32.ne if unreachable end
            i32.const 1 i32.const 0 i32.const 1 i32.const 8 call $write if unreachable end
            i32.const 8 i32.load i32.const 1 i32.ne if unreachable end
            i32.const 0 call $stdio i32.const 1 i64.const -1 call $async_read local.set $job
            local.get $job call $run
            local.get $job call $err if unreachable end
            local.get $job call $ret i32.const 1 i32.ne if unreachable end
            local.get $job i32.const 32 i32.const 0 i32.const 1 call $read_result
            i32.const 32 i32.load8_u i32.const 99 i32.ne if unreachable end
            local.get $job call $free
            i32.const 1 call $stdio i32.const 32 i32.const 0 i32.const 1 i64.const -1
            call $async_write local.set $job
            local.get $job call $run
            local.get $job call $err if unreachable end
            local.get $job call $ret i32.const 1 i32.ne if unreachable end
            local.get $job call $free

            ;; A handle grant does not authorize reopening its path.
            i32.const 3 i32.const 0 i32.const 64 i32.const 5 i32.const 0
            i64.const 2 i64.const 0 i32.const 0 i32.const 16 call $open
            i32.const 2 i32.ne if unreachable end

            ;; Async exposes the OS access mode, even without WASI rights checks.
            i32.const 0 call $stdio i32.const 32 i32.const 0 i32.const 1 i64.const -1
            call $async_write local.set $job
            local.get $job call $run
            local.get $job call $err i32.eqz if unreachable end
            local.get $job call $free
            i32.const 1 call $stdio i32.const 1 i64.const -1 call $async_read local.set $job
            local.get $job call $run
            local.get $job call $err i32.eqz if unreachable end
            local.get $job call $free

            ;; Reserved stdio cannot be closed through the async Handle table.
            i32.const 1 call $stdio call $close i32.eqz if unreachable end
            i32.const 1 i32.const 100 call $char_write
            i32.const 2 i32.const 101 call $char_write))"#, denied = access_errno())).unwrap()).unwrap();
    let mut parent: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/deny_restrictions.json")).unwrap();
    for access in ["read", "write"] {
        parent["rule_groups"][0]["fs"][access] = serde_json::json!([{"kind":"any"}]);
    }
    // The parent can open these files; the child's complete policy denies paths.
    let options = RunOptions::default()
        .with_inherited_policy(serde_json::to_vec(&parent).unwrap())
        .with_policy_file(policy)
        .unwrap()
        .with_working_directory(WorkingDirectory::Fixed(directory.path().to_owned()))
        .with_stdio(
            std::fs::File::open(&input).unwrap(),
            std::fs::File::create(&output).unwrap(),
            std::fs::File::create(&error).unwrap(),
        );
    assert_eq!(engine.run(&module, options).unwrap(), RunOutcome::Completed);
    assert_eq!(std::fs::read(input).unwrap(), b"abc");
    assert_eq!(std::fs::read(output).unwrap(), b"abcd");
    assert_eq!(std::fs::read(error).unwrap(), b"e");
}
