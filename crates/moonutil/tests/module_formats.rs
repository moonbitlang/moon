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
    MoonMod, MoonModJSON, convert_module_to_mod_json, read_module_desc_file_in_dir_with_legacy,
    write_module_dsl_to_file, write_module_json_to_file,
};
use serde_json_lenient::json;

#[test]
fn legacy_json_and_dsl_have_the_same_normalized_representation() {
    let expected = json!({
        "name": "example/app",
        "import": {"example/dep": "1.2.3"},
        "warnings": "+a",
        "preferred_target": "js",
        "supported_targets": "js",
        "rule": [{"name": "generate", "command": "echo $input", "future": true}],
        "future": true,
        "--moonbit-unstable-prebuild": "build.js",
    });
    for (name, source) in [
        (
            "moon.mod.json",
            json!({
                "name": "example/app",
                "deps": {"example/dep": "1.2.3"},
                "warn-list": "+a",
                "preferred-target": "js",
                "supported-targets": "js",
                "rule": {"name": "generate", "command": "echo $input", "future": true},
                "future": true,
                "--moonbit-unstable-prebuild": "build.js",
            })
            .to_string(),
        ),
        (
            "moon.mod",
            r#"name = "example/app"
import { "example/dep@1.2.3" }
warnings = "+a"
preferred_target = "js"
supported_targets = "js"
rule(name: "generate", command: "echo $input", future: true)
options(future: true, "--moonbit-unstable-prebuild": "build.js")
"#
            .into(),
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(name), source).unwrap();
        let module = read_module_desc_file_in_dir_with_legacy(dir.path(), true).unwrap();
        assert_eq!(
            module.ext,
            json!({"future": true}),
            "known fields should be decoded in {name}"
        );
        let actual = serde_json_lenient::to_value(convert_module_to_mod_json(module)).unwrap();
        assert_eq!(actual, expected, "{name}");
    }
}

#[test]
fn legacy_module_json_round_trips_through_the_formatter() {
    let source = json!({
        "name": "example/app",
        "deps": {"example/dep": "1.2.3"},
        "warn-list": "+a",
        "preferred-target": "js",
        "supported-targets": "js",
        "rule": [
            {"name": "first", "command": "echo first"},
            {"name": "second", "command": "echo second"},
        ],
    });
    let manifest: MoonModJSON = serde_json_lenient::from_value(source).unwrap();
    let normalized = serde_json_lenient::to_value(&manifest).unwrap();
    let dir = tempfile::tempdir().unwrap();
    write_module_dsl_to_file(&manifest, &dir.path().join("moon.mod")).unwrap();
    let module = read_module_desc_file_in_dir_with_legacy(dir.path(), false).unwrap();
    assert_eq!(
        serde_json_lenient::to_value(convert_module_to_mod_json(module)).unwrap(),
        normalized
    );
}

#[test]
fn json_conversion_checks_module_constraints() {
    for source in [
        json!({"name": "example/app", "source": "/outside"}),
        json!({"name": "example/app", "rule": [
            {"name": "same", "command": "echo first"},
            {"name": "same", "command": "echo second"},
        ]}),
    ] {
        let manifest: MoonModJSON = serde_json_lenient::from_value(source.clone()).unwrap();
        assert!(MoonMod::try_from(manifest).is_err(), "accepted {source}");
    }
}

#[test]
fn writing_module_json_preserves_the_legacy_format() {
    let rule = json!({"name": "generate", "command": "echo $input", "future": true});
    for rules in [
        None,
        Some(rule.clone()),
        Some(json!([rule, {
            "name": "second", "command": "echo second"
        }])),
    ] {
        let mut source = json!({
            "name": "example/app",
            "deps": {"example/dep": {"path": "../dep", "version": "1.2.3"}},
            "warn-list": "+a",
            "preferred-target": "js",
            "supported-targets": "js",
            "future": true,
        });
        if let Some(rules) = rules {
            source["rule"] = rules;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("moon.mod.json");
        std::fs::write(&path, source.to_string()).unwrap();

        // The shared writer used by dependency edits must preserve the user's
        // JSON format even though the intermediate model serializes differently.
        let module = read_module_desc_file_in_dir_with_legacy(dir.path(), true).unwrap();
        write_module_json_to_file(&module.into(), dir.path()).unwrap();
        let written: serde_json_lenient::Value =
            serde_json_lenient::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(written, source);
    }
}
