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

use super::*;

#[test]
fn test_moon_install_major_version_root_uses_base_name() {
    let dir = TestDir::new_empty();
    std::fs::write(
        dir.join("moon.mod"),
        "name = \"a/b/v2\"\nversion = \"2.0.0\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("moon.pkg"), "options(\"is-main\": true)\n").unwrap();
    std::fs::write(dir.join("main.mbt"), "fn main { println(\"v2\") }\n").unwrap();
    let install_dir = dir.join("bin");

    moon_cmd(&dir)
        .args(["install", "--path", ".", "--bin"])
        .arg(&install_dir)
        .assert()
        .success();
    let binary = install_dir.join(format!("b{}", std::env::consts::EXE_SUFFIX));
    snapbox::cmd::Command::new(binary)
        .assert()
        .success()
        .stdout_eq("v2\n");
}

#[test]
fn test_moon_install_global_deprecated_warning() {
    // Test that running `moon install` without arguments shows deprecation warning
    let dir = TestDir::new("moon_install_global.in");

    // Running moon install without arguments should show deprecation warning
    let stderr = get_stderr(&dir, ["install"]);
    assert!(
        stderr.contains("deprecated"),
        "Expected deprecation warning in stderr, got: {}",
        stderr
    );
}

#[test]
fn test_moon_install_global_deprecated_uses_workspace_context() {
    let dir = TestDir::new("workspace_basic.in");

    let stderr = get_stderr(&dir, ["-C", "app", "install"]);
    assert!(
        stderr.contains("deprecated"),
        "Expected deprecation warning in stderr, got: {}",
        stderr
    );
}

#[test]
fn test_moon_install_global_local_path() {
    // Test installing from local path using --path
    let dir = TestDir::new("moon_install_global.in");

    // Create a temporary directory for installation
    let install_dir = dir.join("test_bin");
    std::fs::create_dir_all(&install_dir).unwrap();

    // Install using --path
    let _output = get_stdout(
        &dir,
        [
            "install",
            "--path",
            "src/main",
            "--bin",
            install_dir.to_str().unwrap(),
        ],
    );

    // Check that the binary was created
    #[cfg(unix)]
    let binary_path = install_dir.join("main");
    #[cfg(target_os = "windows")]
    let binary_path = install_dir.join("main.exe");

    assert!(
        binary_path.exists(),
        "Expected binary at {:?} to exist",
        binary_path
    );
}

#[test]
fn test_moon_install_global_local_path_uses_workspace_context() {
    let dir = TestDir::new("workspace_basic.in");
    let install_dir = dir.join("test_bin_install_workspace");
    std::fs::create_dir_all(&install_dir).unwrap();

    let _output = get_stdout(
        &dir,
        [
            "-C",
            "app",
            "install",
            "--path",
            "src/main",
            "--bin",
            install_dir.to_str().unwrap(),
        ],
    );

    #[cfg(unix)]
    let binary_path = install_dir.join("main");
    #[cfg(target_os = "windows")]
    let binary_path = install_dir.join("main.exe");

    assert!(
        binary_path.exists(),
        "Expected binary at {:?} to exist",
        binary_path
    );
}

#[test]
fn test_moon_install_global_local_path_renders_build_errors() {
    let dir = TestDir::new("moon_install_global_error.in");
    let install_dir = dir.join("test_bin");
    std::fs::create_dir_all(&install_dir).unwrap();

    let stderr = get_err_stderr(
        &dir,
        [
            "install",
            "--path",
            "src/main",
            "--bin",
            install_dir.to_str().unwrap(),
        ],
    );

    assert!(
        stderr.contains("Error: ["),
        "Expected rendered diagnostic in stderr, got: {stderr}",
    );
    assert!(
        stderr.contains("$ROOT/src/main/main.mbt"),
        "Expected source location in stderr, got: {stderr}",
    );
    assert!(
        stderr.contains("Expr Type Mismatch"),
        "Expected compile error message in stderr, got: {stderr}",
    );
    assert!(
        !stderr.contains("\"$message_type\":\"diagnostic\""),
        "Expected rendered diagnostics instead of raw JSON, got: {stderr}",
    );
}

#[test]
fn test_moon_install_global_defaults_to_moon_home_bin() {
    let dir = TestDir::new("moon_install_global.in");
    let moon_home = tempfile::tempdir().unwrap();

    let _output = get_stdout_with_envs(
        &dir,
        ["install", "--path", "src/main"],
        [("MOON_HOME", moon_home.path().to_string_lossy().into_owned())],
    );

    #[cfg(unix)]
    let binary_path = moon_home.path().join("bin").join("main");
    #[cfg(target_os = "windows")]
    let binary_path = moon_home.path().join("bin").join("main.exe");

    assert!(
        binary_path.exists(),
        "Expected binary at {:?} to exist",
        binary_path
    );
}

#[test]
fn test_moon_install_global_local_path_module_root_is_exact_path() {
    let dir = TestDir::new("moon_install_global.in");

    let stderr = get_err_stderr(&dir, ["install", "--path", "."]);
    assert!(
        stderr.contains("is not a main package"),
        "Expected exact local path behavior in stderr, got: {}",
        stderr
    );
}

#[test]
fn test_moon_install_global_local_source_wildcard_from_module_root() {
    let dir = TestDir::new("moon_install_global.in");
    let install_dir = dir.join("test_bin_wildcard_root");
    std::fs::create_dir_all(&install_dir).unwrap();

    let _output = get_stdout(
        &dir,
        ["install", "./...", "--bin", install_dir.to_str().unwrap()],
    );

    #[cfg(unix)]
    let binary_path = install_dir.join("main");
    #[cfg(target_os = "windows")]
    let binary_path = install_dir.join("main.exe");

    assert!(
        binary_path.exists(),
        "Expected binary at {:?} to exist",
        binary_path
    );
}

#[test]
fn test_moon_install_global_local_path_wildcard_with_path_flag_warns() {
    let dir = TestDir::new("moon_install_global.in");

    let stderr = get_err_stderr(&dir, ["install", "--path", "src/..."]);
    assert!(
        stderr.contains("does not support wildcard selectors like `src/...`"),
        "Expected wildcard warning in stderr, got: {}",
        stderr
    );
    assert!(
        stderr.contains("Use positional SOURCE for wildcard install: `moon install src/...`"),
        "Expected guidance for positional SOURCE in stderr, got: {}",
        stderr
    );
}

struct ArchiveResponse {
    status: &'static str,
    location: Option<&'static str>,
    body: Vec<u8>,
}

fn serve_github_archive(
    responses: Vec<ArchiveResponse>,
) -> (String, std::thread::JoinHandle<Vec<String>>) {
    use std::io::Read;
    use std::time::{Duration, Instant};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for response in responses {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            return requests;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("archive server failed: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut buffer).unwrap();
                assert_ne!(read, 0, "request ended before its headers");
                request.extend_from_slice(&buffer[..read]);
            }
            requests.push(String::from_utf8(request).unwrap());
            write!(
                stream,
                "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                response.status,
                response.body.len()
            )
            .unwrap();
            if let Some(location) = response.location {
                write!(stream, "Location: {location}\r\n").unwrap();
            }
            stream.write_all(b"\r\n").unwrap();
            stream.write_all(&response.body).unwrap();
        }
        requests
    });
    (proxy, server)
}

