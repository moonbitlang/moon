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

use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::Context;
use indexmap::IndexMap;
use mooncake::registry::Registry;
use moonutil::{
    common::MOONBITLANG_CORE,
    dependency::{SourceDependencyInfo, SourceDependencyInfoJson},
    package::Import,
};

#[derive(Default)]
pub(super) struct MbtxFrontMatterImports {
    pub(super) deps: IndexMap<String, SourceDependencyInfoJson>,
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
    let (import_source, _) = split_mbtx(&content)?;
    if import_source.is_empty() {
        return Ok(MbtxFrontMatterImports::default());
    }

    let parsed = moonutil::moon_pkg::parse(&import_source)
        .with_context(|| format!("invalid .mbtx import syntax: `{import_source}`"))?;
    let object = parsed.as_object().ok_or_else(|| {
        anyhow::anyhow!("invalid .mbtx import syntax: malformed import statement")
    })?;
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
        deps.insert(module, SourceDependencyInfoJson::from(version));
    }

    Ok(MbtxFrontMatterImports { deps, imports })
}

/// split "import {...}; code" into ("import {...}", "\n; code")
///
/// Note: this is a temporary solution to handle `import {...}` declaration in mbtx,
/// since moonc doesn't support import syntax in mbt yet.
fn split_mbtx(content: &str) -> anyhow::Result<(String, String)> {
    static IMPORT_BLOCK_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let import_block_re = IMPORT_BLOCK_RE.get_or_init(|| {
        regex::Regex::new(
            r"(?ms)\A(?:(?:[ \t]*//[^\n]*|[ \t]*)\r?\n)*(?P<import>[ \t]*import[ \t]*\{.*?\}[^\n]*(?:\r?\n)?)",
        )
        .expect("valid .mbtx import-block regex")
    });
    if let Some(caps) = import_block_re.captures(content) {
        let m = caps
            .name("import")
            .expect("named group `import` must exist");
        let import_source = m.as_str().to_string();
        let blanked = import_source
            .chars()
            .map(|ch| if matches!(ch, '\n' | '\r') { ch } else { ' ' })
            .collect::<String>();
        return Ok((
            import_source,
            format!(
                "{}{}{}",
                &content[..m.start()],
                blanked,
                &content[m.end()..]
            ),
        ));
    }
    Ok((String::new(), content.to_string()))
}

fn split_mbtx_import_path(
    path: &str,
    registry: &impl Registry,
) -> anyhow::Result<(String, String, String)> {
    if path.starts_with(&format!("{MOONBITLANG_CORE}@")) {
        anyhow::bail!("moonbitlang/core imports must not specify a version");
    }
    let (module, version, full_path_without_version) = registry
        .resolve_path(path, true)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "import path '{path}' must be in the form 'username/module[/package/path]' or \
'path/to/module@version[/package/path]'; \
if version is omitted, the module path must be resolvable from local registry index (run `moon update` if needed)"
            )
        })?;
    let module = module.to_string();
    Ok((module, version, full_path_without_version))
}

