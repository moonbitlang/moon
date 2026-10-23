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

use std::process::{Command, Stdio};

fn moon_bin() -> std::path::PathBuf {
    // NOTE: keep consistent with existing tests.
    snapbox::cargo_bin!("moon").to_owned()
}

fn git_writes_allowed() -> bool {
    // Some environments (e.g. sandboxed runners) disallow creating `.git` directories.
    // `moon update`/`git clone` needs that, so skip the test in such environments.
    let dir = match tempfile::tempdir() {
        Ok(d) => d,
        Err(_) => return false,
    };
    std::fs::create_dir(dir.path().join(".git")).is_ok()
}

fn init_empty_local_registry() -> tempfile::TempDir {
    // Setup (offline): create a local bare git repo to act as registry index.
    // This avoids network usage while still exercising `moon update` / registry clone logic.
    let base = tempfile::tempdir().unwrap();
    let index_repo = base.path().join("git").join("index");
    std::fs::create_dir_all(&index_repo).unwrap();
    let out = Command::new("git")
        .current_dir(&index_repo)
        .args(["init", "--bare"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(out.status.success());
    base
}

fn write_minimal_moon_mod(dir: &std::path::Path) {
    std::fs::write(
        dir.join("moon.mod"),
        r#"name = "test/empty"

version = "0.0.1"
"#,
    )
    .unwrap();
}

#[test]
fn test_moon_add_no_update_skips_registry_update() {
    // Setup: isolated Moon project + empty MOON_HOME.
    let project = tempfile::tempdir().unwrap();
    write_minimal_moon_mod(project.path());

    let moon_home = tempfile::tempdir().unwrap();

    // Execute: ensure `--no-update` does not trigger registry index updates.
    let out = Command::new(moon_bin())
        .current_dir(project.path())
        .env("MOON_HOME", moon_home.path())
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args([
            "add",
            "--no-update",
            "this_user_should_not_exist/this_pkg_should_not_exist",
        ])
        .output()
        .unwrap();

    // Assert: command should fail (package doesn't exist), and no registry update message appears.
    assert!(!out.status.success());
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!combined.contains("Registry index"));
}

#[test]
fn test_moon_add_updates_registry_index_by_default() {
    if !git_writes_allowed() {
        return;
    }

    // Setup (offline): use a local registry so the test does not depend on network.
    let registry_base = init_empty_local_registry();

    // Setup: isolated Moon project + fresh MOON_HOME.
    let project = tempfile::tempdir().unwrap();
    write_minimal_moon_mod(project.path());

    let moon_home = tempfile::tempdir().unwrap();

    // Execute: default `moon add` should update the registry index.
    // Use a non-existent package so we don't depend on downloading package artifacts.
    let out = Command::new(moon_bin())
        .current_dir(project.path())
        .env("MOON_HOME", moon_home.path())
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOONCAKES_REGISTRY", registry_base.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args([
            "add",
            "this_user_should_not_exist/this_pkg_should_not_exist",
        ])
        .output()
        .unwrap();

    // Assert: `moon update`-style clone message is printed.
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("Registry index cloned successfully"),
        "moon add output:\n{combined}"
    );
    // Assert: error message should NOT suggest `moon update` because it was just updated.
    assert!(
        !combined.contains("Please consider running `moon update`"),
        "error message should be concise when updated:\n{combined}"
    );
}

#[test]
fn test_moon_add_no_update_preserves_authoritative_metadata_denial() {
    // Setup: isolated Moon project + empty MOON_HOME.
    let project = tempfile::tempdir().unwrap();
    write_minimal_moon_mod(project.path());

    let moon_home = tempfile::tempdir().unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let registry = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        use std::io::{Read, Write};
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let mut buffer = [0; 1024];
            let read = stream.read(&mut buffer).unwrap();
            assert_ne!(read, 0);
            request.extend_from_slice(&buffer[..read]);
        }
        assert!(request.starts_with(
            b"GET /api/v0/registry/modules/this_user_should_not_exist/this_pkg_should_not_exist "
        ));
        stream
            .write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            )
            .unwrap();
    });
    // --no-update skips Git, but a missing module still gets an API lookup.
    let out = Command::new(moon_bin())
        .current_dir(project.path())
        .env("MOON_HOME", moon_home.path())
        .env("MOONCAKES_REGISTRY", registry)
        .env("NO_PROXY", "*")
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args([
            "add",
            "--no-update",
            "this_user_should_not_exist/this_pkg_should_not_exist",
        ])
        .output()
        .unwrap();

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    server.join().unwrap();
    // The API already answered; do not replace its denial with stale-index advice.
    assert!(
        combined.contains("not found or access denied"),
        "error message should preserve the API denial:\n{combined}"
    );
    assert!(!combined.contains("Please consider running `moon update`"));
}
