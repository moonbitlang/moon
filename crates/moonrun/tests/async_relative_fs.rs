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

#[cfg(feature = "v8")]
#[path = "common/test_init.rs"]
mod test_init;

use moonrun::{Engine, Module, RunOptions, RunOutcome, WorkingDirectory};

fn module(engine: &Engine, body: &str) -> Module {
    let data = [
        "original",
        "moved",
        "file",
        "empty",
        "original/file",
        "legacy",
        "../denied/file",
        "link",
        "file\0/../denied",
        "../denied/file\0/../../original/file",
        "\0original",
        "original/link",
        "original/old-link",
        "link/file",
        "../denied",
    ]
    .iter()
    .enumerate()
    .map(|(i, path)| {
        let bytes = path
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .map(|b| format!("\\{b:02x}"))
            .collect::<String>();
        format!("(data (i32.const {}) \"{bytes}\")", 128 * i)
    })
    .collect::<Vec<_>>()
    .join("\n");
    engine.compile("relative-fs.wasm", wat::parse_str(format!(r#"(module
        (import "moonbitlang/async" "fd_util/invalid_fd" (func $invalid (result i64)))
        (import "moonbitlang/async" "fd_util/close_fd" (func $close (param i64) (result i32)))
        (import "moonbitlang/async" "thread_pool/make_openat_job"
            (func $openat (param i64 i32 i32 i32 i32 i32 i32 i32 i32 i32) (result i64)))
        (import "moonbitlang/async" "thread_pool/make_open_job"
            (func $open_legacy (param i32 i32 i32 i32 i32 i32 i32) (result i64)))
        (import "moonbitlang/async" "thread_pool/make_open_stat_job"
            (func $open_stat (param i32 i32 i32 i32 i32 i32 i32 i32 i32) (result i64)))
        (import "moonbitlang/async" "thread_pool/open_job_get_fd" (func $fd (param i64) (result i64)))
        (import "moonbitlang/async" "thread_pool/make_removeat_job" (func $removeat (param i64 i32 i32) (result i64)))
        (import "moonbitlang/async" "thread_pool/make_remove_job" (func $remove (param i32 i32) (result i64)))
        (import "moonbitlang/async" "thread_pool/make_rename_job" (func $rename (param i32 i32 i32 i32 i32) (result i64)))
        (import "moonbitlang/async" "thread_pool/make_mkdir_job" (func $mkdir (param i32 i32 i32) (result i64)))
        (import "moonbitlang/async" "thread_pool/make_symlink_job" (func $symlink (param i32 i32 i32 i32 i32) (result i64)))
        (import "moonbitlang/async" "thread_pool/run_job" (func $run (param i64)))
        (import "moonbitlang/async" "thread_pool/job_get_err" (func $error (param i64) (result i32)))
        (import "moonbitlang/async" "thread_pool/free_job" (func $free (param i64)))
        (memory (export "memory") 1)
        {data}
        (func $finish (param $job i64)
            (call $run (local.get $job))
            (if (call $error (local.get $job)) (then unreachable))
            (call $free (local.get $job)))
        (func $deny (param $job i64)
            (call $run (local.get $job))
            (if (i32.eqz (call $error (local.get $job))) (then unreachable))
            (call $free (local.get $job)))
        (func $opened (param $job i64) (result i64) (local $file i64)
            (call $run (local.get $job))
            (if (call $error (local.get $job)) (then unreachable))
            (local.set $file (call $fd (local.get $job)))
            (call $free (local.get $job))
            (local.get $file))
        (func (export "_start") (local $parent i64) (local $open i64) (local $remove i64) (local $empty i64)
            {body}))"#)).unwrap()).unwrap()
}

#[test]
fn relative_jobs_retain_parent_after_close_and_rename() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("original")).unwrap();
    std::fs::create_dir(root.path().join("original/empty")).unwrap();
    std::fs::write(root.path().join("original/file"), b"original").unwrap();
    std::fs::write(root.path().join("legacy"), []).unwrap();
    let engine = Engine::default();
    let module = module(
        &engine,
        r#"
        (local.set $parent (call $opened (call $openat (call $invalid)
            (i32.const 0) (i32.const 8) (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 8))))
        (local.set $open (call $openat (local.get $parent)
            (i32.const 256) (i32.const 4) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 8)))
        (local.set $remove (call $removeat (local.get $parent) (i32.const 256) (i32.const 4)))
        (local.set $empty (call $removeat (local.get $parent) (i32.const 384) (i32.const 5)))
        (if (call $close (local.get $parent)) (then unreachable))
        (call $finish (call $rename (i32.const 0) (i32.const 8) (i32.const 128) (i32.const 5) (i32.const 0)))
        (call $finish (call $mkdir (i32.const 0) (i32.const 8) (i32.const 448)))
        (if (call $close (call $opened (call $openat (call $invalid)
            (i32.const 512) (i32.const 13) (i32.const 1) (i32.const 4) (i32.const 0) (i32.const 0) (i32.const 384) (i32.const 0) (i32.const 8)))) (then unreachable))
        (if (call $close (call $opened (local.get $open))) (then unreachable))
        (call $finish (local.get $remove))
        (call $finish (local.get $empty))
        (call $finish (call $remove (i32.const 640) (i32.const 6)))
    "#,
    );
    assert_eq!(
        engine
            .run(
                &module,
                RunOptions::default()
                    .with_working_directory(WorkingDirectory::Fixed(root.path().to_owned()))
            )
            .unwrap(),
        RunOutcome::Completed
    );
    assert!(!root.path().join("moved/file").exists());
    assert!(!root.path().join("moved/empty").exists());
    assert!(root.path().join("original/file").is_file());
    assert!(!root.path().join("legacy").exists());
}

