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

use std::path::Path;

use anyhow::Context;
use indexmap::IndexMap;
use serde::de::DeserializeOwned;

use crate::{
    dependency::SourceDependencyInfo,
    package::{Import, PkgJSONImport},
    target::TargetBackend,
};

/// YAML configuration at the start of a Markdown source file. An explicitly
/// empty import list selects standalone mode; an absent import does not.
#[derive(Debug, Default)]
pub struct MbtMdFrontMatter {
    pub imports: Option<PkgJSONImport>,
    pub backend: Option<TargetBackend>,
}

impl MbtMdFrontMatter {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read Markdown file `{}`", path.display()))?;
        Self::parse(&content)
            .with_context(|| format!("invalid Markdown front matter in `{}`", path.display()))
    }

    pub fn parse(content: &str) -> anyhow::Result<Self> {
        #[derive(serde::Deserialize)]
        struct Header {
            #[serde(default, deserialize_with = "present")]
            moonbit: Option<Section>,
        }
        #[derive(serde::Deserialize)]
        struct Section {
            #[serde(default, deserialize_with = "present")]
            deps: Option<IndexMap<String, SourceDependencyInfo>>,
            #[serde(default, deserialize_with = "present")]
            import: Option<PkgJSONImport>,
            #[serde(default, deserialize_with = "present")]
            backend: Option<String>,
        }
        // Optional fields may be absent, but a present null is not a valid
        // import list, dependency map, backend, or moonbit section.
        fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
        where
            D: serde::Deserializer<'de>,
            T: serde::Deserialize<'de>,
        {
            T::deserialize(deserializer).map(Some)
        }

        let content = content.strip_prefix('\u{feff}').unwrap_or(content);
        let mut lines = content.split_inclusive('\n');
        let Some(first) = lines.next().filter(|line| line.trim_end() == "---") else {
            return Ok(Self::default());
        };
        let start = first.len();
        let mut end = start;
        for line in lines {
            if line.trim_end() == "---" {
                let header: Option<Header> = serde_yaml::from_str(&content[start..end])?;
                let Some(section) = header.and_then(|header| header.moonbit) else {
                    return Ok(Self::default());
                };
                anyhow::ensure!(
                    section.deps.is_none() || section.import.is_none(),
                    "moonbit.deps and moonbit.import are mutually exclusive"
                );
                let backend = section
                    .backend
                    .as_deref()
                    .map(TargetBackend::str_to_backend)
                    .transpose()
                    .context("invalid moonbit.backend")?;
                return Ok(Self {
                    imports: section.import,
                    backend,
                });
            }
            end += line.len();
        }
        anyhow::bail!("Unterminated Markdown front matter: expected a closing `---` line.")
    }
}

/// Script declarations share the `moon.pkg` grammar. Permission values stay
/// untyped until Moonrun selects the effective policy, so an overridden policy
/// need not satisfy the runtime's permission schema.
#[derive(Debug, Default)]
pub struct MbtxFrontMatter {
    pub imports: Vec<Import>,
    pub permission: Option<MbtxPermission>,
}

/// Keep compatibility YAML undecoded until the runtime selects this permission.
/// Explicit and inherited policies must still override malformed legacy YAML.
#[derive(Debug)]
pub enum MbtxPermission {
    MoonPkg(serde_json_lenient::Value),
    // TODO: Remove this variant when comment-based YAML policies are retired.
    LegacyYaml(String),
}

impl MbtxPermission {
    pub fn deserialize<T: DeserializeOwned>(self) -> anyhow::Result<T> {
        match self {
            Self::MoonPkg(value) => Ok(serde_json_lenient::from_value(value)?),
            Self::LegacyYaml(yaml) => {
                #[derive(serde::Deserialize)]
                struct LegacyFrontMatter<T> {
                    policy: T,
                }
                Ok(serde_yaml::from_str::<LegacyFrontMatter<T>>(&yaml)?.policy)
            }
        }
    }
}

