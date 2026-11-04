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

// Shared by the Unix and Windows CLI tests, using their already-built guest.
use std::{
    fs,
    io::{Read, Seek},
    path::Path,
    time::Duration,
};

pub fn assert_execution_options(project: &Path, wasm: &Path, policy: &Path) {
    let moon = escargot::CargoBuild::new()
        .manifest_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("../moon/Cargo.toml"))
        .bin("moon")
        .current_release()
        .run()
        .unwrap();
    let moonx = project.join(format!("moonx{}", std::env::consts::EXE_SUFFIX));
    fs::hard_link(moon.path(), &moonx).unwrap_or_else(|_| {
        fs::copy(moon.path(), &moonx).unwrap();
    });
    let moon_home = project.join("moon-home");
    let cache = moon_home.join("registry/cache/assets/test/ipc/0.0.1/main/main.wasm");
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::copy(wasm, cache).unwrap();
    let (mode, expected_output) = if cfg!(windows) {
        ("windows-parent", "caller state and ceiling preserved\n")
    } else {
        (
            "parent",
            "host=[..]\nclient=[..]\nguest=[..]\ncaller state and ceiling preserved\n",
        )
    };
    let runner = snapbox::cmd::cargo_bin!("moonrun");
    // The selected policy has ipc=false. The explicit execution option enables
    // the session without changing any of its authority or provisioning rules.
    assert!(fs::read_to_string(policy).unwrap().contains("ipc = false"));
    snapbox::cmd::Command::new(runner)
        .current_dir(project)
        .env("IPC_RUNNER", runner)
        .env("IPC_PROGRAM", wasm)
        .env_remove(moonrun::ipc::ENDPOINT_ENV)
        .args(["--daemon", "--policy"])
        .arg(policy)
        .arg(wasm)
        .arg(mode)
        .arg(runner)
        .arg(wasm)
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .stdout_eq(expected_output);

    // Both modes can launch an ordinary native moonrun, which executes under
    // its own OS identity rather than the calling Runtime's identity.
    let mode_policy = project.join("mode-policy.toml");
    for (configured, flag) in [(false, false), (true, false), (false, true)] {
        fs::write(
            &mode_policy,
            format!(
                "[env]\nfrom_host = [\"*\"]\n[process]\nspawn = true\nvirtualize = {configured}\n"
            ),
        )
        .unwrap();
        snapbox::cmd::Command::new(runner)
            .env_remove(moonrun::ipc::ENDPOINT_ENV)
            .args(if flag {
                vec!["--virtual-process"]
            } else {
                vec![]
            })
            .arg("--policy")
            .arg(&mode_policy)
            .arg(wasm)
            .arg("check-native-spawn")
            .arg(runner)
            .arg(wasm)
            .timeout(Duration::from_secs(30))
            .assert()
            .success()
            .stdout_eq("native spawn preserved\n");
    }

    let args = [
        "--daemon",
        "--experimental-policy",
        policy.to_str().unwrap(),
        "test/ipc/main@0.0.1",
        mode,
        runner.to_str().unwrap(),
        wasm.to_str().unwrap(),
    ];
    snapbox::cmd::Command::new(&moonx)
        .current_dir(project)
        .env("MOON_HOME", &moon_home)
        .env("MOONRUN_OVERRIDE", runner)
        .env("IPC_RUNNER", runner)
        .env("IPC_PROGRAM", wasm)
        .env_remove(moonrun::ipc::ENDPOINT_ENV)
        .args(args)
        .timeout(Duration::from_secs(60))
        .assert()
        .success()
        .stdout_eq(expected_output);

    // The same frontend option must survive an embedding host's artifact
    // handoff, which never launches the ordinary moonrun CLI.
    let mut stdout = tempfile::tempfile().unwrap();
    let engine = moonrun::Engine::default();
    let outcome = engine
        .run_moon(
            &moonx,
            &args.iter().map(Into::into).collect::<Vec<_>>(),
            moonrun::RunOptions::default()
                .with_environment(std::env::vars_os().chain([
                    ("MOON_HOME".into(), moon_home.as_os_str().to_owned()),
                    ("IPC_RUNNER".into(), runner.as_os_str().to_owned()),
                    ("IPC_PROGRAM".into(), wasm.as_os_str().to_owned()),
                ]))
                .with_working_directory(moonrun::WorkingDirectory::Fixed(project.to_owned()))
                .with_stdio(
                    tempfile::tempfile().unwrap(),
                    stdout.try_clone().unwrap(),
                    tempfile::tempfile().unwrap(),
                ),
        )
        .unwrap();
    assert_eq!(outcome, moonrun::RunOutcome::Completed);
    assert!(engine.snapshot().is_empty());
    stdout.rewind().unwrap();
    let mut output = String::new();
    stdout.read_to_string(&mut output).unwrap();
    assert!(
        output.ends_with("caller state and ceiling preserved\n"),
        "{output}"
    );

    // A descendant moonx --daemon must reuse inherited discovery, even though
    // the flag would create a host when invoked outside an existing session.
    let descendant_policy = project.join("moonx-child.json");
    fs::write(
        &descendant_policy,
        serde_json::to_vec(&serde_json::json!({
            "env": { "from_host": ["*"] },
            "process": { "allow": [{ "program": moonx }] },
        }))
        .unwrap(),
    )
    .unwrap();
    snapbox::cmd::Command::new(&moonx)
        .current_dir(project)
        .env("MOON_HOME", &moon_home)
        .env("MOONRUN_OVERRIDE", runner)
        .env_remove(moonrun::ipc::ENDPOINT_ENV)
        .args(["--daemon", "--experimental-policy"])
        .arg(&descendant_policy)
        .args(["test/ipc/main@0.0.1", "moonx-probe"])
        .arg(&moonx)
        .arg("test/ipc/main@0.0.1")
        .timeout(Duration::from_secs(60))
        .assert()
        .success()
        .stdout_eq("moonx reused the session\n");

    let script = project.join("entry.mbtx");
    fs::write(
        &script,
        r#"----
import { "moonbitlang/core/env" }
----
fn main { println(@env.args()[1]) }
"#,
    )
    .unwrap();
    snapbox::cmd::Command::new(&moonx)
        .env("MOONRUN_OVERRIDE", runner)
        .env_remove(moonrun::ipc::ENDPOINT_ENV)
        .arg("--daemon")
        .arg(&script)
        .arg("--daemon")
        .timeout(Duration::from_secs(60))
        .assert()
        .success()
        .stdout_eq("--daemon\n");

    let virtual_policy = project.join("virtual.toml");
    fs::write(&virtual_policy, "[process]\nvirtualize = true\n").unwrap();
    snapbox::cmd::Command::new(&moonx)
        .env("MOONRUN_OVERRIDE", runner)
        .env_remove(moonrun::ipc::ENDPOINT_ENV)
        .args(["--daemon", "--experimental-policy"])
        .arg(&virtual_policy)
        .arg(&script)
        .timeout(Duration::from_secs(60))
        .assert()
        .failure()
        .stderr_eq("Error: IPC virtual process sessions are not supported yet\n");
}
