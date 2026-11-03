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

#![cfg(all(unix, feature = "v8"))]

use moonrun::ipc::{ClientRun, Invocation, serve};
use moonrun::{Engine, RunOptions, RunOutcome, RunTermination};
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::time::Duration;

fn invocation(program: &Path, cwd: &Path) -> Invocation {
    Invocation {
        program: program.to_owned(),
        args: Vec::new(),
        working_directory: cwd.to_owned(),
        environment: Vec::new(),
        inherited_policy: None,
        no_stack_trace: false,
        test_args: None,
        stack_size: None,
    }
}

fn stdio() -> [File; 3] {
    std::array::from_fn(|_| tempfile::tempfile().unwrap())
}

fn write_wasm(directory: &Path, text: &str) -> std::path::PathBuf {
    let path = directory.join("program.wasm");
    std::fs::write(&path, wat::parse_str(text).unwrap()).unwrap();
    path
}

#[test]
fn crosses_processes_with_caller_state_and_pipe_stdio() {
    const SERVER: &str = "MOONRUN_IPC_TEST_SOCKET";
    if let Some(path) = std::env::var_os(SERVER) {
        // This process has a different cwd and environment. None of it may
        // become the Run's caller state. The listener belongs to the test client.
        let connection = UnixStream::connect(path).unwrap();
        serve(&Engine::default(), connection, RunOptions::default()).unwrap();
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("ipc.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut server = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "crosses_processes_with_caller_state_and_pipe_stdio",
        ])
        .env(SERVER, &socket)
        .current_dir("/")
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let connection = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if std::time::Instant::now() >= deadline {
                    let _ = server.kill();
                    let _ = server.wait();
                    panic!("IPC server did not connect");
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("{error}"),
        }
    };
    connection
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let program = write_wasm(
        directory.path(),
        r#"(module
        (import "wasi_snapshot_preview1" "args_get" (func $args (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "environ_sizes_get" (func $env_sizes (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "environ_get" (func $env (param i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_read" (func $read (param i32 i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
        (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
        (memory (export "memory") 1)
        (data (i32.const 512) "file")
        (func $ok (param i32) local.get 0 if unreachable end)
        (func (export "_start")
            i32.const 0 i32.const 128 call $args call $ok
            i32.const 4 i32.load i32.load8_u i32.const 65 i32.ne if unreachable end
            i32.const 32 i32.const 36 call $env_sizes call $ok
            i32.const 32 i32.load i32.const 1 i32.ne if unreachable end
            i32.const 40 i32.const 256 call $env call $ok
            i32.const 256 i32.load8_u i32.const 75 i32.ne if unreachable end
            i32.const 258 i32.load8_u i32.const 86 i32.ne if unreachable end
            i32.const 3 i32.const 0 i32.const 512 i32.const 4 i32.const 0
            i64.const 2 i64.const 0 i32.const 0 i32.const 64 call $open call $ok
            i32.const 0 i32.const 600 i32.store
            i32.const 4 i32.const 1 i32.store
            i32.const 0 i32.const 0 i32.const 1 i32.const 8 call $read call $ok
            i32.const 600 i32.load8_u i32.const 90 i32.ne if unreachable end
            i32.const 1 i32.const 0 i32.const 1 i32.const 8 call $write call $ok
            i32.const 2 i32.const 0 i32.const 1 i32.const 8 call $write call $ok
            i32.const 23 call $exit))"#,
    );
    std::fs::write(directory.path().join("file"), b"cwd binding").unwrap();
    let mut request = invocation(&program, directory.path());
    request.args = vec!["Argument".into()];
    request.environment = vec![("K".into(), "V".into())];
    let (stdin, mut input) = std::io::pipe().unwrap();
    input.write_all(b"Z").unwrap();
    drop(input);
    let (mut output, stdout) = std::io::pipe().unwrap();
    let (mut errors, stderr) = std::io::pipe().unwrap();
    let result = ClientRun::submit(
        connection,
        request,
        [
            File::from(std::os::fd::OwnedFd::from(stdin)),
            File::from(std::os::fd::OwnedFd::from(stdout)),
            File::from(std::os::fd::OwnedFd::from(stderr)),
        ],
    )
    .unwrap()
    .wait();
    // Reap the host even if the guest assertion failed.
    let status = server.wait().unwrap();
    assert_eq!(result.unwrap(), RunOutcome::Exited(23));
    assert!(status.success());
    let mut bytes = Vec::new();
    output.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"Z");
    bytes.clear();
    errors.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"Z");
}

#[test]
fn cancellation_and_disconnect_are_scoped_to_one_execution() {
    if std::env::var_os("MOONRUN_IPC_LIFECYCLE_TEST").is_none() {
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "cancellation_and_disconnect_are_scoped_to_one_execution",
            ])
            .env("MOONRUN_IPC_LIFECYCLE_TEST", "1")
            .timeout(Duration::from_secs(20))
            .assert()
            .success();
        return;
    }
    let engine = Engine::default();
    let directory = tempfile::tempdir().unwrap();
    let program = write_wasm(
        directory.path(),
        r#"(module
        (import "wasi_snapshot_preview1" "fd_write" (func $write (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 16) "R")
        (func (export "_start")
            i32.const 0 i32.const 16 i32.store
            i32.const 4 i32.const 1 i32.store
            i32.const 1 i32.const 0 i32.const 1 i32.const 8 call $write drop
            (loop $forever br $forever)))"#,
    );
    let mut runs = Vec::new();
    let mut hosts = Vec::new();
    for _ in 0..2 {
        let (client, server) = UnixStream::pair().unwrap();
        let engine = engine.clone();
        hosts.push(std::thread::spawn(move || {
            serve(&engine, server, RunOptions::default())
        }));
        let (writer, mut ready) = UnixStream::pair().unwrap();
        ready
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let [stdin, _, stderr] = stdio();
        let run = ClientRun::submit(
            client,
            invocation(&program, directory.path()),
            [
                stdin,
                File::from(std::os::fd::OwnedFd::from(writer)),
                stderr,
            ],
        )
        .unwrap();
        let mut byte = [0];
        ready.read_exact(&mut byte).unwrap();
        assert_eq!(byte, *b"R");
        runs.push(run);
    }
    let second = runs.pop().unwrap();
    let first = runs.pop().unwrap();
    let surviving_control = first.control();
    drop(first); // A live control clone must not keep the execution alive.
    let first_host = hosts.remove(0).join().unwrap();
    let _ = first_host; // successful socket writes do not prove the peer observed them
    assert!(!hosts[0].is_finished(), "disconnect stopped the other Run");
    assert!(
        surviving_control
            .terminate(RunTermination::Exit(9))
            .is_err()
    );
    second.control().terminate(RunTermination::Exit(7)).unwrap();
    let _ = second.control().terminate(RunTermination::Exit(8));
    assert_eq!(second.wait().unwrap(), RunOutcome::Exited(7));
    hosts.pop().unwrap().join().unwrap().unwrap();
    assert!(engine.snapshot().is_empty());
    let module = engine
        .compile("after.wasm", wat::parse_str("(module)").unwrap())
        .unwrap();
    assert_eq!(
        engine.run(&module, RunOptions::default()).unwrap(),
        RunOutcome::Completed
    );
}

#[test]
fn client_and_host_restrictions_intersect_and_bad_transfers_fail() {
    let engine = Engine::default();
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("file"), b"secret").unwrap();
    let program = write_wasm(
        directory.path(),
        r#"(module
        (import "wasi_snapshot_preview1" "path_open" (func $open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (data (i32.const 0) "file")
        (func (export "_start")
            i32.const 3 i32.const 0 i32.const 0 i32.const 4 i32.const 0
            i64.const 2 i64.const 0 i32.const 0 i32.const 32 call $open
            i32.const 2 i32.ne if unreachable end))"#,
    );
    let deny = include_bytes!("fixtures/deny_restrictions.json").to_vec();
    let mut allowed: serde_json::Value = serde_json::from_slice(&deny).unwrap();
    *allowed.pointer_mut("/rule_groups/0/fs/read").unwrap() = serde_json::json!([{"kind":"any"}]);
    let allowed = serde_json::to_vec(&allowed).unwrap();
    for (host, client) in [
        (deny.clone(), allowed.clone()),
        (allowed, deny),
        (b"invalid".to_vec(), b"invalid".to_vec()),
    ] {
        let (a, b) = UnixStream::pair().unwrap();
        a.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut request = invocation(&program, directory.path());
        request.inherited_policy = Some(client);
        let invalid = host == b"invalid";
        let engine = engine.clone();
        let server = std::thread::spawn(move || {
            serve(
                &engine,
                b,
                RunOptions::default().with_inherited_policy(host),
            )
        });
        let result = ClientRun::submit(a, request, stdio()).unwrap().wait();
        server.join().unwrap().unwrap();
        if invalid {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("failed to decode inherited policy")
            );
        } else {
            assert_eq!(result.unwrap(), RunOutcome::Completed);
        }
    }
}

#[test]
fn errors_are_reported_without_reexecution() {
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("missing.wasm");
    let (client, server) = UnixStream::pair().unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let host = std::thread::spawn(move || serve(&Engine::default(), server, RunOptions::default()));
    let error = ClientRun::submit(client, invocation(&program, directory.path()), stdio())
        .unwrap()
        .wait()
        .unwrap_err();
    host.join().unwrap().unwrap();
    assert!(error.to_string().contains("no such file"));
}
