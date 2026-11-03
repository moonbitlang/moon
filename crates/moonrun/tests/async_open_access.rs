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

use moonrun::{Engine, RunOptions, RunOutcome, WorkingDirectory};

#[test]
fn append_preserves_access_mode_when_guest_calls_write_directly() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("file");
    let engine = Engine::default();
    for (constructor, extra_params, stat_args) in [
        ("make_open_job", "", ""),
        (
            "make_open_stat_job",
            "i32 i32",
            "(i32.const 0) (i32.const 8)",
        ),
    ] {
        for access in 0..=2 {
            let expected_error = if access == 0 { "i32.eqz" } else { "" };
            // Call the write ABI directly: the host handle must enforce access
            // even when the guest bypasses MoonBit's ReadOnly wrapper.
            let wasm = wat::parse_str(format!(
                r#"(module
                    (import "moonbitlang/async" "thread_pool/{constructor}"
                        (func $open (param i32 i32 i32 i32 i32 i32 i32 {extra_params}) (result i64)))
                    (import "moonbitlang/async" "thread_pool/open_job_get_fd" (func $fd (param i64) (result i64)))
                    (import "moonbitlang/async" "thread_pool/make_write_job" (func $write (param i64 i32 i32 i32 i64) (result i64)))
                    (import "moonbitlang/async" "thread_pool/run_job" (func $run (param i64)))
                    (import "moonbitlang/async" "thread_pool/job_get_err" (func $error (param i64) (result i32)))
                    (import "moonbitlang/async" "thread_pool/free_job" (func $free (param i64)))
                    (import "moonbitlang/async" "fd_util/close_fd" (func $close (param i64) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "f\00i\00l\00e\00")
                    (func (export "_start") (local $job i64) (local $file i64)
                        (local.set $job (call $open (i32.const 0) (i32.const 4)
                            (i32.const {access}) (i32.const 0) (i32.const 1)
                            (i32.const 0) (i32.const 0) {stat_args}))
                        (call $run (local.get $job))
                        (if (call $error (local.get $job)) (then unreachable))
                        (local.set $file (call $fd (local.get $job)))
                        (call $free (local.get $job))
                        (local.set $job (call $write (local.get $file)
                            (i32.const 0) (i32.const 0) (i32.const 1) (i64.const -1)))
                        (call $run (local.get $job))
                        (call $error (local.get $job)) {expected_error} if unreachable end
                        (call $free (local.get $job))
                        (if (call $close (local.get $file)) (then unreachable))))"#,
            ))
            .unwrap();
            let guest = engine.compile("append-access.wasm", wasm).unwrap();
            let access_policy = match access {
                0 => "[fs]\nread = [\"file\"]\n",
                1 => "[fs]\nwrite = [\"file\"]\n",
                _ => "[fs]\nread = [\"file\"]\nwrite = [\"file\"]\n",
            };
            for policy in [None, Some(access_policy)] {
                std::fs::write(&path, b"keep").unwrap();
                let mut options = RunOptions::default()
                    .with_working_directory(WorkingDirectory::Fixed(root.path().to_owned()));
                if let Some(policy) = policy {
                    let policy_path = root.path().join("policy.toml");
                    std::fs::write(&policy_path, policy).unwrap();
                    options = options.with_policy_file(policy_path).unwrap();
                }
                assert_eq!(
                    engine.run(&guest, options).unwrap(),
                    RunOutcome::Completed,
                    "{constructor}, access={access}, policy={policy:?}",
                );
                let expected: &[u8] = if access == 0 { b"keep" } else { b"keepf" };
                assert_eq!(std::fs::read(&path).unwrap(), expected);
            }
        }
    }
}
