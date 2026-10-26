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

use std::net::TcpListener;

use super::{
    tests::{api_test_registry, api_test_server, resolver_response},
    *,
};
use crate::registry::Registry;

#[test]
fn explicit_refresh_observes_deprecation_and_restoration_despite_cache_hits() {
    let home = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let original = resolver_response("alice/module", "PUBLIC", "0.1.0");
    let mut deprecated: serde_json::Value = serde_json::from_str(&original).unwrap();
    deprecated["yanked"] = true.into();
    deprecated["yanked_reason"] = "Use the replacement".into();
    let server = api_test_server(
        listener,
        vec![
            (200, String::new(), original.clone()),
            (200, String::new(), deprecated.to_string()),
            (200, String::new(), original),
        ],
    );
    let mut registry = api_test_registry(home.path(), &api);
    registry.api_metadata = true;
    let name = "alice/module".into();
    let version = Version::new(0, 1, 0);
    assert!(
        !registry
            .version_info(&name, &version)
            .unwrap()
            .unwrap()
            .yanked
    );
    let refreshed = registry.refresh_version_info(&name, &version).unwrap();
    assert!(refreshed.yanked);
    assert_eq!(
        refreshed.yanked_reason.as_deref(),
        Some("Use the replacement")
    );
    assert!(
        registry
            .version_info(&name, &version)
            .unwrap()
            .unwrap()
            .yanked
    );
    assert!(
        !registry
            .refresh_version_info(&name, &version)
            .unwrap()
            .yanked
    );
    let mut offline = api_test_registry(home.path(), &api).offline();
    offline.api_metadata = true;
    assert!(
        !offline
            .version_info(&name, &version)
            .unwrap()
            .unwrap()
            .yanked
    );
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests.iter().all(|request| {
        request.starts_with("GET /api/v0/registry/modules/alice/module?version=0.1.0 ")
    }));
}

#[test]
fn conflicting_refresh_preserves_the_valid_cache() {
    let home = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let original = resolver_response("alice/module", "PUBLIC", "0.1.0");
    let mut changed: serde_json::Value = serde_json::from_str(&original).unwrap();
    changed["checksum"] = "c".repeat(64).into();
    changed["yanked"] = true.into();
    let mut changed_deps: serde_json::Value = serde_json::from_str(&original).unwrap();
    changed_deps["deps"] = serde_json::json!({"alice/other": "0.1.0"});
    let server = api_test_server(
        listener,
        vec![
            (200, String::new(), original),
            (200, String::new(), changed.to_string()),
            (200, String::new(), changed_deps.to_string()),
        ],
    );
    let mut registry = api_test_registry(home.path(), &api);
    registry.api_metadata = true;
    let name = "alice/module".into();
    let version = Version::new(0, 1, 0);
    registry.version_info(&name, &version).unwrap();
    let path = registry.metadata_release_path(&name, &version).unwrap();
    let before = std::fs::read(&path).unwrap();
    let error = registry.refresh_version_info(&name, &version).unwrap_err();
    assert!(error.to_string().contains("immutable"), "{error:#}");
    assert_eq!(std::fs::read(path).unwrap(), before);
    let error = registry.refresh_latest_version(&name).unwrap_err();
    assert!(error.to_string().contains("immutable"), "{error:#}");
    assert_eq!(
        std::fs::read(registry.metadata_release_path(&name, &version).unwrap()).unwrap(),
        before
    );
    assert!(
        !registry
            .metadata_query_path(&name, &MetadataSelector::Latest)
            .unwrap()
            .exists()
    );
    assert!(
        !registry
            .version_info(&name, &version)
            .unwrap()
            .unwrap()
            .yanked
    );
    assert_eq!(server.join().unwrap().len(), 3);
}

