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
    io::{BufRead, BufReader, Cursor, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use sha2::{Digest, Sha256};

fn archive(name: &str, dependency: bool) -> Vec<u8> {
    let import = if dependency {
        "import { \"alice/internal@0.1.0\", \"alice/public@0.1.0\" }\n"
    } else {
        ""
    };
    let package = if dependency {
        "import { \"alice/internal\" @internal }\n"
    } else {
        ""
    };
    let source = if dependency {
        "pub fn value() -> Int { @internal.value() + 1 }\n"
    } else {
        "pub fn value() -> Int { 41 }\n"
    };
    let files = [
        (
            "moon.mod",
            format!(
                "name = \"{name}\"\nversion = \"0.1.0\"\nprivate = {}\n{import}",
                name != "alice/public"
            ),
        ),
        ("moon.pkg", package.to_owned()),
        ("lib.mbt", source.to_owned()),
    ];
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in files {
        zip.start_file(name, zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(contents.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

struct RegistryServer {
    api: String,
    requests: Arc<Mutex<Vec<String>>>,
    stopping: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl RegistryServer {
    fn new() -> Self {
        Self::with_login_switch(None)
    }

    fn with_login_switch(mut credentials_path: Option<PathBuf>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let base = api.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let thread = std::thread::spawn(move || {
            let archives = HashMap::from([
                ("alice/secret", archive("alice/secret", true)),
                ("alice/internal", archive("alice/internal", false)),
                ("alice/public", archive("alice/public", false)),
            ]);
            while !stop.load(Ordering::SeqCst) {
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
                let mut request = String::new();
                let mut reader = BufReader::new(&stream);
                loop {
                    let mut line = String::new();
                    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                    if line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                let line = request.lines().next().unwrap();
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap();
                let target = parts.next().unwrap();
                let path = target.split('?').next().unwrap();
                let authorized = request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer test-cli-owner\r\n");
                let (status, body) = if let Some(name) =
                    path.strip_prefix("/api/v0/registry/modules/")
                {
                    if !authorized {
                        (
                            404,
                            br#"{"detail":"Module not found or access denied"}"#.to_vec(),
                        )
                    } else {
                        let capability = name.strip_suffix("/download-capability");
                        let name = capability.unwrap_or(name);
                        let contents = archives.get(name).unwrap();
                        let checksum = format!("{:x}", Sha256::digest(contents));
                        let response = if capability.is_some() {
                            assert_eq!(method, "POST");
                            assert!(target.ends_with("?version=0.1.0"));
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            serde_json::json!({"module":name,"version":"0.1.0","checksum":checksum,
                                "url":format!("{base}/cdn/{name}?Signature=test-only"),
                                "issued_at":now,"expires_at":now+900})
                        } else {
                            assert_eq!(method, "GET");
                            let deps = if name == "alice/secret" {
                                serde_json::json!({"alice/internal":"0.1.0","alice/public":"0.1.0"})
                            } else {
                                serde_json::json!({})
                            };
                            let visibility = if name == "alice/public" {
                                "PUBLIC"
                            } else {
                                "PRIVATE"
                            };
                            serde_json::json!({"module":name,"visibility":visibility,"version":"0.1.0",
                                "deps":deps,"checksum":checksum})
                        };
                        (200, response.to_string().into_bytes())
                    }
                } else if let Some(name) = path.strip_prefix("/cdn/") {
                    assert!(!request.to_ascii_lowercase().contains("authorization:"));
                    assert_ne!(name, "alice/public");
                    (200, archives[name].clone())
                } else if path == "/user/alice/public/0.1.0.zip" {
                    assert!(!request.to_ascii_lowercase().contains("authorization:"));
                    (200, archives["alice/public"].clone())
                } else {
                    (404, Vec::new())
                };
                recorded.lock().unwrap().push(request);
                if let Some(credentials_path) = credentials_path.take() {
                    moonutil::registry::RegistryCredentials::save(
                        &credentials_path,
                        moonutil::registry::Credentials {
                            username: Some("bob".into()),
                            token: "test-other-user".into(),
                        },
                        &base,
                    )
                    .unwrap();
                }
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
            stopping,
            thread: Some(thread),
        }
    }

    fn stop(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}

impl Drop for RegistryServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[test]
fn registry_server_waits_for_fragmented_request_headers() {
    let mut server = RegistryServer::new();
    let mut stream = TcpStream::connect(server.api.strip_prefix("http://").unwrap()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .write_all(b"GET /api/v0/registry/modules/alice/secret HTTP/1.1\r\n")
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));
    stream
        .write_all(b"Authorization: Bearer test-cli-owner\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert_eq!(response.lines().next(), Some("HTTP/1.1 200 Test"));
    server.stop();
    assert_eq!(server.requests.lock().unwrap().len(), 1);
}

fn moon(
    project: &Path,
    home: &Path,
    cache: Option<&Path>,
    api_metadata: bool,
) -> snapbox::cmd::Command {
    snapbox::cmd::Command::new(snapbox::cargo_bin!("moon"))
        .current_dir(project)
        .env("MOON_HOME", home)
        .env("MOON_TOOLCHAIN_ROOT", moonutil::toolchain::toolchain_root())
        .env("MOON_DEP_CACHE", cache.unwrap_or_else(|| Path::new("off")))
        .env("MOON_REGISTRY_API", if api_metadata { "1" } else { "0" })
        .env("NO_PROXY", "*")
        .env_remove("MOONCAKES_REGISTRY")
        .env_remove("MOON_WORK")
}

#[test]
fn private_dependencies_add_build_run_and_reuse_offline_caches() {
    for (shared_cache, api_metadata) in [(false, false), (true, false), (false, true), (true, true)]
    {
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let cache = shared_cache.then_some(cache.path());
        let mut server =
            RegistryServer::with_login_switch(Some(home.path().join("credentials.json")));
        let public_index = home.path().join("registry/index/user/alice/public.index");
        std::fs::create_dir_all(public_index.parent().unwrap()).unwrap();
        std::fs::write(
            &public_index,
            serde_json::json!({"version":"0.1.0","deps":{},
                "checksum":format!("{:x}", Sha256::digest(archive("alice/public", false)))})
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            home.path().join("config.json"),
            serde_json::json!({
                "api":server.api,"download":server.api,"index":home.path().join("unused-index")
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            project.path().join("app.mbtx"),
            "import { \"alice/secret@0.1.0\" }\nfn main { println(@secret.value()) }\n",
        )
        .unwrap();
        moonutil::registry::RegistryCredentials::save(
            &home.path().join("credentials.json"),
            moonutil::registry::Credentials {
                username: Some("alice".into()),
                token: "test-cli-owner".into(),
            },
            &server.api,
        )
        .unwrap();
        std::fs::write(project.path().join("moon.mod"), "name = \"test/app\"\n").unwrap();
        std::fs::write(
            project.path().join("moon.pkg"),
            "import { \"alice/secret\" @secret }\noptions(\"is-main\": true)\n",
        )
        .unwrap();
        std::fs::write(
            project.path().join("main.mbt"),
            "fn main { println(@secret.value()) }\n",
        )
        .unwrap();
        moon(project.path(), home.path(), cache, api_metadata)
            .args(["add", "--no-update", "alice/secret@0.1.0"])
            .assert()
            .success();
        let saved =
            moonutil::registry::RegistryCredentials::load(&home.path().join("credentials.json"))
                .unwrap()
                .unwrap();
        assert_eq!(saved.credentials.username.as_deref(), Some("bob"));
        moonutil::registry::RegistryCredentials::save(
            &home.path().join("credentials.json"),
            moonutil::registry::Credentials {
                username: Some("alice".into()),
                token: "test-cli-owner".into(),
            },
            &server.api,
        )
        .unwrap();
        moon(project.path(), home.path(), cache, api_metadata)
            .args(["build", "--target", "js"])
            .assert()
            .success()
            .stdout_eq(snapbox::str![[r#"
Artifacts:
  test/app  js  [..]/app.js

"#]]);
        moon(project.path(), home.path(), cache, api_metadata)
            .args(["run", ".", "--target", "js"])
            .assert()
            .success()
            .stdout_eq("42\n");
        if shared_cache {
            moon(project.path(), home.path(), cache, api_metadata)
                .args(["run", "app.mbtx", "--target", "js"])
                .assert()
                .success()
                .stdout_eq("42\n");
            assert!(cache.unwrap().join("v1/private").is_dir());
            assert!(!cache.unwrap().join("v1/sources/alice/secret").exists());
            assert!(cache.unwrap().join("v1/sources/alice/public").is_dir());
        }
        server.stop();
        let requests = server.requests.lock().unwrap();
        assert_eq!(
            requests.iter().filter(|r| r.starts_with("POST ")).count(),
            2
        );
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.starts_with("GET /cdn/"))
                .count(),
            2
        );
        assert!(
            requests.iter().any(
                |r| r.starts_with("GET /api/v0/registry/modules/alice/internal?version=0.1.0 ")
            )
        );
        assert_eq!(
            requests
                .iter()
                .any(|r| r.starts_with("GET /api/v0/registry/modules/alice/public?version=0.1.0 ")),
            api_metadata
        );
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.starts_with("GET /user/alice/public/0.1.0.zip "))
                .count(),
            1
        );
        drop(requests);
        moon(project.path(), home.path(), cache, api_metadata)
            .args(["build", "--frozen", "--target", "js"])
            .assert()
            .success()
            .stdout_eq(snapbox::str![[r#"
Artifacts:
  test/app  js  [..]/app.js

"#]]);
        moon(project.path(), home.path(), cache, api_metadata)
            .args(["run", ".", "--frozen", "--target", "js"])
            .assert()
            .success()
            .stdout_eq("42\n");
        if shared_cache {
            moon(project.path(), home.path(), cache, api_metadata)
                .args(["run", "app.mbtx", "--frozen", "--target", "js"])
                .assert()
                .success()
                .stdout_eq("42\n");
        }
        // Clearing only extracted sources forces verified archive reuse without HTTP.
        if let Some(cache) = cache {
            std::fs::remove_dir_all(cache.join("v1")).unwrap();
        }
        std::fs::remove_dir_all(project.path().join(".mooncakes")).unwrap();
        moon(project.path(), home.path(), cache, api_metadata)
            .args(["build", "--target", "js"])
            .assert()
            .success();
        if shared_cache {
            moon(project.path(), home.path(), cache, api_metadata)
                .args(["run", "app.mbtx", "--target", "js"])
                .assert()
                .success()
                .stdout_eq("42\n");
        }
        assert!(!home.path().join("registry/cache/alice/secret").exists());
        assert!(!home.path().join("registry/cache/alice/internal").exists());
        assert!(
            home.path()
                .join("registry/cache/alice/public/0.1.0.zip")
                .is_file()
        );
        assert!(
            !home
                .path()
                .join("registry/index/user/alice/secret.index")
                .exists()
        );
        moonutil::registry::RegistryCredentials::save(
            &home.path().join("credentials.json"),
            moonutil::registry::Credentials {
                username: Some("bob".into()),
                token: "test-other-user".into(),
            },
            &server.api,
        )
        .unwrap();
        moon(project.path(), home.path(), cache, api_metadata)
            .args(["build", "--frozen", "--target", "js"])
            .assert()
            .failure()
            .stderr_eq(snapbox::str![[r#"
Error: [..]

Caused by:
    Failed to read registry dependency `alice/secret@0.1.0` for module `test/app`: Module `alice/secret` not found or access denied; check your registry login or run `moon login` again

"#]]);
    }
}
