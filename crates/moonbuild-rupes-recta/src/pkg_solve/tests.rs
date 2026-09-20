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

// Only test fixtures perform I/O; dependency resolution remains in memory.
#![allow(clippy::disallowed_methods)]

use std::path::Path;

use moonutil::{
    resolution::{DirSyncResult, ModuleSource, ResolvedEnv},
    user_log::UserLog,
};

use super::{DepRelationship, solve};
use crate::{
    discover::{DiscoverResult, discover_packages},
    model::TargetKind,
};

fn write_module(path: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(path.join("lib"))?;
    std::fs::write(
        path.join("moon.mod"),
        "name = \"example/imports/v2\"\nversion = \"2.0.0\"\n",
    )?;
    std::fs::write(path.join("moon.pkg"), "")?;
    Ok(())
}

fn resolve_module(
    path: &Path,
    user_log: &UserLog,
) -> anyhow::Result<(DiscoverResult, DepRelationship)> {
    let module = moonutil::manifest::read_module_desc_file_in_dir(path)?;
    let source = ModuleSource::from_local_module(&module, path)?;
    let (env, id) = ResolvedEnv::only_one_module(source, module);
    let mut dirs = DirSyncResult::default();
    dirs.insert(id, path.to_owned());
    let packages = discover_packages(&env, &dirs, user_log)?;
    let deps = solve(&env, &packages, false, user_log)?;
    Ok((packages, deps))
}

#[test]
fn redundant_test_imports_compare_resolved_alias_and_import_all() -> anyhow::Result<()> {
    // The root of a /v2 module defaults to @imports, not @v2.
    for (regular, test, redundant, alias, import_all) in [
        ("", "", true, "imports", false),
        ("", "@imports", true, "imports", false),
        ("@imports", "", true, "imports", false),
        ("@custom", "@custom", true, "custom", false),
        ("*", "@imports *", true, "imports", true),
        ("", "@custom", false, "custom", false),
        ("", "*", false, "imports", true),
        ("*", "", false, "imports", false),
        ("@custom *", "@other *", false, "other", true),
    ] {
        let dir = tempfile::tempdir()?;
        write_module(dir.path())?;
        std::fs::write(
            dir.path().join("lib/moon.pkg"),
            format!(
                r#"
import {{ "example/imports/v2" {regular} }}
import {{ "example/imports/v2" {test} }} for "wbtest"
import {{ "example/imports/v2" {test} }} for "test"
"#,
            ),
        )?;
        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
        let (packages, deps) = resolve_module(dir.path(), &user_log)?;
        let warnings = capture.take();
        assert_eq!(
            warnings.len(),
            if redundant { 2 } else { 0 },
            "{warnings:?}"
        );
        let lib = packages
            .get_package_id_by_name("example/imports/v2/lib")
            .expect("fixture contains the importing package");
        let imported = packages
            .get_package_id_by_name("example/imports/v2")
            .expect("fixture contains the root package");
        for (index, (kind, field)) in [
            (TargetKind::WhiteboxTest, "wbtest-import"),
            (TargetKind::BlackboxTest, "test-import"),
        ]
        .into_iter()
        .enumerate()
        {
            if redundant {
                assert_eq!(
                    warnings[index].message,
                    format!(
                        "Redundant import of package `example/imports/v2` in `{field}` of package `example/imports/v2/lib`; it is already available through `import`."
                    )
                );
            }
            let edge = deps
                .dep_graph
                .edge_weight(
                    lib.build_target(kind),
                    imported.build_target(TargetKind::Source),
                )
                .expect("test import remains in the dependency graph");
            assert_eq!(edge.short_alias, alias);
            assert_eq!(edge.import_all, import_all);
            assert_eq!(edge.kind, kind);
        }
    }
    Ok(())
}

#[test]
fn redundant_test_imports_use_final_import_blocks() -> anyhow::Result<()> {
    for (regular, test, duplicate_count, redundant_count) in [
        ("", "import { \"example/imports/v2\" }", 0, 0),
        ("import { \"example/imports/v2\" }", "", 0, 0),
        (
            "import { \"example/imports/v2\" @old }\nimport { \"example/imports/v2\" }",
            "import { \"example/imports/v2\" }",
            1,
            2,
        ),
        (
            "import { \"example/imports/v2\" }",
            "import { \"example/imports/v2\", \"example/imports/v2\" @other }",
            2,
            0,
        ),
        (
            "import { \"example/imports/v2\" }",
            "import { \"example/imports/v2\" @other, \"example/imports/v2\" }",
            2,
            2,
        ),
    ] {
        let dir = tempfile::tempdir()?;
        write_module(dir.path())?;
        let tests = if test.is_empty() {
            String::new()
        } else {
            format!("{test} for \"wbtest\"\n{test} for \"test\"\n")
        };
        std::fs::write(
            dir.path().join("lib/moon.pkg"),
            format!("{regular}\n{tests}"),
        )?;
        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
        resolve_module(dir.path(), &user_log)?;
        let warnings = capture.take();
        assert_eq!(
            warnings.len(),
            duplicate_count + redundant_count,
            "{warnings:?}"
        );
        assert_eq!(
            warnings
                .iter()
                .filter(|w| w.message.starts_with("Redundant import"))
                .count(),
            redundant_count,
            "{warnings:?}"
        );
    }
    Ok(())
}

#[test]
fn redundant_json_test_imports_respect_warning_visibility() -> anyhow::Result<()> {
    for (cached, level, count) in [
        (false, log::LevelFilter::Warn, 1),
        (false, log::LevelFilter::Error, 0),
        (true, log::LevelFilter::Warn, 0),
    ] {
        let dir = tempfile::tempdir()?;
        let path = if cached {
            dir.path().join(".mooncakes/module")
        } else {
            dir.path().to_owned()
        };
        write_module(&path)?;
        std::fs::write(
            path.join("lib/moon.pkg.json"),
            r#"{
                "import": [{"path": "example/imports/v2", "alias": ""}],
                "wbtest-import": {"example/imports/v2": "imports"},
                "test-import": [{"path": "example/imports/v2", "alias": "custom"}]
            }"#,
        )?;
        let (user_log, capture) = UserLog::captured(level);
        resolve_module(&path, &user_log)?;
        let warnings = capture.take();
        assert_eq!(warnings.len(), count, "{warnings:?}");
    }
    Ok(())
}

#[test]
fn redundant_test_imports_warn_for_local_core_but_not_installed_stdlib() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    write_module(dir.path())?;
    std::fs::write(dir.path().join("moon.mod"), "name = \"moonbitlang/core\"\n")?;
    std::fs::create_dir(dir.path().join("prelude"))?;
    std::fs::write(dir.path().join("prelude/moon.pkg"), "")?;
    std::fs::write(
        dir.path().join("lib/moon.pkg"),
        r#"
import { "moonbitlang/core", "moonbitlang/core/prelude" }
import { "moonbitlang/core" } for "test"
"#,
    )?;
    let module = moonutil::manifest::read_module_desc_file_in_dir(dir.path())?;
    for (source, count) in [
        (ModuleSource::from_local_module(&module, dir.path())?, 1),
        (ModuleSource::from_stdlib(&module, dir.path())?, 0),
    ] {
        let (env, id) = ResolvedEnv::only_one_module(source, module.clone());
        let mut dirs = DirSyncResult::default();
        dirs.insert(id, dir.path().to_owned());
        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
        let packages = discover_packages(&env, &dirs, &user_log)?;
        solve(&env, &packages, false, &user_log)?;
        let warnings = capture.take();
        assert_eq!(warnings.len(), count, "{warnings:?}");
    }
    Ok(())
}
