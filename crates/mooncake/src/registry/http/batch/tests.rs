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

#[tokio::test]
async fn split_failures_preserve_success_without_get_fallback() {
    let server = Server::new(|request| {
        let response = batch(request);
        if response["results"].as_array().unwrap().len() > 1 {
            return json(422, serde_json::json!({"code":"batch_response_too_large"}));
        }
        if response["results"][0]["module"] == "alice/two" {
            return json(503, serde_json::json!({}));
        }
        json(200, response)
    });
    let home = tempfile::tempdir().unwrap();
    let http = server.http(home.path());
    let results = http
        .version_infos(&[coordinate("alice/one"), coordinate("alice/two")])
        .await;
    assert!(results[0].is_ok());
    assert!(results[1].as_ref().unwrap_err().is::<RegistryUnavailable>());
    assert!(
        http.state
            .cached_release(&"alice/one".into(), &Version::new(0, 1, 0))
            .unwrap()
            .is_some()
    );
    assert!(
        http.state
            .cached_release(&"alice/two".into(), &Version::new(0, 1, 0))
            .unwrap()
            .is_none()
    );
    assert_eq!(server.count(), 3);
    assert!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|request| request.method == "POST")
    );
}

#[tokio::test]
async fn request_limit_rejections_split_without_retrying_other_errors() {
    let server = Server::new(|request| {
        let response = batch(request);
        if response["results"].as_array().unwrap().len() > 1 {
            return json(413, serde_json::json!({}));
        }
        json(200, response)
    });
    let home = tempfile::tempdir().unwrap();
    assert!(
        server
            .http(home.path())
            .version_infos(&[coordinate("alice/one"), coordinate("alice/two")])
            .await
            .iter()
            .all(Result::is_ok)
    );
    assert_eq!(server.count(), 3);
}

#[tokio::test]
async fn cached_private_bulk_reads_require_the_recorded_owner_even_when_frozen() {
    let home = tempfile::tempdir().unwrap();
    let server = Server::new(|request| {
        let mut response = batch(request);
        response["results"][0]["metadata"]["visibility"] = serde_json::json!("PRIVATE");
        json(200, response)
    });
    let path = home.path().join("credentials.json");
    moonutil::registry::RegistryCredentials::save(
        &path,
        moonutil::registry::Credentials {
            username: Some("alice".into()),
            token: "alice-token".into(),
        },
        &server.api,
    )
    .unwrap();
    let credentials = moonutil::registry::RegistryCredentials::load(&path)
        .unwrap()
        .unwrap();
    let http = HttpMetadata::new(
        server.api.clone(),
        moonutil::MoonHomeLayout::new(home.path().to_owned()),
        Some(Arc::new(credentials)),
    )
    .version_reader(false);
    assert!(http.version_infos(&[coordinate("alice/one")]).await[0].is_ok());
    let mut anonymous = server.http(home.path());
    anonymous.frozen = true;
    assert!(anonymous.version_infos(&[coordinate("alice/one")]).await[0].is_err());
    moonutil::registry::RegistryCredentials::save(
        &path,
        moonutil::registry::Credentials {
            username: Some("bob".into()),
            token: "bob-token".into(),
        },
        &server.api,
    )
    .unwrap();
    let credentials = moonutil::registry::RegistryCredentials::load(&path)
        .unwrap()
        .unwrap();
    let other = HttpMetadata::new(
        server.api.clone(),
        moonutil::MoonHomeLayout::new(home.path().to_owned()),
        Some(Arc::new(credentials)),
    )
    .version_reader(false);
    let mut other = other;
    other.frozen = true;
    assert!(other.version_infos(&[coordinate("alice/one")]).await[0].is_err());
    assert_eq!(server.count(), 1);
}

use super::*;
use crate::registry::http::HttpMetadata;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Debug)]
struct Request {
    method: String,
    path: String,
    headers: String,
    body: Vec<u8>,
}

