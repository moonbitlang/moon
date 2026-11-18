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

use serde::{Deserialize, Deserializer};
use serde_json_lenient::Value;

use crate::{package::Link, user_log::UserLog};

/// Use the deserializer's accepted fields (including aliases) for diagnostics.
/// A compiler-owned field is known when it has a typed manifest declaration,
/// regardless of whether Moon itself lowers it to a compiler argument.
pub(super) fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    value: D,
    prefix: &str,
    unknown: &mut Vec<String>,
) -> Result<T, D::Error> {
    serde_ignored::deserialize(value, |path| {
        fn append(path: &serde_ignored::Path<'_>, output: &mut String) {
            use serde_ignored::Path;
            match path {
                Path::Root => {}
                Path::Map { parent, key } => {
                    append(parent, output);
                    if !output.is_empty() {
                        output.push('.');
                    }
                    output.push_str(key);
                }
                Path::Seq { parent, index } => {
                    append(parent, output);
                    use std::fmt::Write;
                    write!(output, "[{index}]").unwrap();
                }
                Path::Some { parent }
                | Path::NewtypeStruct { parent }
                | Path::NewtypeVariant { parent } => append(parent, output),
            }
        }
        let mut field = prefix.to_owned();
        append(&path, &mut field);
        unknown.push(field);
    })
}

pub(super) fn warn(unknown: &[String], manifest: &std::path::Path, user_log: &UserLog) {
    for field in unknown {
        user_log.warn(format!(
            "Unknown configuration key `{field}` in `{}`.",
            manifest.display()
        ));
    }
}

/// Serde buffers untagged enums before selecting a variant, so serde_ignored
/// cannot observe ignored fields inside them. Inspect those object boundaries
/// separately; the ordinary deserializer remains responsible for their types.
pub(super) fn package_untagged_fields(
    fields: &serde_json_lenient::Map<String, Value>,
    unknown: &mut Vec<String>,
) -> anyhow::Result<()> {
    if let Some(value @ Value::Object(_)) = fields.get("link") {
        let _: Link = deserialize(value, "link", unknown)?;
    }
    for key in [
        "import",
        "test-import",
        "test_import",
        "wbtest-import",
        "wbtest_import",
    ] {
        if let Some(value) = fields.get(key) {
            import_fields(value, key, unknown);
        }
    }
    for key in ["pre-build", "pre_build"] {
        if let Some(Value::Array(tasks)) = fields.get(key) {
            for (index, task) in tasks.iter().enumerate() {
                // An untagged variant can otherwise ignore a malformed field
                // belonging to the other variant and accept the whole task.
                for field in ["command", "rule"] {
                    if let Some(value) = task.get(field) {
                        String::deserialize(value)?;
                    }
                }
                object_fields(
                    task,
                    &format!("{key}[{index}]"),
                    &["input", "output", "command", "rule"],
                    unknown,
                );
            }
        }
    }
    Ok(())
}

pub(super) fn import_fields(value: &Value, key: &str, unknown: &mut Vec<String>) {
    if let Value::Array(imports) = value {
        for (index, import) in imports.iter().enumerate() {
            object_fields(
                import,
                &format!("{key}[{index}]"),
                &["path", "alias", "import-all", "import_all", "value"],
                unknown,
            );
        }
    }
}

fn object_fields(value: &Value, prefix: &str, known: &[&str], unknown: &mut Vec<String>) {
    if let Value::Object(fields) = value {
        unknown.extend(
            fields
                .keys()
                .filter(|key| !known.contains(&key.as_str()))
                .map(|key| format!("{prefix}.{key}")),
        );
    }
}
