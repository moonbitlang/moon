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
    path::{Path, PathBuf},
};

use anyhow::Context;
use serde::{Deserialize, Deserializer};

use crate::common::{MOON_WORK, TargetBackend};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MoonWork {
    #[serde(rename = "use")]
    pub use_paths: Vec<PathBuf>,
    #[serde(
        rename = "preferred-target",
        default,
        deserialize_with = "deserialize_preferred_target"
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
    let parsed: MoonWork = serde_json_lenient::from_str(content)?;
    if parsed.use_paths.is_empty() {
        anyhow::bail!("workspace file must list at least one path in `use`");
    }
    Ok(parsed)
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
}
