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
use std::str::FromStr;

use anyhow::Context;
use indexmap::IndexMap;
use mooncake::registry::{Registry, path as registry_path};
use moonutil::{constants::MOONBITLANG_CORE, dependency::SourceDependencyInfo, package::Import};

#[derive(Default)]
pub(super) struct MbtxFrontMatterImports {
    pub(super) deps: IndexMap<String, SourceDependencyInfo>,
    pub(super) imports: Vec<Import>,
}

#[allow(clippy::disallowed_methods)] // .mbtx parsing needs explicit file reads.
pub(super) fn parse_mbtx_imports(file: &Path) -> anyhow::Result<MbtxFrontMatterImports> {
    if file.extension().is_none_or(|x| x != "mbtx") {
        return Ok(MbtxFrontMatterImports::default());
    }

    let content = std::fs::read_to_string(file)
        .with_context(|| format!("failed to read .mbtx file `{}`", file.display()))?;
    let registry = mooncake::registry::OnlineRegistry::mooncakes_io();
    let Some(import_source) = extract_mbtx_import_source(&content) else {
        return Ok(MbtxFrontMatterImports::default());
    };

    let parsed = moonutil::moon_pkg::parse(import_source)
        .with_context(|| format!("invalid .mbtx import syntax: `{import_source}`"))?;
    let object: std::collections::HashMap<_, _> = parsed.iter().collect();
    if object.len() != 1 {
        anyhow::bail!("invalid .mbtx import syntax: malformed import statement");
    }
    if object.contains_key("test-import") || object.contains_key("wbtest-import") {
        anyhow::bail!(
            "`test-import` and `wbtest-import` are not supported in .mbtx import prelude"
        );
    }
    let import_values = object
        .get("import")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            anyhow::anyhow!("invalid .mbtx import block entries: `import` must be an array")
        })?;

    let mut deps = IndexMap::new();
    let mut module_versions: IndexMap<String, String> = IndexMap::new();
    let mut imports = Vec::with_capacity(import_values.len());

    for value in import_values {
        let (import_path, alias) = if let Some(path) = value.as_str() {
            (path.to_string(), None)
        } else {
            let obj = value.as_object().ok_or_else(|| {
                anyhow::anyhow!("invalid .mbtx import block entry: expected string or object")
            })?;
            let path = obj
                .get("path")
                .and_then(|value| value.as_str())
                .ok_or_else(|| anyhow::anyhow!("invalid .mbtx import block entry: missing `path`"))?
                .to_string();
            let alias = obj
                .get("alias")
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "invalid .mbtx import block entry: `alias` must be a string"
                            )
                        })
                        .map(str::to_string)
                })
                .transpose()?;
            (path, alias)
        };
        let (module, version, package) = split_mbtx_import_path(&import_path, &registry)?;

        match module_versions.entry(module.clone()) {
            indexmap::map::Entry::Occupied(existing) if existing.get() != &version => {
                anyhow::bail!(
                    "multiple versions specified for module '{module}': '{}' and '{version}'",
                    existing.get()
                );
            }
            indexmap::map::Entry::Vacant(entry) => {
                entry.insert(version);
            }
            _ => {}
        }

        let normalized_import = match alias {
            Some(alias) => Import::Alias {
                path: package,
                alias: Some(alias),
                sub_package: false,
            },
            None => Import::Simple(package),
        };
        imports.push(normalized_import);
    }

    for (module, version) in module_versions {
        if module == MOONBITLANG_CORE {
            continue;
        }
        let version = SourceDependencyInfo::from_str(&version)?;
        deps.insert(module, version);
    }

    Ok(MbtxFrontMatterImports { deps, imports })
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

