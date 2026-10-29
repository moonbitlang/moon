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

#![cfg(feature = "v8")]

use moonrun::{Engine, GuestProcessMode, RunOptions, RunOutcome};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, Write};
#[cfg(unix)]
use std::os::fd::OwnedFd as OwnedFile;
#[cfg(windows)]
use std::os::windows::io::OwnedHandle as OwnedFile;
use std::path::Path;
use std::time::Duration;

#[test]
fn async_virtual_process_control() {
    let Ok(moon) = std::env::var("MOONRUN_CONTROL_TEST_MOON") else {
        let moon = escargot::CargoBuild::new()
            .manifest_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("../moon/Cargo.toml"))
            .bin("moon")
            .current_release()
            .run()
            .unwrap();
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "async_virtual_process_control", "--nocapture"])
            .env("MOONRUN_CONTROL_TEST_MOON", moon.path())
            .timeout(Duration::from_secs(120))
            .assert()
            .success();
        return;
    };
    let project = tempfile::tempdir().unwrap();
    let upstream =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/moonbitlang_async");
    moon_test_util::test_dir::copy_tree(&upstream.join("src"), &project.path().join("src"), false)
        .unwrap();
    fs::copy(upstream.join("moon.mod"), project.path().join("moon.mod")).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/test_cases/test_virtual_process_control.in");
    moon_test_util::test_dir::copy_tree(
        &fixture,
        &project.path().join("src/virtual_control"),
        false,
    )
    .unwrap();
    snapbox::cmd::Command::new(&moon)
        .current_dir(project.path())
        .args(["build", "src/virtual_control", "--target", "wasm"])
        .assert()
        .success();
    let engine = Engine::default();
    let wasm_path = project
        .path()
        .join("_build/wasm/debug/build/virtual_control/virtual_control.wasm");
    let module = engine.load_file(&wasm_path).unwrap();
    let unhandled = engine.compile("unhandled.wasm", wat::parse_str(r#"(module
        (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "\10\00\00\00\06\00\00\00")
        (data (i32.const 16) "ready\0a")
        (func (export "_start")
            i32.const 1 i32.const 0 i32.const 1 i32.const 8 call $write drop
            (loop br 0)))"#).unwrap()).unwrap();
    for (mode, handoff) in [
        ("exit", false),
        ("retry", false),
        ("graceful", false),
        ("unhandled", false),
        ("busy", false),
        ("graceful", true),
    ] {
        eprintln!("virtual control: {mode}, handoff: {handoff}");
        let tree = engine.clone();
        let expected = match mode {
            "exit" | "retry" => 23,
            "busy" => {
                if cfg!(windows) {
                    1
                } else {
                    -9
                }
            }
            _ => {
                #[cfg(unix)]
                {
                    -libc::SIGTERM
                }
                #[cfg(windows)]
                {
                    0xC000_013A_u32 as i32
                }
            }
        };
        let (parent_input, mut send_pid) = std::io::pipe().unwrap();
        let (parent_output, parent_stdout) = std::io::pipe().unwrap();
        let mut errors = tempfile::tempfile().unwrap();
        let parent = tree
            .spawn(
                None,
                &module,
                RunOptions::default()
                    .with_guest_process_mode(GuestProcessMode::Virtual)
                    .with_args(["parent", mode, &expected.to_string()])
                    .with_stdio(
                        File::from(OwnedFile::from(parent_input)),
                        File::from(OwnedFile::from(parent_stdout)),
                        errors.try_clone().unwrap(),
                    ),
            )
            .unwrap();
        let (child_input, mut release_child) = std::io::pipe().unwrap();
        let (child_output, child_stdout) = std::io::pipe().unwrap();
        let options = RunOptions::default()
            .with_guest_process_mode(GuestProcessMode::Virtual)
            .with_stdio(
                File::from(OwnedFile::from(child_input)),
                File::from(OwnedFile::from(child_stdout)),
                errors.try_clone().unwrap(),
            );
        // Artifact preparation must retain the tree's signal receiver: a
        // registered graceful handler must run instead of default termination.
        let child = if handoff {
            tree.spawn_moon(
                Some(parent),
                Path::new(&moon),
                &[
                    "run".into(),
                    wasm_path.as_os_str().to_owned(),
                    "--".into(),
                    "child".into(),
                    mode.into(),
                ],
                options,
            )
        } else {
            tree.spawn(
                Some(parent),
                if mode == "unhandled" {
                    &unhandled
                } else {
                    &module
                },
                options.with_args(["child", mode]),
            )
        }
        .unwrap();
        let mut child_output = BufReader::new(child_output);
        let mut ready = String::new();
        child_output.read_line(&mut ready).unwrap();
        assert_eq!(ready, "ready\n");
        writeln!(send_pid, "{child}").unwrap();
        drop(send_pid);
        let mut parent_output = BufReader::new(parent_output);
        if mode == "retry" {
            let mut line = String::new();
            parent_output.read_line(&mut line).unwrap();
            assert_eq!(line, "retry\n");
            release_child.write_all(b"G").unwrap();
        }
        let outcome = tree.wait(parent);
        errors.rewind().unwrap();
        let mut diagnostics = String::new();
        errors.read_to_string(&mut diagnostics).unwrap();
        assert_eq!(outcome.unwrap(), RunOutcome::Completed, "{diagnostics}");
        let mut output = String::new();
        parent_output.read_to_string(&mut output).unwrap();
        assert_eq!(output, "done\n", "{diagnostics}");
        output.clear();
        child_output.read_to_string(&mut output).unwrap();
        assert_eq!(output, if mode == "graceful" { "cleanup\n" } else { "" });
        assert!(tree.snapshot().is_empty(), "child was not reaped");
    }
}
