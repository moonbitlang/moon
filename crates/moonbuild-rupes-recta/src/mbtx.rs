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
use std::str::FromStr;

use anyhow::Context;
use indexmap::IndexMap;
use mooncake::registry::{Registry, RegistryClient, path as registry_path};
use moonutil::{
    constants::MOONBITLANG_CORE, dependency::SourceDependencyInfo, package::Import,
    user_log::UserLog,
};

#[derive(Default)]
pub(super) struct MbtxFrontMatterImports {
    pub(super) deps: IndexMap<String, SourceDependencyInfo>,
    pub(super) imports: Vec<Import>,
}

pub(super) fn parse_mbtx_imports(
    file: &Path,
    user_log: &UserLog,
) -> anyhow::Result<MbtxFrontMatterImports> {
    if file.extension().is_none_or(|x| x != "mbtx") {
        return Ok(MbtxFrontMatterImports::default());
    }

    let header = moonutil::front_matter::MbtxFrontMatter::from_file(file)?;
    let registry = RegistryClient::configured(user_log);

    let mut deps = IndexMap::new();
    let mut module_versions: IndexMap<String, String> = IndexMap::new();
    let mut imports = Vec::with_capacity(header.imports.len());

    for mut import in header.imports {
        let (module, version, package) = split_mbtx_import_path(import.get_path(), &registry)?;

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

        match &mut import {
            Import::Simple(path) | Import::Alias { path, .. } => *path = package,
        }
        imports.push(import);
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
            parsed.version.expect("versioned path has a version"),
            full_path_without_version,
        ));
    }

    let (module, version, full_path_without_version) = registry
        .resolve_cached_unversioned_path(path)
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

    use super::{MbtxFrontMatterImports, parse_mbtx_imports, split_mbtx_import_path};
    use mooncake::registry::RegistryClient;
    use moonutil::package::Import;
    use moonutil::resolution::DEFAULT_VERSION;
    use moonutil::user_log::UserLog;

    #[allow(clippy::disallowed_methods)] // test fixture setup/cleanup on temp files.
    fn parse_imports_from_source(content: &str) -> anyhow::Result<MbtxFrontMatterImports> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let suffix = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "moon-mbtx-parse-test-{}-{suffix}.mbtx",
            std::process::id()
        ));
        std::fs::write(&path, content)?;
        let parsed = parse_mbtx_imports(&path, &UserLog::new(log::LevelFilter::Error));
        let _ = std::fs::remove_file(&path);
        parsed
    }

    #[test]
    fn parse_mbtx_imports_from_delimited_front_matter() {
        let parsed = parse_imports_from_source(
            r#"---
import { "example/lib@1.2.3/first" @first }
permission(env: { "set": { "MODE": "embedded" } })
import { "example/lib@1.2.3/second" @second * }
---
fn main {}
"#,
        )
        .expect("front matter imports should resolve");
        assert_eq!(parsed.imports.len(), 2);
        assert_eq!(parsed.imports[0].get_path(), "example/lib/first");
        assert!(
            matches!(&parsed.imports[0], Import::Alias { alias: Some(alias), .. } if alias == "first")
        );
        assert_eq!(parsed.imports[1].get_path(), "example/lib/second");
        assert!(
            matches!(&parsed.imports[1], Import::Alias { alias: Some(alias), import_all: true, .. } if alias == "second")
        );
        assert_eq!(parsed.deps.len(), 1);
        assert!(parsed.deps.contains_key("example/lib"));
    }

    #[test]
    fn parse_mbtx_rejects_conditional_imports() {
        let error = parse_imports_from_source(
            r#"import { "example/first@1.0.0" } #cfg(target = "native") import { "example/second@1.0.0" @second }"#,
        )
        .err()
        .expect("conditional script imports must be rejected");
        assert_eq!(
            error.root_cause().to_string(),
            "Conditional imports are only supported in moon.pkg."
        );
    }

    #[test]
    fn split_mbtx_import_path_supports_module_package() {
        let registry = RegistryClient::configured(&UserLog::new(log::LevelFilter::Error));
        let (module, version, package) =
            split_mbtx_import_path("path/module@0.4.38/package/path", &registry)
                .expect("module package import should parse");
        assert_eq!(module, "path/module");
        assert_eq!(version, "0.4.38");
        assert_eq!(package, "path/module/package/path");
    }

    #[test]
    fn split_mbtx_import_path_normalizes_relative_package_with_module_prefix() {
        let registry = RegistryClient::configured(&UserLog::new(log::LevelFilter::Error));
        let (module, version, package) = split_mbtx_import_path("a/b@version/c/d/e", &registry)
            .expect("module package import should parse");
        assert_eq!(module, "a/b");
        assert_eq!(version, "version");
        assert_eq!(package, "a/b/c/d/e");
    }

    #[test]
    fn split_mbtx_import_path_supports_module_root() {
        let registry = RegistryClient::configured(&UserLog::new(log::LevelFilter::Error));
        let (module, version, package) = split_mbtx_import_path("path/module@0.4.38", &registry)
            .expect("module root import should parse");
        assert_eq!(module, "path/module");
        assert_eq!(version, "0.4.38");
        assert_eq!(package, "path/module");
    }

    #[test]
    fn split_mbtx_import_path_rejects_three_segment_module_with_version() {
        let registry = RegistryClient::configured(&UserLog::new(log::LevelFilter::Error));
        assert!(split_mbtx_import_path("moonbitlang/x/fs@0.4.39/path", &registry).is_err());
    }

    #[test]
    fn split_mbtx_import_path_supports_core_without_version() {
        let registry = RegistryClient::configured(&UserLog::new(log::LevelFilter::Error));
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
    fn parse_mbtx_imports_supports_import_all_alias() {
        let input = r#"import {
  "moonbitlang/x@0.4.38/stack" *,
}

        fn main {}
"#;
        let imports = parse_imports_from_source(input).expect("import should decode");
        assert_eq!(imports.imports.len(), 1);
        assert_eq!(imports.imports[0].get_path(), "moonbitlang/x/stack");
        assert!(imports.deps.contains_key("moonbitlang/x"));
        assert!(matches!(
            &imports.imports[0],
            Import::Alias {
                alias: None,
                import_all: true,
                ..
            }
        ));
    }

    #[test]
    fn parse_mbtx_imports_supports_import_all_with_named_alias() {
        let input = r#"import {
  "moonbitlang/x@0.4.38/stack" @xstack *,
}

        fn main {}
"#;
        let imports = parse_imports_from_source(input).expect("import should decode");
        assert_eq!(imports.imports.len(), 1);
        assert_eq!(imports.imports[0].get_path(), "moonbitlang/x/stack");
        assert!(imports.deps.contains_key("moonbitlang/x"));
        assert!(matches!(
            &imports.imports[0],
            Import::Alias {
                alias: Some(alias),
                import_all: true,
                ..
            } if alias == "xstack"
        ));
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