struct Server {
    api: String,
    requests: Arc<Mutex<Vec<Request>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new(handler: impl Fn(&Request) -> (u16, Vec<u8>) + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let recorded = Arc::clone(&requests);
        let stopping = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&stream);
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let mut headers = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((key, value)) = line.split_once(':')
                        && key.eq_ignore_ascii_case("Content-Length")
                    {
                        length = value.trim().parse().unwrap();
                    }
                    headers.push_str(&line);
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let request = Request {
                    method: first.split_whitespace().next().unwrap().to_owned(),
                    path: first.split_whitespace().nth(1).unwrap().to_owned(),
                    headers,
                    body,
                };
                let (status, body) = handler(&request);
                recorded.lock().unwrap().push(request);
                // EOF framing also exercises limits without Content-Length.
                write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
                let _ = stream.write_all(&body); // Oversized clients may close early.
            }
        });
        Self {
            api,
            requests,
            stop,
            thread: Some(thread),
        }
    }
    fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
    fn http(&self, home: &std::path::Path) -> HttpVersionReader {
        HttpMetadata::new(
            self.api.clone(),
            moonutil::MoonHomeLayout::new(home.to_owned()),
            None,
        )
        .version_reader(false)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let result = self.thread.take().unwrap().join();
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}
fn metadata(name: &str, version: &str) -> serde_json::Value {
    serde_json::json!({"module":name,"version":version,"visibility":"PUBLIC","deps":{},
        "checksum":"a".repeat(64),"yanked":false})
}
fn item(name: &str, version: &str) -> serde_json::Value {
    serde_json::json!({"module":name,"version":version,"status":200,"metadata":metadata(name,version)})
}
fn batch(request: &Request) -> serde_json::Value {
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/api/v0/registry/metadata");
    assert!(request.body.len() <= MAX_REQUEST_BYTES);
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    let results = body["modules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            item(
                v["module"].as_str().unwrap(),
                v["version"].as_str().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    serde_json::json!({"results":results})
}
fn json(status: u16, body: serde_json::Value) -> (u16, Vec<u8>) {
    (status, serde_json::to_vec(&body).unwrap())
}
fn coordinate(name: &str) -> Coordinate {
    (name.into(), Version::new(0, 1, 0))
}

#[tokio::test]
async fn batches_deduplicate_chunk_and_reuse_exact_cache_across_clients() {
    let server = Server::new(|request| json(200, batch(request)));
    let home = tempfile::tempdir().unwrap();
    let http = server.http(home.path());
    let mut versions = (0..70)
        .map(|i| coordinate(&format!("alice/m{i}")))
        .collect::<Vec<_>>();
    versions.push(versions[0].clone());
    assert!(
        http.version_infos(&versions)
            .await
            .iter()
            .all(Result::is_ok)
    );
    assert_eq!(server.count(), 2);
    {
        let requests = server.requests.lock().unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&requests[0].body).unwrap()["modules"]
                .as_array()
                .unwrap()
                .len(),
            64
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&requests[1].body).unwrap()["modules"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
    }
    assert!(
        http.version_infos(&versions)
            .await
            .iter()
            .all(Result::is_ok)
    );
    assert!(
        HttpMetadata::new(
            server.api.clone(),
            moonutil::MoonHomeLayout::new(home.path().to_owned()),
            None
        )
        .version_reader(true)
        .version_infos(&versions)
        .await
        .iter()
        .all(Result::is_ok)
    );
    let mut http = http;
    http.frozen = true;
    assert!(http.version_infos(&[coordinate("alice/missing")]).await[0].is_err());
    assert_eq!(server.count(), 2);
}

#[tokio::test]
async fn request_bytes_and_unicode_coordinates_are_split_before_sending() {
    let server = Server::new(|request| json(200, batch(request)));
    let home = tempfile::tempdir().unwrap();
    let http = server.http(home.path());
    let versions = (0..64)
        .map(|i| coordinate(&format!("alice/{}{}", "界".repeat(900), i)))
        .collect::<Vec<_>>();
    assert!(request_body(&versions).unwrap().len() > MAX_REQUEST_BYTES);
    assert!(
        http.version_infos(&versions)
            .await
            .iter()
            .all(Result::is_ok)
    );
    assert!(server.count() > 1);
}

#[tokio::test]
async fn structured_422_splits_and_single_oversized_item_uses_exact_get() {
    let server = Server::new(|request| {
        if request.method == "POST" {
            return json(422, serde_json::json!({"code":"batch_response_too_large"}));
        }
        assert_eq!(
            request.path,
            "/api/v0/registry/modules/alice/one?version=0.1.0"
        );
        json(200, metadata("alice/one", "0.1.0"))
    });
    let home = tempfile::tempdir().unwrap();
    assert!(
        server
            .http(home.path())
            .version_infos(&[coordinate("alice/one")])
            .await[0]
            .is_ok()
    );
    assert_eq!(server.count(), 2);
    let server = Server::new(|request| {
        let answer = batch(request);
        if answer["results"].as_array().unwrap().len() > 1 {
            json(422, serde_json::json!({"code":"batch_response_too_large"}))
        } else {
            json(200, answer)
        }
    });
    let home = tempfile::tempdir().unwrap();
    assert!(
        server
            .http(home.path())
            .version_infos(&[
                coordinate("alice/one"),
                coordinate("alice/two"),
                coordinate("alice/three")
            ])
            .await
            .iter()
            .all(Result::is_ok)
    );
    assert_eq!(server.count(), 5);
}