#[test]
fn relative_job_constructors_reject_invalid_parent_and_memory() {
    let engine = Engine::default();
    for (args, error) in [
        ("(i64.const 0) (i32.const 0) (i32.const 8)", "Badf"),
        ("(call $invalid) (i32.const 65535) (i32.const 8)", "Fault"),
    ] {
        for operation in [
            format!("(call $removeat {args})"),
            format!(
                "(call $openat {args} (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 8))"
            ),
        ] {
            let module = module(&engine, &format!("(drop {operation})"));
            let result = engine.run(&module, RunOptions::default()).unwrap_err();
            assert!(format!("{result:#}").contains(error), "{result:#}");
        }
    }
}

#[test]
fn open_jobs_reject_invalid_stat_requests_without_filesystem_effects() {
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::default();
    #[cfg(unix)]
    let invalid_parameter = libc::EINVAL;
    #[cfg(windows)]
    let invalid_parameter = windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER as i32;
    // Both the existing ABI and the new relative ABI must return a failed Job,
    // without opening the file or trapping in the constructor.
    for constructor in ["$open_stat", "$openat (call $invalid)"] {
        for (mask, capacity) in [
            (0x0100, 8), // Unsupported property.
            (0, 7),      // Missing part of the header.
            (1, 8),      // Missing space for the requested file kind.
            (0, -1),     // Length exceeds the signed native ABI range.
        ] {
            for existing in [false, true] {
                let path = root.path().join("file");
                if existing {
                    std::fs::write(&path, b"keep").unwrap();
                }
                let module = module(
                    &engine,
                    &format!(
                        r#"
                    (local.set $open (call {constructor}
                        (i32.const 256) (i32.const 4) (i32.const 1) (i32.const 3)
                        (i32.const 0) (i32.const 0) (i32.const 384)
                        (i32.const {mask}) (i32.const {capacity})))
                    (call $run (local.get $open))
                    (if (i32.ne (call $error (local.get $open)) (i32.const {invalid_parameter})) (then unreachable))
                    (call $free (local.get $open))
                    "#
                    ),
                );
                assert_eq!(
                    engine
                        .run(
                            &module,
                            RunOptions::default().with_working_directory(WorkingDirectory::Fixed(
                                root.path().to_owned()
                            ))
                        )
                        .unwrap(),
                    RunOutcome::Completed
                );
                if existing {
                    assert_eq!(std::fs::read(&path).unwrap(), b"keep");
                    std::fs::remove_file(path).unwrap();
                } else {
                    assert!(
                        !path.try_exists().unwrap(),
                        "invalid stat request created a file"
                    );
                }
            }
        }
    }
}

#[test]
fn relative_jobs_use_nul_terminated_paths_before_policy_checks() {
    let root = tempfile::tempdir().unwrap();
    for name in ["original", "denied"] {
        std::fs::create_dir(root.path().join(name)).unwrap();
        std::fs::write(root.path().join(name).join("file"), b"keep").unwrap();
    }
    let policy = root.path().join("policy.toml");
    std::fs::write(
        &policy,
        "[fs]\nread = [\"original\"]\nwrite = [\"original\"]\n",
    )
    .unwrap();
    let engine = Engine::default();
    let guest = module(
        &engine,
        r#"
        (local.set $parent (call $opened (call $openat (call $invalid)
            (i32.const 0) (i32.const 8) (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 8))))
        (call $deny (call $openat (local.get $parent) (i32.const 1152) (i32.const 35)
            (i32.const 1) (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 384) (i32.const 0) (i32.const 8)))
        (call $deny (call $removeat (local.get $parent) (i32.const 1152) (i32.const 35)))
        (if (call $close (call $opened (call $openat (local.get $parent)
            (i32.const 1024) (i32.const 15) (i32.const 1) (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 384) (i32.const 0) (i32.const 8)))) (then unreachable))
        (call $finish (call $removeat (local.get $parent) (i32.const 1024) (i32.const 15)))
        (if (call $close (local.get $parent)) (then unreachable))
    "#,
    );
    assert_eq!(
        engine
            .run(
                &guest,
                RunOptions::default()
                    .with_policy_file(policy)
                    .unwrap()
                    .with_working_directory(WorkingDirectory::Fixed(root.path().to_owned()))
            )
            .unwrap(),
        RunOutcome::Completed
    );
    assert!(!root.path().join("original/file").exists());
    assert_eq!(
        std::fs::read(root.path().join("denied/file")).unwrap(),
        b"keep"
    );

    // A leading NUL is an empty filename, even when a fixed cwd is configured.
    let empty = module(
        &engine,
        r#"
        (call $deny (call $openat (call $invalid) (i32.const 1280) (i32.const 9)
            (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 8)))
    "#,
    );
    assert_eq!(
        engine
            .run(
                &empty,
                RunOptions::default()
                    .with_working_directory(WorkingDirectory::Fixed(root.path().to_owned()))
            )
            .unwrap(),
        RunOutcome::Completed
    );
}

