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

use std::{net::Ipv4Addr, path::PathBuf, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::{TestDir, moon_cmd};

#[test]
fn moonrun_library_returns_guest_exit_without_terminating_embedder() {
    let dir = TestDir::new("test_cli_args.in");

    moon_cmd()
        .current_dir(&dir)
        .args(["build", "--target", "wasm-gc"])
        .assert()
        .success();

    let wasm = dir.join("_build/wasm-gc/debug/build/main/main.wasm");
    let engine = moonrun::Engine::default();
    let options = || {
        moonrun::RunOptions::default()
            .with_args(["exit-7"])
            .with_working_directory(moonrun::WorkingDirectory::Ambient)
    };

    assert_eq!(
        engine.run_file(&wasm, options()).unwrap(),
        moonrun::RunOutcome::Exited(7)
    );
    assert_eq!(
        engine.run_file(&wasm, options()).unwrap(),
        moonrun::RunOutcome::Exited(7)
    );
}

#[test]
fn moonrun_library_compiles_modules_when_loading() {
    let wasm = tempfile::Builder::new()
        .prefix("invalid.")
        .suffix(".wasm")
        .tempfile()
        .unwrap();
    std::fs::write(wasm.path(), b"not WebAssembly").unwrap();

    let error = moonrun::Engine::default()
        .load_file(wasm.path())
        .unwrap_err();

    assert!(format!("{error:#}").contains("failed to compile"));
}

#[tokio::test(flavor = "multi_thread")]
async fn moonrun_library_runs_multiple_modules_with_caller_owned_scheduling() {
    let case_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases");
    let case_dir = case_root.join("test_concurrent_http_instances.in");
    let dir = tempfile::Builder::new()
        .prefix("test_concurrent_http_instances.")
        .tempdir_in(&case_root)
        .expect("create temp fixture");
    moon_test_util::test_dir::copy_tree(&case_dir, dir.path(), false).expect("copy test fixture");

    moon_cmd()
        .current_dir(dir.path())
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let first_readiness = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let second_readiness = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let first_readiness_port = first_readiness.local_addr().unwrap().port();
    let second_readiness_port = second_readiness.local_addr().unwrap().port();

    let wasm_path = dir
        .path()
        .join("_build/wasm/debug/build/moon/concurrent_http_instances/main/main.wasm");
    let wasm = std::fs::read(&wasm_path).unwrap();
    std::fs::remove_file(&wasm_path).unwrap();
    let engine = moonrun::Engine::default();
    let first_module = engine.compile("first-http-module", &wasm).unwrap();
    let second_module = engine.compile("second-http-module", &wasm).unwrap();
    let first_engine = engine.clone();
    let first_run = tokio::task::spawn_blocking(move || {
        first_engine.run(
            &first_module,
            moonrun::RunOptions::default()
                .with_args([first_readiness_port.to_string(), "first".to_owned()]),
        )
    });
    let second_run = tokio::task::spawn_blocking(move || {
        engine.run(
            &second_module,
            moonrun::RunOptions::default()
                .with_args([second_readiness_port.to_string(), "second".to_owned()]),
        )
    });

    let (first_port, second_port) = tokio::join!(
        wait_for_guest_port(&first_readiness, "first module"),
        wait_for_guest_port(&second_readiness, "second module")
    );
    let (first_response, second_response) = tokio::join!(request(first_port), request(second_port));
    assert!(first_response.starts_with("HTTP/1.1 200"));
    assert!(first_response.contains("first"));
    assert!(second_response.starts_with("HTTP/1.1 200"));
    assert!(second_response.contains("second"));

    assert_eq!(
        first_run.await.unwrap().unwrap(),
        moonrun::RunOutcome::Completed
    );
    assert_eq!(
        second_run.await.unwrap().unwrap(),
        moonrun::RunOutcome::Completed
    );
}

async fn wait_for_guest_port(listener: &TcpListener, module: &str) -> u16 {
    tokio::time::timeout(Duration::from_secs(10), async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut port = String::new();
        stream.read_to_string(&mut port).await.unwrap();
        port.parse().unwrap()
    })
    .await
    .unwrap_or_else(|_| panic!("{module} did not report readiness"))
}

async fn request(port: u16) -> String {
    tokio::time::timeout(Duration::from_secs(5), async move {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    })
    .await
    .unwrap_or_else(|_| panic!("HTTP instance on port {port} did not respond"))
}
