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
fn excluded_globs_override_grants_regardless_of_order() {
    let directory = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(directory.path()).unwrap();
    std::fs::create_dir(base.join("src")).unwrap();
    for write in [
        r#"["src", { glob = "!src/**/*.mbt" }]"#,
        r#"[{ glob = "!src/**/*.mbt" }, "src"]"#,
        r#"[{ glob = "src/**" }, { glob = "!src/**/*.mbt" }]"#,
    ] {
        let rules = policy(&base, &format!("[fs]\nread = [\"src\"]\nwrite = {write}\n"));
        for (path, writable) in [
            ("src/main.mbt", false),
            ("src/deep/lib.mbt", false),
            ("src/main.txt", true),
            ("src/deep/lib.txt", true),
        ] {
            let path = base.join(path);
            assert!(allows(&rules, &path, FsIntents::read()));
            assert_eq!(
                allows(&rules, &path, FsIntents::write()),
                writable,
                "{write}: {path:?}"
            );
            assert_eq!(allows(&rules, &path, FsIntents::new(true, true)), writable);
        }
        assert!(!allows(
            &rules,
            &base.join("other/main.txt"),
            FsIntents::write()
        ));
    }
}

#[test]
fn excluded_globs_alone_do_not_grant_access() {
    let directory = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(directory.path()).unwrap();
    let rules = policy(
        &base,
        r#"[fs]
read = [{ glob = "!**/*.mbt" }]
write = [{ glob = "!**/*.mbt" }]
"#,
    );
    for path in ["src/main.mbt", "src/main.txt", "!src/main.mbt"] {
        assert!(!allows(&rules, &base.join(path), FsIntents::read()));
        assert!(!allows(&rules, &base.join(path), FsIntents::write()));
    }
}

#[test]
fn excluded_globs_resolve_anchors_after_the_negation_prefix() {
    let directory = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(directory.path()).unwrap();
    let source = base.join("config");
    std::fs::create_dir(&source).unwrap();
    for glob in [
        "!../src/**/*.mbt".to_owned(),
        format!("!{}", base.join("src/**/*.mbt").display()),
    ] {
        let config = serde_json::from_value(serde_json::json!({
            "fs": {"read": ["*", {"glob": glob}], "write": ["*", {"glob": glob}]}
        }))
        .unwrap();
        let rules = source::resolve(config, &source).unwrap().policy;
        for (path, allowed) in [
            ("src/main.mbt", false),
            ("src/deep/lib.mbt", false),
            ("src/main.txt", true),
            ("config/src/main.mbt", true),
            ("other/main.mbt", true),
        ] {
            let path = base.join(path);
            assert_eq!(
                allows(&rules, &path, FsIntents::read()),
                allowed,
                "{glob}: {path:?}"
            );
            assert_eq!(
                allows(&rules, &path, FsIntents::write()),
                allowed,
                "{glob}: {path:?}"
            );
        }
    }
}

#[test]
fn literal_bangs_and_negated_character_classes_remain_supported() {
    let directory = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(directory.path()).unwrap();
    std::fs::create_dir(base.join("!src")).unwrap();
    let rules = policy(
        &base,
        r#"[fs]
read = ["!src", { glob = "./!notes.txt" }]
write = [{ glob = "[!a]*.txt" }, { glob = "!!blocked.txt" }]
"#,
    );
    assert!(allows(
        &rules,
        &base.join("!src/main.mbt"),
        FsIntents::read()
    ));
    assert!(allows(&rules, &base.join("!notes.txt"), FsIntents::read()));
    assert!(!allows(&rules, &base.join("notes.txt"), FsIntents::read()));
    assert!(allows(&rules, &base.join("b.txt"), FsIntents::write()));
    assert!(!allows(&rules, &base.join("a.txt"), FsIntents::write()));
    assert!(allows(&rules, &base.join("!other.txt"), FsIntents::write()));
    assert!(!allows(
        &rules,
        &base.join("!blocked.txt"),
        FsIntents::write()
    ));
}