#[cfg(unix)]
#[test]
fn relative_jobs_recheck_policy_after_guest_replaces_a_path() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("original/link")).unwrap();
    std::fs::create_dir(root.path().join("denied")).unwrap();
    std::fs::write(root.path().join("original/link/file"), b"allowed").unwrap();
    std::fs::write(root.path().join("denied/file"), b"keep").unwrap();
    let policy = root.path().join("policy.toml");
    std::fs::write(
        &policy,
        "[fs]\nread = [\"original\"]\nwrite = [\"original\"]\n",
    )
    .unwrap();
    let engine = Engine::default();
    // Construct both Jobs before changing the namespace entirely through guest
    // imports. Construction must not cache authority for their later execution.
    let guest = module(
        &engine,
        r#"
        (local.set $parent (call $opened (call $openat (call $invalid)
            (i32.const 0) (i32.const 8) (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 8))))
        (local.set $open (call $openat (local.get $parent) (i32.const 1664) (i32.const 9)
            (i32.const 1) (i32.const 1) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 8)))
        (local.set $remove (call $removeat (local.get $parent) (i32.const 1664) (i32.const 9)))
        (if (call $close (local.get $parent)) (then unreachable))
        (call $finish (call $rename (i32.const 1408) (i32.const 13) (i32.const 1536) (i32.const 17) (i32.const 0)))
        (call $finish (call $symlink (i32.const 1792) (i32.const 9) (i32.const 1408) (i32.const 13) (i32.const 1)))
        (call $deny (local.get $open))
        (call $deny (local.get $remove))
        "#,
    );
    assert_eq!(
        engine
            .run(
                &guest,
                RunOptions::default()
                    .with_policy_file(policy)
                    .unwrap()
                    .with_working_directory(WorkingDirectory::Fixed(root.path().to_owned()))
            )
            .unwrap(),
        RunOutcome::Completed
    );
    assert_eq!(
        std::fs::read(root.path().join("denied/file")).unwrap(),
        b"keep"
    );
    assert_eq!(
        std::fs::read(root.path().join("original/old-link/file")).unwrap(),
        b"allowed"
    );
}

#[cfg(unix)]
#[test]
fn relative_jobs_check_entry_and_target_permissions() {
    let root = tempfile::tempdir().unwrap();
    for name in ["original", "denied"] {
        std::fs::create_dir(root.path().join(name)).unwrap();
    }
    std::fs::write(root.path().join("denied/file"), b"keep").unwrap();
    std::os::unix::fs::symlink("../denied/file", root.path().join("original/link")).unwrap();
    let policy = root.path().join("policy.toml");
    std::fs::write(
        &policy,
        "[fs]\nread = [\"original\"]\nwrite = [\"original\"]\n",
    )
    .unwrap();
    let engine = Engine::default();
    let module = module(
        &engine,
        r#"
        (local.set $parent (call $opened (call $openat (call $invalid)
            (i32.const 0) (i32.const 8) (i32.const 3) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 8))))
        (call $deny (call $openat (local.get $parent) (i32.const 768) (i32.const 14)
            (i32.const 1) (i32.const 1) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 8)))
        (call $deny (call $removeat (local.get $parent) (i32.const 768) (i32.const 14)))
        (call $deny (call $openat (local.get $parent) (i32.const 896) (i32.const 4)
            (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 0) (i32.const 8)))
        (call $finish (call $removeat (local.get $parent) (i32.const 896) (i32.const 4)))
        (if (call $close (local.get $parent)) (then unreachable))
    "#,
    );
    assert_eq!(
        engine
            .run(
                &module,
                RunOptions::default()
                    .with_policy_file(policy)
                    .unwrap()
                    .with_working_directory(WorkingDirectory::Fixed(root.path().to_owned()))
            )
            .unwrap(),
        RunOutcome::Completed
    );
    assert_eq!(
        std::fs::read(root.path().join("denied/file")).unwrap(),
        b"keep"
    );
    assert!(std::fs::symlink_metadata(root.path().join("original/link")).is_err());
}
