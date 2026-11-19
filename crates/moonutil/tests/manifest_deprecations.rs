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
fn legacy_package_kinds_warn_without_changing_the_selected_kind() {
    let dir = tempfile::tempdir().unwrap();
    for (fields, kind, expected) in [
        (
            r#""is-main": true"#,
            (true, false),
            vec!["`is-main: true` is deprecated; use `pkgtype(kind: \"executable\")` instead."],
        ),
        (
            r#""is-main": false"#,
            (false, false),
            vec!["`is-main: false` is deprecated; remove it."],
        ),
        (
            r#""link": true"#,
            (false, true),
            vec!["`link: true` is deprecated; use `pkgtype(kind: \"foreign_library\")` instead."],
        ),
        (
            r#""link": false"#,
            (false, false),
            vec!["`link: false` is deprecated; remove it."],
        ),
        (
            r#""is-main": true, "link": true"#,
            (true, true),
            vec![
                "`is-main: true` is deprecated; use `pkgtype(kind: \"executable\")` instead.",
                "`link: true` is deprecated; remove it from executable packages.",
            ],
        ),
    ] {
        for filename in ["moon.pkg", "moon.pkg.json"] {
            let path = dir.path().join(filename);
            let source = if filename == "moon.pkg" {
                format!("options({fields})")
            } else {
                format!("{{{fields}}}")
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
                assert_eq!((package.is_main, package.force_link), kind);
                let actual: Vec<_> = capture
                    .take()
                    .into_iter()
                    .map(|entry| entry.message)
                    .collect();
                assert_eq!(
                    actual,
                    if emit_warnings {
                        expected.clone()
                    } else {
                        vec![]
                    }
                );
            }
        }
    }
}

#[test]
fn backend_exports_warn_including_empty_lists() {
    let dir = tempfile::tempdir().unwrap();
    let fields = r#""link": {
        "wasm": {"exports": ["hello:renamed"]},
        "wasm-gc": {"exports": []},
        "js": {"exports": ["hello"]},
        "native": {"exports": []}
    }"#;
    for filename in ["moon.pkg", "moon.pkg.json"] {
        let path = dir.path().join(filename);
        let source = if filename == "moon.pkg" {
            format!("options({fields})")
        } else {
            format!("{{{fields}}}")
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
            assert_eq!(
                package.link.unwrap().wasm.unwrap().exports.unwrap(),
                ["hello:renamed"]
            );
            let actual: Vec<_> = capture
                .take()
                .into_iter()
                .map(|entry| entry.message)
                .collect();
            let expected = if emit_warnings {
                vec![
                    "`link.wasm.exports` is deprecated; use `#export_name` attributes in MoonBit source instead.",
                    "`link.wasm-gc.exports` is deprecated; use `#export_name` attributes in MoonBit source instead.",
                    "`link.js.exports` is deprecated; use `#export_name` attributes in MoonBit source instead.",
                    "`link.native.exports` is deprecated; use `#export_name` attributes in MoonBit source instead.",
                ]
            } else {
                vec![]
            };
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn prebuild_stdin_warning_describes_the_protocol_without_running_the_script() {
    let dir = tempfile::tempdir().unwrap();
    for (filename, source) in [
        (
            "moon.mod",
            "name = \"test/prebuild\"\noptions(\"--moonbit-unstable-prebuild\": \"build.js\")",
        ),
        (
            "moon.mod.json",
            r#"{"name":"test/prebuild","--moonbit-unstable-prebuild":"build.js"}"#,
        ),
    ] {
        let path = dir.path().join(filename);
        std::fs::write(&path, source).unwrap();
        let (format, module) = read_module_manifest(dir.path(), true).unwrap();
        std::fs::remove_file(path).unwrap();
        for level in [log::LevelFilter::Warn, log::LevelFilter::Error] {
            let (log, capture) = UserLog::captured(level);
            warn_module_manifest(dir.path(), format, &module, &log);
            let actual: Vec<_> = capture
                .take()
                .into_iter()
                .map(|entry| entry.message)
                .collect();
            let expected = if level == log::LevelFilter::Warn {
                vec![
                    "Legacy stdin JSON for prebuild script `build.js` in module `test/prebuild` is deprecated and will be removed; read `MOON_MOD`, `MOON_BUILD_DIR`, and the other `MOON_*` environment variables instead.",
                ]
            } else {
                vec![]
            };
            assert_eq!(actual, expected);
        }
    }
}