impl MbtxFrontMatter {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read .mbtx file `{}`", path.display()))?;
        Self::parse(&content)
            .with_context(|| format!("invalid .mbtx front matter in `{}`", path.display()))
    }

    pub fn parse(content: &str) -> anyhow::Result<Self> {
        let (front_matter, body) = extract_mbtx_front_matter(content)?;
        let mut header = Self::default();
        // TODO: Remove the legacy prelude parser when imports outside front matter
        // are retired. It also supports old imports following a permission header.
        for source in [front_matter, extract_mbtx_import_source(body).unwrap_or("")] {
            let parsed = crate::moon_pkg::parse(source)?;
            for (key, value) in parsed.iter() {
                match key {
                    "import" => {
                        if value.as_array().is_some_and(|imports| {
                            imports.iter().any(|import| import.get("targets").is_some())
                        }) {
                            anyhow::bail!("Conditional imports are only supported in moon.pkg.");
                        }
                        header
                            .imports
                            .extend(serde_json_lenient::from_value::<Vec<Import>>(
                                value.clone(),
                            )?);
                    }
                    "permission" => {
                        anyhow::ensure!(
                            header.permission.is_none(),
                            "Duplicate `permission` declaration in .mbtx front matter."
                        );
                        header.permission = Some(MbtxPermission::MoonPkg(value.clone()));
                    }
                    "test-import" | "wbtest-import" => anyhow::bail!(
                        "`test-import` and `wbtest-import` are not supported in .mbtx front matter"
                    ),
                    _ => anyhow::bail!("Unexpected key `{key}` in .mbtx front matter."),
                }
            }
        }
        if let Some(yaml) = extract_legacy_mbtx_policy(body) {
            anyhow::ensure!(
                header.permission.is_none(),
                "Cannot combine `permission` front matter with a legacy `// policy:` block."
            );
            header.permission = Some(MbtxPermission::LegacyYaml(yaml));
        }
        Ok(header)
    }
}

/// Compiler front matter fences contain at least three hyphens; the opening
/// and closing lengths need not match. Keep the body borrowed for legacy imports.
fn extract_mbtx_front_matter(content: &str) -> anyhow::Result<(&str, &str)> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    // A shebang belongs on the first line. Strip it before looking for either
    // front matter or the legacy import prelude, without changing source files.
    let content = if content.starts_with("#!") {
        content.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        content
    };
    let trimmed = content.trim_start();
    let mut lines = trimmed.split_inclusive('\n');
    let Some(first) = lines.next().filter(|line| {
        let delimiter = line.trim();
        delimiter.len() >= 3 && delimiter.bytes().all(|byte| byte == b'-')
    }) else {
        return Ok(("", content));
    };
    let start = first.len();
    let mut end = start;
    for line in lines {
        let delimiter = line.trim();
        if delimiter.len() >= 3 && delimiter.bytes().all(|byte| byte == b'-') {
            return Ok((&trimmed[start..end], &trimmed[end + line.len()..]));
        }
        end += line.len();
    }
    anyhow::bail!("Unterminated .mbtx front matter: expected a closing `---` line.")
}