#[test]
fn excluded_globs_survive_inheritance_and_transfer() {
    let directory = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(directory.path()).unwrap();
    let child_dir = base.join("child");
    std::fs::create_dir(&child_dir).unwrap();
    let parent = policy(
        &base,
        r#"[fs]
read = ["*", { glob = "!**/*.secret" }]
write = ["*", { glob = "!**/*.mbt" }]
"#,
    );
    let child = policy(
        &child_dir,
        r#"[fs]
read = ["*"]
write = ["*", { glob = "!private/**" }]
"#,
    );
    let mut effective = parent.intersect(child);
    std::fs::remove_dir(&child_dir).unwrap();
    for _ in 0..3 {
        for (path, read, write) in [
            ("public/main.mbt", true, false),
            ("public/main.secret", false, true),
            ("public/main.txt", true, true),
            ("private/main.txt", true, false),
        ] {
            let path = child_dir.join(path);
            assert_eq!(
                allows(&effective, &path, FsIntents::read()),
                read,
                "{path:?}"
            );
            assert_eq!(
                allows(&effective, &path, FsIntents::write()),
                write,
                "{path:?}"
            );
        }
        let bytes = effective.open_transfer().unwrap().read().unwrap();
        effective = Policy::from_transfer(&bytes).unwrap();
    }
}

