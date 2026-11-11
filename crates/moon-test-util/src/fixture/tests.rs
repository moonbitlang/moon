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
fn inline_files_follow_cwd_and_preserve_default_build_paths() {
    let fixture = Fixture::new().with_files([("moon.mod", "root"), ("pkg/input", "before")]);
    let root = fixture.cwd().to_owned();
    let fixture = fixture.in_subdir("pkg").with_files([("input", "after")]);
    assert_eq!(
        std::fs::read_to_string(root.join("moon.mod")).unwrap(),
        "root"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.join("input")).unwrap(),
        "after"
    );
    assert!(!root.join("input").exists());
    assert!(fixture.target_dir().is_none());
    let cmd = fixture.moon_process_cmd("moon");
    assert_eq!(cmd.get_current_dir(), Some(root.join("pkg").as_path()));
    assert_eq!(cmd.get_args().count(), 0);
    drop(fixture);
    assert!(!root.exists());
}

#[test]
fn copied_sources_are_private_and_preserve_default_build_paths() {
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("input"), "original").unwrap();
    let fixture = Fixture::copied(source.path());
    let workspace = fixture.cwd().to_owned();
    assert_ne!(workspace, source.path());
    assert_eq!(
        std::fs::read_to_string(fixture.join("input")).unwrap(),
        "original"
    );
    let fixture = fixture.with_files([("input", "changed")]);
    assert_eq!(
        std::fs::read_to_string(fixture.join("input")).unwrap(),
        "changed"
    );
    assert_eq!(
        std::fs::read_to_string(source.path().join("input")).unwrap(),
        "original"
    );
    assert!(fixture.target_dir().is_none());
    let cmd = fixture.moon_process_cmd("moon");
    assert_eq!(cmd.get_current_dir(), Some(workspace.as_path()));
    assert_eq!(cmd.get_args().count(), 0);
    drop(fixture);
    assert!(!workspace.exists());
    assert!(source.path().join("input").exists());
}

#[test]
fn shared_sources_keep_distinct_targets_when_changing_cwd() {
    let source = tempfile::tempdir().unwrap();
    std::fs::create_dir(source.path().join("pkg")).unwrap();
    std::fs::write(source.path().join("pkg/input"), "shared").unwrap();
    let first = Fixture::readonly(source.path());
    let target = first.target_dir().unwrap().to_owned();
    let first = first.in_subdir("pkg");
    let second = Fixture::readonly(source.path()).in_subdir("pkg");
    let other_target = second.target_dir().unwrap().to_owned();
    assert_ne!(target, other_target);
    assert_eq!(first.target_dir(), Some(target.as_path()));
    assert_eq!(first.cwd(), source.path().join("pkg"));
    assert_eq!(first.join("input"), source.path().join("pkg/input"));
    // Both the public command entry point and legacy generic helpers retain the override.
    for (cmd, expected_target) in [
        (first.moon_process_cmd("moon"), &target),
        (
            TestCommandDir::moon_process_cmd(&second, "moon"),
            &other_target,
        ),
    ] {
        assert_eq!(
            cmd.get_current_dir(),
            Some(source.path().join("pkg").as_path())
        );
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(
            args,
            [OsStr::new("--target-dir"), expected_target.as_os_str()]
        );
    }
    for build in [&target, &other_target] {
        std::fs::create_dir(build).unwrap();
        std::fs::write(build.join("artifact"), "generated").unwrap();
    }
    drop(first);
    assert!(!target.exists());
    assert!(other_target.join("artifact").exists());
    assert_eq!(
        std::fs::read_to_string(source.path().join("pkg/input")).unwrap(),
        "shared"
    );
    drop(second);
    assert!(!other_target.exists());
    assert!(source.path().join("pkg/input").exists());
}

#[test]
fn shared_sources_reject_inline_writes() {
    let source = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("input"), "original").unwrap();
    let result = std::panic::catch_unwind(|| {
        Fixture::readonly(source.path()).with_files([("input", "changed")])
    });
    assert!(result.is_err());
    assert_eq!(
        std::fs::read_to_string(source.path().join("input")).unwrap(),
        "original"
    );
}

#[test]
fn legacy_path_commands_do_not_override_the_build_directory() {
    let source = tempfile::tempdir().unwrap();
    let cmd = TestCommandDir::moon_process_cmd(&source.path(), "moon");
    assert_eq!(cmd.get_current_dir(), Some(source.path()));
    assert_eq!(cmd.get_args().count(), 0);
}
