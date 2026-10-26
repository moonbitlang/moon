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
                let (status, body) = releases
                    .get(target)
                    .map_or((404, "{}"), |body| (200, body.as_str()));
                recorded.lock().unwrap().push(request);
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        Self {
            api,
            requests,
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
        .args(["update"])
        .assert()
        .failure()
        .stderr_eq(snapbox::str![[r#"
Error: registry API metadata refresh is not implemented yet; `moon update` cannot be used with `MOON_REGISTRY_API=1`

"#]]);
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
