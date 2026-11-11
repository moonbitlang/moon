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

//! Package discovery for builds and formatting.
//!
//! [`discover_for_build`] scans resolved modules and prepares build inputs.
//! [`discover_for_fmt`] scans project roots without resolving dependencies.
//! Both entry points use the same package scanner.

// Filesystem APIs are allowed here; later build phases consume the results.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

mod model;
mod prepare;
mod scan;
pub mod special_case;
pub mod synth;

pub use model::{
    DiscoverError, DiscoverResult, DiscoveredPackage, FmtProject, SingleFileSourceKind,
};
pub(crate) use model::{FmtModule, resolve_prebuild_path};
pub(crate) use scan::PackageInput;

use crate::pkg_name::PackageFQN;
use moonutil::{
    manifest::{read_module_manifest, warn_module_manifest},
    package::resolve_supported_targets,
    project::ProjectManifest,
    resolution::{
        DirSyncResult, ModuleDependencyGraph, ModuleSource, ModuleSourceKind, ResolvedModule,
    },
    user_log::UserLog,
};
use slotmap::SlotMap;
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::Arc,
};
use tracing::{info, instrument};

/// Discover packages and prepare their build inputs across resolved modules.
/// Formatting uses [`discover_for_fmt`] instead.
#[instrument(skip_all)]
pub fn discover_for_build(
    module_graph: &ModuleDependencyGraph,
    dirs: &DirSyncResult,
    user_log: &UserLog,
) -> Result<DiscoverResult, DiscoverError> {
    info!("Starting package discovery across all modules");
    let mut res = DiscoverResult::default();
    for (id, source) in module_graph.all_modules_and_id() {
        if matches!(source.source(), ModuleSourceKind::SingleFile(_)) {
            continue;
        }
        let dir = dirs
            .get(id)
            .expect("resolved module has a source directory");
        // Author-facing manifest warnings depend on the module's role, not its
        // storage directory. Read/validation errors are still returned for every module.
        let emit_warnings = module_graph.input_module_ids().contains(&id)
            && !matches!(source.source(), ModuleSourceKind::Stdlib(_));
        let (format, manifest) = read_module_manifest(dir, false).map_err(|inner| {
            DiscoverError::CantReadModuleFile {
                module: source.clone(),
                path: dir.clone(),
                inner,
            }
        })?;
        if emit_warnings {
            warn_module_manifest(dir, format, &manifest, user_log);
        }
        if manifest.name != module_graph.module_info(id).name {
            return Err(DiscoverError::ModuleNameMismatch {
                registry: module_graph.module_info(id).name.clone(),
                read: manifest.name,
            });
        }
        let (targets, _) =
            resolve_supported_targets(manifest.supported_targets.as_ref()).map_err(|inner| {
                DiscoverError::CantReadModuleFile {
                    module: source.clone(),
                    path: dir.clone(),
                    inner,
                }
            })?;
        let manifest = Arc::new(manifest);
        res.set_module_info(id, Arc::clone(&manifest));
        for input in scan::scan_packages(source, dir, &manifest, false, emit_warnings, user_log)? {
            let package = prepare::prepare_package(id, input, &targets)?;
            if emit_warnings {
                scan::warn_package_name(&package.fqn, user_log);
            }
            res.add_package(id, package.fqn.package().clone(), package)?;
        }
    }
    info!(
        packages = res.package_count(),
        modules = module_graph.module_count(),
        "Package discovery completed"
    );
    Ok(res)
}

