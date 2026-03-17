// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::{
    collections::BTreeSet,
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::common::{MOON_WORK, TargetBackend};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct MoonWork {
    #[serde(
        rename = "use",
        deserialize_with = "deserialize_use_paths",
        serialize_with = "serialize_use_paths"
    )]
    pub use_paths: Vec<PathBuf>,
    #[serde(
        rename = "preferred-target",
        default,
        deserialize_with = "deserialize_preferred_target",
        serialize_with = "serialize_preferred_target",
        skip_serializing_if = "Option::is_none"
    )]
    pub preferred_target: Option<TargetBackend>,
}

fn deserialize_preferred_target<'de, D>(deserializer: D) -> Result<Option<TargetBackend>, D::Error>
where
    D: Deserializer<'de>,
{
    let preferred_target = Option::<String>::deserialize(deserializer)?;
    preferred_target
        .map(|target| TargetBackend::str_to_backend(&target))
        .transpose()
        .map_err(serde::de::Error::custom)
}

fn deserialize_use_paths<'de, D>(deserializer: D) -> Result<Vec<PathBuf>, D::Error>
where
    D: Deserializer<'de>,
{
    let use_paths = Vec::<String>::deserialize(deserializer)?;
    Ok(use_paths.into_iter().map(PathBuf::from).collect())
}

fn serialize_preferred_target<S>(
    preferred_target: &Option<TargetBackend>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match preferred_target {
        Some(preferred_target) => serializer.serialize_some(preferred_target.to_flag()),
        None => serializer.serialize_none(),
    }
}

fn serialize_use_paths<S>(use_paths: &[PathBuf], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let use_paths: Vec<String> = use_paths
        .iter()
        .map(|path| manifest_path_string(path))
        .collect();
    use_paths.serialize(serializer)
}

fn manifest_path_string(path: &Path) -> String {
    if path.is_absolute() {
        path.to_string_lossy().into_owned()
    } else {
        path.to_string_lossy().replace('\\', "/")
    }
}

pub fn read_workspace(dir: &Path) -> anyhow::Result<Option<MoonWork>> {
    let path = dir.join(MOON_WORK);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read workspace file `{}`", path.display()))?;
    parse_workspace_json(&content)
        .with_context(|| format!("failed to parse workspace file `{}`", path.display()))
        .map(Some)
}

pub fn write_workspace(dir: &Path, work: &MoonWork) -> anyhow::Result<()> {
    let path = dir.join(MOON_WORK);
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    serde_json_lenient::to_writer_pretty(&mut writer, work)?;
    Ok(())
}

pub fn canonical_workspace_module_dirs(
    workspace_root: &Path,
    work: &MoonWork,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut deduped = BTreeSet::new();

    for use_path in &work.use_paths {
        let path = if use_path.is_absolute() {
            use_path.clone()
        } else {
            workspace_root.join(use_path)
        };
        let path = dunce::canonicalize(&path).with_context(|| {
            format!(
                "failed to resolve workspace member `{}` from `{}`",
                use_path.display(),
                workspace_root.display()
            )
        })?;
        deduped.insert(path);
    }

    Ok(deduped.into_iter().collect())
}

fn parse_workspace_json(content: &str) -> anyhow::Result<MoonWork> {
    Ok(serde_json_lenient::from_str(content)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_workspace_json_with_target() {
        let parsed = parse_workspace_json(
            r#"
                {
                  "preferred-target": "wasm-gc",
                  "use": ["./app", "./shared lib"]
                }
            "#,
        )
        .unwrap();

        assert_eq!(
            parsed.use_paths,
            vec![PathBuf::from("./app"), PathBuf::from("./shared lib")]
        );
        assert_eq!(parsed.preferred_target, Some(TargetBackend::WasmGC));
    }

    #[test]
    fn parse_workspace_json_without_target() {
        let parsed = parse_workspace_json(
            r#"
                {
                  "use": ["./app", "./shared"]
                }
            "#,
        )
        .unwrap();

        assert_eq!(
            parsed.use_paths,
            vec![PathBuf::from("./app"), PathBuf::from("./shared")]
        );
        assert_eq!(parsed.preferred_target, None);
    }

    #[test]
    fn parse_workspace_json_with_empty_use() {
        let parsed = parse_workspace_json(
            r#"
                {
                  "use": []
                }
            "#,
        )
        .unwrap();

        assert!(parsed.use_paths.is_empty());
        assert_eq!(parsed.preferred_target, None);
    }

    #[test]
    fn serialize_relative_workspace_paths_with_forward_slashes() {
        let workspace = MoonWork {
            use_paths: vec![PathBuf::from(".").join("app").join("main")],
            preferred_target: Some(TargetBackend::WasmGC),
        };

        let json = serde_json_lenient::to_string_pretty(&workspace).unwrap();
        assert_eq!(
            json,
            "{\n  \"use\": [\n    \"./app/main\"\n  ],\n  \"preferred-target\": \"wasm-gc\"\n}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn serialize_absolute_workspace_paths_without_normalizing_separators() {
        let workspace = MoonWork {
            use_paths: vec![PathBuf::from(r"C:\repo\app")],
            preferred_target: None,
        };

        let json = serde_json_lenient::to_string_pretty(&workspace).unwrap();
        assert_eq!(json, "{\n  \"use\": [\n    \"C:\\\\repo\\\\app\"\n  ]\n}");
    }

    #[cfg(not(windows))]
    #[test]
    fn serialize_absolute_workspace_paths_without_normalizing_separators() {
        let workspace = MoonWork {
            use_paths: vec![PathBuf::from("/repo/app")],
            preferred_target: None,
        };

        let json = serde_json_lenient::to_string_pretty(&workspace).unwrap();
        assert_eq!(json, "{\n  \"use\": [\n    \"/repo/app\"\n  ]\n}");
    }
}
