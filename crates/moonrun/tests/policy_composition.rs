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

//! Additional child restrictions must narrow the inherited authority.

use moonrun::{Engine, RunOptions, RunOutcome, WorkingDirectory};

const DENIED_READ: &str = r#"(module
        (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "file")
        (func (export "_start")
            i32.const 3 i32.const 0 i32.const 0 i32.const 4 i32.const 0
            i64.const 2 i64.const 0 i32.const 0 i32.const 32 call $open
            i32.const 2 i32.ne if unreachable end))"#;

fn read_only_parent() -> Vec<u8> {
    let mut transfer: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/deny_restrictions.json")).unwrap();
    *transfer.pointer_mut("/rule_groups/0/fs/read").unwrap() = serde_json::json!([{"kind":"any"}]);
    serde_json::to_vec(&transfer).unwrap()
}

#[test]
fn inherited_and_child_policies_both_restrict_filesystem_access() {
    let engine = Engine::default();
    let module = engine
        .compile(
            "policy-composition.wasm",
            wat::parse_str(DENIED_READ).unwrap(),
        )
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("file"), b"secret").unwrap();
    let child = directory.path().join("child.json");
    for (parent, restriction) in [
        (read_only_parent(), "{}"),
        (
            include_bytes!("fixtures/deny_restrictions.json").to_vec(),
            r#"{"fs":{"read":["*"]}}"#,
        ),
    ] {
        std::fs::write(&child, restriction).unwrap();
        for parent_first in [false, true] {
            let options = RunOptions::default()
                .with_working_directory(WorkingDirectory::Fixed(directory.path().to_owned()));
            let options = if parent_first {
                options
                    .with_inherited_policy(parent.clone())
                    .with_policy_file(&child)
                    .unwrap()
            } else {
                options
                    .with_policy_file(&child)
                    .unwrap()
                    .with_inherited_policy(parent.clone())
            };
            assert_eq!(
                engine.run(&module, options).unwrap(),
                RunOutcome::Completed,
                "parent={parent:?}, child={restriction}, parent_first={parent_first}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn an_installed_timezone_file_is_readable_under_every_policy() {
    let zone = std::path::Path::new("/usr/share/zoneinfo/UTC");
    if !zone.is_file() {
        return; // Some Unix installations omit tzdata.
    }
    let engine = Engine::default();
    let source = r#"(module
        (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_close" (func $close (param i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "UTC")
        (func (export "_start")
            i32.const 3 i32.const 1 i32.const 0 i32.const 3 i32.const 0
            i64.const 2 i64.const 0 i32.const 0 i32.const 8 call $open if unreachable end
            (i32.store (i32.const 16) (i32.const 32)) (i32.store (i32.const 20) (i32.const 4))
            i32.const 8 i32.load i32.const 16 i32.const 1 i32.const 24 call $read if unreachable end
            i32.const 24 i32.load i32.const 4 i32.ne if unreachable end
            i32.const 32 i32.load i32.const 0x66695a54 i32.ne if unreachable end ;; TZif
            i32.const 8 i32.load call $close if unreachable end))"#;
    let module = engine
        .compile("read-zone.wasm", wat::parse_str(source).unwrap())
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("child.json");
    std::fs::write(&child, "{}").unwrap();
    let exclusion = dir.path().join("exclude.json");
    std::fs::write(&exclusion, r#"{"fs":{"read":["*",{"glob":"!**"}]}}"#).unwrap();
    let root = std::path::Path::new("/");
    for (name, options) in [
        ("unrestricted", RunOptions::default()),
        (
            "empty",
            RunOptions::default().with_policy_file(&child).unwrap(),
        ),
        (
            "explicit exclusion",
            RunOptions::default()
                .with_policy_file_source_dir(&exclusion, Some(root))
                .unwrap(),
        ),
        (
            "parent and child",
            RunOptions::default()
                .with_inherited_policy(include_bytes!("fixtures/deny_restrictions.json").to_vec())
                .with_policy_file_source_dir(&exclusion, Some(root))
                .unwrap(),
        ),
    ] {
        let options = options
            .with_working_directory(WorkingDirectory::Fixed(zone.parent().unwrap().to_owned()));
        assert_eq!(
            engine.run(&module, options).unwrap(),
            RunOutcome::Completed,
            "{name}"
        );
    }
}

#[test]
fn cli_combines_transferred_policy_with_child_file() {
    use std::io::{Seek, Write};
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("file"), b"secret").unwrap();
    let wasm = directory.path().join("program.wasm");
    std::fs::write(&wasm, wat::parse_str(DENIED_READ).unwrap()).unwrap();
    let policy = directory.path().join("child.json");
    std::fs::write(&policy, "{}").unwrap();
    let mut inherited = tempfile::tempfile().unwrap();
    inherited.write_all(&read_only_parent()).unwrap();
    inherited.rewind().unwrap();
    let transfer = moonutil::policy_transport::PolicyTransfer::from_file(inherited).unwrap();
    let mut command = std::process::Command::new(snapbox::cmd::cargo_bin!("moonrun"));
    command
        .current_dir(directory.path())
        .arg("--policy")
        .arg(policy)
        .arg(wasm);
    let relay = transfer.into_relay().attach_to(&mut command).unwrap();
    snapbox::cmd::Command::from_std(command)
        .timeout(std::time::Duration::from_secs(20))
        .assert()
        .success()
        .stdout_eq("");
    relay.finish().unwrap();
}
