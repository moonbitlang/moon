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
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    common::{MOON_WORK, MOON_WORK_JSON, TargetBackend},
    moon_pkg,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoonWork {
    pub use_paths: Vec<PathBuf>,
    pub preferred_target: Option<TargetBackend>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MoonWorkFile {
    #[serde(
        rename = "members",
        alias = "use",
        deserialize_with = "deserialize_use_paths"
    )]
    use_paths: Vec<PathBuf>,
    #[serde(
        alias = "preferred-target",
        default,
        deserialize_with = "deserialize_preferred_target"
    )]
    preferred_target: Option<TargetBackend>,
}

#[derive(Serialize)]
struct LegacyMoonWorkFile<'a> {
    #[serde(rename = "use", serialize_with = "serialize_use_paths")]
    use_paths: &'a [PathBuf],
    #[serde(
        rename = "preferred-target",
        serialize_with = "serialize_preferred_target",
        skip_serializing_if = "Option::is_none"
    )]
    preferred_target: &'a Option<TargetBackend>,
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

pub fn workspace_manifest_path(dir: &Path) -> Option<PathBuf> {
    let dsl = dir.join(MOON_WORK);
    if dsl.exists() {
        return Some(dsl);
    }

    let json = dir.join(MOON_WORK_JSON);
    json.exists().then_some(json)
}

pub fn read_workspace(dir: &Path) -> anyhow::Result<Option<MoonWork>> {
    let Some(path) = workspace_manifest_path(dir) else {
        return Ok(None);
    };

    read_workspace_file(&path).map(Some)
}

pub fn read_workspace_file(path: &Path) -> anyhow::Result<MoonWork> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read workspace file `{}`", path.display()))?;
    let workspace = match path.file_name().and_then(|name| name.to_str()) {
        Some(MOON_WORK) => parse_workspace_dsl(&content),
        Some(MOON_WORK_JSON) => parse_workspace_json(&content),
        _ => anyhow::bail!(
            "expected workspace file to be `{}` or `{}`, got `{}`",
            MOON_WORK,
            MOON_WORK_JSON,
            path.display()
        ),
    };
    workspace.with_context(|| format!("failed to parse workspace file `{}`", path.display()))
}

pub fn format_workspace_file(path: &Path) -> anyhow::Result<String> {
    let workspace = read_workspace_file(path)?;
    format_workspace_dsl(&workspace)
}

pub fn write_workspace(dir: &Path, work: &MoonWork) -> anyhow::Result<()> {
    let path = dir.join(MOON_WORK);
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    write_text_with_trailing_newline(&mut writer, &format_workspace_dsl(work)?)
}

pub fn write_workspace_legacy_json(dir: &Path, work: &MoonWork) -> anyhow::Result<()> {
    let path = dir.join(MOON_WORK_JSON);
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    let legacy = LegacyMoonWorkFile {
        use_paths: &work.use_paths,
        preferred_target: &work.preferred_target,
    };
    let content = serde_json_lenient::to_string_pretty(&legacy)?;
    write_text_with_trailing_newline(&mut writer, &content)
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
    let file: MoonWorkFile = serde_json_lenient::from_str(content)?;
    Ok(MoonWork {
        use_paths: file.use_paths,
        preferred_target: file.preferred_target,
    })
}

fn parse_workspace_dsl(content: &str) -> anyhow::Result<MoonWork> {
    let json = moon_pkg::parse(content)?;
    let file: MoonWorkFile = serde_json_lenient::from_value(json)?;
    Ok(MoonWork {
        use_paths: file.use_paths,
        preferred_target: file.preferred_target,
    })
}

fn format_workspace_dsl(work: &MoonWork) -> anyhow::Result<String> {
    let mut out = String::new();

    if work.use_paths.is_empty() {
        out.push_str("members = []\n");
    } else {
        out.push_str("members = [\n");
        for use_path in &work.use_paths {
            out.push_str("  ");
            out.push_str(&serde_json_lenient::to_string(&manifest_path_string(
                use_path,
            ))?);
            out.push_str(",\n");
        }
        out.push_str("]\n");
    }

    if let Some(preferred_target) = work.preferred_target {
        out.push_str("preferred_target = ");
        out.push_str(&serde_json_lenient::to_string(preferred_target.to_flag())?);
        out.push('\n');
    }

    Ok(out)
}

fn write_text_with_trailing_newline(writer: &mut impl Write, content: &str) -> anyhow::Result<()> {
    writer.write_all(content.as_bytes())?;
    if !content.ends_with('\n') {
        writer.write_all(b"\n")?;
    }
    Ok(())
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
    fn parse_workspace_dsl_with_target() {
        let parsed = parse_workspace_dsl(
            r#"
                members = ["./app", "./shared"]
                preferred_target = "wasm-gc"
            "#,
        )
        .unwrap();

        assert_eq!(
            parsed.use_paths,
            vec![PathBuf::from("./app"), PathBuf::from("./shared")]
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
    fn parse_workspace_dsl_with_empty_members() {
        let parsed = parse_workspace_dsl("members = []").unwrap();

        assert!(parsed.use_paths.is_empty());
        assert_eq!(parsed.preferred_target, None);
    }

    #[test]
    fn format_relative_workspace_paths_with_forward_slashes() {
        let workspace = MoonWork {
            use_paths: vec![PathBuf::from(".").join("app").join("main")],
            preferred_target: Some(TargetBackend::WasmGC),
        };

        let json = format_workspace_dsl(&workspace).unwrap();
        assert_eq!(
            json,
            "members = [\n  \"./app/main\",\n]\npreferred_target = \"wasm-gc\"\n"
        );
    }

    #[cfg(windows)]
    #[test]
    fn format_absolute_workspace_paths_without_normalizing_separators() {
        let workspace = MoonWork {
            use_paths: vec![PathBuf::from(r"C:\repo\app")],
            preferred_target: None,
        };

        let json = format_workspace_dsl(&workspace).unwrap();
        assert_eq!(json, "members = [\n  \"C:\\\\repo\\\\app\",\n]\n");
    }

    #[cfg(not(windows))]
    #[test]
    fn format_absolute_workspace_paths_without_normalizing_separators() {
        let workspace = MoonWork {
            use_paths: vec![PathBuf::from("/repo/app")],
            preferred_target: None,
        };

        let json = format_workspace_dsl(&workspace).unwrap();
        assert_eq!(json, "members = [\n  \"/repo/app\",\n]\n");
    }

    #[test]
    fn write_text_with_trailing_newline_appends_missing_newline() {
        let mut out = Vec::new();
        write_text_with_trailing_newline(&mut out, "members = []").unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "members = []\n");
    }
}
