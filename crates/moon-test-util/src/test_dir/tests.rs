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

fn records(root: &Path) -> Vec<serde_json::Value> {
    let files: Vec<_> = std::fs::read_dir(root).unwrap().collect();
    assert_eq!(files.len(), 1);
    std::fs::read_to_string(files[0].as_ref().unwrap().path())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn copied_fixture_preserves_siblings_and_isolates_writes() {
    let source = tempfile::tempdir().unwrap();
    for name in ["app", "dependency", "target"] {
        std::fs::create_dir(source.path().join(name)).unwrap();
        std::fs::write(source.path().join(name).join("input"), name).unwrap();
    }
    let workspace;
    {
        let dir = TestDir::prepare(Some(source.path()), true, Options::default());
        workspace = dir.as_ref().to_owned();
        assert_eq!(
            std::fs::read_to_string(dir.join("app/input")).unwrap(),
            "app"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("dependency/input")).unwrap(),
            "dependency"
        );
        assert!(!dir.join("target").exists());
        std::fs::write(dir.join("app/input"), "changed").unwrap();
        assert_eq!(
            std::fs::read_to_string(source.path().join("app/input")).unwrap(),
            "app"
        );
        std::fs::create_dir(dir.join("_build")).unwrap();
        std::fs::write(dir.join("_build/artifact"), "generated").unwrap();
    }
    assert!(
        !workspace.exists(),
        "default cleanup includes generated artifacts"
    );
}

