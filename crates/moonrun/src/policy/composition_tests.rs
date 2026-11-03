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

fn resolved(document: &[u8]) -> Policy {
    source::resolve(serde_json::from_slice(document).unwrap(), Path::new("."))
        .unwrap()
        .policy
}

fn load(
    inherited: Option<&[u8]>,
    path: Option<&Path>,
    source_dir: Option<&Path>,
    source: &Env,
) -> anyhow::Result<(Option<Policy>, Env)> {
    RuntimeConfig {
        inherited_policy: inherited.map(|bytes| InheritedPolicyInput::Transfer(bytes.to_vec())),
        selected_policy: path.map(|path| read_file(path, source_dir)).transpose()?,
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
    let (parent, _) = load(None, Some(&parent_file), None, &source).unwrap();
    let parent = parent.unwrap().open_transfer().unwrap().read().unwrap();
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
    let (child, environment) = load(Some(&parent), Some(&child_file), None, &source).unwrap();
    assert_eq!(
        environment.get(OsStr::new("VISIBLE")),
        Some("changed-by-parent".into())
    );
    assert_eq!(environment.get(OsStr::new("HIDDEN")), None);
    assert_eq!(
        environment.get(OsStr::new("CHILD")),
        Some("must-not-be-serialized".into())
    );
    let mut transfer = child.unwrap().open_transfer().unwrap().read().unwrap();
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
        let (derived, _) = load(Some(&transfer), Some(&child_file), None, &environment).unwrap();
        let fs = derived.as_ref().unwrap().filesystem();
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
        transfer = derived.unwrap().open_transfer().unwrap().read().unwrap();
        // Ordinary inheritance adds no new restriction and preserves all rules.
        let (inherited, _) = load(Some(&transfer), None, None, &environment).unwrap();
        assert_eq!(
            inherited.unwrap().open_transfer().unwrap().read().unwrap(),
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
    let (policy, _) = load(Some(&parent), None, None, &source).unwrap();
    policy
        .as_ref()
        .unwrap()
        .filesystem()
        .authorize(Some(Path::new("file")), FsIntents::read(), "file")
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let child = root.path().join("child.json");
    for contents in ["{}", r#"{"fs":{},"net":{},"process":{}}"#] {
        std::fs::write(&child, contents).unwrap();
        let (policy, _) = load(Some(&parent), Some(&child), None, &source).unwrap();
        assert_eq!(
            policy.as_ref().unwrap().filesystem().authorize(
                Some(Path::new("file")),
                FsIntents::read(),
                "file"
            ),
            Err(AsyncHostError::PermissionDenied)
        );
        assert_eq!(
            policy
                .as_ref()
                .unwrap()
                .network()
                .check_dns(OsStr::new("example.com")),
            Err(AsyncHostError::PermissionDenied)
        );
        let process = policy.as_ref().unwrap().process();
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
    let (policy, environment) = load(None, None, None, &source).unwrap();
    assert!(policy.is_none());
    assert_eq!(environment.entries(), source.entries());
}

#[test]
fn a_child_policy_is_complete_with_or_without_an_inherited_policy() {
    let directory = tempfile::tempdir().unwrap();
    let child = directory.path().join("child.json");
    std::fs::write(&child, r#"{"net":{"dns":["*"]}}"#).unwrap();
    let parent = resolved(
        br#"{"fs":{"read":["*"],"write":["*"]},"net":{"dns":["*"]},"process":{"spawn":true}}"#,
    )
    .open_transfer()
    .unwrap()
    .read()
    .unwrap();
    let source = Env::owned([]).unwrap();
    for inherited in [None, Some(parent.as_slice())] {
        let (policy, _) = load(inherited, Some(&child), None, &source).unwrap();
        let policy = policy.unwrap();
        policy.network().check_dns("example.com".as_ref()).unwrap();
        // Omitted domains deny, even when an inherited policy permits them.
        for intent in [FsIntents::read(), FsIntents::write()] {
            assert_eq!(
                policy
                    .filesystem()
                    .authorize(Some(Path::new("file")), intent, "file"),
                Err(AsyncHostError::PermissionDenied)
            );
        }
        #[cfg(unix)]
        assert_eq!(
            policy
                .process()
                .allows_unix("tool".as_ref(), &["tool".into()]),
            Err(AsyncHostError::PermissionDenied)
        );
        #[cfg(windows)]
        assert_eq!(
            policy.process().allows_windows("tool.exe".as_ref()),
            Err(AsyncHostError::PermissionDenied)
        );
    }
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
        r#"{"version":2,"rule_groups":[]}"#,
        r#"{"fs":{"read":["relative-root"]}}"#,
        r#"{"version":2,"rule_groups":[{}]}"#,
        r#"{"version":1,"rule_groups":[{"env":{}}]}"#,
        r#"{"version":1,"rule_groups":[{}],"unexpected":true}"#,
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
            selected_policy: Some(read_file(&child_file, None).unwrap()),
            ..Default::default()
        };
        let (policy, environment) = config
            .realize_policy(Env::owned([("VALUE".into(), "child".into())]).unwrap())
            .unwrap();
        assert_eq!(environment.get("VALUE".as_ref()), Some("child".into()));
        let fs = policy.as_ref().unwrap().filesystem();
        fs.authorize(Some(Path::new("file")), FsIntents::read(), "file")
            .unwrap();
        assert_eq!(
            fs.authorize(Some(Path::new("file")), FsIntents::write(), "file"),
            Err(AsyncHostError::PermissionDenied)
        );
    }
}
