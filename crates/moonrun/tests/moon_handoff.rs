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

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::time::Duration;

use moonrun::{
    Engine, ProcessState, RunOptions, RunOutcome, RunTermination, VirtualProcessTree,
    WorkingDirectory,
};
use moonutil::run_handoff::{HANDOFF_ARG, RunHandoff};

const SOURCE: &str = r#"----
import {
  "moonbitlang/core/env",
}
----
#unsafe_skip_stub_check
fn pid() -> Int = "moonbitlang/async" "env_util/getpid"
fn main {
  println(pid())
  let args = @env.args()
  for i = 1; i < args.length(); i = i + 1 { println(args[i]) }
  println(@env.get_env_var("HANDOFF_POLICY"))
}
"#;

#[test]
fn moon_commands_run_in_virtual_tree() {
    let Ok(moon) = std::env::var("MOONRUN_HANDOFF_TEST_MOON") else {
        let moon = escargot::CargoBuild::new()
            .manifest_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("../moon/Cargo.toml"))
            .bin("moon")
            .current_release()
            .run()
            .expect("build Moon helper");
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "moon_commands_run_in_virtual_tree",
                "--nocapture",
            ])
            .env("MOONRUN_HANDOFF_TEST_MOON", moon.path())
            .timeout(Duration::from_secs(120))
            .assert()
            .success();
        return;
    };
    let moon = Path::new(&moon);
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let moonx = root.join(format!("moonx{}", std::env::consts::EXE_SUFFIX));
    fs::hard_link(moon, &moonx).unwrap_or_else(|_| {
        fs::copy(moon, &moonx).unwrap();
    });
    fs::write(root.join("script.mbtx"), SOURCE).unwrap();
    fs::write(
        root.join("policy.toml"),
        "[env.set]\nHANDOFF_POLICY = 'child'\n",
    )
    .unwrap();
    let tree = std::sync::Arc::new(VirtualProcessTree::new(Engine::default()).unwrap());

    let inline = format!("-e={SOURCE}");
    // Actual compiler output must see the very same virtual PID allocated
    // before building, including when the source was a temporary inline file.
    for (program, args, input) in [
        (moon, vec!["run", &inline], ""),
        (moon, vec!["run", "-", "--", "stdin arg"], SOURCE),
        (moonx.as_path(), vec!["script.mbtx", "moonx arg"], ""),
        (
            moonx.as_path(),
            vec!["--experimental-policy", "policy.toml", "script.mbtx"],
            "",
        ),
    ] {
        let mut stdin = tempfile::tempfile().unwrap();
        stdin.write_all(input.as_bytes()).unwrap();
        stdin.rewind().unwrap();
        let stdout = tempfile::tempfile().unwrap();
        let stderr = tempfile::tempfile().unwrap();
        let options = RunOptions::default()
            .with_working_directory(WorkingDirectory::Fixed(root.to_owned()))
            .with_stdio(
                stdin,
                stdout.try_clone().unwrap(),
                stderr.try_clone().unwrap(),
            );
        let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
        let pid = tree.spawn_moon(None, program, &args, options).unwrap();
        let result = tree.wait(pid);
        let diagnostics = contents(stderr);
        assert_eq!(result.unwrap(), RunOutcome::Completed, "{diagnostics}");
        let output = contents(stdout);
        let expected = if !input.is_empty() {
            "stdin arg\nNone\n"
        } else if args
            .iter()
            .any(|arg| arg.to_string_lossy().starts_with("-e="))
        {
            "None\n"
        } else if args.iter().any(|arg| arg == "--experimental-policy") {
            "Some(child)\n"
        } else {
            "moonx arg\nNone\n"
        };
        assert_eq!(output, format!("{pid}\n{expected}"), "{diagnostics}");

        // The public Moon/moonx switch starts a virtual root without requiring
        // callers to locate the built artifact or invoke moonrun themselves.
        let mut cli_args = args.clone();
        cli_args.insert(
            usize::from(program == moon),
            "--virtual-process-tree".into(),
        );
        let output = snapbox::cmd::Command::new(program)
            .current_dir(root)
            .args(&cli_args)
            .env("MOONRUN_OVERRIDE", snapbox::cmd::cargo_bin!("moonrun"))
            .stdin(input)
            .timeout(Duration::from_secs(30))
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let output = String::from_utf8(output).unwrap();
        let (pid, rest) = output.split_once('\n').unwrap();
        assert!(pid.trim().parse::<i32>().unwrap() >= 1 << 30);
        assert_eq!(rest, expected);
    }

    // Parent-provided canonical policy wins even over an unreadable child file.
    let stdout = tempfile::tempfile().unwrap();
    let options = RunOptions::default()
        .with_working_directory(WorkingDirectory::Fixed(root.to_owned()))
        .with_inherited_policy(br#"{"env":{"set":{"HANDOFF_POLICY":"parent"}}}"#.to_vec())
        .with_stdio(
            tempfile::tempfile().unwrap(),
            stdout.try_clone().unwrap(),
            tempfile::tempfile().unwrap(),
        );
    let pid = tree
        .spawn_moon(
            None,
            &moonx,
            &[
                "--experimental-policy".into(),
                "does-not-exist.toml".into(),
                "script.mbtx".into(),
            ],
            options,
        )
        .unwrap();
    assert_eq!(tree.wait(pid).unwrap(), RunOutcome::Completed);
    assert_eq!(contents(stdout), format!("{pid}\nSome(parent)\n"));

    // Export mode must never execute the program, and temporary source cleanup
    // cannot invalidate its copied artifact or embedded policy source.
    let policy_source = SOURCE.replacen(
        "----\nimport",
        "----\npermission(env: { \"set\": { \"HANDOFF_POLICY\": \"embedded\" } })\nimport",
        1,
    );
    let handoff_dir = tempfile::tempdir().unwrap();
    let handoff = RunHandoff::new(handoff_dir.path(), false).unwrap();
    snapbox::cmd::Command::new(moon)
        .current_dir(root)
        .arg(HANDOFF_ARG)
        .arg(handoff.directory())
        .arg("run")
        .args(["--target", "wasm-gc"])
        .arg(format!("-e={policy_source}"))
        .assert()
        .success()
        .stdout_eq("");
    let prepared = handoff.load().unwrap();
    let prepared_wasm = fs::read(&prepared.artifact).unwrap();
    assert!(prepared_wasm.starts_with(b"\0asm"));
    assert!(prepared.source_map.is_some());
    assert!(!prepared.executable.exists());
    assert!(prepared.args.is_empty());
    let (policy, source_dir) = prepared.policy.unwrap();
    assert!(policy.exists());
    assert_eq!(source_dir, dunce::canonicalize(root).unwrap());

    // Inline source and its map are deleted by the helper before Wasm runs.
    // The retained map must still locate a trap in the original source.
    let stderr = tempfile::tempfile().unwrap();
    let pid = tree
        .spawn_moon(
            None,
            moon,
            &[
                "run".into(),
                "--target=wasm-gc".into(),
                "-e".into(),
                "fn main {\n  abort(\"handoff trap\")\n}".into(),
            ],
            RunOptions::default().with_stdio(
                tempfile::tempfile().unwrap(),
                tempfile::tempfile().unwrap(),
                stderr.try_clone().unwrap(),
            ),
        )
        .unwrap();
    assert_eq!(tree.wait(pid).unwrap(), RunOutcome::Exited(1));
    let diagnostics = contents(stderr);
    assert!(diagnostics.contains(".mbtx:2"), "{diagnostics}");

    for args in [
        vec!["run", "-e", "this is invalid MoonBit"],
        vec!["run", "--build-only", "-e", "fn main {}"],
        vec!["new", "must-not-be-created"],
    ] {
        let stdout = tempfile::tempfile().unwrap();
        let options = RunOptions::default()
            .with_working_directory(WorkingDirectory::Fixed(root.to_owned()))
            .with_stdio(
                tempfile::tempfile().unwrap(),
                stdout.try_clone().unwrap(),
                tempfile::tempfile().unwrap(),
            );
        let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
        let pid = tree.spawn_moon(None, moon, &args, options).unwrap();
        assert!(matches!(tree.wait(pid).unwrap(), RunOutcome::Exited(code) if code != 0));
        assert_eq!(contents(stdout), "");
    }
    assert!(!root.join("must-not-be-created").exists());

    // Moon may prepare any target. The supervisor, not the CLI or delivery
    // protocol, owns the still-unimplemented native/Node execution branch.
    let js_directory = tempfile::tempdir().unwrap();
    let js_handoff = RunHandoff::new(js_directory.path(), false).unwrap();
    let js_args = [
        "run",
        "--target=js",
        "-e",
        "fn main { println(\"must not execute\") }",
    ];
    snapbox::cmd::Command::new(moon)
        .arg(HANDOFF_ARG)
        .arg(js_handoff.directory())
        .args(js_args)
        .assert()
        .success()
        .stdout_eq("");
    let prepared_js = js_handoff.load().unwrap();
    assert_eq!(
        prepared_js.execution_kind,
        moonutil::run_handoff::ExecutionKind::Js
    );
    assert!(prepared_js.artifact.is_file());
    assert!(!prepared_js.executable.exists());
    let child = tree
        .spawn_moon(
            None,
            moon,
            &js_args.map(OsString::from),
            RunOptions::default(),
        )
        .unwrap();
    let error = anyhow::Error::from(tree.wait(child).unwrap_err());
    assert!(
        format!("{error:#}").contains("native process nodes"),
        "{error:#}"
    );
    let output = snapbox::cmd::Command::new(moon)
        .args([
            "run",
            "--virtual-process-tree",
            "--target=js",
            "-e",
            "fn main {}",
        ])
        .env("MOONRUN_OVERRIDE", snapbox::cmd::cargo_bin!("moonrun"))
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("native process nodes")
    );

    // The producer uses existing registry acquisition, including its pinned
    // artifact cache. This fixture must not need a live registry or a compiler.
    let cached =
        root.join("registry/cache/assets/moonbitlang/parser/0.3.3/cmd/moonfmt/moonfmt.wasm");
    fs::create_dir_all(cached.parent().unwrap()).unwrap();
    fs::write(&cached, &prepared_wasm).unwrap();
    let registry_dir = tempfile::tempdir().unwrap();
    let registry_handoff = RunHandoff::new(registry_dir.path(), false).unwrap();
    snapbox::cmd::Command::new(&moonx)
        .arg(HANDOFF_ARG)
        .arg(registry_handoff.directory())
        .args(["moonbitlang/parser/cmd/moonfmt@0.3.3", "registry arg"])
        .env("MOON_HOME", root)
        .assert()
        .success()
        .stdout_eq("");
    let registry = registry_handoff.load().unwrap();
    assert_eq!(fs::read(&registry.artifact).unwrap(), prepared_wasm);
    assert_eq!(registry.args, ["registry arg"]);

    let output = snapbox::cmd::Command::new(&moonx)
        .args([
            "--virtual-process-tree",
            "moonbitlang/parser/cmd/moonfmt@0.3.3",
            "registry arg",
        ])
        .env("MOON_HOME", root)
        .env("MOONRUN_OVERRIDE", snapbox::cmd::cargo_bin!("moonrun"))
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).unwrap();
    let (pid, rest) = output.split_once('\n').unwrap();
    assert!(pid.trim().parse::<i32>().unwrap() >= 1 << 30);
    assert_eq!(rest, "registry arg\nNone\n");

    // The helper changes cwd while processing -C; the guest must observe it.
    fs::create_dir(root.join("subdir")).unwrap();
    let cwd_dir = tempfile::tempdir().unwrap();
    let cwd_handoff = RunHandoff::new(cwd_dir.path(), false).unwrap();
    fs::write(
        root.join("subdir/empty.wasm"),
        wat::parse_str("(module)").unwrap(),
    )
    .unwrap();
    snapbox::cmd::Command::new(moon)
        .current_dir(root)
        .arg(HANDOFF_ARG)
        .arg(cwd_handoff.directory())
        .args(["-C", "subdir", "run", "empty.wasm"])
        .assert()
        .success();
    assert_eq!(
        dunce::canonicalize(cwd_handoff.load().unwrap().working_directory).unwrap(),
        dunce::canonicalize(root.join("subdir")).unwrap()
    );

    // A build helper held at a deterministic barrier must finish before a
    // retained termination is published. Its own later exit code cannot win.
    let helper = root.join(format!("helper{}", std::env::consts::EXE_SUFFIX));
    snapbox::cmd::Command::new("rustc")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/moon_handoff_helper.rs"))
        .arg("-o")
        .arg(&helper)
        .assert()
        .success();
    for (terminate, publish) in [(false, false), (true, false), (true, true)] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut args = vec![listener.local_addr().unwrap().to_string().into()];
        if publish {
            args.push(handoff.directory().as_os_str().to_owned());
        }
        let stdout = tempfile::tempfile().unwrap();
        let pid = tree
            .spawn_moon(
                None,
                &helper,
                &args,
                RunOptions::default().with_stdio(
                    tempfile::tempfile().unwrap(),
                    stdout.try_clone().unwrap(),
                    tempfile::tempfile().unwrap(),
                ),
            )
            .unwrap();
        let (mut helper, _) = listener.accept().unwrap();
        helper
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut ready = [0];
        helper.read_exact(&mut ready).unwrap();
        assert_eq!(ready, [b'R']);
        if terminate {
            assert!(tree.terminate(pid, RunTermination::Exit(42)).unwrap());
            assert_eq!(
                tree.snapshot()
                    .iter()
                    .find(|info| info.pid == pid)
                    .unwrap()
                    .state,
                ProcessState::Terminating
            );
        }
        let (send, receive) = std::sync::mpsc::channel();
        let waiting_tree = tree.clone();
        let waiter = std::thread::spawn(move || send.send(waiting_tree.wait(pid)).unwrap());
        assert!(matches!(
            receive.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        helper.write_all(b"G").unwrap();
        assert_eq!(
            receive
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap(),
            RunOutcome::Exited(if terminate { 42 } else { 23 })
        );
        waiter.join().unwrap();
        assert_eq!(contents(stdout), "", "cancelled preparation started Wasm");
    }

    // Termination still interrupts guest execution after the handoff.
    fs::write(root.join("loop.wasm"), wat::parse_str(r#"(module
        (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "\10\00\00\00\01\00\00\00")
        (data (i32.const 16) "R")
        (func (export "_start")
            i32.const 1 i32.const 0 i32.const 1 i32.const 8 call $write drop
            (loop br 0)))"#).unwrap()).unwrap();
    let marker = tempfile::tempfile().unwrap();
    let pid = tree
        .spawn_moon(
            None,
            moon,
            &["run".into(), "loop.wasm".into()],
            RunOptions::default()
                .with_working_directory(WorkingDirectory::Fixed(root.to_owned()))
                .with_stdio(
                    tempfile::tempfile().unwrap(),
                    marker.try_clone().unwrap(),
                    tempfile::tempfile().unwrap(),
                ),
        )
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while marker.metadata().unwrap().len() == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "Wasm did not enter its loop"
        );
        std::thread::yield_now();
    }
    tree.terminate(pid, RunTermination::Exit(41)).unwrap();
    assert_eq!(tree.wait(pid).unwrap(), RunOutcome::Exited(41));
    assert!(tree.snapshot().is_empty());
}

fn contents(mut file: File) -> String {
    file.rewind().unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    contents
}
