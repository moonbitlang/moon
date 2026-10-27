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

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

struct Server {
    api: String,
    requests: Arc<Mutex<Vec<String>>>,
    responses: Arc<Mutex<HashMap<String, (u16, String)>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn new(releases: &[(&str, &str, serde_json::Value)]) -> Self {
        let releases: HashMap<_, _> = releases
            .iter()
            .map(|(module, version, deps)| {
                (format!("/api/v0/registry/modules/{module}?version={version}"),
                serde_json::json!({"module": module, "visibility": "PUBLIC", "version": version,
                    "deps": deps, "checksum": "a".repeat(64)}).to_string())
            })
            .collect();
        let responses = Arc::new(Mutex::new(
            releases
                .into_iter()
                .map(|(path, body)| (path, (200, body)))
                .collect::<HashMap<_, _>>(),
        ));
        let served = responses.clone();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                let mut stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                    if line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                let target = request.split_whitespace().nth(1).unwrap();
                let (status, body) = served
                    .lock()
                    .unwrap()
                    .get(target)
                    .cloned()
                    .unwrap_or((404, "{}".to_owned()));
                recorded.lock().unwrap().push(request);
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        Self {
            api,
            requests,
            responses,
            stop,
            thread: Some(thread),
        }
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if self.thread.is_some() {
            self.stop();
        }
    }
}

fn moon(project: &Path, home: &Path, api: bool) -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(project)
        .env("MOON_HOME", home)
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_REGISTRY_API", if api { "1" } else { "0" })
        .env("MOON_DEP_CACHE", "off")
        .env("NO_PROXY", "*")
        .env("no_proxy", "*")
        .env("HTTP_PROXY", "none://")
        .env("HTTPS_PROXY", "none://")
        .env("ALL_PROXY", "none://")
        .env("http_proxy", "none://")
        .env("https_proxy", "none://")
        .env("all_proxy", "none://")
        .env_remove("MOONCAKES_REGISTRY")
        .env_remove("MOON_REGISTRY_CACHE")
        .env_remove("MOON_WORK")
}

fn configure(home: &Path, api: &str) {
    std::fs::write(
        home.join("config.json"),
        serde_json::json!({
            "api": api, "download": api, "index": home.join("nonexistent-index"),
        })
        .to_string(),
    )
    .unwrap();
}

#[test]
fn api_mode_preserves_exact_mvs_traversal_and_offline_resolution() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut server = Server::new(&[
        (
            "alice/a",
            "0.1.0",
            serde_json::json!({"alice/shared": "0.1.0"}),
        ),
        (
            "alice/shared",
            "0.1.0",
            serde_json::json!({"alice/leaf": "0.1.0"}),
        ),
        ("alice/shared", "0.1.5", serde_json::json!({})),
        ("alice/leaf", "0.1.0", serde_json::json!({})),
    ]);
    configure(home.path(), &server.api);
    std::fs::write(
        project.path().join("moon.mod"),
        "name = \"test/app\"\nimport { \"alice/a@0.1.0\", \"alice/shared@0.1.5\" }\n",
    )
    .unwrap();
    let output = moon(project.path(), home.path(), true)
        .args(["tree", "--json"])
        .assert()
        .success();
    let report: serde_json::Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(report["status"], "success");
    assert!(
        report["modules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|module| module["name"] == "alice/shared" && module["version"] == "0.1.5")
    );
    server.stop();
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    assert!(
        requests
            .iter()
            .all(|r| r.contains("?version=") && !r.to_lowercase().contains("authorization:"))
    );
    assert!(
        requests
            .iter()
            .any(|r| r.starts_with("GET /api/v0/registry/modules/alice/shared?version=0.1.0 "))
    );
    assert!(
        requests
            .iter()
            .any(|r| r.starts_with("GET /api/v0/registry/modules/alice/leaf?version=0.1.0 "))
    );
    drop(requests);
    let offline = moon(project.path(), home.path(), true)
        .args(["tree", "--json"])
        .assert()
        .success();
    let offline: serde_json::Value = serde_json::from_slice(&offline.get_output().stdout).unwrap();
    assert_eq!(report["modules"], offline["modules"]);
    assert_eq!(report["edges"], offline["edges"]);
    assert!(!home.path().join("registry/index").exists());
}

