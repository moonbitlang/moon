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

use std::{fs, time::Duration};

#[cfg(feature = "v8")]
#[test]
fn managed_roots_accept_wasm_contents_regardless_of_extension() {
    let directory = tempfile::tempdir().unwrap();
    let wasm = wat::parse_str(
        r#"(module
        (import "wasi_snapshot_preview1" "proc_exit" (func $exit (param i32)))
        (func (export "_start") i32.const 23 call $exit))"#,
    )
    .unwrap();
    for name in ["program.bin", "program"] {
        let file = directory.path().join(name);
        fs::write(&file, &wasm).unwrap();
        for flag in ["--daemon", "--virtual-process"] {
            snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
                .env_remove(moonrun::ipc::ENDPOINT_ENV)
                .arg(flag)
                .arg(&file)
                .timeout(Duration::from_secs(10))
                .assert()
                .code(23)
                .stdout_eq("")
                .stderr_eq("");
        }
    }
}

#[test]
fn incompatible_configuration_fails_before_loading_wasm_or_connecting() {
    let directory = tempfile::tempdir().unwrap();
    let policy = directory.path().join("policy.toml");
    let missing = directory.path().join("missing.wasm");
    for (configuration, flag) in [
        ("virtualize = true", "--daemon"),
        ("ipc = true", "--virtual-process"),
    ] {
        fs::write(&policy, format!("[process]\n{configuration}\n")).unwrap();
        snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
            .env_remove(moonrun::ipc::ENDPOINT_ENV)
            .arg(flag)
            .arg("--policy")
            .arg(&policy)
            .arg(&missing)
            .timeout(Duration::from_secs(10))
            .assert()
            .failure()
            .stdout_eq("")
            .stderr_eq("Error: IPC virtual process sessions are not supported yet\n");
    }
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .env(
            moonrun::ipc::ENDPOINT_ENV,
            directory.path().join("stale-session"),
        )
        .arg("--virtual-process")
        .arg(&missing)
        .timeout(Duration::from_secs(10))
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq("Error: IPC virtual process sessions are not supported yet\n");
}