#[tokio::test]
async fn legacy_support_is_remembered_per_client() {
    for status in [404, 405, 501] {
        let server = Server::new(move |request| {
            if request.method == "POST" {
                return json(status, serde_json::json!({}));
            }
            let name = request
                .path
                .strip_prefix("/api/v0/registry/modules/")
                .unwrap()
                .split('?')
                .next()
                .unwrap();
            json(200, metadata(name, "0.1.0"))
        });
        let home = tempfile::tempdir().unwrap();
        let http = server.http(home.path());
        assert!(
            http.version_infos(&[coordinate("alice/one"), coordinate("alice/two")])
                .await
                .iter()
                .all(Result::is_ok)
        );
        assert!(http.version_infos(&[coordinate("alice/three")]).await[0].is_ok());
        let requests = server.requests.lock().unwrap();
        assert_eq!(requests.iter().filter(|r| r.method == "POST").count(), 1);
        assert_eq!(requests.len(), 4);
    }
}

#[tokio::test]
async fn errors_do_not_fallback_and_keep_unavailable_classification() {
    for status in [400, 401, 403, 408, 422, 429, 500, 502] {
        let server =
            Server::new(move |_| json(status, serde_json::json!({"code":"another_error"})));
        let home = tempfile::tempdir().unwrap();
        let results = server
            .http(home.path())
            .version_infos(&[coordinate("alice/one"), coordinate("alice/two")])
            .await;
        assert!(results.iter().all(Result::is_err));
        assert_eq!(
            results[0].as_ref().unwrap_err().is::<RegistryUnavailable>(),
            matches!(status, 408 | 429 | 500 | 502)
        );
        assert_eq!(server.count(), 1);
    }
}

#[tokio::test]
async fn invalid_envelopes_do_not_publish_any_of_their_metadata() {
    let good = item("alice/one", "0.1.0");
    let mut wrong_version = item("alice/two", "0.1.0");
    wrong_version["metadata"]["version"] = serde_json::json!("0.1.1");
    let mut checksum = item("alice/two", "0.1.0");
    checksum["metadata"]["checksum"] = serde_json::json!("invalid");
    let mut private = item("alice/two", "0.1.0");
    private["metadata"]["visibility"] = serde_json::json!("PRIVATE");
    let denied_payload = serde_json::json!({"module":"alice/two","version":"0.1.0","status":404,"metadata":metadata("alice/two","0.1.0")});
    for response in [
        serde_json::json!({"results":[good]}),
        serde_json::json!({"results":[good,item("alice/other","0.1.0")]}),
        serde_json::json!({"results":[good,wrong_version]}),
        serde_json::json!({"results":[good,checksum]}),
        serde_json::json!({"results":[good,private]}),
        serde_json::json!({"results":[good,denied_payload]}),
    ] {
        let server = Server::new(move |_| json(200, response.clone()));
        let home = tempfile::tempdir().unwrap();
        let http = server.http(home.path());
        assert!(
            http.version_infos(&[coordinate("alice/one"), coordinate("alice/two")])
                .await
                .iter()
                .all(Result::is_err)
        );
        assert!(
            http.state
                .cached_release(&"alice/one".into(), &Version::new(0, 1, 0))
                .unwrap()
                .is_none()
        );
        assert_eq!(server.count(), 1);
    }
}

