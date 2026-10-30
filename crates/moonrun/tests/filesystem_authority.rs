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

use moonrun::{Engine, RunOptions, RunOutcome, WorkingDirectory};

#[test]
fn filesystem_policy_applies_to_legacy_async_and_wasi_imports() {
    let engine = Engine::default();
    for (policy, read_allowed, write_allowed) in [
        (None, true, true),
        (Some(""), false, false),
        (Some("[fs]\nread = [\".\"]\n"), true, false),
        (Some("[fs]\nread = [\".\"]\nwrite = [\".\"]\n"), true, true),
    ] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("file"), b"keep").unwrap();
        let module = engine.compile("filesystem-authority.wasm", wat::parse_str(format!(r#"(module
            (import "_" "file" (global $file (ref extern)))
            (import "_" "legacy" (global $directory (ref extern)))
            (import "__moonbit_fs_unstable" "read_file_to_bytes_new" (func $legacy_read (param externref) (result i32)))
            (import "__moonbit_fs_unstable" "create_dir" (func $legacy_mkdir (param externref)))
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
            options = options.with_policy_file(path);
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
