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

//! Loading, delegation, and transport preserve a conjunction of restrictions.

use std::ffi::OsStr;

use super::*;
use crate::async_host::AsyncHostError;
use crate::runtime::{Env, RuntimeConfig};

fn resolved(document: &[u8]) -> PolicyInheritance {
    source::resolve(serde_json::from_slice(document).unwrap(), Path::new("."))
        .unwrap()
        .0
}

fn load(
    inherited: Option<&[u8]>,
    path: Option<&Path>,
    source_dir: Option<&Path>,
    source: &Env,
) -> anyhow::Result<(Policy, Env)> {
    RuntimeConfig {
        inherited_policy: inherited.map(|bytes| InheritedPolicyInput::Transfer(bytes.to_vec())),
        policy_file: path.map(Path::to_owned),
        policy_source_dir: source_dir.map(Path::to_owned),
        ..Default::default()
    }
    .realize_policy(Env::owned(source.entries())?)
}

#[test]
fn composition_survives_repeated_derivation_without_replaying_provisioning() {
    let root = tempfile::tempdir().unwrap();
    let shared = root.path().join("shared");
    let child_root = shared.join("child");
    std::fs::create_dir_all(&child_root).unwrap();
    let child_root = std::fs::canonicalize(child_root).unwrap();
    let source = Env::owned([("VISIBLE".into(), "initial".into())]).unwrap();
    let parent_file = root.path().join("parent.json");
    std::fs::write(
        &parent_file,
        r#"{
        "fs":{"read":["shared"],"write":[]},
        "env":{"set":{"VISIBLE":"must-not-be-replayed"}}
    }"#,
    )
    .unwrap();
    let (mut parent, _) = load(None, Some(&parent_file), None, &source).unwrap();
    let parent = parent
        .take_policy_inheritance()
        .unwrap()
        .open_transfer()
        .unwrap()
        .read()
        .unwrap();
    let source = Env::owned([
        ("VISIBLE".into(), "changed-by-parent".into()),
        ("HIDDEN".into(), "delegated-but-filtered".into()),
    ])
    .unwrap();
    let child_file = root.path().join("child.json");
    std::fs::write(
        &child_file,
        r#"{
        "fs":{"read":["shared/child"],"write":["*"]},
        "env":{"from_host":["VISIBLE"],"set":{"CHILD":"must-not-be-serialized"}}
    }"#,
    )
    .unwrap();
    let (mut child, environment) = load(Some(&parent), Some(&child_file), None, &source).unwrap();
    assert_eq!(
        environment.get(OsStr::new("VISIBLE")),
        Some("changed-by-parent".into())
    );
    assert_eq!(environment.get(OsStr::new("HIDDEN")), None);
    assert_eq!(
        environment.get(OsStr::new("CHILD")),
        Some("must-not-be-serialized".into())
    );
    let mut transfer = child
        .take_policy_inheritance()
        .unwrap()
        .open_transfer()
        .unwrap()
        .read()
        .unwrap();
    assert!(!String::from_utf8_lossy(&transfer).contains("must-not"));
    let outside = std::fs::canonicalize(shared).unwrap().join("outside");
    for _ in 0..3 {
        // A descendant tries to widen the intersection. Its source file can
        // change, but the inherited restrictions must remain frozen.
        std::fs::write(
            &child_file,
            r#"{"fs":{"read":["*"],"write":["*"]},"env":{"from_host":["*"]}}"#,
        )
        .unwrap();
        let (mut derived, _) =
            load(Some(&transfer), Some(&child_file), None, &environment).unwrap();
        let fs = derived.take_filesystem_policy().unwrap();
        fs.authorize(
            Some(&child_root.join("file")),
            FsIntents::read(),
            "child file",
        )
        .unwrap();
        assert_eq!(
            fs.authorize(Some(&outside), FsIntents::read(), "outside"),
            Err(AsyncHostError::PermissionDenied)
        );
        assert_eq!(
            fs.authorize(
                Some(&child_root.join("file")),
                FsIntents::write(),
                "child file"
            ),
            Err(AsyncHostError::PermissionDenied)
        );
        transfer = derived
            .take_policy_inheritance()
            .unwrap()
            .open_transfer()
            .unwrap()
            .read()
            .unwrap();
        // Ordinary inheritance adds no new restriction and preserves all rules.
        let (mut inherited, _) = load(Some(&transfer), None, None, &environment).unwrap();
        assert_eq!(
            inherited
                .take_policy_inheritance()
                .unwrap()
                .open_transfer()
                .unwrap()
                .read()
                .unwrap(),
            transfer
        );
    }
}

