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
    read_dependency_module_manifest, read_module_manifest,
    read_package_desc_file_from_path_with_supported_targets_decl, warn_module_manifest,
};
use moonutil::user_log::UserLog;

#[test]
fn module_target_aliases_reject_duplicate_declarations() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("moon.mod");
    for (canonical, legacy) in [
        ("preferred_target", "preferred-target"),
        ("supported_targets", "supported-targets"),
    ] {
        // Both spellings remain usable on their own.
        for declaration in [
            format!("{canonical} = \"wasm\""),
            format!("options(\"{legacy}\": \"wasm\")"),
        ] {
            std::fs::write(&path, format!("name = \"test/aliases\"\n{declaration}")).unwrap();
            read_module_manifest(dir.path(), false).unwrap();
        }
        // Equal values and malformed values must not disappear behind an alias.
        for value in [r#""wasm""#, r#""js""#, "123", "false"] {
            for declaration in [
                format!("{canonical} = \"wasm\"\noptions(\"{legacy}\": {value})"),
                format!("options({canonical}: \"wasm\", \"{legacy}\": {value})"),
            ] {
                std::fs::write(&path, format!("name = \"test/aliases\"\n{declaration}")).unwrap();
                let error = read_module_manifest(dir.path(), false).unwrap_err();
                assert_eq!(
                    error.to_string(),
                    format!("Duplicate key '{canonical}' found in moon.mod.")
                );
            }
        }
    }
}

#[test]
fn removed_package_name_is_unknown_and_does_not_select_an_executable() {
    let dir = tempfile::tempdir().unwrap();
    for filename in ["moon.pkg", "moon.pkg.json"] {
        let path = dir.path().join(filename);
        for value in [r#""main""#, r#""other""#, "123"] {
            let source = if filename == "moon.pkg" {
                format!("options(name: {value})")
            } else {
                format!(r#"{{"name": {value}}}"#)
            };
            std::fs::write(&path, source).unwrap();
            for emit_warnings in [true, false] {
                let (log, capture) = UserLog::captured(log::LevelFilter::Warn);
                let (package, _) = read_package_desc_file_from_path_with_supported_targets_decl(
                    &path,
                    &log,
                    true,
                    emit_warnings,
                )
                .unwrap();
                assert!(!package.is_main);
                assert!(!package.force_link);
                let warnings = capture.take();
                assert_eq!(warnings.len(), usize::from(emit_warnings));
                if emit_warnings {
                    assert_eq!(
                        warnings[0].message,
                        format!("Unknown configuration key `name` in `{}`.", path.display())
                    );
                }
            }
        }
    }
}

#[test]
fn unknown_package_keys_warn_without_rejecting_the_package() {
    let dir = tempfile::tempdir().unwrap();
    for (name, source) in [
        (
            "moon.pkg",
            r#"options(
  future: {"opaque": true},
  "bin-name": 123,
  "bin-target": "removed-backend",
  pkgtype: {"kind": "library", "future": true},
  "virtual": {"has-default": true, "future": true},
  link: {"future": true, "native": {"cc_link_flags": "-lm"}, "wasm-gc": {
    "import-memory": {"module": "env", "name": "memory", "future": true}
  }},
  formatter: {"future": true},
  "import": [{"path": "test/dependency", "future": true}],
  "pre-build": [{"input": "a", "output": "b", "command": "echo", "future": true}]
)"#,
        ),
        (
            "moon.pkg.json",
            r#"{
  "future": {"opaque": true},
  "bin-name": 123,
  "bin-target": "removed-backend",
  "pkgtype": {"kind": "library", "future": true},
  "virtual": {"has-default": true, "future": true},
  "link": {"future": true, "native": {"cc_link_flags": "-lm"}, "wasm-gc": {
    "import-memory": {"module": "env", "name": "memory", "future": true}
  }},
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
            assert_eq!(warnings.len(), if emit_warnings { 11 } else { 0 });
            for key in [
                "future",
                "bin-name",
                "bin-target",
                "pkgtype.future",
                "virtual.future",
                "link.future",
                "link.native.cc_link_flags",
                "link.wasm-gc.import-memory.future",
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
fn removed_stub_link_settings_are_unknown_in_both_package_formats() {
    let dir = tempfile::tempdir().unwrap();
    for (name, source) in [
        (
            "moon.pkg",
            r#"options(link: {"native": {
  "stub-cc-link-flags": "-lunused",
  "stub-lib-deps": ["unused.o"]
}})"#,
        ),
        // Removed fields accept arbitrary values, like other unknown keys.
        (
            "moon.pkg.json",
            r#"{"link": {"native": {
  "stub-cc-link-flags": 123,
  "stub-lib-deps": ["unused.o"]
}}}"#,
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
            let warnings: Vec<_> = capture
                .take()
                .into_iter()
                .map(|entry| entry.message)
                .collect();
            let expected = if emit_warnings {
                vec![
                    format!(
                        "Unknown configuration key `link.native.stub-cc-link-flags` in `{}`.",
                        path.display()
                    ),
                    format!(
                        "Unknown configuration key `link.native.stub-lib-deps` in `{}`.",
                        path.display()
                    ),
                ]
            } else {
                vec![]
            };
            assert_eq!(warnings, expected);
        }
    }
}

