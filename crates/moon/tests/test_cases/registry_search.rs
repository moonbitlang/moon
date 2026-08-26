// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::io::{Read, Write};

use super::*;

const SEARCH_RESULTS: &[u8] = br#"[{"name":"example/json","version":"1.2.3","description":"JSON query\ntools\r\u001b[31mwith color\u001b[0m\tand spaces for MoonBit package development and automation."},{"name":"example/no-description","version":"0.4.0"}]"#;

fn moon_search(
    args: &[&str],
    response_status: &'static str,
    response_body: &'static [u8],
) -> snapbox::cmd::OutputAssert {
    // Initialize the shared test dependency before starting the server's timeout.
    let _ = moonrun_bin();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let registry = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(connection) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "moon search did not contact the configured registry"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("registry test server failed: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();

        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).unwrap();
            assert_ne!(read, 0, "request ended before its headers");
            request.extend_from_slice(&buffer[..read]);
        }
        assert!(
            request.starts_with(b"GET /api/v0/search?kw=json+query&limit=2 HTTP/1.1\r\n"),
            "unexpected registry request: {}",
            String::from_utf8_lossy(&request)
        );

        write!(
            stream,
            "HTTP/1.1 {response_status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response_body.len()
        )
        .unwrap();
        stream.write_all(response_body).unwrap();
    });

    let dir = TestDir::new_empty();
    let moon_home = tempfile::TempDir::new().expect("failed to create temp MOON_HOME");
    let assert = moon_cmd(&dir)
        .env("MOON_HOME", moon_home.path())
        .env("MOONCAKES_REGISTRY", registry)
        .env("NO_PROXY", "*")
        .args(args)
        .assert();
    server.join().unwrap();
    assert
}

#[test]
fn test_moon_search_uses_configured_registry() {
    moon_search(
        &["search", "json query", "--limit", "2"],
        "200 OK",
        SEARCH_RESULTS,
    )
    .success()
    .stdout_eq(snapbox::str![[r#"
2 modules found

MODULE                  VERSION  DESCRIPTION
example/json            1.2.3    JSON query tools with color and spaces for MoonBit package development and automation.
example/no-description  0.4.0    —

Run `moon add <module>@<version>` to add a dependency.

"#]])
    .stderr_eq("");
}

#[test]
fn test_moon_search_json_is_one_complete_result() {
    let assert = moon_search(
        &["search", "json query", "--limit", "2", "--json"],
        "200 OK",
        SEARCH_RESULTS,
    )
    .success()
    .stderr_eq("");
    let report: serde_json::Value = serde_json::from_slice(&assert.get_output().stdout)
        .expect("stdout should contain one complete JSON value");

    assert_eq!(report["version"], 1);
    assert_eq!(report["status"], "success");
    assert_eq!(report["messages"], serde_json::json!([]));
    assert_eq!(
        report["results"],
        serde_json::json!([
            {
                "name": "example/json",
                "version": "1.2.3",
                "description": "JSON query\ntools\r\u{1b}[31mwith color\u{1b}[0m\tand spaces for MoonBit package development and automation."
            },
            {
                "name": "example/no-description",
                "version": "0.4.0",
                "description": null
            }
        ])
    );
}

#[test]
fn test_moon_search_json_failure_stays_in_json() {
    let assert = moon_search(
        &["search", "json query", "--limit", "2", "--json"],
        "500 Internal Server Error",
        b"",
    )
    .failure()
    .stderr_eq("");
    let report: serde_json::Value = serde_json::from_slice(&assert.get_output().stdout)
        .expect("stdout should contain one complete JSON value");

    assert_eq!(report["version"], 1);
    assert_eq!(report["status"], "failure");
    assert_eq!(report["results"], serde_json::json!([]));
    assert_eq!(report["messages"].as_array().unwrap().len(), 1);
    assert_eq!(report["messages"][0]["level"], "error");
    assert!(
        report["messages"][0]["message"]
            .as_str()
            .unwrap()
            .contains("registry search request failed at http://127.0.0.1:")
    );
}

#[test]
fn test_moon_search_json_bootstrap_failure_stays_in_json() {
    let dir = TestDir::new_empty();
    let assert = moon_cmd(&dir)
        .args(["-C", "missing", "search", "json", "--json"])
        .assert()
        .failure()
        .stderr_eq("");
    let report: serde_json::Value = serde_json::from_slice(&assert.get_output().stdout)
        .expect("stdout should contain one complete JSON value");

    assert_eq!(report["status"], "failure");
    assert_eq!(report["results"], serde_json::json!([]));
    assert_eq!(report["messages"][0]["level"], "error");
    assert!(
        report["messages"][0]["message"]
            .as_str()
            .unwrap()
            .contains("failed to change directory")
    );
}