#[test]
fn no_update_requires_a_latest_pointer_but_allows_missing_exact_metadata() {
    let home = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let body = resolver_response("alice/module", "PUBLIC", "0.1.0");
    let server = api_test_server(
        listener,
        vec![
            (200, String::new(), body.clone()),
            (200, String::new(), body),
        ],
    );
    let mut registry = api_test_registry(home.path(), &api);
    registry.api_metadata = true;
    let exact = super::super::path::parse_module_path("alice/module@0.1.0").unwrap();
    let latest = super::super::path::parse_module_path("alice/module").unwrap();
    let log = UserLog::new(log::LevelFilter::Off);
    registry
        .resolve_module_for_command(&exact, true, &log)
        .unwrap();
    let error = registry
        .resolve_module_for_command(&latest, true, &log)
        .unwrap_err();
    assert!(error.to_string().contains("no cached latest"));
    let version = Version::new(0, 1, 0);
    registry
        .write_metadata_cache(
            &registry
                .metadata_query_path(&latest.module, &MetadataSelector::Latest)
                .unwrap(),
            &version,
        )
        .unwrap();
    let release_path = registry
        .metadata_release_path(&latest.module, &version)
        .unwrap();
    std::fs::remove_file(&release_path).unwrap();
    registry.metadata.borrow_mut().clear();
    let selected = registry
        .resolve_module_for_command(&latest, true, &log)
        .unwrap();
    assert_eq!(selected.version(), &version);
    assert!(release_path.exists());
    assert!(
        registry
            .version_info(selected.name(), selected.version())
            .unwrap()
            .is_some()
    );
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn only_advisory_transport_failure_can_reuse_valid_exact_content() {
    let home = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let original = resolver_response("alice/module", "PUBLIC", "0.1.0");
    let mut changed: serde_json::Value = serde_json::from_str(&original).unwrap();
    changed["checksum"] = "c".repeat(64).into();
    let server = api_test_server(
        listener,
        vec![
            (200, String::new(), original),
            (503, String::new(), "{}".to_owned()),
            (200, String::new(), changed.to_string()),
            (403, String::new(), "{}".to_owned()),
            (503, String::new(), "{}".to_owned()),
        ],
    );
    let mut registry = api_test_registry(home.path(), &api);
    registry.api_metadata = true;
    let path = super::super::path::parse_module_path("alice/module@0.1.0").unwrap();
    let (log, capture) = UserLog::captured(log::LevelFilter::Warn);
    registry
        .resolve_module_for_command(&path, true, &log)
        .unwrap();
    registry
        .resolve_module_for_command(&path, false, &log)
        .unwrap();
    let warnings = capture.take();
    assert_eq!(warnings.len(), 1);
    assert!(
        warnings[0]
            .message
            .contains("failed to refresh deprecation status")
    );
    assert!(
        registry
            .resolve_module_for_command(&path, false, &log)
            .unwrap_err()
            .to_string()
            .contains("immutable")
    );
    assert!(
        registry
            .resolve_module_for_command(&path, false, &log)
            .is_err()
    );
    let missing = super::super::path::parse_module_path("alice/module@0.1.1").unwrap();
    assert!(
        registry
            .resolve_module_for_command(&missing, false, &log)
            .is_err()
    );
    assert!(capture.take().is_empty());
    assert_eq!(server.join().unwrap().len(), 5);
}

#[test]
fn required_latest_refresh_never_falls_back_to_the_cached_answer() {
    let home = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let server = api_test_server(
        listener,
        vec![
            (
                200,
                String::new(),
                resolver_response("alice/module", "PUBLIC", "0.1.0"),
            ),
            (503, String::new(), "{}".to_owned()),
        ],
    );
    let mut registry = api_test_registry(home.path(), &api);
    registry.api_metadata = true;
    let path = super::super::path::parse_module_path("alice/module").unwrap();
    let log = UserLog::new(log::LevelFilter::Off);
    registry
        .resolve_module_for_command(&path, false, &log)
        .unwrap();
    assert!(
        registry
            .resolve_module_for_command(&path, false, &log)
            .is_err()
    );
    assert_eq!(
        registry
            .resolve_module_for_command(&path, true, &log)
            .unwrap()
            .version(),
        &Version::new(0, 1, 0)
    );
    assert_eq!(server.join().unwrap().len(), 2);
}

#[test]
fn frozen_resolution_does_not_fetch_or_refresh_missing_metadata() {
    let home = tempfile::tempdir().unwrap();
    let mut registry = api_test_registry(home.path(), "http://127.0.0.1:1").offline();
    registry.api_metadata = true;
    let name = "alice/module".into();
    let version = Version::new(0, 1, 0);
    assert!(
        registry
            .version_info(&name, &version)
            .unwrap_err()
            .to_string()
            .contains("frozen")
    );
    assert!(
        registry
            .refresh_version_info(&name, &version)
            .unwrap_err()
            .to_string()
            .contains("frozen")
    );
    assert!(
        registry
            .refresh_latest_version(&name)
            .unwrap_err()
            .to_string()
            .contains("frozen")
    );
}

#[test]
fn executable_latest_refresh_is_distinct_from_locally_known_latest() {
    let home = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let server = api_test_server(
        listener,
        vec![
            (
                200,
                String::new(),
                resolver_response("alice/module", "PUBLIC", "0.1.0"),
            ),
            (
                200,
                String::new(),
                resolver_response("alice/module", "PUBLIC", "0.1.1"),
            ),
        ],
    );
    let mut registry = api_test_registry(home.path(), &api);
    registry.api_metadata = true;
    let log = UserLog::new(log::LevelFilter::Off);
    assert_eq!(
        registry
            .resolve_executable_package("alice/module", &log)
            .unwrap()
            .module
            .version(),
        &Version::new(0, 1, 0)
    );
    assert_eq!(
        registry
            .resolve_executable_package("alice/module", &log)
            .unwrap()
            .module
            .version(),
        &Version::new(0, 1, 0)
    );
    assert_eq!(
        registry
            .resolve_executable_package("alice/module@latest", &log)
            .unwrap()
            .module
            .version(),
        &Version::new(0, 1, 1)
    );
    assert_eq!(
        registry
            .resolve_executable_package("alice/module", &log)
            .unwrap()
            .module
            .version(),
        &Version::new(0, 1, 1)
    );
    assert_eq!(server.join().unwrap().len(), 2);
}
