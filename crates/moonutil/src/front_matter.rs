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

use std::path::Path;

use indexmap::IndexMap;

use crate::constants::DOT_MBT_DOT_MD;

#[derive(Debug, serde::Deserialize)]
pub struct MbtMdHeader {
    pub moonbit: Option<MbtMdSection>,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct MbtMdSection {
    pub deps: Option<IndexMap<String, crate::dependency::SourceDependencyInfo>>,
    pub import: Option<crate::package::PkgJSONImport>,
    pub backend: Option<String>,
}

pub fn parse_front_matter_config(single_file_path: &Path) -> anyhow::Result<Option<MbtMdHeader>> {
    let single_file_string = single_file_path.display().to_string();
    let front_matter_config: Option<MbtMdHeader> = if single_file_string.ends_with(DOT_MBT_DOT_MD) {
        let content = std::fs::read_to_string(single_file_path)?;
        let pattern = regex::Regex::new(r"(?s)^---\s*\n((?:[^\n]+\n)*?)---\s*\n")?;
        if let Some(cap) = pattern.captures(&content) {
            let yaml_content = cap.get(1).unwrap().as_str();
            let config: MbtMdHeader = serde_yaml::from_str(yaml_content).map_err(|e| {
                anyhow::anyhow!("Failed to parse front matter in markdown file: {}", e)
            })?;

            Some(config)
        } else {
            None
        }
    } else {
        None
    };
    Ok(front_matter_config)
}