#[test]
fn excluded_globs_prevent_filesystem_mutations() {
    use crate::filesystem::{FsError, HostFs};
    use crate::runtime::{Env, WorkingDirectory};
    let directory = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(directory.path()).unwrap();
    std::fs::create_dir_all(base.join("src/deep")).unwrap();
    std::fs::write(base.join("src/deep/keep.mbt"), b"keep").unwrap();
    let rules = policy(
        &base,
        r#"[fs]
read = ["src"]
write = ["src", { glob = "!src/**/*.mbt" }]
"#,
    );
    let host = HostFs::new(
        Some(rules.filesystem()),
        Arc::new(Env::ambient()),
        Arc::new(WorkingDirectory::Fixed(base.clone())),
    );
    for path in ["src/new.mbt", "src/deep/new.mbt", "src/deep/keep.mbt"] {
        assert!(matches!(
            host.write_file(Path::new(path), b"denied"),
            Err(FsError::Denied)
        ));
    }
    host.write_file(Path::new("src/deep/new.txt"), b"allowed")
        .unwrap();
    for (source, target) in [
        ("src/deep/new.txt", "src/deep/new.mbt"),
        ("src/deep/new.txt", "src/deep/keep.mbt"),
        ("src/deep/keep.mbt", "src/deep/moved.txt"),
    ] {
        assert!(matches!(
            host.rename(Path::new(source), Path::new(target)),
            Err(FsError::Denied)
        ));
    }
    assert!(matches!(
        host.unlink_file(Path::new("src/deep/keep.mbt")),
        Err(FsError::Denied)
    ));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("keep.mbt", base.join("src/deep/link.txt")).unwrap();
        assert!(matches!(
            host.write_file(Path::new("src/deep/link.txt"), b"denied"),
            Err(FsError::Denied)
        ));
    }
    assert_eq!(
        host.read_file(Path::new("src/deep/keep.mbt")).unwrap(),
        b"keep"
    );
    assert_eq!(
        host.read_file(Path::new("src/deep/new.txt")).unwrap(),
        b"allowed"
    );
    assert!(!base.join("src/new.mbt").exists());
    assert!(!base.join("src/deep/new.mbt").exists());
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
fn a_bare_policy_filename_anchors_globs_in_the_current_directory() {
    // Path::parent for "policy.toml" is the empty path, not None.
    let rules = policy(
        Path::new("policy.toml").parent().unwrap(),
        "[fs]\nread = [{ glob = \"logs/*.txt\" }]\n",
    );
    let base = std::fs::canonicalize(".").unwrap();
    assert!(allows(
        &rules,
        &base.join("logs/new.txt"),
        FsIntents::read()
    ));
    assert!(!allows(
        &rules,
        &base.join("logs/deep/new.txt"),
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
    for glob in [
        "[",
        "",
        "logs/**/../*.txt",
        "bad\0name",
        "!",
        "![",
        "!logs/**/../*.txt",
        "!bad\0name",
    ] {
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

#[test]
fn transferred_globs_validate_the_anchor_and_pattern() {
    let directory = tempfile::tempdir().unwrap();
    let rules = policy(
        directory.path(),
        "[fs]\nread = [{ glob = \"**/*.txt\" }, { glob = \"!**/*.mbt\" }]\n",
    );
    let bytes = rules.open_transfer().unwrap().read().unwrap();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for (field, value) in [
        ("pattern", serde_json::json!("[")),
        ("pattern", serde_json::json!("../**")),
        (
            "base",
            serde_json::to_value(std::ffi::OsString::from("relative")).unwrap(),
        ),
        ("unknown", serde_json::json!(true)),
    ] {
        for rule in 0..2 {
            let mut document = original.clone();
            document["rule_groups"][0]["fs"]["read"][rule]["value"][field] = value.clone();
            assert!(
                Policy::from_transfer(&serde_json::to_vec(&document).unwrap()).is_err(),
                "{field}: rule {rule}"
            );
        }
    }
}

#[test]
fn native_names_are_not_lossily_substituted_before_matching() {
    let directory = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(directory.path()).unwrap();
    #[cfg(unix)]
    let name = {
        use std::os::unix::ffi::OsStringExt;
        std::ffi::OsString::from_vec(b"bad-\xff.txt".to_vec())
    };
    #[cfg(windows)]
    let name = {
        use std::os::windows::ffi::OsStringExt;
        std::ffi::OsString::from_wide(&[98, 97, 100, 45, 0xd800, 46, 116, 120, 116])
    };
    let rules = policy(
        &base,
        "[fs]\nread = [{ glob = \"bad-�.txt\" }]\nwrite = [{ glob = \"*.txt\" }]\n",
    );
    assert!(!allows(&rules, &base.join(&name), FsIntents::read()));
    assert!(allows(&rules, &base.join(&name), FsIntents::write()));
}

#[test]
fn glob_backslashes_follow_native_separator_rules() {
    let directory = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(directory.path()).unwrap();
    let rules = policy(&base, "[fs]\nread = [{ glob = 'logs\\*.txt' }]\n");
    assert!(allows(
        &rules,
        &base.join(r"logs\new.txt"),
        FsIntents::read()
    ));
    assert_eq!(
        allows(&rules, &base.join("logs/new.txt"), FsIntents::read()),
        cfg!(windows)
    );
    assert!(!allows(
        &rules,
        &base.join("logs/deep/new.txt"),
        FsIntents::read()
    ));
    #[cfg(windows)]
    for pattern in [r"C:logs\*.txt", r"\logs\*.txt"] {
        let config =
            serde_json::from_value(serde_json::json!({"fs":{"read":[{"glob":pattern}]}})).unwrap();
        assert!(source::resolve(config, &base).is_err());
    }
}

#[cfg(unix)]
#[test]
fn host_filesystem_matches_resolved_targets_without_expanding_symlink_grants() {
    use crate::filesystem::{FsError, HostFs};
    use crate::runtime::{Env, WorkingDirectory};
    let directory = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(directory.path()).unwrap();
    std::fs::create_dir(base.join("logs")).unwrap();
    std::fs::create_dir(base.join("other")).unwrap();
    std::fs::write(base.join("other/secret.txt"), b"secret").unwrap();
    std::os::unix::fs::symlink("../other/secret.txt", base.join("logs/link.txt")).unwrap();
    let rules = policy(
        &base,
        "[fs]\nread = [{ glob = \"logs/**/*.txt\" }]\nwrite = [{ glob = \"logs/**/*.txt\" }]\n",
    );
    let host = HostFs::new(
        Some(rules.filesystem()),
        Arc::new(Env::ambient()),
        Arc::new(WorkingDirectory::Fixed(base.clone())),
    );
    host.write_file(Path::new("logs/new.txt"), b"allowed")
        .unwrap();
    assert_eq!(
        host.read_file(Path::new("logs/new.txt")).unwrap(),
        b"allowed"
    );
    assert!(matches!(
        host.write_file(Path::new("logs/new.bin"), b"denied"),
        Err(FsError::Denied)
    ));
    assert!(matches!(
        host.read_file(Path::new("logs/link.txt")),
        Err(FsError::Denied)
    ));
    assert!(matches!(
        host.write_file(Path::new("logs/link.txt"), b"denied"),
        Err(FsError::Denied)
    ));
    assert_eq!(
        std::fs::read(base.join("other/secret.txt")).unwrap(),
        b"secret"
    );
    assert!(!base.join("logs/new.bin").exists());
}