fn github_source_archive() -> Vec<u8> {
    let gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(gzip);
    // The wrapper can change when a repository is renamed. The module is nested
    // and its path contains a space, just as it does in the pasted permalink.
    for (path, contents) in [
        (
            "renamed-repository/examples/hello world/moon.mod",
            "name = \"test/permalink\"\n",
        ),
        (
            "renamed-repository/examples/hello world/moon.pkg",
            "options(\"is-main\": true)\n",
        ),
        (
            "renamed-repository/examples/hello world/main.mbt",
            "fn main { println(\"pinned archive\") }\n",
        ),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, path, contents.as_bytes())
            .unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap()
}

#[test]
fn test_moon_install_global_hosted_permalink_downloads_pinned_archive_without_git() {
    let (proxy, server) = serve_github_archive(vec![
        ArchiveResponse {
            status: "302 Found",
            location: Some(
                "http://codeload.github.com/owner/repo/tar.gz/0123456789abcdef0123456789abcdef01234567",
            ),
            body: Vec::new(),
        },
        ArchiveResponse {
            status: "200 OK",
            location: None,
            body: github_source_archive(),
        },
    ]);
    let dir = TestDir::new_empty();
    let install_dir = dir.join("bin");
    let assert = snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .env("MOON_TOOLCHAIN_ROOT", toolchain_root_for_tests())
        .env("MOON_DEP_CACHE", "off")
        .env("MOON_GIT_OVERRIDE", dir.join("git-must-not-run"))
        .env("HTTP_PROXY", &proxy)
        .env("http_proxy", &proxy)
        .env("NO_PROXY", "")
        .env("no_proxy", "")
        .args([
            "install",
            "http://github.com/owner/repo/tree/0123456789abcdef0123456789abcdef01234567/examples/hello%20world",
            "--bin",
        ])
        .arg(&install_dir)
        .assert();
    let requests = server.join().unwrap();
    assert.success();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].starts_with("GET http://github.com/owner/repo/archive/0123456789abcdef0123456789abcdef01234567.tar.gz HTTP/1.1\r\n"));
    assert!(requests[1].starts_with("GET http://codeload.github.com/owner/repo/tar.gz/0123456789abcdef0123456789abcdef01234567 HTTP/1.1\r\n"));
    snapbox::cmd::Command::new(
        install_dir.join(format!("permalink{}", std::env::consts::EXE_SUFFIX)),
    )
    .assert()
    .success()
    .stdout_eq("pinned archive\n");
}

