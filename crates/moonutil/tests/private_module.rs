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
    convert_module_to_mod_json, read_module_desc_file_in_dir,
    read_module_desc_file_in_dir_with_legacy, write_module_dsl_to_file,
};
use moonutil::moon_mod_patch::{MoonModPatch, patch_module_dsl_to_file};
use serde_json_lenient::{Value, json};

#[test]
fn private_is_optional_and_survives_manifest_roundtrips() {
    for private in [None, Some(true), Some(false)] {
        for file in ["moon.mod", "moon.mod.json"] {
            let dir = tempfile::tempdir().unwrap();
            let mut json = json!({ "name": "test/internal" });
            let mut dsl = "name = \"test/internal\"\n".to_owned();
            if let Some(private) = private {
                json["private"] = private.into();
                dsl.push_str(&format!("private = {private}\n"));
            }
            let contents = if file == "moon.mod" {
                dsl
            } else {
                json.to_string()
            };
            std::fs::write(dir.path().join(file), contents).unwrap();
            let module = read_module_desc_file_in_dir_with_legacy(dir.path(), true).unwrap();
            assert_eq!(module.private, private);
            let json = serde_json_lenient::to_value(convert_module_to_mod_json(module)).unwrap();
            assert_eq!(json.get("private"), private.map(Value::Bool).as_ref());
            std::fs::remove_file(dir.path().join(file)).unwrap();
            std::fs::write(dir.path().join("moon.mod.json"), json.to_string()).unwrap();
            let module = read_module_desc_file_in_dir_with_legacy(dir.path(), true).unwrap();
            let json = serde_json_lenient::to_value(convert_module_to_mod_json(module)).unwrap();
            assert_eq!(json.get("private"), private.map(Value::Bool).as_ref());
        }
    }
}

#[test]
fn private_rejects_non_boolean_values_in_both_manifest_formats() {
    for value in ["null", "\"true\"", "1", "[]", "{}"] {
        for (file, contents) in [
            (
                "moon.mod",
                format!("name = \"test/internal\"\nprivate = {value}\n"),
            ),
            (
                "moon.mod",
                format!("name = \"test/internal\"\noptions(private: {value})\n"),
            ),
            (
                "moon.mod.json",
                format!(r#"{{"name":"test/internal","private":{value}}}"#),
            ),
        ] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join(file), contents).unwrap();
            assert!(
                read_module_desc_file_in_dir_with_legacy(dir.path(), true).is_err(),
                "{file}: {value}"
            );
        }
    }
}

#[test]
fn private_rejects_duplicate_declarations() {
    for contents in [
        "name = \"test/internal\"\nprivate = true\nprivate = false\n",
        "name = \"test/internal\"\nprivate = true\noptions(private: false)\n",
        "name = \"test/internal\"\noptions(private: true)\nprivate = false\n",
        "name = \"test/internal\"\noptions(private: true, private: false)\n",
        "name = \"test/internal\"\noptions = { \"private\": true, \"private\": false }\n",
        r#"{"name":"test/internal","private":true,"private":false}"#,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let file = if contents.starts_with('{') {
            "moon.mod.json"
        } else {
            "moon.mod"
        };
        std::fs::write(dir.path().join(file), contents).unwrap();
        assert!(read_module_desc_file_in_dir_with_legacy(dir.path(), true).is_err());
    }
}

#[test]
fn private_survives_dependency_edits() {
    for private in [None, Some(true), Some(false)] {
        let dir = tempfile::tempdir().unwrap();
        let mut contents = "name = \"test/internal\"\n".to_owned();
        if let Some(private) = private {
            contents.push_str(&format!("// Publishing policy.\nprivate = {private}\n"));
        }
        std::fs::write(dir.path().join("moon.mod"), &contents).unwrap();
        for patch in [
            MoonModPatch::InsertImportItem {
                name: "test/dep".into(),
                version: "0.1.0".parse().unwrap(),
            },
            MoonModPatch::UpdateImportItems(indexmap::IndexMap::from([(
                "test/dep".into(),
                "0.2.0".parse().unwrap(),
            )])),
            MoonModPatch::RemoveImportItem {
                name: "test/dep".into(),
            },
        ] {
            patch_module_dsl_to_file(dir.path(), patch).unwrap();
            let actual = std::fs::read_to_string(dir.path().join("moon.mod")).unwrap();
            assert!(actual.starts_with(&contents));
            assert_eq!(
                read_module_desc_file_in_dir(dir.path()).unwrap().private,
                private
            );
        }
    }
}

#[test]
fn private_survives_json_to_dsl_migration() {
    for private in [None, Some(true), Some(false)] {
        let dir = tempfile::tempdir().unwrap();
        let mut json = json!({ "name": "test/internal" });
        if let Some(private) = private {
            json["private"] = private.into();
        }
        let module = serde_json_lenient::from_value(json).unwrap();
        write_module_dsl_to_file(&module, dir.path()).unwrap();
        assert_eq!(
            read_module_desc_file_in_dir(dir.path()).unwrap().private,
            private
        );
        if private.is_none() {
            let contents = std::fs::read_to_string(dir.path().join("moon.mod")).unwrap();
            assert!(!contents.contains("private"));
        }
    }
}