// TODO: Remove this parser when comment-based YAML policies are retired.
fn extract_legacy_mbtx_policy(content: &str) -> Option<String> {
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

fn extract_mbtx_import_source(content: &str) -> Option<&str> {
    static IMPORT_BLOCK_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let import_block_re = IMPORT_BLOCK_RE.get_or_init(|| {
        regex::Regex::new(
            r"(?ms)\A(?:(?:[ \t]*//[^\n]*|[ \t]*)\r?\n)*(?P<import>[ \t]*import[ \t]*\{.*?\}[^\n]*(?:\r?\n)?)",
        )
        .expect("valid .mbtx import-block regex")
    });
    import_block_re
        .captures(content)
        .and_then(|captures| captures.name("import"))
        .map(|import| import.as_str())
}

#[cfg(test)]
mod tests {
    use super::{
        MbtMdFrontMatter, MbtxFrontMatter, extract_mbtx_front_matter, extract_mbtx_import_source,
    };

    #[test]
    fn markdown_distinguishes_empty_and_absent_imports() {
        for section in ["import: []", "import: {}"] {
            let parsed =
                MbtMdFrontMatter::parse(&format!("---\nmoonbit:\n  {section}\n---")).unwrap();
            assert!(parsed.imports.is_some());
        }
        for source in [
            "```mbt check\ntest {}\n```",
            "---\n---",
            "---\ntitle: A document\n---",
            "---\nmoonbit:\n  deps: {example/lib: '1.2.3'}\n---",
            "---\nmoonbit:\n  backend: js\n---",
        ] {
            assert!(MbtMdFrontMatter::parse(source).unwrap().imports.is_none());
        }
    }

    #[test]
    fn markdown_supports_blank_lines_crlf_and_bom() {
        let parsed = MbtMdFrontMatter::parse(
            "\u{feff}---\r\ntitle: Document\r\n\r\nmoonbit:\r\n  import: []\r\n  backend: js\r\n---\r\n```mbt check\r\n```\r\n",
        ).unwrap();
        assert!(parsed.imports.is_some());
        assert_eq!(parsed.backend, Some(crate::target::TargetBackend::Js));
    }

    #[test]
    fn markdown_rejects_invalid_configuration_even_without_imports() {
        for source in [
            "---\nmoonbit: [\n---",
            "---\nmoonbit:\n  import: []",
            "---\nmoonbit: invalid\n---",
            "---\nmoonbit: null\n---",
            "---\nmoonbit:\n  import: invalid\n---",
            "---\nmoonbit:\n  import: null\n---",
            "---\nmoonbit:\n  import: [42]\n---",
            "---\nmoonbit:\n  import: [{alias: integer}]\n---",
            "---\nmoonbit:\n  deps: invalid\n---",
            "---\nmoonbit:\n  deps: {}\n  import: []\n---",
            "---\nmoonbit:\n  backend: removed-backend\n---",
            "---\nmoonbit:\n  backend: []\n---",
            "---\nmoonbit:\n  backend: null\n---",
            "---\nmoonbit:\n  import: []\n  import: {}\n---",
        ] {
            assert!(MbtMdFrontMatter::parse(source).is_err(), "{source}");
        }
    }

    #[test]
    fn parses_imports_and_permission_using_moon_pkg_grammar() {
        let header = MbtxFrontMatter::parse(
            r#"---
// Script dependencies and runtime permissions.
import { "moonbitlang/core/env" @env }
permission(
  env: { "set": { "MODE": "embedded" } },
  fs: { "read": ["."], "write": [] },
  process: { "spawn": true },
)
---
fn main {}
"#,
        )
        .unwrap();
        assert_eq!(header.imports.len(), 1);
        assert_eq!(header.imports[0].get_path(), "moonbitlang/core/env");
        assert_eq!(
            header
                .permission
                .unwrap()
                .deserialize::<serde_json_lenient::Value>()
                .unwrap(),
            serde_json_lenient::json!({
                "env": { "set": { "MODE": "embedded" } },
                "fs": { "read": ["."], "write": [] },
                "process": { "spawn": true },
            })
        );
    }

    #[test]
    fn combines_front_matter_and_legacy_imports_in_source_order() {
        let header = MbtxFrontMatter::parse(
            r#"---
import { "example/lib/first" }
permission()
import { "example/lib/second" }
---
import { "example/lib/third" }
fn main {}
"#,
        )
        .unwrap();
        assert_eq!(
            header
                .imports
                .iter()
                .map(|import| import.get_path())
                .collect::<Vec<_>>(),
            [
                "example/lib/first",
                "example/lib/second",
                "example/lib/third"
            ]
        );
    }

    #[test]
    fn supports_compiler_front_matter_fences() {
        for (opening, closing) in [("---", "---"), ("----", "----"), ("-----", "---")] {
            let source = format!("{opening}\npermission()\n{closing}\nfn main {{}}\n");
            let header = MbtxFrontMatter::parse(&source).unwrap();
            assert!(header.permission.is_some(), "header: {source}");
        }
    }

    #[test]
    fn supports_crlf_bom_empty_and_permission_only_headers() {
        let header =
            MbtxFrontMatter::parse("\u{feff}\r\n---\r\npermission()\r\n---\r\nfn main {}\r\n")
                .unwrap();
        assert!(header.imports.is_empty());
        assert_eq!(
            header
                .permission
                .unwrap()
                .deserialize::<serde_json_lenient::Value>()
                .unwrap(),
            serde_json_lenient::json!({})
        );
        for source in ["---\n---", "fn main {}", "// ordinary comment\nfn main {}"] {
            let header = MbtxFrontMatter::parse(source).unwrap();
            assert!(header.imports.is_empty());
            assert!(header.permission.is_none());
        }
    }

    #[test]
    fn supports_a_shebang_before_front_matter_or_legacy_imports() {
        for (bom, newline) in [("", "\n"), ("", "\r\n"), ("\u{feff}", "\n")] {
            for declarations in [
                "---\nimport { \"moonbitlang/core/env\" }\npermission()\n---\n",
                "import { \"moonbitlang/core/env\" }\n",
                "// policy: {}\nimport { \"moonbitlang/core/env\" }\n",
            ] {
                let source = format!("{bom}#!/usr/bin/env moonx\n\n{declarations}fn main {{}}\n")
                    .replace('\n', newline);
                let header = MbtxFrontMatter::parse(&source).unwrap();
                assert_eq!(header.imports.len(), 1);
                assert_eq!(header.imports[0].get_path(), "moonbitlang/core/env");
                assert_eq!(
                    header.permission.is_some(),
                    !declarations.starts_with("import")
                );
            }
        }
    }

    #[test]
    fn only_skips_a_shebang_on_the_first_line() {
        for source in [
            "\n#!/usr/bin/env moonx\n---\npermission()\n---\n",
            "// comment\n#!/usr/bin/env moonx\n---\npermission()\n---\n",
        ] {
            assert_eq!(extract_mbtx_front_matter(source).unwrap(), ("", source));
        }
        let header = MbtxFrontMatter::parse("#!/usr/bin/env moonx").unwrap();
        assert!(header.imports.is_empty());
        assert!(header.permission.is_none());
    }

    #[test]
    fn leaves_permission_schema_validation_to_the_runtime() {
        let header = MbtxFrontMatter::parse("---\npermission(env: true)\n---\n").unwrap();
        assert_eq!(
            header
                .permission
                .unwrap()
                .deserialize::<serde_json_lenient::Value>()
                .unwrap(),
            serde_json_lenient::json!({"env": true})
        );
    }

    #[test]
    fn rejects_invalid_headers() {
        for (source, message) in [
            ("---\npermission()", "Unterminated .mbtx front matter"),
            (
                "---\npermission()\npermission()\n---",
                "Duplicate `permission`",
            ),
            ("---\npolicy()\n---", "Unexpected key `policy`"),
            ("---\noptions()\n---", "Unexpected key `options`"),
            (
                "---\nimport {} for \"test\"\n---",
                "`test-import` and `wbtest-import`",
            ),
            (
                "---\n#cfg(target = \"wasm\") import { \"a/b\" }\n---",
                "Conditional imports",
            ),
        ] {
            let error = MbtxFrontMatter::parse(source).unwrap_err();
            assert!(error.to_string().contains(message), "{error}");
        }
        assert!(MbtxFrontMatter::parse("---\npermission(env: [)\n---").is_err());
    }

    #[test]
    fn body_delimiters_and_comments_are_not_front_matter() {
        assert_eq!(
            extract_mbtx_front_matter("fn main {}\n---\npermission()\n---")
                .unwrap()
                .0,
            ""
        );
        assert!(
            MbtxFrontMatter::parse("// ordinary comment\nfn main {}")
                .unwrap()
                .permission
                .is_none()
        );
    }

    #[test]
    fn parses_legacy_yaml_permissions_without_consuming_following_comments() {
        for ending in ["\n", "// explanation\n", "///|\n"] {
            let source = format!(
                "// policy:\n//   env:\n//     set:\n//       MODE: embedded\n{ending}import {{ \"moonbitlang/core/env\" }}\nfn main {{}}\n"
            );
            let header = MbtxFrontMatter::parse(&source).unwrap();
            assert_eq!(header.imports.len(), 1);
            assert_eq!(
                header
                    .permission
                    .unwrap()
                    .deserialize::<serde_json_lenient::Value>()
                    .unwrap(),
                serde_json_lenient::json!({ "env": { "set": { "MODE": "embedded" } } })
            );
        }
    }

    #[test]
    fn defers_invalid_legacy_yaml_until_permission_is_used() {
        let header = MbtxFrontMatter::parse("// policy:\n//   env: [\n\nfn main {}\n").unwrap();
        assert!(
            header
                .permission
                .unwrap()
                .deserialize::<serde_json_lenient::Value>()
                .is_err()
        );
    }

    #[test]
    fn rejects_permissions_declared_in_both_formats() {
        let error = MbtxFrontMatter::parse("---\npermission()\n---\n// policy: {}\nfn main {}\n")
            .unwrap_err();
        assert!(error.to_string().contains("Cannot combine"));
    }

    #[test]
    fn extract_mbtx_import_source_returns_none_without_import() {
        let input = "fn main { println(\"ok\") }\n";
        assert_eq!(extract_mbtx_import_source(input), None);
    }

    #[test]
    fn extract_mbtx_import_source_supports_crlf() {
        let input = "import {\r\n  \"a/b@0.1.0/c\",\r\n}\r\n\r\nfn main {}\r\n";
        assert_eq!(
            extract_mbtx_import_source(input),
            Some("import {\r\n  \"a/b@0.1.0/c\",\r\n}\r\n")
        );
    }

    #[test]
    fn extract_mbtx_import_source_stops_before_doc_comment_sentinel() {
        let input = "import {\n  \"a/b@0.1.0/c\",\n}\n///|\nfn main {}\n";
        assert_eq!(
            extract_mbtx_import_source(input),
            Some("import {\n  \"a/b@0.1.0/c\",\n}\n")
        );
    }

    #[test]
    fn extract_mbtx_import_source_stops_before_pub_sentinel() {
        let input = "import {\n  \"a/b@0.1.0/c\",\n}\npub fn main {}\n";
        assert_eq!(
            extract_mbtx_import_source(input),
            Some("import {\n  \"a/b@0.1.0/c\",\n}\n")
        );
    }

    #[test]
    fn extract_mbtx_import_source_only_returns_first_import_statement() {
        let input = r#"import {
  "a/b@0.1.0/c",
}
import {
  "x/y@1.2.3",
}
"#;
        assert_eq!(
            extract_mbtx_import_source(input),
            Some("import {\n  \"a/b@0.1.0/c\",\n}\n")
        );
    }

    #[test]
    fn extract_mbtx_import_source_finds_import_after_leading_comment() {
        let input = "// leading comment\nimport {\n  \"a/b@0.1.0/c\",\n}\nfn main {}\n";
        assert_eq!(
            extract_mbtx_import_source(input),
            Some("import {\n  \"a/b@0.1.0/c\",\n}\n")
        );
    }
}