fn split_mbtx_import_path(
    path: &str,
    registry: &impl Registry,
) -> anyhow::Result<(String, String, String)> {
    if path.starts_with(&format!("{MOONBITLANG_CORE}@")) {
        anyhow::bail!("moonbitlang/core imports must not specify a version");
    }

    if path.contains('@') {
        let parsed = registry_path::parse_module_at_version_path(path).with_context(|| {
            format!(
                "import path '{path}' must be in the form \
'username/module@version[/package/path]'"
            )
        })?;
        let full_path_without_version = parsed.full_path_without_version();
        return Ok((
            parsed.module.to_string(),
            parsed.version,
            full_path_without_version,
        ));
    }

    let (module, version, full_path_without_version) = registry
        .resolve_unversioned_path(path)
        .with_context(|| {
            format!(
                "import path '{path}' must be in the form 'username/module[/package/path]'; \
if version is omitted, the module path must be resolvable from local registry index (run `moon update` if needed)"
            )
        })?;
    Ok((module.to_string(), version, full_path_without_version))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{
        MbtxFrontMatterImports, extract_mbtx_import_source, parse_mbtx_imports,
        split_mbtx_import_path,
    };
    use mooncake::registry::OnlineRegistry;
    use moonutil::package::Import;
    use moonutil::resolution::DEFAULT_VERSION;

    #[allow(clippy::disallowed_methods)] // test fixture setup/cleanup on temp files.
    fn parse_imports_from_source(content: &str) -> anyhow::Result<MbtxFrontMatterImports> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let suffix = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "moon-mbtx-parse-test-{}-{suffix}.mbtx",
            std::process::id()
        ));
        std::fs::write(&path, content)?;
        let parsed = parse_mbtx_imports(&path)?;
        let _ = std::fs::remove_file(&path);
        Ok(parsed)
    }

    #[test]
    fn split_mbtx_import_path_supports_module_package() {
        let registry = OnlineRegistry::mooncakes_io();
        let (module, version, package) =
            split_mbtx_import_path("path/module@0.4.38/package/path", &registry)
                .expect("module package import should parse");
        assert_eq!(module, "path/module");
        assert_eq!(version, "0.4.38");
        assert_eq!(package, "path/module/package/path");
    }

    #[test]
    fn split_mbtx_import_path_normalizes_relative_package_with_module_prefix() {
        let registry = OnlineRegistry::mooncakes_io();
        let (module, version, package) = split_mbtx_import_path("a/b@version/c/d/e", &registry)
            .expect("module package import should parse");
        assert_eq!(module, "a/b");
        assert_eq!(version, "version");
        assert_eq!(package, "a/b/c/d/e");
    }

    #[test]
    fn split_mbtx_import_path_supports_module_root() {
        let registry = OnlineRegistry::mooncakes_io();
        let (module, version, package) = split_mbtx_import_path("path/module@0.4.38", &registry)
            .expect("module root import should parse");
        assert_eq!(module, "path/module");
        assert_eq!(version, "0.4.38");
        assert_eq!(package, "path/module");
    }

    #[test]
    fn split_mbtx_import_path_rejects_three_segment_module_with_version() {
        let registry = OnlineRegistry::mooncakes_io();
        assert!(split_mbtx_import_path("moonbitlang/x/fs@0.4.39/path", &registry).is_err());
    }

    #[test]
    fn split_mbtx_import_path_supports_core_without_version() {
        let registry = OnlineRegistry::mooncakes_io();
        let (module, version, package) = split_mbtx_import_path("moonbitlang/core/env", &registry)
            .expect("core import without version should parse");
        assert_eq!(module, "moonbitlang/core");
        assert_eq!(version, DEFAULT_VERSION.to_string());
        assert_eq!(package, "moonbitlang/core/env");
    }

    #[test]
    fn parse_mbtx_imports_supports_block_syntax_and_alias() {
        let input = r#"import {
  "moonbitlang/x@0.4.38/stack" @xstack,
  "moonbitlang/x@0.4.38/queue",
}

        fn main {}
"#;
        let imports = parse_imports_from_source(input).expect("import should decode");
        assert_eq!(imports.imports.len(), 2);
        assert_eq!(imports.imports[0].get_path(), "moonbitlang/x/stack");
        assert_eq!(imports.imports[1].get_path(), "moonbitlang/x/queue");
        assert!(imports.deps.contains_key("moonbitlang/x"));
        assert!(matches!(
            &imports.imports[0],
            Import::Alias {
                alias: Some(alias),
                ..
            } if alias == "xstack"
        ));
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

    #[test]
    fn parse_mbtx_imports_supports_top_level_package_path() {
        let parsed = parse_imports_from_source("import { \"path/module@0.4.38/path/to/pkg\" }\n")
            .expect("value should parse");
        assert_eq!(parsed.imports.len(), 1);
        assert_eq!(parsed.imports[0].get_path(), "path/module/path/to/pkg");
        assert!(parsed.deps.contains_key("path/module"));
    }

    #[test]
    fn parse_mbtx_imports_allow_core_without_version() {
        let parsed = parse_imports_from_source("import { \"moonbitlang/core/env\" }\n")
            .expect("core import should parse");
        assert_eq!(parsed.imports.len(), 1);
        assert_eq!(parsed.imports[0].get_path(), "moonbitlang/core/env");
        assert!(parsed.deps.is_empty());
    }

    #[test]
    fn parse_mbtx_imports_reject_core_with_version() {
        let err = match parse_imports_from_source("import { \"moonbitlang/core@0.1.0/env\" }\n") {
            Ok(_) => panic!("core import with version should fail"),
            Err(err) => err,
        };
        assert!(
            err.to_string()
                .contains("moonbitlang/core imports must not specify a version")
        );
    }

    #[test]
    fn parse_mbtx_imports_allow_corexx_with_version() {
        let parsed = parse_imports_from_source("import { \"moonbitlang/corexx@0.1.0/env\" }\n")
            .expect("corexx import with version should parse");
        assert_eq!(parsed.imports.len(), 1);
        assert_eq!(parsed.imports[0].get_path(), "moonbitlang/corexx/env");
        assert!(parsed.deps.contains_key("moonbitlang/corexx"));
    }
}
