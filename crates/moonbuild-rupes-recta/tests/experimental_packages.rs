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

#![allow(clippy::disallowed_methods)] // These tests exercise manifest discovery.

use moonbuild_rupes_recta::{ProjectPreparationConfig, discover_synced_project};
use moonutil::{
    manifest::read_module_desc_file_in_dir,
    project::WorkspaceEnv,
    resolution::{DirSyncResult, ModuleDependencyGraph, ModuleSource},
    target::TargetBackend::{self, Js, Native},
    user_log::{UserLog, UserLogEntryLevel},
};

fn warnings(packages: &[(&str, &str)], backends: &[TargetBackend]) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("moon.mod"),
        "name = \"example/experimental\"\n",
    )
    .unwrap();
    for (path, manifest) in packages {
        let root = dir.path().join(path);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("moon.pkg"), manifest).unwrap();
    }
    let module = read_module_desc_file_in_dir(dir.path()).unwrap();
    let source = ModuleSource::from_local_module(&module, dir.path()).unwrap();
    let (module_graph, mid) = ModuleDependencyGraph::only_one_module(source, module);
    let mut module_dirs = DirSyncResult::new();
    module_dirs.insert(mid, dir.path().to_owned());
    let (log, capture) = UserLog::captured(log::LevelFilter::Warn);
    let config =
        ProjectPreparationConfig::new_with_load_defaults(false, true, false, WorkspaceEnv::Auto);
    discover_synced_project(&config, (module_graph, module_dirs), &log)
        .unwrap()
        .resolve_packages(backends, &log)
        .unwrap();
    capture
        .take()
        .into_iter()
        .map(|entry| {
            assert!(matches!(entry.level, UserLogEntryLevel::Warning));
            entry.message
        })
        .collect()
}

#[test]
fn direct_experimental_imports_warn_once_across_target_kinds_and_backends() {
    for imports in [
        r#"import { "example/experimental/unstable" @renamed }"#,
        r#"import { "example/experimental/unstable" } for "wbtest""#,
        r#"import { "example/experimental/unstable" } for "test""#,
        r#"
import { "example/experimental/unstable" }
import { "example/experimental/unstable" @whitebox } for "wbtest"
import { "example/experimental/unstable" @blackbox } for "test"
"#,
    ] {
        let messages = warnings(
            &[("app", imports), ("unstable", "experimental = true")],
            TargetBackend::all(),
        );
        assert_eq!(
            messages,
            [
                "Package `example/experimental/app` depends on experimental package `example/experimental/unstable`. \
Set `options(\"allow-experimental-packages\": [\"example/experimental/unstable\"])` in the importing package's moon.pkg to acknowledge this dependency."
            ],
            "{imports}"
        );
    }
}

#[test]
fn experimental_allowance_matches_full_package_names() {
    for (option, expected_count) in [
        ("", 2),
        (r#"options("allow-experimental-packages": false)"#, 2),
        (r#"options("allow-experimental-packages": true)"#, 0),
        (r#"options("allow-experimental-packages": [])"#, 2),
        (
            r#"options("allow-experimental-packages": ["example/experimental/first"])"#,
            1,
        ),
        (
            r#"options("allow-experimental-packages": ["example/experimental/first", "example/experimental/second"])"#,
            0,
        ),
        (
            r#"options("allow-experimental-packages": ["renamed", "example/experimental", "example/experimental/first/child"])"#,
            2,
        ),
    ] {
        let manifest = format!(
            r#"
import {{ "example/experimental/first" @renamed, "example/experimental/second", "example/experimental/stable" }}
{option}
"#
        );
        let messages = warnings(
            &[
                ("app", &manifest),
                ("first", "experimental = true"),
                ("second", "experimental = true"),
                ("stable", "experimental = false"),
            ],
            &[Native],
        );
        assert_eq!(messages.len(), expected_count, "{option}: {messages:?}");
        if expected_count == 1 {
            assert!(
                messages[0]
                    .contains("depends on experimental package `example/experimental/second`")
            );
        }
    }
}

#[test]
fn experimental_warnings_follow_active_imports() {
    let packages = [
        (
            "app",
            r#"#cfg(target = "native") import { "example/experimental/unstable" }"#,
        ),
        ("unstable", "experimental = true"),
    ];
    assert!(warnings(&packages, &[Js]).is_empty());
    assert_eq!(warnings(&packages, &[Native]).len(), 1);
    assert_eq!(warnings(&packages, &[Js, Native]).len(), 1);
}

#[test]
fn experimental_status_and_allowances_do_not_propagate() {
    for parent_option in ["", r#"options("allow-experimental-packages": true)"#] {
        let manifest = format!(
            r#"import {{ "example/experimental/wrapper" }}
{parent_option}"#
        );
        let messages = warnings(
            &[
                ("app", &manifest),
                ("wrapper", r#"import { "example/experimental/unstable" }"#),
                ("unstable", "experimental = true"),
            ],
            &[Native],
        );
        assert_eq!(messages.len(), 1);
        assert!(messages[0].starts_with("Package `example/experimental/wrapper` depends on"));
    }
    // Blackbox tests implicitly depend on their own source package.
    assert!(warnings(&[("unstable", "experimental = true")], &[Native]).is_empty());
}