#[test]
fn trace_records_phase_boundaries_and_durations() {
    let source = tempfile::tempdir().unwrap();
    let traces = tempfile::tempdir().unwrap();
    std::fs::write(source.path().join("input"), "abc").unwrap();
    let dir = TestDir::prepare(
        Some(source.path()),
        true,
        Options {
            trace_dir: Some(traces.path().to_owned()),
            keep_dir: None,
        },
    );
    let workspace = dir.as_ref().to_owned();
    // Completed preparation is available while the test is still running.
    let prepared = records(traces.path());
    assert_eq!(prepared.last().unwrap()["event"]["phase"], "copy");
    assert_eq!(prepared.last().unwrap()["event"]["state"], "ok");
    drop(dir);
    let data = records(traces.path());
    let phases: Vec<_> = data
        .iter()
        .map(|row| {
            (
                row["event"]["phase"].as_str().unwrap(),
                row["event"]["state"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        phases,
        [
            ("create", "started"),
            ("create", "ok"),
            ("copy", "started"),
            ("copy", "ok"),
            ("cleanup", "started"),
            ("cleanup", "ok"),
        ]
    );
    for row in data.iter().filter(|row| row["event"]["state"] != "started") {
        assert!(row["event"]["elapsed_us"].is_u64());
    }
    assert!(!workspace.exists());
}

#[test]
fn inline_files_record_setup_and_are_cleaned_up() {
    let traces = tempfile::tempdir().unwrap();
    let dir = TestDir::prepare(
        None,
        false,
        Options {
            trace_dir: Some(traces.path().to_owned()),
            keep_dir: None,
        },
    )
    .with_files([
        ("moon.mod", b"name = \"test/inline\"\n".as_slice()),
        ("src/nested/input", &[0, 255, 10]),
        ("src/moon.pkg", b""),
    ]);
    let workspace = dir.as_ref().to_owned();
    assert_eq!(
        std::fs::read_to_string(dir.join("moon.mod")).unwrap(),
        "name = \"test/inline\"\n"
    );
    assert_eq!(
        std::fs::read(dir.join("src/nested/input")).unwrap(),
        [0, 255, 10]
    );
    assert_eq!(std::fs::read(dir.join("src/moon.pkg")).unwrap(), b"");
    let prepared = records(traces.path());
    assert_eq!(prepared.last().unwrap()["event"]["phase"], "setup");
    assert_eq!(prepared.last().unwrap()["event"]["state"], "ok");
    drop(dir);
    let data = records(traces.path());
    let phases: Vec<_> = data
        .iter()
        .map(|row| {
            (
                row["event"]["phase"].as_str().unwrap(),
                row["event"]["state"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        phases,
        [
            ("create", "started"),
            ("create", "ok"),
            ("setup", "started"),
            ("setup", "ok"),
            ("cleanup", "started"),
            ("cleanup", "ok"),
        ]
    );
    for row in data.iter().filter(|row| row["event"]["state"] != "started") {
        assert!(row["event"]["elapsed_us"].is_u64());
    }
    assert!(!workspace.exists());
}

#[test]
fn inline_file_error_cleans_up_or_retains_partial_setup() {
    for retain in [false, true] {
        let traces = tempfile::tempdir().unwrap();
        let kept = tempfile::tempdir().unwrap();
        let result = std::panic::catch_unwind(|| {
            TestDir::prepare(
                None,
                false,
                Options {
                    trace_dir: Some(traces.path().to_owned()),
                    keep_dir: retain.then(|| kept.path().to_owned()),
                },
            )
            // The first file prevents creating the second file's parent directory.
            .with_files([("input", "keep this"), ("input/blocked", "unreachable")])
        });
        assert!(result.is_err());
        let data = records(traces.path());
        let setup = data
            .iter()
            .find(|row| row["event"]["phase"] == "setup" && row["event"]["state"] == "error")
            .unwrap();
        assert!(setup["event"]["elapsed_us"].is_u64());
        assert!(setup["event"]["error"].as_str().unwrap().contains("input"));
        let workspace = Path::new(setup["workspace"].as_str().unwrap());
        if retain {
            assert_eq!(
                std::fs::read_to_string(workspace.join("input")).unwrap(),
                "keep this"
            );
            assert_eq!(data.last().unwrap()["event"]["state"], "retained");
        } else {
            assert!(!workspace.exists());
            assert_eq!(data.last().unwrap()["event"]["state"], "ok");
        }
    }
}

#[test]
fn inline_file_paths_reject_parent_components_and_absolute_paths() {
    let outside = tempfile::tempdir().unwrap();
    for path in [PathBuf::from("../outside"), outside.path().join("absolute")] {
        let result = std::panic::catch_unwind(|| {
            TestDir::prepare(
                None,
                false,
                Options {
                    trace_dir: None,
                    keep_dir: Some(outside.path().to_owned()),
                },
            )
            .with_files([(path, "invalid")])
        });
        assert!(result.is_err());
    }
    assert!(!outside.path().join("outside").exists());
    assert!(!outside.path().join("absolute").exists());
}

#[test]
fn retention_keeps_generated_files_and_reports_no_cleanup() {
    let kept = tempfile::tempdir().unwrap();
    let traces = tempfile::tempdir().unwrap();
    let dir = TestDir::prepare(
        None,
        false,
        Options {
            trace_dir: Some(traces.path().to_owned()),
            keep_dir: Some(kept.path().to_owned()),
        },
    );
    let workspace = dir.as_ref().to_owned();
    std::fs::write(dir.join("result"), "inspect me").unwrap();
    drop(dir);
    assert_eq!(
        std::fs::read_to_string(workspace.join("result")).unwrap(),
        "inspect me"
    );
    let data = records(traces.path());
    assert!(data.iter().all(|row| row["event"]["phase"] != "copy"));
    assert_eq!(data.last().unwrap()["event"]["state"], "retained");
}

#[test]
fn copy_error_keeps_the_workspace_when_requested() {
    let source = tempfile::tempdir().unwrap();
    let kept = tempfile::tempdir().unwrap();
    let traces = tempfile::tempdir().unwrap();
    let result = std::panic::catch_unwind(|| {
        TestDir::prepare(
            Some(&source.path().join("missing")),
            true,
            Options {
                trace_dir: Some(traces.path().to_owned()),
                keep_dir: Some(kept.path().to_owned()),
            },
        )
    });
    assert!(result.is_err());
    let data = records(traces.path());
    let copy = data
        .iter()
        .find(|row| row["event"]["phase"] == "copy" && row["event"]["state"] == "error")
        .unwrap();
    let workspace = Path::new(copy["workspace"].as_str().unwrap());
    assert!(workspace.is_dir());
    assert!(copy["event"]["error"].is_string());
    assert_eq!(data.last().unwrap()["event"]["state"], "retained");
}

#[test]
fn unwinding_still_cleans_up_by_default() {
    let traces = tempfile::tempdir().unwrap();
    let result = std::panic::catch_unwind(|| {
        let _dir = TestDir::prepare(
            None,
            false,
            Options {
                trace_dir: Some(traces.path().to_owned()),
                keep_dir: None,
            },
        );
        panic!("simulated test failure");
    });
    assert!(result.is_err());
    let data = records(traces.path());
    let cleanup = data.last().unwrap();
    assert_eq!(cleanup["event"]["phase"], "cleanup");
    assert_eq!(cleanup["event"]["state"], "ok");
    assert!(!Path::new(cleanup["workspace"].as_str().unwrap()).exists());
}

#[test]
fn unavailable_trace_destination_does_not_break_a_test() {
    let root = tempfile::tempdir().unwrap();
    let blocker = root.path().join("file");
    std::fs::write(&blocker, "not a directory").unwrap();
    let dir = TestDir::prepare(
        None,
        false,
        Options {
            trace_dir: Some(blocker),
            keep_dir: None,
        },
    );
    let workspace = dir.as_ref().to_owned();
    std::fs::write(dir.join("input"), "ok").unwrap();
    drop(dir);
    assert!(!workspace.exists());
}
