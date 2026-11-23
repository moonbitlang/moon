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
use crate::{
    package::StringOrArray,
    target::TargetBackend::{Js, LLVM, Native},
};

fn convert_test_pkg_dsl(
    dsl: moon_pkg::Dsl,
    emit_warnings: bool,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    convert_pkg_dsl_to_package_with_supported_targets_decl(
        dsl,
        Path::new("moon.pkg"),
        emit_warnings,
        &UserLog::new(log::LevelFilter::Error),
    )
}

fn convert_test_pkg_json(
    json: MoonPkgJSON,
    emit_warnings: bool,
) -> anyhow::Result<(MoonPkg, SupportedTargetsDeclKind)> {
    convert_pkg_json_to_package_with_supported_targets_decl(
        json,
        emit_warnings,
        &UserLog::new(log::LevelFilter::Error),
    )
}

fn assert_pkg_dsl_matches_json(source: &str, json: Value) {
    for emit_warnings in [false, true] {
        let (dsl_log, dsl_capture) = UserLog::captured(log::LevelFilter::Warn);
        let (json_log, json_capture) = UserLog::captured(log::LevelFilter::Warn);
        let (actual, actual_decl) = convert_pkg_dsl_to_package_with_supported_targets_decl(
            moon_pkg::parse(source).unwrap(),
            Path::new("moon.pkg"),
            emit_warnings,
            &dsl_log,
        )
        .unwrap();
        let (expected, expected_decl) = convert_pkg_json_to_package_with_supported_targets_decl(
            serde_json_lenient::from_value(json.clone()).unwrap(),
            emit_warnings,
            &json_log,
        )
        .unwrap();
        assert_eq!(
            serde_json_lenient::to_value(actual).unwrap(),
            serde_json_lenient::to_value(expected).unwrap(),
            "{source}",
        );
        assert_eq!(actual_decl, expected_decl, "{source}");
        assert_eq!(
            dsl_capture
                .take()
                .into_iter()
                .map(|w| w.message)
                .collect::<Vec<_>>(),
            json_capture
                .take()
                .into_iter()
                .map(|w| w.message)
                .collect::<Vec<_>>(),
            "{source}",
        );
    }
}

#[test]
fn convert_pkg_dsl_matches_json_fields_and_defaults() {
    assert_pkg_dsl_matches_json("", serde_json_lenient::json!({}));
    assert_pkg_dsl_matches_json(
        r#"
import { "example/first", "example/first" @again }
import { "example/second" }
import { "example/test" } for "test"
import { "example/whitebox" } for "wbtest"
pkgtype(kind: "foreign_library")
formatter(ignore: ["generated.mbt"])
warnings = "+1-2"
supported_targets = "all-js"
rule(name: "first", command: "first $input $output")
rule(name: "second", command: "second $input $output")
dev_build(rule: "first", input: "a", output: "b")
dev_build(input: "b", output: "c", command: "generate")
options(
  "is-main": false,
  link: { "wasm": { "heap-start-address": 2214592512 } },
  "proof-enabled": true,
  targets: { "native.mbt": "native" },
  "native-stub": ["stub.c"],
  virtual: { "has-default": true },
  implement: "example/virtual",
  overrides: ["example/impl"],
  "max-concurrent-tests": 8,
  "regex-backend": "table",
)
"#,
        serde_json_lenient::json!({
            "import": ["example/first", {"path": "example/first", "alias": "again"}, "example/second"],
            "test-import": ["example/test"],
            "wbtest-import": ["example/whitebox"],
            "pkgtype": {"kind": "foreign_library"},
            "formatter": {"ignore": ["generated.mbt"]},
            "warn-list": "+1-2",
            "supported-targets": "all-js",
            "rule": [
                {"name": "first", "command": "first $input $output"},
                {"name": "second", "command": "second $input $output"},
            ],
            "pre-build": [
                {"rule": "first", "input": "a", "output": "b"},
                {"input": "b", "output": "c", "command": "generate"},
            ],
            "is-main": false,
            "link": {"wasm": {"heap-start-address": 2214592512_u32}},
            "proof-enabled": true,
            "targets": {"native.mbt": "native"},
            "native-stub": ["stub.c"],
            "virtual": {"has-default": true},
            "implement": "example/virtual",
            "overrides": ["example/impl"],
            "max-concurrent-tests": 8,
            "regex-backend": "table",
        }),
    );
}