#[tokio::test]
async fn partial_denial_keeps_success_and_diagnostics_per_coordinate() {
    let server = Server::new(|_| {
        json(
            200,
            serde_json::json!({"results":[item("alice/one","0.1.0"),
        {"module":"alice/two","version":"0.1.0","status":404}]}),
        )
    });
    let home = tempfile::tempdir().unwrap();
    let http = server.http(home.path());
    let results = http
        .version_infos(&[
            coordinate("alice/one"),
            coordinate("alice/two"),
            coordinate("alice/two"),
        ])
        .await;
    assert!(results[0].is_ok());
    assert!(results[1].is_err());
    assert!(results[2].is_err());
    assert!(
        format!("{:#}", results[1].as_ref().unwrap_err()).contains("not found or access denied")
    );
    assert!(
        http.state
            .cached_release(&"alice/one".into(), &Version::new(0, 1, 0))
            .unwrap()
            .is_some()
    );
    assert!(
        http.state
            .cached_release(&"alice/two".into(), &Version::new(0, 1, 0))
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn oversized_eof_framed_response_is_rejected_without_cache_or_retry() {
    let server = Server::new(|_| (200, vec![b' '; MAX_RESPONSE_BYTES + 1]));
    let home = tempfile::tempdir().unwrap();
    let http = server.http(home.path());
    let results = http.version_infos(&[coordinate("alice/one")]).await;
    assert!(format!("{:#}", results[0].as_ref().unwrap_err()).contains("4 MiB"));
    assert!(
        http.state
            .cached_release(&"alice/one".into(), &Version::new(0, 1, 0))
            .unwrap()
            .is_none()
    );
    assert_eq!(server.count(), 1);
}

#[tokio::test]
async fn splitting_retains_the_original_login_when_credentials_change() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("credentials.json");
    let change = path.clone();
    let server = Server::new(move |request| {
        assert!(
            request
                .headers
                .to_lowercase()
                .contains("authorization: bearer alice-original")
        );
        if request.method == "POST" {
            std::fs::write(&change,r#"{"username":"bob","token":"bob-replacement","registry_api":"https://other.invalid"}"#).unwrap();
            return json(422, serde_json::json!({"code":"batch_response_too_large"}));
        }
        let mut value = metadata("alice/one", "0.1.0");
        value["visibility"] = serde_json::json!("PRIVATE");
        json(200, value)
    });
    moonutil::registry::RegistryCredentials::save(
        &path,
        moonutil::registry::Credentials {
            username: Some("alice".into()),
            token: "alice-original".into(),
        },
        &server.api,
    )
    .unwrap();
    let credentials = moonutil::registry::RegistryCredentials::load(&path)
        .unwrap()
        .unwrap();
    let http = HttpMetadata::new(
        server.api.clone(),
        moonutil::MoonHomeLayout::new(home.path().to_owned()),
        Some(Arc::new(credentials)),
    )
    .version_reader(false);
    assert!(http.version_infos(&[coordinate("alice/one")]).await[0].is_ok());
    assert_eq!(server.count(), 2);
}

#[tokio::test]
async fn exact_prerelease_and_build_identity_are_preserved() {
    let server = Server::new(|request| json(200, batch(request)));
    let home = tempfile::tempdir().unwrap();
    let http = server.http(home.path());
    let version = Version::parse("0.1.0-rc.1+001").unwrap();
    assert!(
        http.version_infos(&[("alice/one".into(), version.clone())])
            .await[0]
            .is_ok()
    );
    assert!(
        http.state
            .cached_release(&"alice/one".into(), &version)
            .unwrap()
            .is_some()
    );
    assert!(
        http.state
            .cached_release(
                &"alice/one".into(),
                &Version::parse("0.1.0-rc.1+01").unwrap()
            )
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn batches_share_an_async_connection_pool_without_blocking_the_runtime() {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let server = tokio::spawn(async move {
        let mut requests = tokio::task::JoinSet::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let barrier = Arc::clone(&barrier);
            requests.spawn(async move {
                let mut reader = tokio::io::BufReader::new(&mut stream);
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                assert!(line.starts_with("POST /api/v0/registry/metadata "));
                let mut length = 0;
                loop {
                    line.clear();
                    assert_ne!(reader.read_line(&mut line).await.unwrap(), 0);
                    if line == "\r\n" { break; }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("Content-Length") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).await.unwrap();
                let query: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                let coordinate = &query["modules"][0];
                let body = serde_json::json!({"results": [item(coordinate["module"].as_str().unwrap(), coordinate["version"].as_str().unwrap())]}).to_string();
                // Neither response is sent until both requests have arrived.
                barrier.wait().await;
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            });
        }
        while let Some(result) = requests.join_next().await {
            result.unwrap();
        }
    });
    let home = tempfile::tempdir().unwrap();
    let reader = HttpMetadata::new(
        api,
        moonutil::MoonHomeLayout::new(home.path().to_owned()),
        None,
    )
    .version_reader(false);
    let one = [coordinate("alice/one")];
    let two = [coordinate("alice/two")];
    let (one, two) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(reader.version_infos(&one), reader.version_infos(&two))
    })
    .await
    .expect("both batches must make progress on the same single-thread runtime");
    assert!(one[0].is_ok());
    assert!(two[0].is_ok());
    server.await.unwrap();
}

#[tokio::test]
async fn stalled_batch_body_times_out_without_fallback_or_publication() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let home = tempfile::tempdir().unwrap();
    let reader = HttpMetadata::new(
        api,
        moonutil::MoonHomeLayout::new(home.path().to_owned()),
        None,
    )
    .version_reader(false);
    let server = async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            assert_ne!(socket.read_buf(&mut request).await.unwrap(), 0);
        }
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n{")
            .await
            .unwrap();
        // The response headers arrived; the body must share the same timeout.
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(31)).await;
        socket
    };
    let versions = [coordinate("alice/one"), coordinate("alice/two")];
    let read = tokio::time::timeout(Duration::from_secs(35), reader.version_infos(&versions));
    let (_connection, result) = tokio::join!(server, read);
    let results = result.expect("a stalled body must fail before the test deadline");
    assert!(
        results
            .iter()
            .all(|result| result.as_ref().unwrap_err().is::<RegistryUnavailable>())
    );
    for (name, version) in versions {
        assert!(
            reader
                .state
                .cached_release(&name, &version)
                .unwrap()
                .is_none()
        );
    }
    assert!(reader.batch_supported.get());
}