#[test]
fn missing_declared_version_is_not_replaced_by_an_available_newer_release() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut server = Server::new(&[("alice/shared", "0.1.5", serde_json::json!({}))]);
    configure(home.path(), &server.api);
    std::fs::write(
        project.path().join("moon.mod"),
        "name = \"test/app\"\nimport { \"alice/shared@0.1.1\" }\n",
    )
    .unwrap();
    let output = moon(project.path(), home.path(), true)
        .args(["tree", "--json"])
        .assert()
        .failure();
    let report: serde_json::Value = serde_json::from_slice(&output.get_output().stdout).unwrap();
    assert_eq!(report["status"], "failure");
    assert!(
        report["error"]
            .as_str()
            .unwrap()
            .contains("not found or access denied")
    );
    server.stop();
    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET /api/v0/registry/modules/alice/shared?version=0.1.1 "));
}

#[test]
fn api_mode_is_opt_in_and_ignores_existing_index_contents() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut server = Server::new(&[("alice/shared", "0.1.0", serde_json::json!({}))]);
    configure(home.path(), &server.api);
    let index = home.path().join("registry/index/user/alice/shared.index");
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    std::fs::write(
        &index,
        serde_json::json!({"version": "0.1.0", "deps": {}, "checksum": "b".repeat(64)}).to_string(),
    )
    .unwrap();
    std::fs::write(
        project.path().join("moon.mod"),
        "name = \"test/app\"\nimport { \"alice/shared@0.1.0\" }\n",
    )
    .unwrap();
    moon(project.path(), home.path(), false)
        .env_remove("MOON_REGISTRY_API")
        .args(["tree"])
        .assert()
        .success();
    assert!(server.requests.lock().unwrap().is_empty());
    std::fs::write(&index, "invalid index metadata").unwrap();
    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success();
    server.stop();
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}

