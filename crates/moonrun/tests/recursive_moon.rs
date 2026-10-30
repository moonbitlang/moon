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

use moonrun::{Engine, GuestProcessMode, RunOptions, RunOutcome, WorkingDirectory};
use std::fs::{self, File};
use std::io::{Read, Seek};
use std::path::Path;
use std::time::Duration;

#[test]
fn recursive_moon_processes() {
    let Ok(moon) = std::env::var("MOONRUN_RECURSIVE_TEST_MOON") else {
        let moon = escargot::CargoBuild::new()
            .manifest_path(Path::new(env!("CARGO_MANIFEST_DIR")).join("../moon/Cargo.toml"))
            .bin("moon")
            .current_release()
            .run()
            .unwrap();
        snapbox::cmd::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "recursive_moon_processes", "--nocapture"])
            .env("MOONRUN_RECURSIVE_TEST_MOON", moon.path())
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
    let tree = Engine::default();
    // Compile against the pinned async implementation, then exercise its real
    // process APIs without changing guest imports or platform dispatch.
    let upstream =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../third_party/moonbitlang_async");
    let project = root.join("project");
    moon_test_util::test_dir::copy_tree(&upstream.join("src"), &project.join("src"), false)
        .unwrap();
    fs::copy(upstream.join("moon.mod"), project.join("moon.mod")).unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/test_cases/test_virtual_moon.in");
    moon_test_util::test_dir::copy_tree(
        &fixture.join("parent"),
        &project.join("src/virtual_parent"),
        false,
    )
    .unwrap();
    fs::create_dir(project.join("subdir")).unwrap();
    fs::copy(fixture.join("leaf.mbtx"), project.join("subdir/leaf.mbtx")).unwrap();
    // Both helper names resolve inside the guest's PATH, including recursive
    // calls. No process-global environment mutation is needed.
    let helper_moon = root.join(format!("moon{}", std::env::consts::EXE_SUFFIX));
    fs::hard_link(moon, &helper_moon).unwrap_or_else(|_| {
        fs::copy(moon, &helper_moon).unwrap();
    });
    let path = std::env::join_paths(
        std::iter::once(root.to_owned()).chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )),
    )
    .unwrap();
    let policy = root.join("parent-policy.json");
    fs::write(&policy, serde_json::to_vec(&serde_json::json!({
        "process": { "spawn": true }, "env": { "from_host": ["*"], "set": { "PATH": path.to_str().unwrap() } }
    })).unwrap()).unwrap();
    let stdout = tempfile::tempfile().unwrap();
    let stderr = tempfile::tempfile().unwrap();
    let parent = tree
        .spawn_moon(
            None,
            moon,
            &["run".into(), "src/virtual_parent".into()],
            RunOptions::default()
                .with_guest_process_mode(GuestProcessMode::Virtual)
                .with_policy_file(&policy)
                .with_working_directory(WorkingDirectory::Fixed(project.clone()))
                .with_stdio(
                    tempfile::tempfile().unwrap(),
                    stdout.try_clone().unwrap(),
                    stderr.try_clone().unwrap(),
                ),
        )
        .unwrap();
    let outcome = tree.wait(parent);
    let diagnostics = contents(stderr);
    assert_eq!(outcome.unwrap(), RunOutcome::Completed, "{diagnostics}");
    let output = contents(stdout);
    let lines: Vec<_> = output.lines().collect();
    assert_eq!(lines.len(), 5, "{output}\n{diagnostics}");
    assert_eq!(lines[0], parent.to_string());
    let ids = lines[..3]
        .iter()
        .map(|line| line.parse::<i32>().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 3);
    assert!(ids.iter().all(|pid| *pid >= 1 << 30));
    assert_eq!(&lines[3..], ["grandchild", "arg with spaces"]);
    assert!(tree.snapshot().is_empty(), "descendants were not reaped");

    snapbox::cmd::Command::new(moon)
        .current_dir(&project)
        .args(["run", "--virtual-process-tree", "--wasm-policy"])
        .arg(&policy)
        .arg("src/virtual_parent")
        .env("MOONRUN_OVERRIDE", snapbox::cmd::cargo_bin!("moonrun"))
        .timeout(Duration::from_secs(90))
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
[..]
[..]
[..]
grandchild
arg with spaces

"#]]);

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(&project)
        .arg("--virtual-process-tree")
        .arg("--policy")
        .arg(&policy)
        .arg(project.join("_build/wasm/debug/build/virtual_parent/virtual_parent.wasm"))
        .timeout(Duration::from_secs(90))
        .assert()
        .success()
        .stdout_eq(snapbox::str![[r#"
[..]
[..]
[..]
grandchild
arg with spaces

"#]]);
    let denied_policy = root.join("denied.json");
    fs::write(&denied_policy, b"{}").unwrap();
    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(&project)
        .args(["--virtual-process-tree", "--policy"])
        .arg(&denied_policy)
        .arg(project.join("_build/wasm/debug/build/virtual_parent/virtual_parent.wasm"))
        .arg("denied")
        .timeout(Duration::from_secs(10))
        .assert()
        .success()
        .stdout_eq("spawn denied\n");

    snapbox::cmd::Command::new(snapbox::cmd::cargo_bin!("moonrun"))
        .current_dir(&project)
        .args(["--virtual-process-tree", "--policy"])
        .arg(&policy)
        .arg(project.join("_build/wasm/debug/build/virtual_parent/virtual_parent.wasm"))
        .arg("orphan")
        .timeout(Duration::from_secs(30))
        .assert()
        .success()
        .stdout_eq("root spawning\nadopted child completed\n");
}

fn contents(mut file: File) -> String {
    file.rewind().unwrap();
    let mut output = String::new();
    file.read_to_string(&mut output).unwrap();
    output
}
