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

use std::path::PathBuf;

use moonbuild_rupes_recta::discover::{discover_for_build, discover_for_fmt};
use moonutil::manifest::read_module_desc_file_in_dir;
use moonutil::project::ProjectManifest;
use moonutil::resolution::{DirSyncResult, ModuleDependencyGraph, ModuleSource};
use moonutil::user_log::UserLog;

fn fixture_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
#[allow(clippy::disallowed_methods)]
fn discover_skips_nested_module_with_moon_mod() {
    let root = fixture_dir("module_dsl_skip_nested");
    let module = read_module_desc_file_in_dir(&root).expect("read module");
    let source = ModuleSource::from_local_module(&module, &root).unwrap();
    let (resolved, module_id) = ModuleDependencyGraph::only_one_module(source, module);

    let mut dirs = DirSyncResult::new();
    dirs.insert(module_id, root.clone());

    let discovered = discover_for_build(
        &resolved,
        &dirs,
        Some(2),
        &UserLog::new(log::LevelFilter::Error),
    )
    .expect("discover packages");
    let mut packages = discovered
        .all_packages(false)
        .map(|(_, pkg)| pkg.fqn.to_string())
        .collect::<Vec<_>>();
    packages.sort();
    let actual = packages.join("\n");

    assert_eq!(actual, "example/root\nexample/root/lib");
}

#[test]
#[allow(clippy::disallowed_methods)]
fn parallel_discovery_preserves_package_ids_files_and_module_boundaries() -> anyhow::Result<()> {
    let root = fixture_dir("parallel_discovery");
    let module = read_module_desc_file_in_dir(&root)?;
    let source = ModuleSource::from_local_module(&module, &root)?;
    let (mut graph, id) = ModuleDependencyGraph::only_one_module(source, module);
    let mut dirs = DirSyncResult::new();
    dirs.insert(id, root.clone());
    // The nested module is both a traversal boundary and a separate scan root.
    for dir in [
        fixture_dir("module_dsl_skip_nested"),
        fixture_dir("module_dsl_skip_nested/submod"),
    ] {
        let module = read_module_desc_file_in_dir(&dir)?;
        let source = ModuleSource::from_local_module(&module, &dir)?;
        let id = graph.add_module(source, std::sync::Arc::new(module));
        dirs.insert(id, dir);
    }
    let mut baseline = None;
    for workers in [1, 2, 4] {
        let (log, capture) = UserLog::captured(log::LevelFilter::Warn);
        let discovered = discover_for_build(&graph, &dirs, Some(workers), &log)?;
        let packages = discovered
            .all_packages(false)
            .map(|(id, package)| {
                (
                    id,
                    package.fqn.to_string(),
                    package.source_files.clone(),
                    package.manifest_path.clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            packages.iter().map(|p| p.1.as_str()).collect::<Vec<_>>(),
            [
                "example/parallel",
                "example/parallel/a",
                "example/parallel/a/child",
                "example/parallel/a-b",
                "example/root",
                "example/root/lib",
                "example/nested",
            ]
        );
        assert_eq!(
            packages[0]
                .2
                .iter()
                .map(|p| p.file_name().unwrap().to_str().unwrap())
                .collect::<Vec<_>>(),
            [".hidden.mbt", "root.mbt"]
        );
        assert_eq!(packages[2].2.len(), 1);
        assert_eq!(packages[2].2[0].file_name().unwrap(), "child.mbt");
        assert_eq!(
            capture.take().len(),
            1,
            "only the root's shadowed manifest warns"
        );
        if let Some(baseline) = &baseline {
            assert_eq!(&packages, baseline);
        } else {
            baseline = Some(packages);
        }
    }
    // Formatting uses the same directory rules without launching workers.
    let project = discover_for_fmt(
        &root,
        &ProjectManifest::None,
        &UserLog::new(log::LevelFilter::Error),
    )?;
    let source_root = root.join("src").canonicalize()?;
    assert_eq!(
        project
            .package_roots()
            .map(|(_, path)| path.strip_prefix(&source_root).unwrap().to_owned())
            .collect::<Vec<_>>(),
        ["", "a", "a/child", "a-b"].map(PathBuf::from),
    );
    Ok(())
}