#[test]
fn http_commands_distinguish_refresh_from_cached_metadata() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut server = Server::new(&[("alice/shared", "0.1.0", serde_json::json!({}))]);
    configure(home.path(), &server.api);
    let manifest = "name = \"test/app\"\nimport { \"alice/shared@0.1.0\" }\n";
    std::fs::write(project.path().join("moon.mod"), manifest).unwrap();
    // Existing materialized sources let checks exercise metadata reuse without
    // conflating it with an archive download.
    let dependency = project.path().join(".mooncakes/alice/shared");
    std::fs::create_dir_all(&dependency).unwrap();
    std::fs::write(
        dependency.join("moon.mod"),
        "name = \"alice/shared\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(dependency.join("moon.pkg"), "").unwrap();
    std::fs::write(
        dependency.join("lib.mbt"),
        "pub fn answer() -> Int { 42 }\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("moon.pkg"),
        "import { \"alice/shared\" }\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("lib.mbt"),
        "pub fn answer() -> Int { @shared.answer() }\n",
    )
    .unwrap();

    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success();
    let exact = "/api/v0/registry/modules/alice/shared?version=0.1.0";
    let latest = "/api/v0/registry/modules/alice/shared";
    assert_eq!(server.requests.lock().unwrap().len(), 1);

    // A pinned command refreshes even after tree has cached the same release.
    moon(project.path(), home.path(), true)
        .args(["add", "--upgrade", "alice/shared@0.1.0"])
        .assert()
        .success();
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    moon(project.path(), home.path(), true)
        .args(["add", "--upgrade", "--no-update", "alice/shared@0.1.0"])
        .assert()
        .success();
    // An exact record alone cannot answer an unversioned request.
    moon(project.path(), home.path(), true)
        .args(["add", "--upgrade", "--no-update", "alice/shared"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: No cached latest version; retry without `--no-update` to query the registry.
...
"#]]);
    assert_eq!(server.requests.lock().unwrap().len(), 2);

    let response = server.responses.lock().unwrap().get(exact).unwrap().clone();
    server
        .responses
        .lock()
        .unwrap()
        .insert(latest.into(), response);
    moon(project.path(), home.path(), true)
        .args(["add", "--upgrade", "alice/shared"])
        .assert()
        .success();
    assert_eq!(server.requests.lock().unwrap().len(), 3);

    for (target, coordinate) in [(exact, "alice/shared@0.1.0"), (latest, "alice/shared")] {
        server
            .responses
            .lock()
            .unwrap()
            .insert(target.into(), (503, "{}".into()));
        moon(project.path(), home.path(), true)
            .args(["add", "--upgrade", coordinate])
            .assert()
            .success()
            .stderr_eq(snapbox::str![[r#"
Warning: failed to [..], using cached [..]
Warning: dependency `alice/shared` is already at version 0.1.0

"#]]);
        server
            .responses
            .lock()
            .unwrap()
            .insert(target.into(), (404, "{}".into()));
        moon(project.path(), home.path(), true)
            .args(["add", "--upgrade", coordinate])
            .assert()
            .failure()
            .stderr_eq("Error: Module `alice/shared` not found or access denied\n");
    }
    assert_eq!(server.requests.lock().unwrap().len(), 7);
    server.stop();
    // Cached latest remains a usable explicit choice, even though it cannot
    // override a failed authoritative refresh.
    moon(project.path(), home.path(), true)
        .args(["add", "--upgrade", "--no-update", "alice/shared"])
        .assert()
        .success();
    for args in [vec!["check"], vec!["check", "--frozen"]] {
        moon(project.path(), home.path(), true)
            .args(args)
            .assert()
            .success();
    }
    assert_eq!(
        std::fs::read_to_string(project.path().join("moon.mod")).unwrap(),
        manifest
    );
    assert!(!home.path().join("registry/index").exists());
}

#[test]
fn no_update_allows_acquiring_missing_exact_metadata() {
    use sha2::{Digest, Sha256};
    for command in ["add", "fetch"] {
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let mut server = Server::new(&[]);
        configure(home.path(), &server.api);
        std::fs::write(project.path().join("moon.mod"), "name = \"test/app\"\n").unwrap();
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("moon.mod", zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(b"name = \"alice/shared\"\nversion = \"0.1.0\"\n")
            .unwrap();
        let archive = zip.finish().unwrap().into_inner();
        let layout = moonutil::MoonHomeLayout::new(home.path().to_owned());
        let path =
            layout.registry_source_archive_path(&"alice/shared".into(), &"0.1.0".parse().unwrap());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, &archive).unwrap();
        server.responses.lock().unwrap().insert(
            "/api/v0/registry/modules/alice/shared?version=0.1.0".into(),
            (200, serde_json::json!({
                "module": "alice/shared", "visibility": "PUBLIC", "version": "0.1.0", "deps": {},
                "checksum": format!("{:x}", Sha256::digest(&archive)),
            }).to_string()),
        );
        moon(project.path(), home.path(), true)
            .args([command, "--no-update", "alice/shared@0.1.0"])
            .assert()
            .success();
        server.stop();
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].starts_with("GET /api/v0/registry/modules/alice/shared?version=0.1.0 ")
        );
        let source = match command {
            "add" => project.path().join(".mooncakes/alice/shared/moon.mod"),
            "fetch" => project.path().join(".repos/alice/shared/0.1.0/moon.mod"),
            _ => unreachable!(),
        };
        assert!(source.is_file());
    }
}

#[test]
fn update_refreshes_selected_status_and_latest_without_changing_the_project() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut server = Server::new(&[("alice/shared", "0.1.0", serde_json::json!({}))]);
    configure(home.path(), &server.api);
    let manifest = "name = \"test/app\"\nimport { \"alice/shared@0.1.0\" }\n";
    std::fs::write(project.path().join("moon.mod"), manifest).unwrap();
    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success();
    let exact = "/api/v0/registry/modules/alice/shared?version=0.1.0";
    let latest = "/api/v0/registry/modules/alice/shared";
    let release = serde_json::json!({
        "module": "alice/shared", "visibility": "PUBLIC", "version": "0.1.0",
        "deps": {}, "checksum": "a".repeat(64), "yanked": true,
        "yanked_reason": "use the new release",
    });
    server
        .responses
        .lock()
        .unwrap()
        .insert(exact.into(), (200, release.to_string()));
    server.responses.lock().unwrap().insert(
        latest.into(),
        (
            200,
            serde_json::json!({
                "module": "alice/shared", "visibility": "PUBLIC", "version": "0.2.0",
                "deps": {}, "checksum": "b".repeat(64),
            })
            .to_string(),
        ),
    );
    moon(project.path(), home.path(), true)
        .args(["update", "--quiet"])
        .assert()
        .success()
        .stdout_eq("alice/shared@0.1.0: latest 0.2.0 (update available); deprecated: use the new release\n")
        .stderr_eq("");
    assert_eq!(server.requests.lock().unwrap().len(), 3);
    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success()
        .stderr_eq("Warning: Dependency `alice/shared@0.1.0` is deprecated: use the new release\n");
    let mut restored = release;
    restored["yanked"] = false.into();
    restored["yanked_reason"] = serde_json::Value::Null;
    server
        .responses
        .lock()
        .unwrap()
        .insert(exact.into(), (200, restored.to_string()));
    moon(project.path(), home.path(), true)
        .args(["update"])
        .assert()
        .success()
        .stdout_eq("alice/shared@0.1.0: latest 0.2.0 (update available)\n")
        .stderr_eq("");
    server.stop();
    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success()
        .stderr_eq("");
    assert_eq!(
        std::fs::read_to_string(project.path().join("moon.mod")).unwrap(),
        manifest
    );
    assert!(!project.path().join(".mooncakes").exists());
    assert!(!home.path().join("registry/cache").exists());
}

#[test]
fn update_reports_partial_failure_and_keeps_successful_refreshes() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut server = Server::new(&[
        ("alice/a", "0.1.0", serde_json::json!({})),
        ("alice/b", "0.1.0", serde_json::json!({})),
    ]);
    configure(home.path(), &server.api);
    std::fs::write(
        project.path().join("moon.mod"),
        "name = \"test/app\"\nimport { \"alice/a@0.1.0\", \"alice/b@0.1.0\" }\n",
    )
    .unwrap();
    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success();
    for name in ["alice/a", "alice/b"] {
        server.responses.lock().unwrap().insert(
            format!("/api/v0/registry/modules/{name}"),
            (
                200,
                serde_json::json!({
                    "module": name, "visibility": "PUBLIC", "version": "0.2.0",
                    "deps": {}, "checksum": "b".repeat(64),
                })
                .to_string(),
            ),
        );
    }
    server.responses.lock().unwrap().insert(
        "/api/v0/registry/modules/alice/a?version=0.1.0".into(),
        (503, "{}".into()),
    );
    server.responses.lock().unwrap().insert(
        "/api/v0/registry/modules/alice/b?version=0.1.0".into(),
        (
            200,
            serde_json::json!({
                "module": "alice/b", "visibility": "PUBLIC", "version": "0.1.0",
                "deps": {}, "checksum": "a".repeat(64), "yanked": true,
            })
            .to_string(),
        ),
    );
    moon(project.path(), home.path(), true)
        .args(["update"])
        .assert()
        .failure()
        .stdout_eq("alice/a@0.1.0: latest 0.2.0 (update available); status refresh failed\nalice/b@0.1.0: latest 0.2.0 (update available); deprecated\n")
        .stderr_eq(snapbox::str![[r#"
Error: registry refresh incomplete:
  alice/a@0.1.0: [..]503[..]

"#]]);
    server.stop();
    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success()
        .stderr_eq("Warning: Dependency `alice/b@0.1.0` is deprecated.\n");
}

#[test]
fn metadata_cache_migrates_and_can_be_cleaned_independently() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut server = Server::new(&[("alice/shared", "0.1.0", serde_json::json!({}))]);
    configure(home.path(), &server.api);
    let latest =
        server.responses.lock().unwrap()["/api/v0/registry/modules/alice/shared?version=0.1.0"]
            .clone();
    server
        .responses
        .lock()
        .unwrap()
        .insert("/api/v0/registry/modules/alice/shared".into(), latest);
    std::fs::write(
        project.path().join("moon.mod"),
        "name = \"test/app\"\nimport { \"alice/shared@0.1.0\" }\n",
    )
    .unwrap();
    moon(project.path(), home.path(), true)
        .args(["update"])
        .assert()
        .success();
    let metadata = home.path().join("cache/registry/metadata");
    assert!(metadata.join(".moon-cache").is_file());
    let legacy = home.path().join("registry/metadata");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::rename(metadata.join("v1"), legacy.join("v1")).unwrap();
    std::fs::remove_dir_all(&metadata).unwrap();
    server.stop();
    // The previous layout remains usable without contacting the server.
    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success();
    assert!(metadata.join("v1").is_dir());
    assert!(!legacy.join("v1").exists());
    // Latest remains a usable observation after migration too. The existing
    // dependency makes this an add no-op, without source acquisition.
    moon(project.path(), home.path(), true)
        .args(["add", "--no-update", "alice/shared"])
        .assert()
        .success();
    for name in ["index", "cache"] {
        let path = home.path().join("registry").join(name);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("must-survive"), "other registry state").unwrap();
    }
    moon(project.path(), home.path(), true)
        .args(["clean"])
        .assert()
        .success();
    assert!(metadata.join("v1").is_dir());
    moon(home.path(), home.path(), true)
        .args(["clean", "--registry-cache"])
        .assert()
        .success()
        .stdout_eq("")
        .stderr_eq("");
    assert!(!metadata.exists());
    assert!(!legacy.exists());
    for name in ["index", "cache"] {
        assert!(
            home.path()
                .join("registry")
                .join(name)
                .join("must-survive")
                .is_file()
        );
    }
}

#[test]
fn update_refreshes_only_selected_workspace_versions_once() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let server = Server::new(&[
        ("alice/shared", "0.1.0", serde_json::json!({})),
        ("alice/shared", "0.2.0", serde_json::json!({})),
    ]);
    configure(home.path(), &server.api);
    let workspace = "members = [\"./a\", \"./b\"]\n";
    std::fs::write(project.path().join("moon.work"), workspace).unwrap();
    for (member, imports) in [
        ("a", "\"test/b@0.1.0\", \"alice/shared@0.1.0\""),
        ("b", "\"alice/shared@0.2.0\""),
    ] {
        let path = project.path().join(member);
        std::fs::create_dir(&path).unwrap();
        std::fs::write(
            path.join("moon.mod"),
            format!("name = \"test/{member}\"\nversion = \"0.1.0\"\nimport {{ {imports} }}\n"),
        )
        .unwrap();
    }
    moon(&project.path().join("a"), home.path(), true)
        .args(["tree"])
        .assert()
        .success();
    server.responses.lock().unwrap().insert(
        "/api/v0/registry/modules/alice/shared".into(),
        (
            200,
            serde_json::json!({
                "module": "alice/shared", "visibility": "PUBLIC", "version": "0.3.0",
                "deps": {}, "checksum": "b".repeat(64),
            })
            .to_string(),
        ),
    );
    // Both root and member invocations refresh the entire selected workspace.
    for directory in [project.path().to_owned(), project.path().join("a")] {
        server.requests.lock().unwrap().clear();
        moon(&directory, home.path(), true)
            .args(["update"])
            .assert()
            .success()
            .stdout_eq("alice/shared@0.2.0: latest 0.3.0 (update available)\n")
            .stderr_eq("");
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[0].starts_with("GET /api/v0/registry/modules/alice/shared?version=0.2.0 ")
        );
        assert!(requests[1].starts_with("GET /api/v0/registry/modules/alice/shared "));
    }
    assert_eq!(
        std::fs::read_to_string(project.path().join("moon.work")).unwrap(),
        workspace
    );
    assert!(!project.path().join(".mooncakes").exists());
}

