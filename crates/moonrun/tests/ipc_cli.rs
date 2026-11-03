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

use std::fs;
use std::path::Path;
use std::time::Duration;

#[test]
fn cli_reuses_the_host_through_a_native_shell() {
    let directory = tempfile::tempdir().unwrap();
    let project = directory.path().join("project");
    let upstream = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/moonbitlang_async");
    moon_test_util::test_dir::copy_tree(&upstream.join("src"), &project.join("src"), false).unwrap();
    fs::copy(upstream.join("moon.mod"), project.join("moon.mod")).unwrap();
    moon_test_util::test_dir::copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases/test_ipc_cli.in"),
        &project.join("src/ipc_guest"), false,
    ).unwrap();
    snapbox::cmd::Command::new("moon")
        .current_dir(&project)
        .args(["build", "--target", "wasm", "src/ipc_guest"])
        .timeout(Duration::from_secs(90)).assert().success();
    let wasm = project.join("_build/wasm/debug/build/ipc_guest/ipc_guest.wasm");
    fs::create_dir(project.join("child")).unwrap();
    fs::write(project.join("child/input"), "caller cwd").unwrap();
    fs::write(project.join("secret"), "must remain inaccessible").unwrap();
    let policy = project.join("policy.toml");
    fs::write(&policy, r#"
[env]
from_host = ["*"]
[fs]
read = ["child"]
[process]
ipc = true
allow = [{ program = "/bin/sh" }]
"#).unwrap();
    let output = snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(&project)
        .arg("--policy").arg(policy).arg(&wasm)
        .arg("parent").arg(snapbox::cmd::cargo_bin!("moonrun")).arg(&wasm)
        .timeout(Duration::from_secs(30))
        .assert().success();
    // The shell execs the real client PID; guest getpid identifies the host.
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let lines = stdout.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 4, "{stdout}");
    assert_eq!(lines[0].strip_prefix("host="), lines[2].strip_prefix("guest="));
    assert_ne!(lines[0].strip_prefix("host="), lines[1].strip_prefix("client="));
    assert_eq!(lines[3], "caller state and ceiling preserved");
}
