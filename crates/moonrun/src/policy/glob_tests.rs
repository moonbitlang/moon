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

fn policy(directory: &Path, document: &str) -> Policy {
    source::resolve(toml::from_str(document).unwrap(), directory)
        .unwrap()
        .policy
}

fn allows(policy: &Policy, path: &Path, intents: FsIntents) -> bool {
    policy
        .filesystem()
        .authorize(Some(path), intents, "test path")
        .is_ok()
}

#[test]
fn glob_rules_match_whole_paths_without_granting_subtrees() {
    let directory = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(directory.path()).unwrap();
    let rules = policy(
        &base,
        r#"[fs]
read = [{ glob = "logs/*.txt" }, { glob = "logs" }]
write = [{ glob = "logs/**/*.txt" }]
"#,
    );
    for (path, read, write) in [
        ("logs", true, false),
        ("logs/top.txt", true, true),
        ("logs/deep/child.txt", false, true),
        ("logs/top.bin", false, false),
        ("logs/top.txt/child", false, false),
        ("elsewhere/top.txt", false, false),
    ] {
        let path = base.join(path);
        assert_eq!(allows(&rules, &path, FsIntents::read()), read, "{path:?}");
        assert_eq!(allows(&rules, &path, FsIntents::write()), write, "{path:?}");
        assert_eq!(
            allows(&rules, &path, FsIntents::new(true, true)),
            read && write
        );
    }
}

#[test]
fn ordinary_paths_remain_literal_and_keep_subtree_semantics() {
    let directory = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(directory.path()).unwrap();
    std::fs::create_dir(base.join("data[1]")).unwrap();
    let rules = policy(
        &base,
        r#"[fs]
read = ["data[1]"]
write = ["*"]
"#,
    );
    assert!(allows(
        &rules,
        &base.join("data[1]/new.bin"),
        FsIntents::read()
    ));
    assert!(!allows(
        &rules,
        &base.join("data1/new.bin"),
        FsIntents::read()
    ));
    assert!(allows(&rules, &base.join("anything"), FsIntents::write()));
}

#[test]
fn glob_anchors_support_source_parent_and_absolute_paths() {
    let directory = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(directory.path()).unwrap();
    let source = base.join("config[1]");
    std::fs::create_dir(&source).unwrap();
    let relative = policy(
        &source,
        r#"[fs]
read = [{ glob = "../logs/**/*.txt" }]
"#,
    );
    assert!(allows(
        &relative,
        &base.join("logs/new.txt"),
        FsIntents::read()
    ));
    assert!(!allows(
        &relative,
        &source.join("logs/new.txt"),
        FsIntents::read()
    ));

    let absolute = serde_json::json!({"fs":{"read":[{"glob":base.join("logs/**/*.txt")} ]}});
    let absolute = source::resolve(serde_json::from_value(absolute).unwrap(), &source)
        .unwrap()
        .policy;
    assert!(allows(
        &absolute,
        &base.join("logs/new.txt"),
        FsIntents::read()
    ));

    let anchored = policy(
        &source,
        r#"[fs]
read = [{ glob = "*.txt" }]
"#,
    );
    assert!(allows(
        &anchored,
        &source.join("new.txt"),
        FsIntents::read()
    ));
    assert!(!allows(
        &anchored,
        &base.join("config1/new.txt"),
        FsIntents::read()
    ));
}

#[test]
fn glob_inheritance_keeps_frozen_anchors_and_complete_group_intersection() {
    let directory = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(directory.path()).unwrap();
    let child_dir = base.join("child");
    std::fs::create_dir(&child_dir).unwrap();
    let parent = policy(
        &base,
        r#"[fs]
read = [{ glob = "**/*.txt" }]
write = [{ glob = "**/*.log" }]
"#,
    );
    let child = policy(
        &child_dir,
        r#"[fs]
read = [{ glob = "public/**" }]
"#,
    );
    let mut effective = parent.intersect(child);
    // Decoding must not re-observe anchors or select a descendant's cwd.
    std::fs::remove_dir(&child_dir).unwrap();
    for _ in 0..3 {
        let bytes = effective.open_transfer().unwrap().read().unwrap();
        effective = Policy::from_transfer(&bytes).unwrap();
        assert!(allows(
            &effective,
            &child_dir.join("public/new.txt"),
            FsIntents::read()
        ));
        assert!(!allows(
            &effective,
            &child_dir.join("public/new.bin"),
            FsIntents::read()
        ));
        assert!(!allows(
            &effective,
            &child_dir.join("private/new.txt"),
            FsIntents::read()
        ));
        assert!(!allows(
            &effective,
            &child_dir.join("public/new.log"),
            FsIntents::write()
        ));
    }
}

#[test]
fn invalid_globs_and_malformed_rule_objects_fail_policy_loading() {
    let directory = tempfile::tempdir().unwrap();
    for glob in ["[", "", "logs/**/../*.txt", "bad\0name"] {
        let document = serde_json::json!({"fs":{"read":[{"glob":glob}]}});
        let config = serde_json::from_value(document).unwrap();
        assert!(
            source::resolve(config, directory.path()).is_err(),
            "{glob:?}"
        );
    }
    for entry in [
        serde_json::json!({"glob":"**", "typo":true}),
        serde_json::json!({"glob":12}),
    ] {
        assert!(
            serde_json::from_value::<config::PolicyConfig>(
                serde_json::json!({"fs":{"read":[entry]}})
            )
            .is_err()
        );
    }
}
