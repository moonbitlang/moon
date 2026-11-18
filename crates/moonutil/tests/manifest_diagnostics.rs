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

use moonutil::manifest::{
    read_module_manifest, read_package_desc_file_from_path_with_supported_targets_decl,
    warn_module_manifest,
};
use moonutil::user_log::UserLog;

#[test]
fn unknown_package_keys_warn_without_rejecting_the_package() {
    let dir = tempfile::tempdir().unwrap();
    for (name, source) in [
        (
            "moon.pkg",
            r#"options(
  future: true,
  link: {"native": {"cc_link_flags": "-lm"}},
  formatter: {"future": true},
  "import": [{"path": "test/dependency", "future": true}],
  "pre-build": [{"input": "a", "output": "b", "command": "echo", "future": true}]
)"#,
        ),
        (
            "moon.pkg.json",
            r#"{
  "future": true,
  "link": {"native": {"cc_link_flags": "-lm"}},
  "formatter": {"future": true},
  "import": [{"path": "test/dependency", "future": true}],
  "pre-build": [{"input": "a", "output": "b", "command": "echo", "future": true}]
}"#,
        ),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, source).unwrap();
        for emit_warnings in [true, false] {
            let (log, capture) = UserLog::captured(log::LevelFilter::Warn);
            read_package_desc_file_from_path_with_supported_targets_decl(
                &path,
                &log,
                true,
                emit_warnings,
            )
            .unwrap();
            let warnings = capture.take();
            assert_eq!(warnings.len(), if emit_warnings { 5 } else { 0 });
            for key in [
                "future",
                "link.native.cc_link_flags",
                "formatter.future",
                "import[0].future",
                "pre-build[0].future",
            ] {
                if emit_warnings {
                    assert!(
                        warnings
                            .iter()
                            .any(|warning| warning.message.contains(&format!("`{key}`"))),
                        "{warnings:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn module_unknown_key_warnings_reuse_the_parsed_manifest() {
    let dir = tempfile::tempdir().unwrap();
    for (name, source) in [
        (
            "moon.mod",
            r#"name = "test/diagnostics"
options(future: true)
rule(name: "generate", command: "echo", typo: true)
"#,
        ),
        (
            "moon.mod.json",
            r#"{
  "name": "test/diagnostics",
  "future": true,
  "rule": [{"name": "generate", "command": "echo", "typo": true}]
}"#,
        ),
    ] {
        let path = dir.path().join(name);
        std::fs::write(&path, source).unwrap();
        let (format, module) = read_module_manifest(dir.path(), true).unwrap();
        std::fs::remove_file(path).unwrap();
        let (log, capture) = UserLog::captured(log::LevelFilter::Warn);
        warn_module_manifest(dir.path(), format, &module, &log);
        let warnings = capture.take();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        for key in ["future", "rule[0].typo"] {
            assert!(
                warnings
                    .iter()
                    .any(|warning| warning.message.contains(&format!("`{key}`"))),
                "{warnings:?}"
            );
        }
    }
}

#[test]
fn known_option_types_are_checked_even_when_warnings_are_suppressed() {
    let dir = tempfile::tempdir().unwrap();
    let log = UserLog::new(log::LevelFilter::Off);
    for source in [
        "warnings = 123",
        r#"warnings = "+a"
options("warn-list": false)"#,
        "options = false",
        r#"options(link: {"native": {"cc-flags": 123}})"#,
        r#"options("alert-list": false)"#,
        r#"options(link: {"wasm-gc": {"use-ffi-bytes": "true"}})"#,
        r#"options("pre-build": [{"input": "a", "output": "b", "rule": "generate", "command": 123}])"#,
        r#"options("pre-build": [{"input": "a", "output": "b", "command": "echo", "rule": null}])"#,
    ] {
        let path = dir.path().join("moon.pkg");
        std::fs::write(&path, source).unwrap();
        assert!(read_package_desc_file_from_path_with_supported_targets_decl(
            &path, &log, false, false,
        ).is_err(), "{source}");
    }
    for source in [
        "warnings = 123",
        "options = false",
        "warnings = \"+a\"\noptions(\"warn-list\": false)",
    ] {
        std::fs::write(
            dir.path().join("moon.mod"),
            format!("name = \"test/diagnostics\"\n{source}"),
        )
        .unwrap();
        assert!(read_module_manifest(dir.path(), true).is_err(), "{source}");
    }
}

#[test]
fn compiler_owned_options_are_not_unknown_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("moon.pkg");
    std::fs::write(
        &path,
        r#"
regex_backend = "table"
options(
  "alert-list": "+all",
  link: {"wasm-gc": {
    "use-js-builtin-string": true,
    "imported-string-constants": "strings",
    "use-ffi-bytes": true
  }}
)
"#,
    )
    .unwrap();
    let (log, capture) = UserLog::captured(log::LevelFilter::Warn);
    let (package, _) =
        read_package_desc_file_from_path_with_supported_targets_decl(&path, &log, false, true)
            .unwrap();
    assert_eq!(package.alert_list.as_deref(), Some("+all"));
    assert_eq!(
        package.link.unwrap().wasm_gc.unwrap().use_ffi_bytes,
        Some(true)
    );
    assert!(capture.take().is_empty());
}

#[test]
fn json_duplicate_known_fields_remain_errors() {
    let dir = tempfile::tempdir().unwrap();
    let log = UserLog::new(log::LevelFilter::Off);
    let path = dir.path().join("moon.pkg.json");
    for (source, field) in [
        (r#"{"name": "test/first", "name": "test/second"}"#, "name"),
        (
            r#"{"link": {"native": {"cc": "clang", "cc": "gcc"}}}"#,
            "cc",
        ),
        (r#"{"import": [{"path": "a/b", "path": "c/d"}]}"#, "path"),
        (
            r#"{"pre-build": [{"input": "a", "output": "b", "command": "echo", "command": "other"}]}"#,
            "command",
        ),
    ] {
        std::fs::write(&path, source).unwrap();
        let error =
            read_package_desc_file_from_path_with_supported_targets_decl(&path, &log, true, false)
                .unwrap_err();
        assert!(
            format!("{error:#}").contains(&format!("duplicate field `{field}`")),
            "{error:#}"
        );
    }
    let source = r#"{"name": "test/first", "name": "test/second"}"#;
    std::fs::write(dir.path().join("moon.mod.json"), source).unwrap();
    let error = read_module_manifest(dir.path(), true).unwrap_err();
    assert!(format!("{error:#}").contains("duplicate field `name`"));
}