/// Read only the current project's root modules and scan their packages.
/// Legacy manifests remain readable here because formatting migrates them.
#[tracing::instrument(skip_all)]
pub fn discover_for_fmt(
    source_dir: &Path,
    project_manifest: &ProjectManifest,
    user_log: &UserLog,
) -> Result<FmtProject, DiscoverError> {
    let single_module = [source_dir.to_owned()];
    let module_dirs = match project_manifest {
        ProjectManifest::Workspace(workspace) => workspace.members(),
        ProjectManifest::None | ProjectManifest::Module(_) => &single_module,
    };
    let mut project = FmtProject {
        modules: Vec::new(),
        packages: SlotMap::with_key(),
    };
    let mut names = HashMap::<String, PackageFQN>::new();
    for module_dir in module_dirs {
        let (manifest_format, manifest) =
            read_module_manifest(module_dir, true).map_err(|inner| {
                DiscoverError::CantReadLocalModuleFile {
                    path: module_dir.to_path_buf(),
                    inner,
                }
            })?;
        warn_module_manifest(module_dir, manifest_format, &manifest, user_log);
        let source = ModuleSource::from_local_module(&manifest, module_dir).map_err(|inner| {
            DiscoverError::CantReadLocalModuleFile {
                path: module_dir.to_path_buf(),
                inner: inner.into(),
            }
        })?;
        let mut packages = BTreeMap::new();
        for input in scan::scan_packages(&source, module_dir, &manifest, true, true, user_log)? {
            scan::warn_package_name(&input.fqn, user_log);
            if let Some(first) = names.insert(input.fqn.to_string(), input.fqn.clone()) {
                return Err(DiscoverError::ConflictingPackageNameString {
                    first: first.into(),
                    second: input.fqn.into(),
                });
            }
            let path = input.fqn.package().clone();
            let id = project.packages.insert(input);
            packages.insert(path, id);
        }
        project.modules.push(FmtModule {
            module: ResolvedModule::new(source, Arc::new(manifest)),
            manifest_format,
            packages,
        });
    }
    Ok(project)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use moonutil::{
        manifest::{MoonMod, read_package_desc_file_from_path_with_supported_targets_decl},
        resolution::{DirSyncResult, ModuleDependencyGraph, ModuleSource},
        user_log::UserLog,
    };

    use super::prepare::{discover_c_stub_headers, discover_virtual_mbti_files};
    use super::{DiscoverError, discover_for_build, discover_for_fmt};
    use moonutil::project::ProjectManifest;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "moonbuild-rupes-recta-discover-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temporary module directory");
        dir
    }

    #[test]
    fn formatting_does_not_prepare_native_build_inputs() -> anyhow::Result<()> {
        // The fixture has a C-stub path that builds must reject, while its
        // MoonBit files and manifests remain valid formatter inputs.
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fmt_invalid_native_stub");
        let log = UserLog::new(log::LevelFilter::Off);
        let project = discover_for_fmt(&dir, &ProjectManifest::None, &log)?;
        assert_eq!(project.packages.len(), 1);

        let module = &project.modules[0].module;
        let (graph, id) = moonutil::resolution::ModuleDependencyGraph::only_one_module(
            module.source().clone(),
            module.module_info().as_ref().clone(),
        );
        let mut dirs = moonutil::resolution::DirSyncResult::default();
        dirs.insert(id, dir);
        assert!(matches!(
            crate::discover::discover_for_build(&graph, &dirs, &log),
            Err(DiscoverError::InvalidStubPath { .. })
        ));
        Ok(())
    }

    #[test]
    fn manifest_warnings_follow_module_role_not_directory_name() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let root_dir = dir.path().join(".mooncakes/project");
        let dependency_dir = dir.path().join("local-dependency");
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/manifest_warning_roles");
        for relative in [".mooncakes/project", "local-dependency"] {
            let target = dir.path().join(relative);
            std::fs::create_dir_all(&target)?;
            for file in ["moon.mod", "moon.pkg"] {
                std::fs::copy(fixture.join(relative).join(file), target.join(file))?;
            }
        }
        let root = moonutil::manifest::read_module_desc_file_in_dir(&root_dir)?;
        let dependency = moonutil::manifest::read_module_desc_file_in_dir(&dependency_dir)?;
        let (mut graph, root_id) = ModuleDependencyGraph::only_one_module(
            ModuleSource::from_local_module(&root, &root_dir)?,
            root,
        );
        let dependency_id = graph.add_module(
            ModuleSource::from_local_module(&dependency, &dependency_dir)?,
            std::sync::Arc::new(dependency),
        );
        let mut dirs = DirSyncResult::default();
        dirs.insert(root_id, root_dir);
        dirs.insert(dependency_id, dependency_dir.clone());

        for level in [log::LevelFilter::Warn, log::LevelFilter::Error] {
            let (log, capture) = UserLog::captured(level);
            discover_for_build(&graph, &dirs, &log)?;
            let warnings = capture.take();
            assert_eq!(warnings.len(), usize::from(level == log::LevelFilter::Warn));
            if let Some(warning) = warnings.first() {
                assert_eq!(
                    warning.message,
                    "`options(\"supported-targets\": ...)` in `moon.pkg` is deprecated. Please use `supported_targets = ...` instead."
                );
            }
        }
        // Suppressing a dependency's author-facing warnings must not suppress
        // its parse errors.
        std::fs::copy(
            fixture.join("local-dependency/moon.pkg.invalid"),
            dependency_dir.join("moon.pkg"),
        )?;
        assert!(matches!(
            discover_for_build(&graph, &dirs, &UserLog::new(log::LevelFilter::Warn)),
            Err(super::DiscoverError::CantReadPackageFile { .. })
        ));
        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn native_stub_paths_use_windows_separators_and_stay_in_the_package() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(
            dir.path().join("moon.mod"),
            "name = \"test/native-paths\"\n",
        )?;
        std::fs::write(dir.path().join("stub.c"), "int stub(void) { return 42; }\n")?;
        let root = std::fs::canonicalize(dir.path())?;
        let module = moonutil::manifest::read_module_desc_file_in_dir(&root)?;
        let source = ModuleSource::from_local_module(&module, &root)?;
        let (module_graph, id) = ModuleDependencyGraph::only_one_module(source, module);
        let mut dirs = DirSyncResult::default();
        dirs.insert(id, root.clone());
        let log = UserLog::new(log::LevelFilter::Error);

        for (stub, valid) in [
            ("native/../stub.c", true),
            (r"native\..\stub.c", true),
            (r"native\nested/../../stub.c", true),
            ("../outside.c", false),
            (r"..\outside.c", false),
            (r"native\../../outside.c", false),
        ] {
            std::fs::write(
                root.join("moon.pkg"),
                format!("options(\"native-stub\": [{stub:?}])\n"),
            )?;
            let result = discover_for_build(&module_graph, &dirs, &log);
            if valid {
                let discovered = result?;
                let package = discovered
                    .get_package_id_by_name("test/native-paths")
                    .expect("fixture package was discovered");
                let files = &discovered.get_package(package).c_stub_files;
                assert_eq!(files, &[root.join("stub.c")], "{stub:?}");
                assert!(files[0].is_file(), "{stub:?}");
            } else {
                assert!(
                    matches!(result, Err(super::DiscoverError::InvalidStubPath { .. })),
                    "{stub:?}: {result:?}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn discovery_warns_for_packages_named_like_major_version_modules() -> anyhow::Result<()> {
        for (name, version, warning_count) in [
            ("a/b", "0.1.0", 2),
            ("a/b/v2", "2.0.0", 0),
            ("moonbitlang/core", "0.1.0", 0),
        ] {
            let dir = tempfile::tempdir()?;
            std::fs::write(
                dir.path().join("moon.mod"),
                format!(
                    "name = \"{name}\"\nversion = \"{version}\"\noptions(\"source\": \"src\")\n"
                ),
            )?;
            for package in ["", "c", "v0", "v1", "v2", "v3", "v02", "vx", "v2/c", "c/v2"] {
                let path = dir.path().join("src").join(package);
                std::fs::create_dir_all(&path)?;
                std::fs::write(path.join("moon.pkg"), "")?;
            }
            let module = moonutil::manifest::read_module_desc_file_in_dir(dir.path())?;
            let source = if name == "moonbitlang/core" {
                ModuleSource::from_stdlib(&module, dir.path())?
            } else {
                ModuleSource::from_local_module(&module, dir.path())?
            };
            let (module_graph, id) = ModuleDependencyGraph::only_one_module(source, module);
            let mut dirs = DirSyncResult::default();
            dirs.insert(id, dir.path().to_owned());

            for level in [log::LevelFilter::Warn, log::LevelFilter::Error] {
                let (user_log, capture) = UserLog::captured(level);
                let discovered = discover_for_build(&module_graph, &dirs, &user_log)?;
                assert!(
                    discovered
                        .get_package_id_by_name(&format!("{name}/v2"))
                        .is_some()
                );
                let warnings = capture.take();
                let expected_count = if level == log::LevelFilter::Warn {
                    warning_count
                } else {
                    0
                };
                assert_eq!(warnings.len(), expected_count, "{warnings:?}");
                if expected_count != 0 {
                    assert_eq!(
                        warnings[0].message,
                        "Package `a/b/v2` in module `a/b` has the same import path as a major-version module. Consider renaming package `v2` to avoid ambiguity."
                    );
                    assert_eq!(
                        warnings[1].message,
                        "Package `a/b/v3` in module `a/b` has the same import path as a major-version module. Consider renaming package `v3` to avoid ambiguity."
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    fn discovery_does_not_warn_consumers_about_dependency_package_names() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let app_dir = dir.path().join("app");
        let dep_dir = dir.path().join("dep");
        std::fs::create_dir_all(&app_dir)?;
        std::fs::create_dir_all(dep_dir.join("v2"))?;
        std::fs::write(app_dir.join("moon.mod"), "name = \"test/app\"\n")?;
        std::fs::write(
            dep_dir.join("moon.mod"),
            "name = \"a/b\"\nversion = \"0.1.0\"\n",
        )?;
        std::fs::write(dep_dir.join("v2/moon.pkg"), "")?;
        let app = moonutil::manifest::read_module_desc_file_in_dir(&app_dir)?;
        let dep = moonutil::manifest::read_module_desc_file_in_dir(&dep_dir)?;

        for source in [
            ModuleSource::from_local_module(&dep, &dep_dir)?,
            ModuleSource::from_version(
                "a/b".into(),
                dep.version
                    .clone()
                    .expect("test dependency declares a version"),
            )?,
        ] {
            let (mut module_graph, app_id) = ModuleDependencyGraph::only_one_module(
                ModuleSource::from_local_module(&app, &app_dir)?,
                app.clone(),
            );
            let dep_id = module_graph.add_module(source, std::sync::Arc::new(dep.clone()));
            let mut dirs = DirSyncResult::default();
            dirs.insert(app_id, app_dir.clone());
            dirs.insert(dep_id, dep_dir.clone());
            let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);

            let discovered = discover_for_build(&module_graph, &dirs, &user_log)?;
            assert!(discovered.get_package_id_by_name("a/b/v2").is_some());
            assert!(capture.take().is_empty());
        }
        Ok(())
    }

    #[test]
    fn discovery_temporarily_allows_core_v128() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(
            dir.path().join("moon.mod"),
            "name = \"moonbitlang/core\"\nversion = \"0.1.0\"\n",
        )?;
        for package in ["v128", "v129"] {
            let path = dir.path().join(package);
            std::fs::create_dir_all(&path)?;
            std::fs::write(path.join("moon.pkg"), "")?;
        }
        let module = moonutil::manifest::read_module_desc_file_in_dir(dir.path())?;
        let source = ModuleSource::from_local_module(&module, dir.path())?;
        let (module_graph, id) = ModuleDependencyGraph::only_one_module(source, module);
        let mut dirs = DirSyncResult::default();
        dirs.insert(id, dir.path().to_owned());
        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);

        discover_for_build(&module_graph, &dirs, &user_log)?;

        let warnings = capture.take();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(
            warnings[0].message,
            "Package `moonbitlang/core/v129` in module `moonbitlang/core` has the same import path as a major-version module. Consider renaming package `v129` to avoid ambiguity."
        );
        Ok(())
    }

    #[test]
    fn discover_for_build_keeps_installed_module_manifest() {
        let dir = temp_dir("module-info");
        std::fs::write(
            dir.join("moon.mod"),
            r#"name = "example/pkg"

version = "0.2.1"

warnings = "+a"

rule(name: "gen", command: "tool $input -o $output")

options(
  "compile-flags": ["-DREGISTRY"],
  "link-flags": ["-lregistry"],
  "--moonbit-unstable-prebuild": "prebuild.py",
)"#,
        )
        .expect("failed to write test moon.mod");

        let source: ModuleSource = "example/pkg@0.2.1"
            .parse()
            .expect("failed to parse test module source");
        let stub = MoonMod {
            name: "example/pkg".to_string(),
            version: Some(source.version().clone()),
            ..Default::default()
        };
        let (module_graph, id) = ModuleDependencyGraph::only_one_module(source, stub);
        let mut dirs = DirSyncResult::default();
        dirs.insert(id, dir.clone());

        let discovered =
            discover_for_build(&module_graph, &dirs, &UserLog::new(log::LevelFilter::Error))
                .expect("failed to discover test packages");

        let module = discovered.module_info(id);
        assert_eq!(
            module.compile_flags.as_deref(),
            Some(&["-DREGISTRY".into()][..])
        );
        assert_eq!(
            module.link_flags.as_deref(),
            Some(&["-lregistry".into()][..])
        );
        assert_eq!(module.warn_list.as_deref(), Some("+a"));
        assert_eq!(
            module.__moonbit_unstable_prebuild.as_deref(),
            Some("prebuild.py")
        );
        let rule = module.rule.as_ref().expect("expected test rule to be kept");
        assert_eq!(rule.len(), 1);
        assert_eq!(rule[0].name, "gen");
        assert_eq!(rule[0].command, "tool $input -o $output");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn package_discovery_keeps_explicit_dot_source_root_but_skips_dot_descendants() {
        let dir = temp_dir("dot-directories");
        std::fs::write(
            dir.join("moon.mod"),
            r#"name = "example/pkg"

version = "0.2.1"

source = ".src""#,
        )
        .expect("failed to write test moon.mod");
        std::fs::create_dir_all(dir.join(".src/.hidden"))
            .expect("failed to create test package directories");
        std::fs::write(dir.join(".src/moon.pkg"), "")
            .expect("failed to write root package manifest");
        std::fs::write(dir.join(".src/.hidden/moon.pkg"), "")
            .expect("failed to write hidden package manifest");

        let source: ModuleSource = "example/pkg@0.2.1"
            .parse()
            .expect("failed to parse test module source");
        let stub = MoonMod {
            name: "example/pkg".to_string(),
            version: Some(source.version().clone()),
            ..Default::default()
        };
        let (module_graph, id) = ModuleDependencyGraph::only_one_module(source, stub);
        let mut dirs = DirSyncResult::default();
        dirs.insert(id, dir.clone());

        let discovered =
            discover_for_build(&module_graph, &dirs, &UserLog::new(log::LevelFilter::Error))
                .expect("failed to discover test packages");

        assert!(discovered.get_package_id_by_name("example/pkg").is_some());
        assert!(
            discovered
                .get_package_id_by_name("example/pkg/.hidden")
                .is_none()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn discovery_records_only_existing_virtual_contracts() {
        let dir = temp_dir("dependency-virtual-contract");
        let manifest_path = dir.join("moon.pkg");
        std::fs::write(
            &manifest_path,
            r#"
options(
  "virtual": { "has-default": false },
)

rule(
  name: "generate",
  command: "generate",
)
dev_build(
  rule: "generate",
  input: "contract.txt",
  output: "./pkg.mbti",
)
"#,
        )
        .expect("failed to write package manifest");
        std::fs::write(dir.join("virtual.mbti"), "legacy contract")
            .expect("failed to write legacy contract");

        let (pkg, _) = read_package_desc_file_from_path_with_supported_targets_decl(
            &manifest_path,
            &UserLog::new(log::LevelFilter::Error),
            false,
            true,
        )
        .expect("failed to read package manifest");

        assert!(discover_virtual_mbti_files(&pkg, &dir).is_empty());

        std::fs::write(dir.join("pkg.mbti"), "canonical contract")
            .expect("failed to write canonical contract");
        assert_eq!(
            discover_virtual_mbti_files(&pkg, &dir),
            [dir.join("pkg.mbti")]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn c_stub_headers_follow_package_file_set_boundaries() -> anyhow::Result<()> {
        let dir = temp_dir("c-stub-headers");
        std::fs::create_dir_all(dir.join("native/include"))?;
        std::fs::write(dir.join("native/stub.h"), "stub")?;
        std::fs::write(dir.join("native/include/detail.hpp"), "detail")?;
        std::fs::write(dir.join("native/ignored.txt"), "ignored")?;

        std::fs::create_dir_all(dir.join("_build/generated"))?;
        std::fs::write(dir.join("_build/generated/stale.h"), "stale")?;

        std::fs::create_dir_all(dir.join(".generated"))?;
        std::fs::write(dir.join(".generated/stale.hpp"), "stale")?;

        std::fs::create_dir_all(dir.join("nested-module"))?;
        std::fs::write(
            dir.join("nested-module/moon.mod"),
            r#"
name = "nested/module"
"#,
        )?;
        std::fs::write(dir.join("nested-module/foreign.h"), "foreign")?;

        std::fs::create_dir_all(dir.join("nested-package"))?;
        std::fs::write(dir.join("nested-package/moon.pkg"), "")?;
        std::fs::write(dir.join("nested-package/foreign.hpp"), "foreign")?;

        let headers = discover_c_stub_headers(&dir)?
            .into_iter()
            .map(|path| {
                path.strip_prefix(&dir)
                    .map(Path::to_path_buf)
                    .map_err(anyhow::Error::from)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        assert_eq!(
            headers,
            [
                PathBuf::from("native/include/detail.hpp"),
                PathBuf::from("native/stub.h"),
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
        Ok(())
    }
}