#[test]
fn experimental_package_configuration() {
    for experimental in [false, true] {
        for allowance in ["false", "true", "[]", r#"["example/experimental"]"#] {
            let source = format!(
                "experimental = {experimental}\noptions(\"allow-experimental-packages\": {allowance})"
            );
            let json = serde_json_lenient::from_str(&format!(
                r#"{{"experimental": {experimental}, "allow-experimental-packages": {allowance}}}"#
            ))
            .unwrap();
            assert_pkg_dsl_matches_json(&source, json);
        }
    }
}

#[test]
fn experimental_package_configuration_rejects_invalid_values() {
    for (key, value) in [
        ("experimental", r#""true""#),
        ("experimental", "[]"),
        ("experimental", "1"),
        ("allow-experimental-packages", r#""example/experimental""#),
        ("allow-experimental-packages", "[true]"),
        ("allow-experimental-packages", "{}"),
        ("allow-experimental-packages", "1"),
    ] {
        let source = if key == "experimental" {
            format!("experimental = {value}")
        } else {
            format!("options(\"{key}\": {value})")
        };
        assert!(
            convert_test_pkg_dsl(moon_pkg::parse(&source).unwrap(), false).is_err(),
            "{source}"
        );
        assert!(
            serde_json_lenient::from_str::<MoonPkgJSON>(&format!(r#"{{"{key}": {value}}}"#))
                .is_err(),
            "{key}: {value}"
        );
    }
}

#[test]
fn convert_pkg_dsl_preserves_option_aliases_and_validation() {
    for (key, value) in [
        ("allow_experimental_packages", "true"),
        ("is_main", "true"),
        ("warn_list", r#""+1""#),
        ("proof_enabled", "true"),
        (
            "pre_build",
            r#"[{"input": "a", "output": "b", "command": "generate"}]"#,
        ),
        ("native_stub", r#"["stub.c"]"#),
        ("virtual_pkg", r#"{"has-default": true}"#),
        ("max_concurrent_tests", "4294967295"),
        ("regex_backend", r#""runtime""#),
    ] {
        let alias = if key == "virtual_pkg" {
            "virtual".to_string()
        } else {
            key.replace('_', "-")
        };
        for spelling in [key, &alias] {
            let source = format!(r#"options("{spelling}": {value})"#);
            let json = serde_json_lenient::from_str(&format!(r#"{{"{key}": {value}}}"#)).unwrap();
            assert_pkg_dsl_matches_json(&source, json);

            let source = format!(r#"options("{spelling}": {{}})"#);
            let json =
                serde_json_lenient::from_str::<MoonPkgJSON>(&format!(r#"{{"{key}": {{}}}}"#));
            assert!(json.is_err(), "{key}");
            assert!(
                convert_test_pkg_dsl(moon_pkg::parse(&source).unwrap(), false).is_err(),
                "{source}"
            );
        }
        let source = format!(r#"options("{key}": {value}, "{alias}": {value})"#);
        let error = convert_test_pkg_dsl(moon_pkg::parse(&source).unwrap(), false).unwrap_err();
        assert_eq!(error.to_string(), format!("duplicate field `{key}`"));
    }
}

#[test]
fn convert_pkg_dsl_rejects_duplicate_options() {
    for (declaration, option, field) in [
        (
            r#"pkgtype(kind: "library")"#,
            r#"pkgtype: { "kind": "library" }"#,
            "pkgtype",
        ),
        (
            r#"formatter(ignore: ["a.mbt"])"#,
            r#"formatter: { "ignore": [] }"#,
            "formatter",
        ),
        (r#"rule(name: "a", command: "echo a")"#, "rule: []", "rule"),
        ("experimental = false", "experimental: true", "experimental"),
        (r#"warnings = "+1""#, r#"warnings: "+1""#, "warnings"),
        (r#"warnings = """#, r#"warn_list: "+1""#, "warn_list"),
        (r#"warnings = "+1""#, r#""warn-list": "-2""#, "warn_list"),
        (
            r#"supported_targets = "js""#,
            r#"supported_targets: "js""#,
            "supported_targets",
        ),
        (
            r#"supported_targets = "js""#,
            r#""supported-targets": "+native""#,
            "supported_targets",
        ),
        (
            "proof_enabled = true",
            r#""proof-enabled": false"#,
            "proof_enabled",
        ),
        (
            "virtual(has_default: true)",
            r#"virtual: { "has-default": false }"#,
            "virtual",
        ),
        (
            "virtual(has_default: true)",
            r#"virtual_pkg: { "has-default": true }"#,
            "virtual_pkg",
        ),
        (
            r#"virtual(implement: "example/lib")"#,
            r#"implement: "example/other""#,
            "implement",
        ),
    ] {
        for source in [
            format!("{declaration}\noptions({option})"),
            format!("options({option})\n{declaration}"),
        ] {
            let error = convert_test_pkg_dsl(moon_pkg::parse(&source).unwrap(), false).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("duplicate field `{field}`"),
                "{source}"
            );
        }
    }
}

#[test]
fn convert_pkg_dsl_accepts_legacy_options() {
    for (source, json) in [
        (
            r#"options(link: true)"#,
            serde_json_lenient::json!({"link": true}),
        ),
        (
            r#"supported_targets = ["wasm", "js"]"#,
            serde_json_lenient::json!({"supported-targets": ["wasm", "js"]}),
        ),
    ] {
        assert_pkg_dsl_matches_json(source, json);
    }
}

#[test]
fn ignore_unknown_package_fields() {
    for key in ["sub-package", "sub_package", "unknown-option"] {
        let json = format!(r#"{{"is-main": true, "{key}": {{"files": []}}}}"#);
        let dsl = moon_pkg::parse(&format!(
            r#"options("is-main": true, "{key}": {{"files": []}})"#
        ))
        .unwrap();
        for (pkg, _) in [
            convert_test_pkg_json(serde_json_lenient::from_str(&json).unwrap(), false).unwrap(),
            convert_test_pkg_dsl(dsl, false).unwrap(),
        ] {
            assert!(pkg.is_main);
            assert!(pkg.sub_package.is_none());
        }
    }
}

#[test]
fn ignore_unknown_import_fields() {
    for import_key in ["import", "wbtest-import", "test-import"] {
        for key in ["sub-package", "sub_package", "unknown-option"] {
            for value in ["true", "false"] {
                let imports = format!(r#"[{{"path": "example/dep", "{key}": {value}}}]"#);
                let json = format!(r#"{{"{import_key}": {imports}}}"#);
                let dsl =
                    moon_pkg::parse(&format!(r#"options("{import_key}": {imports})"#)).unwrap();
                for (pkg, _) in [
                    convert_test_pkg_json(serde_json_lenient::from_str(&json).unwrap(), false)
                        .unwrap(),
                    convert_test_pkg_dsl(dsl, false).unwrap(),
                ] {
                    let imports = pkg
                        .imports
                        .into_iter()
                        .chain(pkg.wbtest_imports)
                        .chain(pkg.test_imports)
                        .collect::<Vec<_>>();
                    assert!(
                        matches!(imports.as_slice(), [Import::Simple(path)] if path == "example/dep")
                    );
                }
            }
        }
    }
}

#[test]
fn convert_pkg_imports_normalize_aliases() {
    for source in [
        r#"["example/lib"]"#,
        r#"{"example/lib": null}"#,
        r#"{"example/lib": ""}"#,
        r#"[{"path": "example/lib"}]"#,
        r#"[{"path": "example/lib", "alias": ""}]"#,
        r#"[{"path": "example/lib", "sub-package": false}]"#,
        r#"[{"path": "example/lib", "alias": "", "sub-package": false}]"#,
    ] {
        let imports =
            pkg_json_imports_to_imports(Some(serde_json_lenient::from_str(source).unwrap()));
        assert!(matches!(&imports[..], [Import::Simple(path)] if path == "example/lib"));
    }
}

#[test]
fn convert_pkg_imports_preserve_aliases() {
    let dsl =
        crate::moon_pkg::parse(r#"import { "example/lib/default", "example/lib/named" @named }"#)
            .unwrap();
    let (dsl_pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
    for imports in [
        r#"["example/lib/default", {"path": "example/lib/named", "alias": "named"}]"#,
        r#"{"example/lib/default": null, "example/lib/named": "named"}"#,
    ] {
        let json = serde_json_lenient::from_str(&format!(r#"{{"import": {imports}}}"#)).unwrap();
        let (json_pkg, _) = convert_test_pkg_json(json, false).unwrap();
        assert_eq!(
            serde_json_lenient::to_value(json_pkg.imports).unwrap(),
            serde_json_lenient::to_value(&dsl_pkg.imports).unwrap(),
        );
    }
}

#[test]
fn convert_pkg_imports_preserve_import_all() {
    for (alias_syntax, alias) in [("", None), ("@named", Some("named"))] {
        let dsl =
            crate::moon_pkg::parse(&format!(r#"import {{ "example/lib" {alias_syntax} * }}"#,))
                .unwrap();
        let (dsl_pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
        assert!(matches!(
            &dsl_pkg.imports[..],
            [Import::Alias { alias: actual_alias, import_all: true, .. }]
                if actual_alias.as_deref() == alias
        ));
        let json = serde_json_lenient::from_value(serde_json_lenient::json!({
            "import": [{"path": "example/lib", "alias": alias, "import-all": true}],
        }))
        .unwrap();
        let (json_pkg, _) = convert_test_pkg_json(json, false).unwrap();
        assert_eq!(
            serde_json_lenient::to_value(json_pkg.imports).unwrap(),
            serde_json_lenient::to_value(dsl_pkg.imports).unwrap(),
        );
    }

    for source in [
        r#"{"example/lib": "*"}"#,
        r#"[{"path": "example/lib", "alias": "*"}]"#,
    ] {
        let imports =
            pkg_json_imports_to_imports(Some(serde_json_lenient::from_str(source).unwrap()));
        assert!(matches!(
            &imports[..],
            [Import::Alias { alias: Some(alias), import_all: false, .. }] if alias == "*"
        ));
    }
}

#[test]
fn convert_pkg_dsl_combines_repeated_import_blocks() {
    for (suffix, key) in [
        ("", "import"),
        ("for \"test\"", "test-import"),
        ("for \"wbtest\"", "wbtest-import"),
    ] {
        for first in ["", r#""example/lib/first""#] {
            let dsl = crate::moon_pkg::parse(&format!(
                r#"
import {{ {first} }} {suffix}
import {{ "example/lib/second" }} {suffix}
"#,
            ))
            .unwrap();
            let (pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
            let imports = match key {
                "import" => pkg.imports,
                "test-import" => pkg.test_imports,
                _ => pkg.wbtest_imports,
            };
            assert_eq!(
                imports.iter().map(Import::get_path).collect::<Vec<_>>(),
                if first.is_empty() {
                    vec!["example/lib/second"]
                } else {
                    vec!["example/lib/first", "example/lib/second"]
                },
            );
        }
    }
}

#[test]
fn convert_pkg_dsl_rejects_duplicate_import_option_spellings() {
    for kind in ["test", "wbtest"] {
        let dsl = crate::moon_pkg::parse(&format!(
            r#"options("{kind}-import": [], {kind}_import: [])"#,
        ))
        .unwrap();
        let error = convert_test_pkg_dsl(dsl, false).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("duplicate field `{kind}-import`"),
        );
    }
}

#[test]
fn convert_pkg_imports_preserve_repeated_packages() {
    for (suffix, key) in [
        ("", "import"),
        ("for \"test\"", "test-import"),
        ("for \"wbtest\"", "wbtest-import"),
    ] {
        let json = serde_json_lenient::from_value(serde_json_lenient::json!({
            key: ["example/lib", {"path": "example/lib", "alias": "other"}],
        }))
        .unwrap();
        let (json_pkg, _) = convert_test_pkg_json(json, false).unwrap();
        let dsl = crate::moon_pkg::parse(&format!(
            r#"import {{ "example/lib", "example/lib" @other }} {suffix}"#,
        ))
        .unwrap();
        let (dsl_pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
        for pkg in [json_pkg, dsl_pkg] {
            let imports = match key {
                "import" => pkg.imports,
                "test-import" => pkg.test_imports,
                _ => pkg.wbtest_imports,
            };
            assert_eq!(
                imports.iter().map(Import::get_path).collect::<Vec<_>>(),
                ["example/lib", "example/lib"],
            );
        }
    }
}

#[test]
fn convert_pkg_json_imports_are_unconditional() {
    let json = serde_json_lenient::from_str(
        r#"{
          "import": [{
            "path": "example/lib/platform",
            "alias": "platform",
            "targets": ["native"]
          }]
        }"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_json(json, false).unwrap();
    // Unknown JSON fields retain their existing behavior: they are ignored.
    for &backend in TargetBackend::all() {
        assert!(pkg.imports[0].supports_backend(backend));
    }
}

#[test]
fn convert_pkg_dsl_rejects_import_blocks_with_legacy_options() {
    for (source, field) in [
        (r#"import {} options("import": [])"#, "import"),
        (
            r#"import { "example/lib", "example/lib" } options("import": ["example/other"])"#,
            "import",
        ),
        (
            r#"import {} for "test" options(test_import: [])"#,
            "test-import",
        ),
        (
            r#"import { "example/lib" } for "test" options("test-import": [])"#,
            "test-import",
        ),
        (
            r#"import {} for "wbtest" options(wbtest_import: [])"#,
            "wbtest-import",
        ),
        (
            r#"import { "example/lib" } for "wbtest" options("wbtest-import": [])"#,
            "wbtest-import",
        ),
    ] {
        let (log, capture) = UserLog::captured(log::LevelFilter::Warn);
        let error = parse_package(source, Path::new("moon.pkg"), true, &log).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("duplicate field `{field}`"),
            "{source}"
        );
        // Reject ambiguous declarations before producing duplicate-item warnings.
        assert!(capture.take().is_empty());
    }
}

#[test]
fn convert_pkg_dsl_options_preserve_import_aliases() {
    let dsl = crate::moon_pkg::parse(
        r#"options(
  test_import: ["example/lib/test"],
  wbtest_import: ["example/lib/wbtest"],
)"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
    assert_eq!(pkg.test_imports.len(), 1);
    assert_eq!(pkg.test_imports[0].get_path(), "example/lib/test");
    assert_eq!(pkg.wbtest_imports.len(), 1);
    assert_eq!(pkg.wbtest_imports[0].get_path(), "example/lib/wbtest");
}

#[test]
fn convert_pkg_dsl_conditional_imports() {
    let dsl = crate::moon_pkg::parse(
        r#"
import { "example/lib/common" }
#cfg(target = "native")
import { "example/lib/native" @platform }
#cfg(not(target = "native"))
import { "example/lib/portable" @platform }
#cfg(any(target = "native", target = "js"))
import { "example/lib/test" } for "test"
#cfg(all(target = "native", true))
import { "example/lib/wbtest" } for "wbtest"
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
    assert_eq!(
        pkg.imports.iter().map(Import::get_path).collect::<Vec<_>>(),
        [
            "example/lib/common",
            "example/lib/native",
            "example/lib/portable"
        ]
    );
    assert_eq!(pkg.test_imports[0].get_path(), "example/lib/test");
    assert_eq!(pkg.wbtest_imports[0].get_path(), "example/lib/wbtest");
    for &backend in TargetBackend::all() {
        assert!(pkg.imports[0].supports_backend(backend));
        assert_eq!(
            pkg.imports[1].supports_backend(backend),
            matches!(backend, Native | LLVM)
        );
        assert_eq!(
            pkg.imports[2].supports_backend(backend),
            !matches!(backend, Native | LLVM)
        );
        assert_eq!(
            pkg.test_imports[0].supports_backend(backend),
            matches!(backend, Native | LLVM | Js)
        );
        assert_eq!(
            pkg.wbtest_imports[0].supports_backend(backend),
            matches!(backend, Native | LLVM)
        );
    }
}

#[test]
fn convert_pkg_imports_warns_and_preserves_duplicate_items() {
    for key in ["import", "test-import", "wbtest-import"] {
        for alias in ["", "@named", "*", "@named *"] {
            for emit_warnings in [false, true] {
                let dsl = moon_pkg::parse(&format!(
                    "import {{ \"example/lib\" {alias}, \"example/lib\" {alias} }} {}",
                    match key {
                        "test-import" => "for \"test\"",
                        "wbtest-import" => "for \"wbtest\"",
                        _ => "",
                    },
                ))
                .unwrap();
                let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
                let (pkg, _) = convert_pkg_dsl_to_package_with_supported_targets_decl(
                    dsl,
                    Path::new("moon.pkg"),
                    emit_warnings,
                    &user_log,
                )
                .unwrap();
                let imports = match key {
                    "import" => pkg.imports,
                    "test-import" => pkg.test_imports,
                    _ => pkg.wbtest_imports,
                };
                assert_eq!(imports.len(), 2);
                let warnings = capture.take();
                assert_eq!(warnings.len(), usize::from(emit_warnings));
                if emit_warnings {
                    assert_eq!(
                        warnings[0].message,
                        format!("Duplicate import of package `example/lib` in `{key}`.",)
                    );
                }
            }
        }
        let json = serde_json_lenient::from_value(serde_json_lenient::json!({
            key: ["example/lib", {"path": "example/lib", "alias": ""}, {"path": "example/lib", "alias": "other"}],
        })).unwrap();
        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
        let (pkg, _) =
            convert_pkg_json_to_package_with_supported_targets_decl(json, true, &user_log).unwrap();
        let imports = match key {
            "import" => pkg.imports,
            "test-import" => pkg.test_imports,
            _ => pkg.wbtest_imports,
        };
        assert_eq!(imports.len(), 3);
        assert!(matches!(&imports[0], Import::Simple(_)));
        assert!(
            matches!(&imports[2], Import::Alias { alias: Some(alias), .. } if alias == "other")
        );
        assert_eq!(capture.take().len(), 1);
    }
}

#[test]
fn convert_pkg_dsl_import_options_allow_other_import_kinds() {
    assert_pkg_dsl_matches_json(
        r#"import { "example/lib" }
options(test_import: ["example/test"], wbtest_import: ["example/wbtest"])"#,
        serde_json_lenient::json!({
            "import": ["example/lib"],
            "test-import": ["example/test"],
            "wbtest-import": ["example/wbtest"],
        }),
    );
}

#[test]
fn convert_pkg_imports_preserves_alias_precedence() {
    let dsl = moon_pkg::parse(
        r#"import { "example/lib" @a, "example/lib" @b }
import { "example/lib" @a }"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
    let aliases = pkg
        .imports
        .iter()
        .map(|import| match import {
            Import::Alias {
                alias: Some(alias), ..
            } => alias.as_str(),
            _ => panic!("explicit alias was lost"),
        })
        .collect::<Vec<_>>();
    assert_eq!(aliases, ["a", "b", "a"]);
}

#[test]
fn convert_pkg_dsl_duplicate_warnings_respect_import_conditions() {
    for (suffix, key) in [
        ("", "import"),
        ("for \"test\"", "test-import"),
        ("for \"wbtest\"", "wbtest-import"),
    ] {
        for (first, second, overlaps) in [
            (r#"target = "native""#, r#"not(target = "native")"#, false),
            (r#"target = "native""#, r#"target = "llvm""#, true),
            (r#"not(target = "native")"#, r#"target = "llvm""#, false),
            (r#"target = "native""#, r#"not(target = "llvm")"#, true),
            ("false", "true", false),
            ("any()", "all()", false),
            ("true", r#"target = "native""#, true),
            (
                r#"any(target = "native", target = "js")"#,
                r#"target = "native""#,
                true,
            ),
            (r#"target = "native""#, r#"target = "native""#, true),
        ] {
            for alias in ["", "@named", "*", "@named *"] {
                for emit_warnings in [false, true] {
                    let dsl = moon_pkg::parse(&format!(
                        r#"
#cfg({first})
import {{ "example/lib" {alias} }} {suffix}
#cfg({second})
import {{ "example/lib" {alias} }} {suffix}
"#,
                    ))
                    .unwrap();
                    let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
                    let (pkg, _) = convert_pkg_dsl_to_package_with_supported_targets_decl(
                        dsl,
                        Path::new("moon.pkg"),
                        emit_warnings,
                        &user_log,
                    )
                    .unwrap();
                    let imports = match key {
                        "import" => pkg.imports,
                        "test-import" => pkg.test_imports,
                        _ => pkg.wbtest_imports,
                    };
                    assert_eq!(imports.len(), 2);
                    let warnings = capture.take();
                    assert_eq!(
                        warnings.len(),
                        usize::from(overlaps && emit_warnings),
                        "{first}, {second}, {alias}, {key}"
                    );
                    if overlaps && emit_warnings {
                        assert_eq!(
                            warnings[0].message,
                            format!("Duplicate import of package `example/lib` in `{key}`.")
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn convert_pkg_dsl_cfg_boolean_and_stacked_conditions() {
    let dsl = crate::moon_pkg::parse(
        r#"
#cfg(all())
import { "example/lib/all" }
#cfg(any())
import { "example/lib/none" }
#cfg(false)
import { "example/lib/false" }
#cfg(any(target = "native", target = "js"))
#cfg(not(target = "js"))
import "test" { "example/lib/native" * }
import { "example/lib/unconditional" }
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(dsl, false).unwrap();
    for &backend in TargetBackend::all() {
        assert!(pkg.imports[0].supports_backend(backend));
        assert!(!pkg.imports[1].supports_backend(backend));
        assert!(!pkg.imports[2].supports_backend(backend));
        assert!(pkg.imports[3].supports_backend(backend));
        assert_eq!(
            pkg.test_imports[0].supports_backend(backend),
            matches!(backend, Native | LLVM)
        );
    }
    assert!(matches!(
        &pkg.test_imports[0],
        Import::Alias {
            import_all: true,
            ..
        }
    ));
}

#[test]
fn convert_pkg_dsl_supports_u32_values() {
    let json = crate::moon_pkg::parse(
        r#"
options(
  link: { "wasm": { "heap-start-address": 2214592512 } },
)
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(json, false).unwrap();

    assert_eq!(
        pkg.link.unwrap().wasm.unwrap().heap_start_address,
        Some(2_214_592_512)
    );
}

#[test]
fn convert_pkg_dsl_supports_leading_zero_integers() {
    let json = crate::moon_pkg::parse(r#"options("max-concurrent-tests": 08)"#).unwrap();
    let (pkg, _) = convert_test_pkg_dsl(json, false).unwrap();

    assert_eq!(pkg.max_concurrent_tests, Some(8));
}

#[test]
fn convert_pkg_dsl_rejects_values_above_u32() {
    let json = crate::moon_pkg::parse(
        r#"
options(
  link: { "wasm": { "heap-start-address": 4294967297 } },
)
"#,
    )
    .unwrap();
    assert!(convert_test_pkg_dsl(json, false).is_err());
}

#[test]
fn convert_pkg_dsl_supports_supported_targets_shorthand() {
    let json = crate::moon_pkg::parse(r#"supported_targets = "js""#).unwrap();
    let (pkg, decl_kind) = convert_test_pkg_dsl(json, true).unwrap();

    assert_eq!(
        pkg.supported_targets.iter().copied().collect::<Vec<_>>(),
        vec![Js]
    );
    assert_eq!(decl_kind, SupportedTargetsDeclKind::Expr);
}

#[test]
fn convert_pkgtype_derives_is_main_and_force_link() {
    let cases = [
        ("library", false, false),
        ("executable", true, false),
        ("foreign_library", false, true),
    ];
    for (kind, want_is_main, want_force_link) in cases {
        let src = format!(r#"pkgtype(kind: "{kind}")"#);
        let json = crate::moon_pkg::parse(&src).unwrap();
        let (pkg, _) = convert_test_pkg_dsl(json, false).unwrap();
        assert_eq!(pkg.is_main, want_is_main, "is_main for kind {kind}");
        assert_eq!(
            pkg.force_link, want_force_link,
            "force_link for kind {kind}"
        );
    }
}

#[test]
fn convert_pkgtype_defaults_to_library_when_absent() {
    let json = crate::moon_pkg::parse("").unwrap();
    let (pkg, _) = convert_test_pkg_dsl(json, false).unwrap();
    assert!(!pkg.is_main);
    assert!(!pkg.force_link);
}

#[test]
fn convert_pkgtype_conflicts_with_is_main() {
    let json = crate::moon_pkg::parse(
        r#"
pkgtype(kind: "library")
options("is-main": true)
"#,
    )
    .unwrap();
    let err = convert_test_pkg_dsl(json, false).unwrap_err();
    assert!(
        err.to_string().contains("conflicts with `is-main"),
        "unexpected error: {err}"
    );
}

#[test]
fn convert_pkgtype_conflicts_with_link_bool() {
    let json = crate::moon_pkg::parse(
        r#"
pkgtype(kind: "library")
options("link": true)
"#,
    )
    .unwrap();
    let err = convert_test_pkg_dsl(json, false).unwrap_err();
    assert!(
        err.to_string().contains("conflicts with `link"),
        "unexpected error: {err}"
    );
}

#[test]
fn convert_pkgtype_accepts_redundant_consistent_flags() {
    let json = crate::moon_pkg::parse(
        r#"
pkgtype(kind: "executable")
options("is-main": true)
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(json, false).unwrap();
    assert!(pkg.is_main);
    assert!(!pkg.force_link);
}

#[test]
fn convert_pkgtype_foreign_library_allows_structured_link() {
    // A structured `link: { ... }` is pure config, not a force-link signal,
    // so it must not conflict with `pkgtype`.
    let json = crate::moon_pkg::parse(
        r#"
pkgtype(kind: "foreign_library")
options("link": { "js": { "format": "esm" } })
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(json, false).unwrap();
    assert!(!pkg.is_main);
    assert!(pkg.force_link);
    assert!(pkg.link.is_some());
}

#[test]
fn convert_pkg_dsl_supports_proof_enabled() {
    let json = crate::moon_pkg::parse(
        r#"
options(
  "proof-enabled": true,
)
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_dsl(json, true).unwrap();

    assert!(pkg.proof_enabled);
}

#[test]
fn convert_pkg_dsl_rejects_removed_use_async_io() {
    let err = convert_test_pkg_dsl(
        crate::moon_pkg::parse("use_async_io = true").unwrap(),
        false,
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Unexpected key 'use_async_io' found in moon.pkg."
    );
}

#[test]
fn convert_pkg_dsl_supports_dev_build_rule() {
    let json = crate::moon_pkg::parse(
        r#"
dev_build(rule: "rule1", input: "abc", output: "def")
"#,
    )
    .unwrap();

    let (pkg, _) = convert_test_pkg_dsl(json, true).unwrap();
    let pre_build = pkg.pre_build.unwrap();
    let MoonPkgGenerate::Rule {
        rule,
        input,
        output,
        ..
    } = &pre_build[0]
    else {
        panic!("expected rule pre-build");
    };
    assert_eq!(rule, "rule1");
    let StringOrArray::String(input) = input else {
        panic!("expected string input");
    };
    assert_eq!(input, "abc");
    let StringOrArray::String(output) = output else {
        panic!("expected string output");
    };
    assert_eq!(output, "def");
}

#[test]
fn convert_pkg_dsl_supports_package_local_rules() {
    let json = crate::moon_pkg::parse(
        r#"
rule(name: "rule1", command: "exe1 $input -o $output")
dev_build(rule: "rule1", input: "abc", output: "def")
"#,
    )
    .unwrap();

    let (pkg, _) = convert_test_pkg_dsl(json, true).unwrap();
    let rules = pkg.local_rules.as_ref().expect("expected local rules");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].name, "rule1");
    assert_eq!(rules[0].command, "exe1 $input -o $output");
}

#[test]
fn convert_pkg_dsl_rejects_duplicate_package_local_rule_names() {
    let json = crate::moon_pkg::parse(
        r#"
rule(name: "rule1", command: "exe1")
rule(name: "rule1", command: "exe2")
"#,
    )
    .unwrap();

    let err = convert_test_pkg_dsl(json, true).unwrap_err();
    assert!(
        err.to_string()
            .contains("Duplicate rule name `rule1` found in moon.pkg.")
    );
}

#[test]
fn convert_pkg_dsl_rejects_mixed_dev_build_and_pre_build() {
    for (option, field) in [
        ("dev_build", "dev_build"),
        ("pre-build", "pre_build"),
        ("pre_build", "pre_build"),
    ] {
        let source = format!(
            r#"
dev_build(rule: "rule1", input: "abc", output: "def")
options("{option}": [])
"#
        );
        let err = convert_test_pkg_dsl(moon_pkg::parse(&source).unwrap(), true).unwrap_err();
        assert_eq!(err.to_string(), format!("duplicate field `{field}`"));
    }
}

#[test]
fn convert_pkg_json_supports_proof_enabled_hyphenated() {
    let json: MoonPkgJSON = serde_json_lenient::from_str(
        r#"
{
  "proof-enabled": true
}
"#,
    )
    .unwrap();
    let (pkg, _) = convert_test_pkg_json(json, true).unwrap();

    assert!(pkg.proof_enabled);
}
