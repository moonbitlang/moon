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

use anyhow::Context;
use indexmap::IndexMap;
use serde::de::DeserializeOwned;

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

/// Parse YAML policy embedded in the leading line comments of an `.mbtx` file.
///
/// Unlike Markdown front matter, the script form has no delimiter. It starts
/// with `// policy:` and continues through the following indented `//` lines.
/// A non-comment line, a MoonBit doc comment, or another unindented comment
/// ends the front matter.
pub fn parse_mbtx_policy<T: DeserializeOwned>(
    single_file_path: &Path,
) -> anyhow::Result<Option<T>> {
    let content = std::fs::read_to_string(single_file_path)
        .with_context(|| format!("failed to read .mbtx file `{}`", single_file_path.display()))?;
    let Some(yaml) = extract_mbtx_front_matter(&content) else {
        return Ok(None);
    };

    #[derive(serde::Deserialize)]
    struct MbtxFrontMatter<T> {
        policy: T,
    }

    let front_matter: MbtxFrontMatter<T> = serde_yaml::from_str(&yaml).with_context(|| {
        format!(
            "failed to parse YAML front matter in .mbtx file `{}`",
            single_file_path.display()
        )
    })?;
    Ok(Some(front_matter.policy))
}

fn extract_mbtx_front_matter(content: &str) -> Option<String> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = content.lines().skip_while(|line| line.trim().is_empty());
    let first = mbtx_comment_content(lines.next()?)?;
    if !first.trim().starts_with("policy:") {
        return None;
    }

    let mut yaml = String::from(first);
    yaml.push('\n');
    for line in lines {
        let Some(comment) = mbtx_comment_content(line) else {
            break;
        };
        if !comment.is_empty() && !comment.starts_with([' ', '\t']) {
            break;
        }
        yaml.push_str(comment);
        yaml.push('\n');
    }
    Some(yaml)
}

fn mbtx_comment_content(line: &str) -> Option<&str> {
    let comment = line.trim_start().strip_prefix("//")?;
    if !comment.is_empty() && !comment.starts_with([' ', '\t']) {
        return None;
    }
    Some(comment.strip_prefix(' ').unwrap_or(comment))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::Deserialize;

    use super::extract_mbtx_front_matter;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Policy {
        env: Env,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Env {
        set: BTreeMap<String, String>,
    }

    #[derive(Deserialize)]
    struct FrontMatter {
        policy: Policy,
    }

    fn parse(source: &str) -> Option<Policy> {
        let yaml = extract_mbtx_front_matter(source)?;
        Some(serde_yaml::from_str::<FrontMatter>(&yaml).unwrap().policy)
    }

    #[test]
    fn extracts_yaml_from_leading_line_comments() {
        let source = r#"// policy:
//   env:
//     set:
//       MODE: embedded

///|
fn main {}
"#;

        assert_eq!(
            parse(source),
            Some(Policy {
                env: Env {
                    set: BTreeMap::from([("MODE".into(), "embedded".into())]),
                },
            })
        );
    }

    #[test]
    fn stops_before_moonbit_and_ordinary_comments() {
        assert_eq!(
            extract_mbtx_front_matter("// policy: {}\n///|\nfn main {}\n").as_deref(),
            Some("policy: {}\n")
        );
        assert_eq!(
            extract_mbtx_front_matter("// policy: {}\n// explanation\nfn main {}\n").as_deref(),
            Some("policy: {}\n")
        );
    }

    #[test]
    fn ignores_ordinary_leading_comments() {
        assert_eq!(
            extract_mbtx_front_matter("// An ordinary comment.\nfn main {}\n"),
            None
        );
    }

    #[test]
    fn supports_crlf_and_a_utf8_bom() {
        assert_eq!(
            extract_mbtx_front_matter("\u{feff}// policy: {}\r\nfn main {}\r\n").as_deref(),
            Some("policy: {}\n")
        );
    }
}
