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

#[test]
fn directory_restrictions_survive_intersection_and_transfer() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("work/private/secret")).unwrap();
    let policy = source::resolve(
        toml::from_str(
            r#"
[dirs]
work = ["write"]
"work/private" = ["read"]
"work/private/secret" = []
"#,
        )
        .unwrap(),
        temp.path(),
    )
    .unwrap()
    .policy;
    let root = std::fs::canonicalize(temp.path()).unwrap();
    // Rule transport carries frozen names, without reopening directories.
    let transferred = Policy::from_transfer(&policy.to_transfer().unwrap()).unwrap();
    for policy in [policy, transferred] {
        let fs = policy.filesystem();
        assert!(fs.has_directory_rules());
        for (path, access) in [
            ("work/file", AccessRights::READ_WRITE),
            ("work/private/file", AccessRights::READ),
            ("work/private/secret/file", AccessRights::NONE),
            ("work-other/file", AccessRights::NONE),
        ] {
            assert_eq!(fs.granted_access(Some(&root.join(path))), access);
        }
        let readonly = source::resolve(
            toml::from_str("[fs]\nread = ['work']").unwrap(),
            temp.path(),
        )
        .unwrap()
        .policy;
        let combined = policy.intersect(readonly).filesystem();
        assert_eq!(
            combined.granted_access(Some(&root.join("work/file"))),
            AccessRights::READ
        );
        assert_eq!(
            combined.granted_access(Some(&root.join("work/private/secret/file"))),
            AccessRights::NONE
        );
    }
}

#[test]
fn invalid_directory_configuration_is_rejected_at_loading() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("work")).unwrap();
    std::fs::write(temp.path().join("file"), "").unwrap();
    for source in [
        "[dirs]\nmissing = ['read']",
        "[dirs]\nfile = ['read']",
        "[dirs]\nwork = ['read']\n'./work' = ['write']",
        "[fs]\nread = ['work']\n[dirs]\nwork = ['read']",
        "[dirs]\n'*' = ['read']",
    ] {
        assert!(
            source::resolve(toml::from_str(source).unwrap(), temp.path()).is_err(),
            "{source}"
        );
    }
    assert!(toml::from_str::<PolicyConfig>("[dirs]\nwork = ['execute']").is_err());
}

#[test]
fn inline_directory_permission_is_source_relative() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("work")).unwrap();
    let script = temp.path().join("main.mbtx");
    std::fs::write(
        &script,
        "---\npermission(dirs: { \"work\": [\"read\"] })\n---\nfn main {}\n",
    )
    .unwrap();
    let policy = source::read_file(&script, None)
        .unwrap()
        .policy
        .filesystem();
    assert_eq!(
        policy.granted_access(Some(
            &std::fs::canonicalize(temp.path())
                .unwrap()
                .join("work/file")
        )),
        AccessRights::READ
    );
}