#[test]
fn test_moon_install_global_hosted_archive_failures_do_not_fall_back_to_git() {
    for (status, body, path, stderr) in [
        (
            "404 Not Found",
            b"not found".to_vec(),
            "cmd/tool",
            snapbox::str![[r#"
Error: Failed to download GitHub source archive; only public repositories are supported

Caused by:
    HTTP status client error (404 Not Found) for url (http://github.com/owner/repo/archive/0123456789abcdef0123456789abcdef01234567.tar.gz)

"#]],
        ),
        (
            "200 OK",
            b"not an archive".to_vec(),
            "cmd/tool",
            snapbox::str![[r#"
Error: Failed to extract GitHub source archive

Caused by:
    0: failed to iterate over archive
    1: invalid gzip header

"#]],
        ),
        (
            "200 OK",
            github_source_archive(),
            "excluded",
            snapbox::str![[r#"
Error: Failed to install from the GitHub source archive

Caused by:
    Path `excluded` does not exist in the repository

"#]],
        ),
        (
            "200 OK",
            github_source_archive(),
            "..%2F..%2F",
            snapbox::str![[r#"
Error: Failed to install from the GitHub source archive

Caused by:
    Path `../../` escapes repository root

"#]],
        ),
    ] {
        let (proxy, server) = serve_github_archive(vec![ArchiveResponse {
            status,
            location: None,
            body,
        }]);
        let dir = TestDir::new_empty();
        let assert = snapbox::cmd::Command::new(moon_bin())
            .current_dir(&dir)
            .env("MOON_GIT_OVERRIDE", dir.join("git-must-not-run"))
            .env("HTTP_PROXY", &proxy)
            .env("http_proxy", &proxy)
            .env("NO_PROXY", "")
            .env("no_proxy", "")
            .arg("install")
            .arg(format!(
                "http://github.com/owner/repo/tree/0123456789abcdef0123456789abcdef01234567/{path}"
            ))
            .arg("--bin")
            .arg(dir.join("bin"))
            .assert();
        let requests = server.join().unwrap();
        assert.failure().stdout_eq("").stderr_eq(stderr);
        assert_eq!(requests.len(), 1);
        assert!(!dir.join("bin").exists());
    }
}

#[test]
fn test_moon_install_global_hosted_tree_url_rejects_extra_path() {
    let dir = TestDir::new_empty();

    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args([
            "install",
            "https://github.com/owner/repo/tree/0123456789abcdef0123456789abcdef01234567/cmd/tool",
            "other/path",
        ])
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Error: PATH_IN_REPO must not be used when SOURCE already contains a /tree/... path

"#]]);
}

#[test]
fn test_moon_install_global_hosted_permalink_rejects_ref_overrides() {
    let dir = TestDir::new_empty();
    for flag in ["--branch", "--tag", "--rev"] {
        snapbox::cmd::Command::new(moon_bin())
            .current_dir(&dir)
            .args([
                "install",
                "https://github.com/owner/repo/tree/0123456789abcdef0123456789abcdef01234567/cmd/tool",
                flag,
                "other-ref",
            ])
            .assert()
            .failure()
            .stdout_eq("")
            .stderr_eq(snapbox::str![[r#"
Error: --rev, --branch, and --tag must not be used with a tree permalink; the URL already selects a commit

"#]]);
    }
}

#[test]
fn test_moon_install_global_hosted_tree_url_requires_permalink() {
    let dir = TestDir::new_empty();
    snapbox::cmd::Command::new(moon_bin())
        .current_dir(&dir)
        .args([
            "install",
            "https://github.com/owner/repo/tree/feature/new-ui/cmd/tool",
        ])
        .assert()
        .failure()
        .stdout_eq("")
        .stderr_eq(snapbox::str![[r#"
Error: Tree URLs must contain a full 40-character commit SHA; copy a permalink, or use the repository URL with PATH_IN_REPO and --branch, --tag, or --rev

"#]]);
}

#[test]
fn test_moon_install_global_git_url_default_root_package() {
    // Test installing from git URL without PATH_IN_REPO.
    // Default behavior installs the module root package only.
    let install_dir = tempfile::tempdir().unwrap();
    let install_path = install_dir.path();
    let work_dir = tempfile::tempdir().unwrap();

    // Install root package only
    get_stdout(
        &work_dir,
        [
            "install",
            "https://github.com/moonbitlang/moon-install-git-test-cases.git",
            "--bin",
            install_path.to_str().unwrap(),
        ],
    );

    // Check that only root package binary was created
    #[cfg(unix)]
    {
        assert!(install_path.join("install-test").exists());
        assert!(!install_path.join("hello").exists());
        assert!(!install_path.join("tool1").exists());
        assert!(!install_path.join("tool2").exists());
    }
    #[cfg(target_os = "windows")]
    {
        assert!(install_path.join("install-test.exe").exists());
        assert!(!install_path.join("hello.exe").exists());
        assert!(!install_path.join("tool1.exe").exists());
        assert!(!install_path.join("tool2.exe").exists());
    }
}

#[test]
fn test_moon_install_global_git_url_specific_package() {
    // Test installing specific package from git URL
    let install_dir = tempfile::tempdir().unwrap();
    let install_path = install_dir.path();
    let work_dir = tempfile::tempdir().unwrap();

    // Install only cmd/tool1
    get_stdout(
        &work_dir,
        [
            "install",
            "https://github.com/moonbitlang/moon-install-git-test-cases.git",
            "cmd/tool1",
            "--bin",
            install_path.to_str().unwrap(),
        ],
    );

    // Check that only tool1 was installed
    #[cfg(unix)]
    {
        assert!(install_path.join("tool1").exists());
        assert!(!install_path.join("tool2").exists());
        assert!(!install_path.join("hello").exists());
        assert!(!install_path.join("install-test").exists());
    }
    #[cfg(target_os = "windows")]
    {
        assert!(install_path.join("tool1.exe").exists());
        assert!(!install_path.join("tool2.exe").exists());
        assert!(!install_path.join("hello.exe").exists());
        assert!(!install_path.join("install-test.exe").exists());
    }
}

#[test]
fn test_moon_install_global_git_url_wildcard() {
    // Test installing with wildcard pattern from git URL
    let install_dir = tempfile::tempdir().unwrap();
    let install_path = install_dir.path();
    let work_dir = tempfile::tempdir().unwrap();

    // Install cmd/... (should install tool1 and tool2)
    get_stdout(
        &work_dir,
        [
            "install",
            "https://github.com/moonbitlang/moon-install-git-test-cases.git",
            "cmd/...",
            "--bin",
            install_path.to_str().unwrap(),
        ],
    );

    // Check that tool1 and tool2 were installed, but not others
    #[cfg(unix)]
    {
        assert!(install_path.join("tool1").exists());
        assert!(install_path.join("tool2").exists());
        assert!(!install_path.join("hello").exists());
        assert!(!install_path.join("install-test").exists());
    }
    #[cfg(target_os = "windows")]
    {
        assert!(install_path.join("tool1.exe").exists());
        assert!(install_path.join("tool2.exe").exists());
        assert!(!install_path.join("hello.exe").exists());
        assert!(!install_path.join("install-test.exe").exists());
    }
}

#[test]
fn test_moon_install_global_git_url_root_wildcard() {
    // Test installing all packages from git URL using /...
    let install_dir = tempfile::tempdir().unwrap();
    let install_path = install_dir.path();
    let work_dir = tempfile::tempdir().unwrap();

    get_stdout(
        &work_dir,
        [
            "install",
            "https://github.com/moonbitlang/moon-install-git-test-cases.git",
            "/...",
            "--bin",
            install_path.to_str().unwrap(),
        ],
    );

    #[cfg(unix)]
    {
        assert!(install_path.join("install-test").exists());
        assert!(install_path.join("hello").exists());
        assert!(install_path.join("tool1").exists());
        assert!(install_path.join("tool2").exists());
    }
    #[cfg(target_os = "windows")]
    {
        assert!(install_path.join("install-test.exe").exists());
        assert!(install_path.join("hello.exe").exists());
        assert!(install_path.join("tool1.exe").exists());
        assert!(install_path.join("tool2.exe").exists());
    }
}