/// Remove the leading `import {...}` declaration in `.mbtx`, then write the
/// remaining code into `<target-dir>/<stem>.mbt` for compilation.
pub(super) fn prepare_single_file_for_compile(
    file: &Path,
    temp_workspace: &Path,
) -> anyhow::Result<PathBuf> {
    if file.extension().is_none_or(|x| x != "mbtx") {
        return Ok(file.to_path_buf());
    }

    #[allow(clippy::disallowed_methods)] // .mbtx preprocessing writes a temp compile input.
    let content = std::fs::read_to_string(file)
        .with_context(|| format!("failed to read .mbtx file `{}`", file.display()))?;
    let (_, mbt_code) = split_mbtx(&content)?;

    #[allow(clippy::disallowed_methods)] // .mbtx preprocessing writes a temp compile input.
    std::fs::create_dir_all(temp_workspace).with_context(|| {
        format!(
            "failed to create directory for preprocessed single-file input `{}`",
            temp_workspace.display()
        )
    })?;
    let mut out_file_name = file
        .file_stem()
        .expect(".mbtx input file should have a filename")
        .to_os_string();
    out_file_name.push(".mbt");
    let out_path = temp_workspace.join(out_file_name);
    #[allow(clippy::disallowed_methods)] // .mbtx preprocessing writes a temp compile input.
    std::fs::write(&out_path, mbt_code).with_context(|| {
        format!(
            "failed to write preprocessed .mbtx input file `{}`",
            out_path.display()
        )
    })?;
    Ok(out_path)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{MbtxFrontMatterImports, parse_mbtx_imports, split_mbtx, split_mbtx_import_path};
    use mooncake::registry::OnlineRegistry;
    use moonutil::mooncakes::DEFAULT_VERSION;
    use moonutil::package::Import;

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
            split_mbtx_import_path("path/to/module@0.4.38/package/path", &registry)
                .expect("module package import should parse");
        assert_eq!(module, "path/to/module");
        assert_eq!(version, "0.4.38");
        assert_eq!(package, "path/to/module/package/path");
    }

    #[test]
    fn split_mbtx_import_path_normalizes_relative_package_with_module_prefix() {
        let registry = OnlineRegistry::mooncakes_io();
        let (module, version, package) = split_mbtx_import_path("a/b/c@version/d/e", &registry)
            .expect("module package import should parse");
        assert_eq!(module, "a/b/c");
        assert_eq!(version, "version");
        assert_eq!(package, "a/b/c/d/e");
    }

    #[test]
    fn split_mbtx_import_path_supports_module_root() {
        let registry = OnlineRegistry::mooncakes_io();
        let (module, version, package) = split_mbtx_import_path("path/to/module@0.4.38", &registry)
            .expect("module root import should parse");
        assert_eq!(module, "path/to/module");
        assert_eq!(version, "0.4.38");
        assert_eq!(package, "path/to/module");
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
    fn split_mbtx_supports_block_syntax_and_alias() {
        let input = r#"import {
  "moonbitlang/x@0.4.38/stack" @xstack,
  "moonbitlang/x@0.4.38/queue",
}

        fn main {}
"#;
        let (_import_source, mbt_code) = split_mbtx(input).expect("split should succeed");
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
        assert!(mbt_code.contains("fn main {}"));
    }

    #[test]
    fn split_mbtx_keeps_non_import_file_unchanged() {
        let input = "fn main { println(\"ok\") }\n";
        let (import_source, output) = split_mbtx(input).expect("split should succeed");
        assert!(import_source.is_empty());
        assert_eq!(output, input);
    }

    #[test]
    fn split_mbtx_preserves_crlf_newlines() {
        let input = "import {\r\n  \"a/b@0.1.0/c\",\r\n}\r\n\r\nfn main {}\r\n";
        let (import_source, output) = split_mbtx(input).expect("split should succeed");
        let blanked_import = import_source
            .chars()
            .map(|ch| if matches!(ch, '\n' | '\r') { ch } else { ' ' })
            .collect::<String>();
        assert_eq!(output, format!("{blanked_import}\r\nfn main {{}}\r\n"));
    }

    #[test]
    fn split_mbtx_stops_before_doc_comment_sentinel() {
        let input = "import {\n  \"a/b@0.1.0/c\",\n}\n///|\nfn main {}\n";
        let (import_source, output) = split_mbtx(input).expect("split should succeed");
        assert!(!import_source.is_empty());
        assert!(output.lines().take(3).all(|line| line.trim().is_empty()));
        assert!(output.contains("///|"));
        assert!(output.contains("fn main {}"));
    }

    #[test]
    fn split_mbtx_stops_before_pub_sentinel() {
        let input = "import {\n  \"a/b@0.1.0/c\",\n}\npub fn main {}\n";
        let (import_source, output) = split_mbtx(input).expect("split should succeed");
        assert!(!import_source.is_empty());
        assert!(output.lines().take(3).all(|line| line.trim().is_empty()));
        assert!(output.contains("pub fn main {}"));
    }

    #[test]
    fn split_mbtx_keeps_following_import_statement() {
        let input = r#"import {
  "a/b@0.1.0/c",
}
import {
  "x/y@1.2.3",
}
"#;
        let (import_source, mbt_code) = split_mbtx(input).expect("split should succeed");
        assert_eq!(import_source, "import {\n  \"a/b@0.1.0/c\",\n}\n");
        assert!(mbt_code.contains("import {\n  \"x/y@1.2.3\",\n}\n"));
    }

    #[test]
    fn split_mbtx_finds_import_after_leading_comment() {
        let input = "// leading comment\nimport {\n  \"a/b@0.1.0/c\",\n}\nfn main {}\n";
        let (import_source, mbt_code) = split_mbtx(input).expect("split should succeed");
        assert_eq!(import_source, "import {\n  \"a/b@0.1.0/c\",\n}\n");
        assert!(mbt_code.starts_with("// leading comment\n"));
    }

    #[test]
    fn split_mbtx_splits_import_and_mbt_code() {
        let input = "import {\n  \"a/b@0.1.0/c\",\n}\n\nfn main {}\n";
        let (import_source, mbt_code) = split_mbtx(input).expect("split should succeed");
        assert_eq!(import_source, "import {\n  \"a/b@0.1.0/c\",\n}\n");
        let blanked_import = import_source
            .chars()
            .map(|ch| if ch == '\n' { '\n' } else { ' ' })
            .collect::<String>();
        assert_eq!(mbt_code, format!("{blanked_import}\nfn main {{}}\n"));
    }

    #[test]
    fn parse_mbtx_imports_supports_top_level_package_path() {
        let parsed =
            parse_imports_from_source("import { \"path/to/module@0.4.38/path/to/pkg\" }\n")
                .expect("value should parse");
        assert_eq!(parsed.imports.len(), 1);
        assert_eq!(parsed.imports[0].get_path(), "path/to/module/path/to/pkg");
        assert!(parsed.deps.contains_key("path/to/module"));
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