#[test]
fn removed_packaging_fields_are_errors_in_both_module_formats() {
    for name in ["moon.mod", "moon.mod.json"] {
        let dir = tempfile::tempdir().unwrap();
        for field in ["include", "exclude"] {
            // Presence is an error, including empty or null values. These
            // controls must not silently stop filtering published files.
            for value in [r#"["src/**"]"#, "[]", "null", "123"] {
                if name == "moon.mod" && value == "null" {
                    continue; // null is not part of the module DSL syntax.
                }
                let source = if name == "moon.mod" {
                    format!("name = \"test/removed\"\noptions({field}: {value})")
                } else {
                    format!(r#"{{"name": "test/removed", "{field}": {value}}}"#)
                };
                std::fs::write(dir.path().join(name), source).unwrap();
                let err = read_module_manifest(dir.path(), true).unwrap_err();
                assert_eq!(
                    err.root_cause().to_string(),
                    format!(
                        "`{field}` is no longer supported in module manifests; use `.gitignore` or `.moonignore` to control packaged files. `.moonignore` overrides `.gitignore` in the same directory."
                    )
                );
                if name == "moon.mod" {
                    read_dependency_module_manifest(dir.path()).unwrap();
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
options(future: true, checksum: 123, scripts: {
  "postadd": "echo installed",
  "prebuild": "echo unused",
  "postbuild": "echo unused"
})
rule(name: "generate", command: "echo", typo: true)
"#,
        ),
        (
            "moon.mod.json",
            r#"{
  "name": "test/diagnostics",
  "future": true,
  "checksum": 123,
  "scripts": {"postadd": "echo installed", "prebuild": "echo unused", "postbuild": "echo unused"},
  "rule": [{"name": "generate", "command": "echo", "typo": true}]
}"#,
        ),
        (
            "moon.mod.json",
            r#"{
  "name": "test/diagnostics",
  "future": true,
  "checksum": 123,
  "scripts": {"postadd": "echo installed", "prebuild": "echo unused", "postbuild": "echo unused"},
  "rule": {"name": "generate", "command": "echo", "typo": true}
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
        assert_eq!(warnings.len(), 5, "{warnings:?}");
        for key in [
            "future",
            "checksum",
            "scripts.prebuild",
            "scripts.postbuild",
            "rule[0].typo",
        ] {
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
        r#"options("warn-list": false)"#,
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
        "options(\"warn-list\": false)",
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
    for source in [
        r#"{"is-main": true, "is-main": false}"#,
        r#"{"link": {"native": {"cc": "clang", "cc": "gcc"}}}"#,
        r#"{"import": [{"path": "a/b", "path": "c/d"}]}"#,
        r#"{"pre-build": [{"input": "a", "output": "b", "command": "echo", "command": "other"}]}"#,
    ] {
        std::fs::write(&path, source).unwrap();
        assert!(
            read_package_desc_file_from_path_with_supported_targets_decl(&path, &log, true, false)
                .is_err(),
            "accepted duplicate fields in {source}"
        );
    }
    let source = r#"{"name": "test/first", "name": "test/second"}"#;
    std::fs::write(dir.path().join("moon.mod.json"), source).unwrap();
    let error = read_module_manifest(dir.path(), true).unwrap_err();
    assert!(format!("{error:#}").contains("duplicate field `name`"));
}

#[test]
fn module_imports_reject_repeated_dependencies() {
    let dir = tempfile::tempdir().unwrap();
    for version in ["1.0.0", "2.0.0"] {
        for declarations in [
            format!("import {{ \"test/dep@1.0.0\", \"test/dep@{version}\" }}"),
            format!(
                "import {{ \"test/dep@{version}\" }}\noptions(deps: {{\"test/dep\": \"1.0.0\"}})"
            ),
        ] {
            std::fs::write(
                dir.path().join("moon.mod"),
                format!("name = \"test/duplicates\"\n{declarations}"),
            )
            .unwrap();
            let error = read_module_manifest(dir.path(), false).unwrap_err();
            assert_eq!(
                error.to_string(),
                "Duplicate dependency `test/dep` found in moon.mod."
            );
        }
    }
}

#[test]
fn prebuild_entries_reject_command_with_rule() {
    let dir = tempfile::tempdir().unwrap();
    let log = UserLog::new(log::LevelFilter::Off);
    for (filename, source) in [
        (
            "moon.pkg",
            r#"dev_build(input: "a", output: "b", command: "echo", rule: "generate")"#,
        ),
        (
            "moon.pkg",
            r#"options("pre-build": [{"input": "a", "output": "b", "command": "echo", "rule": "generate"}])"#,
        ),
        (
            "moon.pkg.json",
            r#"{"pre-build": [{"input": "a", "output": "b", "command": "echo", "rule": "generate"}]}"#,
        ),
    ] {
        let path = dir.path().join(filename);
        std::fs::write(&path, source).unwrap();
        let error =
            read_package_desc_file_from_path_with_supported_targets_decl(&path, &log, true, false)
                .unwrap_err();
        assert!(
            format!("{error:#}").contains("`command` and `rule` cannot be used together"),
            "{error:#}"
        );
    }
}

#[test]
fn module_warning_aliases_reject_duplicate_declarations() {
    let dir = tempfile::tempdir().unwrap();
    for (warnings, legacy) in [
        (r#""+a""#, r#""+a""#),
        (r#""+a""#, r#""-a""#),
        (r#""""#, r#""""#),
        (r#""+a""#, "false"),
    ] {
        for declaration in [
            format!("warnings = {warnings}\noptions(\"warn-list\": {legacy})"),
            format!("options(warnings: {warnings}, \"warn-list\": {legacy})"),
        ] {
            std::fs::write(
                dir.path().join("moon.mod"),
                format!("name = \"test/warnings\"\n{declaration}"),
            )
            .unwrap();
            let error = read_module_manifest(dir.path(), false).unwrap_err();
            assert_eq!(
                error.to_string(),
                "Duplicate key 'warnings' found in moon.mod."
            );
        }
    }
}

#[test]
fn module_warning_spellings_remain_individually_supported() {
    let dir = tempfile::tempdir().unwrap();
    for (filename, source) in [
        ("moon.mod", "name = \"test/warnings\"\nwarnings = \"+a-b\""),
        (
            "moon.mod",
            "name = \"test/warnings\"\noptions(warnings: \"+a-b\")",
        ),
        (
            "moon.mod",
            "name = \"test/warnings\"\noptions(\"warn-list\": \"+a-b\")",
        ),
        (
            "moon.mod.json",
            r#"{"name": "test/warnings", "warn-list": "+a-b"}"#,
        ),
    ] {
        let path = dir.path().join(filename);
        std::fs::write(&path, source).unwrap();
        let (_, module) = read_module_manifest(dir.path(), true).unwrap();
        assert_eq!(module.warn_list.as_deref(), Some("+a-b"));
        std::fs::remove_file(path).unwrap();
    }
}
