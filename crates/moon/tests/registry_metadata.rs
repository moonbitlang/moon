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

type Responses = HashMap<String, (u16, Vec<u8>)>;

struct Server {
    api: String,
    requests: Arc<Mutex<Vec<String>>>,
    responses: Arc<Mutex<Responses>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    fn new(releases: &[(&str, &str, serde_json::Value)]) -> Self {
        let releases: HashMap<_, _> = releases
            .iter()
            .map(|(module, version, deps)| {
                (format!("/api/v0/registry/modules/{module}?version={version}"),
                (200, serde_json::json!({"module": module, "visibility": "PUBLIC", "version": version,
                    "deps": deps, "checksum": "a".repeat(64)}).to_string().into_bytes()))
            })
            .collect();
        let responses = Arc::new(Mutex::new(releases));
        let served = Arc::clone(&responses);
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
                    .unwrap_or((404, b"{}".to_vec()));
                recorded.lock().unwrap().push(request);
                write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
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

    fn respond(&self, target: &str, status: u16, body: serde_json::Value) {
        self.responses
            .lock()
            .unwrap()
            .insert(target.to_owned(), (status, body.to_string().into_bytes()));
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
        .env_remove("MOONCAKES_REGISTRY")
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

fn release(
    module: &str,
    version: &str,
    deps: serde_json::Value,
    yanked: bool,
) -> serde_json::Value {
    serde_json::json!({
        "module": module, "visibility": "PUBLIC", "version": version,
        "deps": deps, "checksum": "a".repeat(64), "yanked": yanked,
        "yanked_reason": if yanked { Some("Use the replacement") } else { None },
    })
}

fn source_archive() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("moon.mod", zip::write::FileOptions::default())
        .unwrap();
    zip.write_all(b"name = \"alice/module\"\nversion = \"0.1.0\"\n")
        .unwrap();
    zip.start_file("moon.pkg", zip::write::FileOptions::default())
        .unwrap();
    zip.write_all(b"").unwrap();
    zip.start_file("lib.mbt", zip::write::FileOptions::default())
        .unwrap();
    zip.write_all(b"pub fn answer() -> Int { 42 }\n").unwrap();
    zip.finish().unwrap().into_inner()
}

#[test]
fn add_fetch_no_update_and_warm_builds_reuse_content_across_processes() {
    use sha2::Digest;
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let mut server = Server::new(&[]);
    configure(home.path(), &server.api);
    let archive = source_archive();
    let checksum = format!("{:x}", sha2::Sha256::digest(&archive));
    let mut metadata = release("alice/module", "0.1.0", serde_json::json!({}), false);
    metadata["checksum"] = checksum.into();
    server.respond(
        "/api/v0/registry/modules/alice/module?version=0.1.0",
        200,
        metadata.clone(),
    );
    server.respond("/api/v0/registry/modules/alice/module", 200, metadata);
    server
        .responses
        .lock()
        .unwrap()
        .insert("/user/alice/module/0.1.0.zip".to_owned(), (200, archive));
    std::fs::write(project.path().join("moon.mod"), "name = \"test/app\"\n").unwrap();
    std::fs::write(
        project.path().join("moon.pkg"),
        "import { \"alice/module\" }\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("app.mbt"),
        "pub fn answer() -> Int { @module.answer() }\n",
    )
    .unwrap();

    // A cached exact release is not a cached latest answer.
    moon(project.path(), home.path(), true)
        .args(["fetch", "alice/module", "--no-update"])
        .assert()
        .failure();
    assert!(server.requests.lock().unwrap().is_empty());
    moon(project.path(), home.path(), true)
        .args(["add", "alice/module@0.1.0", "--no-update"])
        .assert()
        .success();
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    moon(project.path(), home.path(), true)
        .args(["fetch", "alice/module@0.1.0", "--no-update"])
        .assert()
        .success();
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    moon(project.path(), home.path(), true)
        .args(["fetch", "alice/module", "--no-update"])
        .assert()
        .failure();
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    // Exact fetch refreshes status despite already materialized source.
    moon(project.path(), home.path(), true)
        .args(["fetch", "alice/module@0.1.0"])
        .assert()
        .success();
    assert_eq!(server.requests.lock().unwrap().len(), 3);
    moon(project.path(), home.path(), true)
        .args(["fetch", "alice/module"])
        .assert()
        .success();
    assert_eq!(server.requests.lock().unwrap().len(), 4);
    moon(project.path(), home.path(), true)
        .args(["fetch", "alice/module", "--no-update"])
        .assert()
        .success();
    assert_eq!(server.requests.lock().unwrap().len(), 4);
    moon(project.path(), home.path(), true)
        .args(["add", "alice/module@0.1.0", "--upgrade"])
        .assert()
        .success();
    assert_eq!(server.requests.lock().unwrap().len(), 5);
    server.stop();
    moon(project.path(), home.path(), true)
        .args(["check"])
        .assert()
        .success();
    moon(project.path(), home.path(), true)
        .args(["build"])
        .assert()
        .success();
    moon(project.path(), home.path(), true)
        .args(["test"])
        .assert()
        .success();
    moon(project.path(), home.path(), true)
        .args(["check", "--frozen"])
        .assert()
        .success();
    assert_eq!(server.requests.lock().unwrap().len(), 5);
}

#[test]
fn update_at_workspace_root_refreshes_all_members_and_shared_dependencies_once() {
    let home = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let server = Server::new(&[("alice/module", "0.1.0", serde_json::json!({}))]);
    configure(home.path(), &server.api);
    server.respond(
        "/api/v0/registry/modules/alice/module",
        200,
        release("alice/module", "0.1.0", serde_json::json!({}), false),
    );
    std::fs::create_dir(workspace.path().join("a")).unwrap();
    std::fs::create_dir(workspace.path().join("b")).unwrap();
    std::fs::write(
        workspace.path().join("moon.work"),
        "members = [\"a\", \"b\"]\n",
    )
    .unwrap();
    std::fs::write(
        workspace.path().join("a/moon.mod"),
        "name = \"test/a\"\nimport { \"alice/module@0.1.0\" }\n",
    )
    .unwrap();
    std::fs::write(
        workspace.path().join("b/moon.mod"),
        "name = \"test/b\"\nimport { \"alice/module@0.1.0\" }\n",
    )
    .unwrap();
    moon(workspace.path(), home.path(), true)
        .args(["update"])
        .assert()
        .success()
        .stdout_eq("");
    assert_eq!(server.requests.lock().unwrap().len(), 3);
    assert!(!workspace.path().join(".mooncakes").exists());
}

#[test]
fn update_refreshes_selected_old_versions_and_persists_restoration_without_installing_sources() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let server = Server::new(&[
        (
            "alice/direct",
            "0.1.0",
            serde_json::json!({"alice/transitive": "0.1.0"}),
        ),
        ("alice/transitive", "0.1.0", serde_json::json!({})),
    ]);
    configure(home.path(), &server.api);
    let manifest = "name = \"test/app\"\nimport { \"alice/direct@0.1.0\" }\n";
    std::fs::write(project.path().join("moon.mod"), manifest).unwrap();
    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success();
    assert_eq!(server.requests.lock().unwrap().len(), 2);
    server.respond(
        "/api/v0/registry/modules/alice/direct?version=0.1.0",
        200,
        release(
            "alice/direct",
            "0.1.0",
            serde_json::json!({"alice/transitive": "0.1.0"}),
            true,
        ),
    );
    server.respond(
        "/api/v0/registry/modules/alice/direct",
        200,
        release("alice/direct", "0.1.1", serde_json::json!({}), false),
    );
    server.respond(
        "/api/v0/registry/modules/alice/transitive?version=0.1.0",
        200,
        release("alice/transitive", "0.1.0", serde_json::json!({}), true),
    );
    server.respond(
        "/api/v0/registry/modules/alice/transitive",
        200,
        release("alice/transitive", "0.1.1", serde_json::json!({}), false),
    );
    moon(project.path(), home.path(), true).args(["update"]).assert().success()
        .stdout_eq("Update available: alice/direct@0.1.0 -> 0.1.1\nUpdate available: alice/transitive@0.1.0 -> 0.1.1\n");
    assert_eq!(server.requests.lock().unwrap().len(), 6);
    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success()
        .stderr_eq(snapbox::str![[r#"
Warning: Dependency `alice/direct@0.1.0` is deprecated: Use the replacement
Warning: Dependency `alice/transitive@0.1.0` is deprecated: Use the replacement
  required through test/app@0.0.0 -> alice/direct@0.1.0 -> alice/transitive@0.1.0

"#]]);
    assert_eq!(server.requests.lock().unwrap().len(), 6);
    server.respond(
        "/api/v0/registry/modules/alice/direct?version=0.1.0",
        200,
        release(
            "alice/direct",
            "0.1.0",
            serde_json::json!({"alice/transitive": "0.1.0"}),
            false,
        ),
    );
    server.respond(
        "/api/v0/registry/modules/alice/transitive?version=0.1.0",
        200,
        release("alice/transitive", "0.1.0", serde_json::json!({}), false),
    );
    moon(project.path(), home.path(), true)
        .args(["update"])
        .assert()
        .success()
        .stderr_eq("");
    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success()
        .stderr_eq("");
    assert_eq!(server.requests.lock().unwrap().len(), 10);
    assert_eq!(
        std::fs::read_to_string(project.path().join("moon.mod")).unwrap(),
        manifest
    );
    assert!(!project.path().join(".mooncakes").exists());
    assert!(!project.path().join(".repos").exists());
    assert!(!home.path().join("registry/cache").exists());
}

#[test]
fn update_reports_incomplete_refreshes_and_keeps_successful_status_writes() {
    let home = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let server = Server::new(&[
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
    server.respond(
        "/api/v0/registry/modules/alice/a?version=0.1.0",
        200,
        release("alice/a", "0.1.0", serde_json::json!({}), true),
    );
    server.respond(
        "/api/v0/registry/modules/alice/a",
        200,
        release("alice/a", "0.1.1", serde_json::json!({}), false),
    );
    server.respond(
        "/api/v0/registry/modules/alice/b?version=0.1.0",
        500,
        serde_json::json!({}),
    );
    server.respond(
        "/api/v0/registry/modules/alice/b",
        500,
        serde_json::json!({}),
    );
    moon(project.path(), home.path(), true)
        .args(["update"])
        .assert()
        .failure()
        .stdout_eq("Update available: alice/a@0.1.0 -> 0.1.1\n")
        .stderr_eq(snapbox::str![[r#"
Warning: Dependency `alice/a@0.1.0` is deprecated: Use the replacement
Error: registry metadata refresh incomplete:
status for alice/b@0.1.0: [..]
latest for alice/b: [..]
"#]]);
    let requests = server.requests.lock().unwrap().len();
    moon(project.path(), home.path(), true)
        .args(["tree"])
        .assert()
        .success()
        .stderr_eq("Warning: Dependency `alice/a@0.1.0` is deprecated: Use the replacement\n");
    assert_eq!(server.requests.lock().unwrap().len(), requests);
}

#[test]
fn api_update_outside_a_project_fails_before_requesting_metadata() {
    let home = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let server = Server::new(&[]);
    configure(home.path(), &server.api);
    moon(outside.path(), home.path(), true).args(["update"]).assert().failure()
        .stderr_eq(snapbox::str![[r#"
Error: not in a Moon project (no moon.mod, moon.mod.json, or moon.work found starting from [..] or its ancestors)

"#]]);
    assert!(server.requests.lock().unwrap().is_empty());
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
    moon(project.path(), home.path(), true)
        .args(["tree", "--json"])
        .assert()
        .success();
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