#[test]
fn update_rejects_immutable_changes_and_does_not_fall_back_to_cached_latest() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut server = Server::new(&[("alice/shared", "0.1.0", serde_json::json!({}))]);
    configure(home.path(), &server.api);
    std::fs::write(
        project.path().join("moon.mod"),
        "name = \"test/app\"\nimport { \"alice/shared@0.1.0\" }\n",
    )
    .unwrap();
    let exact = "/api/v0/registry/modules/alice/shared?version=0.1.0";
    let latest = "/api/v0/registry/modules/alice/shared";
    let release = server.responses.lock().unwrap()[exact].clone();
    server
        .responses
        .lock()
        .unwrap()
        .insert(latest.into(), release);
    moon(project.path(), home.path(), true)
        .args(["update"])
        .assert()
        .success();
    server
        .responses
        .lock()
        .unwrap()
        .insert(latest.into(), (503, "{}".into()));
    server.responses.lock().unwrap().insert(
        exact.into(),
        (
            200,
            serde_json::json!({
                "module": "alice/shared", "visibility": "PUBLIC", "version": "0.1.0",
                "deps": {}, "checksum": "b".repeat(64), "yanked": true,
            })
            .to_string(),
        ),
    );
    moon(project.path(), home.path(), true)
        .args(["update"]).assert().failure()
        .stdout_eq("alice/shared@0.1.0: latest refresh failed; status refresh failed\n")
        .stderr_eq(snapbox::str![[r#"
Error: registry refresh incomplete:
  alice/shared latest: [..]503[..]
  alice/shared@0.1.0: registry changed immutable metadata for alice/shared@0.1.0; cached release preserved

"#]]);
    server.stop();
    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success()
        .stderr_eq("");
}

#[test]
fn metadata_cache_supports_custom_roots_and_disabling_persistence() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let custom = tempfile::tempdir().unwrap();
    let server = Server::new(&[("alice/shared", "0.1.0", serde_json::json!({}))]);
    configure(home.path(), &server.api);
    std::fs::write(
        project.path().join("moon.mod"),
        "name = \"test/app\"\nimport { \"alice/shared@0.1.0\" }\n",
    )
    .unwrap();
    for _ in 0..2 {
        moon(project.path(), home.path(), true)
            .env("MOON_REGISTRY_CACHE", "off")
            .args(["tree"])
            .assert()
            .success();
    }
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    assert!(!home.path().join("cache/registry").exists());
    // The custom root is reused across processes and cleaned independently.
    for _ in 0..2 {
        moon(project.path(), home.path(), true)
            .env("MOON_REGISTRY_CACHE", custom.path())
            .args(["tree"])
            .assert()
            .success();
    }
    assert_eq!(server.requests.lock().unwrap().len(), 3);
    assert!(custom.path().join(".moon-cache").is_file());
    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success();
    moon(project.path(), home.path(), true)
        .env("MOON_REGISTRY_CACHE", custom.path())
        .args(["clean", "--registry-cache"])
        .assert()
        .success();
    assert!(!custom.path().exists());
    assert!(home.path().join("cache/registry/metadata/v1").is_dir());
}

#[test]
fn update_requires_a_project_but_allows_no_registry_dependencies() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let server = Server::new(&[]);
    configure(home.path(), &server.api);
    moon(project.path(), home.path(), true)
        .args(["update"])
        .assert()
        .failure();
    std::fs::write(project.path().join("moon.mod"), "name = \"test/app\"\n").unwrap();
    moon(project.path(), home.path(), true)
        .args(["update"])
        .assert()
        .success()
        .stdout_eq("No registry dependencies to refresh.\n")
        .stderr_eq("");
    assert!(server.requests.lock().unwrap().is_empty());
}