#[test]
fn no_child_policy_preserves_grants_but_empty_child_policy_denies_domains() {
    let source = Env::owned([]).unwrap();
    let parent =
        resolved(br#"{"fs":{"read":["*"]},"net":{"connect":["*:*"]},"process":{"spawn":true}}"#)
            .open_transfer()
            .unwrap()
            .read()
            .unwrap();
    let (mut policy, _) = load(Some(&parent), None, None, &source).unwrap();
    policy
        .take_filesystem_policy()
        .unwrap()
        .authorize(Some(Path::new("file")), FsIntents::read(), "file")
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let child = root.path().join("child.json");
    for contents in ["{}", r#"{"fs":{},"net":{},"process":{}}"#] {
        std::fs::write(&child, contents).unwrap();
        let (mut policy, _) = load(Some(&parent), Some(&child), None, &source).unwrap();
        assert_eq!(
            policy.take_filesystem_policy().unwrap().authorize(
                Some(Path::new("file")),
                FsIntents::read(),
                "file"
            ),
            Err(AsyncHostError::PermissionDenied)
        );
        assert_eq!(
            policy
                .take_network_policy()
                .unwrap()
                .check_dns(OsStr::new("example.com")),
            Err(AsyncHostError::PermissionDenied)
        );
        let process = policy.take_process_policy().unwrap();
        #[cfg(unix)]
        assert_eq!(
            process.allows_unix(OsStr::new("tool"), &["tool".into()]),
            Err(AsyncHostError::PermissionDenied)
        );
        #[cfg(windows)]
        assert_eq!(
            process.allows_windows(OsStr::new("tool.exe")),
            Err(AsyncHostError::PermissionDenied)
        );
    }
    let (mut policy, environment) = load(None, None, None, &source).unwrap();
    assert!(policy.take_filesystem_policy().is_none());
    assert!(policy.take_network_policy().is_none());
    assert!(policy.take_process_policy().is_none());
    assert_eq!(environment.entries(), source.entries());
}

#[test]
fn inherited_ceiling_does_not_hide_invalid_child_policy() {
    let root = tempfile::tempdir().unwrap();
    let child = root.path().join("child.json");
    let source = Env::owned([]).unwrap();
    let parent = resolved(b"{}").open_transfer().unwrap().read().unwrap();
    assert!(load(Some(&parent), Some(&child), None, &source).is_err());
    for contents in [
        r#"{"unknown":true}"#,
        r#"{"net":{"connect":["invalid"]}}"#,
        r#"{"process":{"spawn":true,"allow":[{"program":"tool"}]}}"#,
    ] {
        std::fs::write(&child, contents).unwrap();
        assert!(
            load(Some(&parent), Some(&child), None, &source).is_err(),
            "{contents}"
        );
    }
}

#[test]
fn malformed_compositions_fail_closed() {
    let source = Env::owned([]).unwrap();
    for contents in [
        r#"{"version":2,"restrictions":[]}"#,
        r#"{"fs":{"read":["relative-root"]}}"#,
        r#"{"version":2,"restrictions":[{}]}"#,
        r#"{"version":1,"restrictions":[{"env":{}}]}"#,
        r#"{"version":1,"restrictions":[{}],"unexpected":true}"#,
    ] {
        assert!(
            load(Some(contents.as_bytes()), None, None, &source).is_err(),
            "{contents}"
        );
    }
}

#[test]
fn typed_and_external_composition_have_the_same_decisions() {
    let directory = tempfile::tempdir().unwrap();
    let child_file = directory.path().join("child.json");
    std::fs::write(
        &child_file,
        r#"{"fs":{"read":["*"],"write":["*"]},"env":{"from_host":["VALUE"]}}"#,
    )
    .unwrap();
    let parent = resolved(br#"{"fs":{"read":["*"]},"env":{"from_host":["*"]}}"#);
    let encoded = parent.open_transfer().unwrap().read().unwrap();
    for inherited in [
        InheritedPolicyInput::Resolved(parent),
        InheritedPolicyInput::Transfer(encoded),
    ] {
        let config = RuntimeConfig {
            inherited_policy: Some(inherited),
            policy_file: Some(child_file.clone()),
            ..Default::default()
        };
        let (mut policy, environment) = config
            .realize_policy(Env::owned([("VALUE".into(), "child".into())]).unwrap())
            .unwrap();
        assert_eq!(environment.get("VALUE".as_ref()), Some("child".into()));
        let fs = policy.take_filesystem_policy().unwrap();
        fs.authorize(Some(Path::new("file")), FsIntents::read(), "file")
            .unwrap();
        assert_eq!(
            fs.authorize(Some(Path::new("file")), FsIntents::write(), "file"),
            Err(AsyncHostError::PermissionDenied)
        );
    }
}
