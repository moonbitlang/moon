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

//! Discovers packages and build targets from source directory layouts.
//!
//! The discover process is designed to be minimal, only fetching required
//! information from the file system. Later stages that do not require file
//! system access should be split into a separate module instead of coupled with
//! this discover process.

// Specifically allow file I/O here, because that what this module is about.
#![allow(clippy::disallowed_types, clippy::disallowed_methods)]

mod model;
pub mod special_case;
pub mod synth;

pub use model::{
    DiscoverError, DiscoverResult, DiscoveredLocalProject, DiscoveredPackage, SingleFileSourceKind,
};
use moonutil::constants::{PackageSourceFileKind, package_source_file_kind};
use moonutil::project::ProjectManifest;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use indexmap::IndexSet;
use log::{debug, info, trace};
use moonutil::package::MoonPkg;
use moonutil::resolution::{
    DirSyncResult, ModuleId, ModuleName, ModuleSource, ModuleSourceKind, ResolvedEnv,
    ResolvedModule, ResolvedRootModules,
};
use moonutil::target::TargetBackend;
use moonutil::{
    constants::{
        MBTI_USER_WRITTEN, MOON_MOD, MOON_MOD_JSON, MOON_PKG, MOON_PKG_JSON, MOONBITLANG_ABORT,
        is_ignored_directory_name,
    },
    manifest::{
        read_module_desc_file_in_dir, read_package_desc_file_from_path_with_supported_targets_decl,
        warn_known_shadowed_manifest, warn_module_manifest,
    },
    package::resolve_supported_targets,
    user_log::UserLog,
};
use relative_path::{PathExt, RelativePath};
use tracing::{Level, instrument};
use walkdir::WalkDir;

use crate::{
    pkg_name::{PackageFQN, PackagePath},
    special_cases::module_name_is_core,
    util::strip_trailing_slash,
};

/// Discover packages contained by all dependencies from their paths
#[instrument(skip_all)]
pub fn discover_packages(
    env: &ResolvedEnv,
    dirs: &DirSyncResult,
    user_log: &UserLog,
) -> Result<DiscoverResult, DiscoverError> {
    info!("Starting package discovery across all modules");
    let mut res = DiscoverResult::default();

    debug!("Discovering packages in {} modules", env.module_count());

    for (id, m) in env.all_modules_and_id() {
        // SPECIAL_CASE: Skip stdlib in discovering. They are handled below.
        // UPDATED: stdlib is not skipped anymore, as we require
        // packages in stdlib to be explicitly imported by users.
        if let ModuleSourceKind::SingleFile(_) = m.source() {
            continue;
        };

        let dir = dirs.get(id).expect("Bad module ID to get directory");
        let location = format!("at module root '{}'", dir.display());
        warn_module_manifest(dir, &location, user_log);
        discover_packages_for_mod(
            &mut res,
            dir,
            id,
            env.resolved_module(id),
            env.input_module_ids().contains(&id),
            user_log,
        )?;
    }

    if let Some(id) = res.get_package_id_by_name(MOONBITLANG_ABORT) {
        res.set_abort_pkg(id);
    }

    info!(
        "Package discovery completed: found {} packages across {} modules",
        res.package_count(),
        env.module_count()
    );

    Ok(res)
}

/// Discover a local project directly from the source tree without dependency resolution.
///
/// This supports either a single local module rooted at `source_dir` or a
/// workspace rooted there via `moon.work`.
#[instrument(skip_all)]
pub fn discover_local_project(
    source_dir: &Path,
    project_manifest: &ProjectManifest,
    user_log: &UserLog,
) -> Result<DiscoveredLocalProject, DiscoverError> {
    info!(
        "Starting local project discovery for {}",
        source_dir.display()
    );

    let module_dirs = match project_manifest {
        ProjectManifest::Workspace(workspace) => workspace.members().to_vec(),
        ProjectManifest::None | ProjectManifest::Module(_) => vec![source_dir.to_path_buf()],
    };

    let mut root_modules = ResolvedRootModules::with_key();
    let mut root_module_ids = Vec::with_capacity(module_dirs.len());
    let mut pkg_dirs = DiscoverResult::default();

    for module_dir in module_dirs {
        warn_module_manifest(
            &module_dir,
            &format!("at module root '{}'", module_dir.display()),
            user_log,
        );
        let module = read_module_desc_file_in_dir(&module_dir).map_err(|inner| {
            DiscoverError::CantReadLocalModuleFile {
                path: module_dir.clone(),
                inner,
            }
        })?;
        let module = Arc::new(module);
        let source = ModuleSource::from_local_module(&module, &module_dir).map_err(|inner| {
            DiscoverError::CantReadLocalModuleFile {
                path: module_dir.clone(),
                inner: inner.into(),
            }
        })?;
        let id = root_modules.insert(ResolvedModule::new(source, module));
        root_module_ids.push(id);

        discover_packages_for_mod(
            &mut pkg_dirs,
            &module_dir,
            id,
            &root_modules[id],
            true,
            user_log,
        )?;
    }

    if let Some(id) = pkg_dirs.get_package_id_by_name(MOONBITLANG_ABORT) {
        pkg_dirs.set_abort_pkg(id);
    }

    info!(
        "Local project discovery completed: found {} packages",
        pkg_dirs.package_count()
    );

    Ok(DiscoveredLocalProject {
        root_modules,
        root_module_ids,
        pkg_dirs,
    })
}

/// Discover packages within the given module directory
#[instrument(level = Level::DEBUG, skip(res, dir, module))]
pub(crate) fn discover_packages_for_mod(
    res: &mut DiscoverResult,
    dir: &Path,
    id: ModuleId,
    module: &ResolvedModule,
    is_root_module: bool,
    user_log: &UserLog,
) -> Result<(), DiscoverError> {
    // This information is the one we get from the registry. We will read again
    // from the resolved directory
    let module_source = module.source();
    let m_registry = module.module_info();

    info!(
        "Begin discovering packages for {} at {}",
        module_source,
        dir.display()
    );

    // This is the version we read from directory
    let m = read_module_desc_file_in_dir(dir).map_err(|e| DiscoverError::CantReadModuleFile {
        module: module_source.clone(),
        path: dir.to_owned(),
        inner: e,
    })?;

    // Do some basic sanity checks
    if m.name != m_registry.name {
        return Err(DiscoverError::ModuleNameMismatch {
            registry: m_registry.name.clone(),
            read: m.name.clone(),
        });
    }

    let source_dir_name = m.source.clone().unwrap_or_default();
    let scan_source_root = {
        let p = dir.join(&source_dir_name);
        dunce::canonicalize(p).map_err(|e| DiscoverError::CantReadModulePackages {
            module: module_source.clone(),
            inner: e.into(),
        })?
    };
    let is_core = module_name_is_core(&m.name);
    let (module_supported_targets, _) = resolve_supported_targets(m.supported_targets.as_ref())
        .map_err(|e| DiscoverError::CantReadModuleFile {
            module: module_source.clone(),
            path: dir.to_owned(),
            inner: e,
        })?;
    res.set_module_info(id, Arc::new(m));

    // Recursively walk through the module's directories
    let mut walkdir = WalkDir::new(&scan_source_root)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|x| x.file_type().is_dir());
    while let Some(entry) = walkdir.next() {
        let entry = entry.map_err(|e| DiscoverError::CantReadModulePackages {
            module: module_source.clone(),
            inner: e.into(),
        })?;

        let abs_path = strip_trailing_slash(entry.path());
        // this will be fed to package path
        let rel_path = abs_path
            .relative_to(&scan_source_root)
            .expect("Walked directory should be a descendant of the scan source");

        // The configured source root is explicit, so only filter its descendants.
        if entry.depth() != 0 && is_ignored_directory_name(entry.file_name()) {
            debug!(
                "Skipping {} recursively because its directory name is ignored",
                abs_path.display()
            );
            walkdir.skip_current_dir();
            continue;
        }

        // Avoid descending into another module
        let mod_path = abs_path.join(MOON_MOD);
        let mod_json_path = abs_path.join(MOON_MOD_JSON);
        if (mod_path.exists() || mod_json_path.exists()) && rel_path != "" {
            debug!(
                "Skipping {} recursively because it contains a module configuration file",
                abs_path.display()
            );
            walkdir.skip_current_dir();
            continue;
        }

        // Check if this directory is a package.
        let moon_pkg_path = abs_path.join(MOON_PKG);
        let moon_pkg_json_path = abs_path.join(MOON_PKG_JSON);
        let pkg_manifest_path = if moon_pkg_path.exists() {
            if moon_pkg_json_path.exists() {
                let location = format!("at package root '{}'", abs_path.display());
                warn_known_shadowed_manifest(
                    abs_path,
                    MOON_PKG_JSON,
                    MOON_PKG,
                    &location,
                    user_log,
                );
            }
            moon_pkg_path
        } else if moon_pkg_json_path.exists() {
            moon_pkg_json_path
        } else {
            debug!(
                "Skipping {} because it does not contain a package configuration file",
                abs_path.display()
            );
            continue;
        };

        // Begin discovering the package
        debug!("Discovering package at {}", abs_path.display());
        let is_stdlib_pkg = matches!(module_source.source(), ModuleSourceKind::Stdlib(_));
        let pkg = discover_one_package(
            id,
            module_source,
            &rel_path,
            is_stdlib_pkg,
            &module_supported_targets,
            &pkg_manifest_path,
            user_log,
        )?;
        debug!(
            "Found package: {} with {} source files",
            pkg.fqn,
            pkg.source_files.len()
        );
        // Warn authors in the selected project, not consumers of dependencies.
        // The same full import path could also denote a versioned module root.
        // Reuse module-name classification so this warning follows its grammar.
        // TODO: Remove this exception once moonbitlang/core/v128 follows the
        // major-version package naming convention.
        let is_temporarily_allowed_core_v128 = is_core && pkg.fqn.package().as_str() == "v128";
        if is_root_module
            && !is_stdlib_pkg
            && !is_temporarily_allowed_core_v128
            && !module_source.name().username.is_empty()
            && !pkg.fqn.package().is_empty()
            && matches!(
                ModuleName::from(pkg.fqn.to_string().as_str()).major_version_suffix(),
                Ok(Some(_))
            )
        {
            user_log.warn(format!(
                "Package `{}` in module `{}` has the same import path as a major-version module. \
                 Consider renaming package `{}` to avoid ambiguity.",
                pkg.fqn,
                module_source.name(),
                pkg.fqn.package(),
            ));
        }
        res.add_package(id, pkg.fqn.package().clone(), pkg)?;
    }

    Ok(())
}

/// Discover one package and get its basic information. This does *not* create
/// e.g. subpackages.
#[instrument(level = Level::DEBUG, skip(m, rel, pkg_manifest_path))]
#[allow(clippy::too_many_arguments)]
fn discover_one_package(
    mid: ModuleId,
    m: &ModuleSource,
    rel: &RelativePath,
    pkg_is_stdlib: bool, // Whether the package being discovered is inside the stdlib (core) module.
    module_supported_targets: &IndexSet<TargetBackend>,
    pkg_manifest_path: &Path,
    user_log: &UserLog,
) -> Result<DiscoveredPackage, DiscoverError> {
    let abs = pkg_manifest_path
        .parent()
        .expect("package manifest path should be inside package root");
    let pkg_path = PackagePath::new_from_rel_path(rel)
        .expect("Generation of package path from relative path should not error");
    let fqn = PackageFQN::new(m.clone(), pkg_path);

    // Discover the package config
    let (pkg_json, supported_targets_decl) =
        read_package_desc_file_from_path_with_supported_targets_decl(pkg_manifest_path, user_log)
            .map_err(|e| DiscoverError::CantReadPackageFile {
            module: m.clone(),
            package: fqn.package().clone(),
            path: abs.to_path_buf(),
            inner: e,
        })?;
    let mut effective_supported_targets = pkg_json.supported_targets.clone();
    effective_supported_targets.retain(|t| module_supported_targets.contains(t));

    // Discover source files within the package
    let mut source_files = Vec::new();
    let mut mbt_lex_files = Vec::new();
    let mut mbt_yacc_files = Vec::new();
    let mut mbt_md_files = Vec::new();
    let mut mbtp_files = Vec::new();

    let dir = abs
        .read_dir()
        .map_err(|x| DiscoverError::CantListPackageDir {
            module: m.clone(),
            package: fqn.package().clone(),
            path: abs.to_owned(),
            inner: x.into(),
        })?;
    for file in dir {
        let file = file.map_err(|e| DiscoverError::CantListPackageDir {
            module: m.clone(),
            package: fqn.package().clone(),
            path: abs.to_owned(),
            inner: e.into(),
        })?;
        let path = file.path();
        let file_type = file
            .file_type()
            .map_err(|e| DiscoverError::CantReadFileInfo {
                module: m.clone(),
                package: fqn.package().clone(),
                file: path.clone(),
                inner: e.into(),
            })?;

        if !file_type.is_file() && !file_type.is_symlink() {
            // Only files (including symlinked files) are included within the package
            continue;
        }
        trace!("Found file {}", path.display());

        let filename = path
            .file_name()
            .expect("We are listing a dir, file should have name");
        let filename_str = filename.to_string_lossy();
        match package_source_file_kind(&filename_str) {
            Some(PackageSourceFileKind::Mbt) => source_files.push(path),
            Some(PackageSourceFileKind::MbtMd) => mbt_md_files.push(path),
            Some(PackageSourceFileKind::Mbtp) => mbtp_files.push(path),
            Some(PackageSourceFileKind::Mbl) => mbt_lex_files.push(path),
            Some(PackageSourceFileKind::Mby) => mbt_yacc_files.push(path),
            None => {
                // File is not one of our expected types, skip
            }
        }
    }

    // Read C stubs from package json
    let mut c_stubs = Vec::new();
    if let Some(stub_list) = &pkg_json.native_stub {
        for stub in stub_list {
            let rel_path = RelativePath::new(&stub).normalize();
            // Check if path is valid
            if rel_path.starts_with("..") {
                return Err(DiscoverError::InvalidStubPath {
                    module: m.clone(),
                    package: fqn.package().clone(),
                    path: stub.clone(),
                    msg: "Path descends into parent directory",
                });
            }
            c_stubs.push(rel_path.to_path(abs));
        }
    };
    let c_stub_headers = if c_stubs.is_empty() {
        Vec::new()
    } else {
        discover_c_stub_headers(abs).map_err(|inner| DiscoverError::CantListPackageDir {
            module: m.clone(),
            package: fqn.package().clone(),
            path: abs.to_owned(),
            inner,
        })?
    };

    // Sort the source files for repeatable results
    let _sort_guard = tracing::debug_span!("sorting_files").entered();
    source_files.sort();
    mbt_lex_files.sort();
    mbt_yacc_files.sort();
    mbt_md_files.sort();
    mbtp_files.sort();
    drop(_sort_guard);

    // Record only virtual contracts that are already part of the Package File
    // Set. Build planning later combines this with actual prebuild providers.
    let virtual_mbti_files = discover_virtual_mbti_files(&pkg_json, &fqn, abs);

    Ok(DiscoveredPackage {
        root_path: abs.to_path_buf(),
        module: mid,
        fqn,
        single_file_source_kind: None,
        manifest_path: Some(pkg_manifest_path.to_owned()),
        raw: Box::new(pkg_json),
        supported_targets_decl,
        effective_supported_targets,
        source_files,
        mbt_lex_files,
        mbt_yacc_files,
        mbt_md_files,
        mbtp_files,
        c_stub_files: c_stubs,
        c_stub_header_files: c_stub_headers,
        virtual_mbti_files,
        is_stdlib: pkg_is_stdlib,
    })
}

fn discover_c_stub_headers(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut headers = Vec::new();
    let mut entries = WalkDir::new(root).sort_by_file_name().into_iter();
    while let Some(entry) = entries.next() {
        let entry = entry?;
        if entry.depth() != 0 && entry.file_type().is_dir() {
            if is_ignored_directory_name(entry.file_name()) {
                entries.skip_current_dir();
                continue;
            }

            let path = entry.path();
            if [MOON_MOD, MOON_MOD_JSON, MOON_PKG, MOON_PKG_JSON]
                .iter()
                .any(|manifest| path.join(manifest).exists())
            {
                entries.skip_current_dir();
                continue;
            }
        }

        if (entry.file_type().is_file() || entry.file_type().is_symlink())
            && entry.path().extension().is_some_and(|extension| {
                matches!(extension.to_str(), Some("h" | "hh" | "hpp" | "hxx"))
            })
        {
            headers.push(entry.into_path());
        }
    }
    headers.sort();
    Ok(headers)
}

fn discover_virtual_mbti_files(pkg_json: &MoonPkg, fqn: &PackageFQN, abs: &Path) -> Vec<PathBuf> {
    if pkg_json.virtual_pkg.is_some() {
        // There are two types of `.mbti` files accepted as input:
        // - The newer version is `pkg.mbti`
        // - The older version is `<pkg_short_name>.mbti`
        let short_name = fqn.short_alias();

        let new_mbti = abs.join(MBTI_USER_WRITTEN);
        let old_mbti = abs.join(format!("{}.mbti", short_name));

        [new_mbti, old_mbti]
            .into_iter()
            .filter(|path| path.exists())
            .collect()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use moonutil::{
        manifest::{MoonMod, read_package_desc_file_from_path_with_supported_targets_decl},
        resolution::{DirSyncResult, ModuleSource, ResolvedEnv},
        user_log::UserLog,
    };
    use relative_path::RelativePath;

    use super::{discover_c_stub_headers, discover_packages, discover_virtual_mbti_files};
    use crate::pkg_name::{PackageFQN, PackagePath};

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
            let (env, id) = ResolvedEnv::only_one_module(source, module);
            let mut dirs = DirSyncResult::default();
            dirs.insert(id, dir.path().to_owned());

            for level in [log::LevelFilter::Warn, log::LevelFilter::Error] {
                let (user_log, capture) = UserLog::captured(level);
                let discovered = discover_packages(&env, &dirs, &user_log)?;
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
            let (mut env, app_id) = ResolvedEnv::only_one_module(
                ModuleSource::from_local_module(&app, &app_dir)?,
                app.clone(),
            );
            let dep_id = env.add_module(source, std::sync::Arc::new(dep.clone()));
            let mut dirs = DirSyncResult::default();
            dirs.insert(app_id, app_dir.clone());
            dirs.insert(dep_id, dep_dir.clone());
            let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);

            let discovered = discover_packages(&env, &dirs, &user_log)?;
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
        let (env, id) = ResolvedEnv::only_one_module(source, module);
        let mut dirs = DirSyncResult::default();
        dirs.insert(id, dir.path().to_owned());
        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);

        discover_packages(&env, &dirs, &user_log)?;

        let warnings = capture.take();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert_eq!(
            warnings[0].message,
            "Package `moonbitlang/core/v129` in module `moonbitlang/core` has the same import path as a major-version module. Consider renaming package `v129` to avoid ambiguity."
        );
        Ok(())
    }

    #[test]
    fn discover_packages_keeps_installed_module_manifest() {
        let dir = temp_dir("module-info");
        std::fs::write(
            dir.join("moon.mod.json"),
            r#"{
  "name": "example/pkg",
  "version": "0.2.1",
  "compile-flags": ["-DREGISTRY"],
  "link-flags": ["-lregistry"],
  "warn-list": "+a",
  "--moonbit-unstable-prebuild": "prebuild.py",
  "rule": [
    { "name": "gen", "command": "tool $input -o $output" }
  ]
}
"#,
        )
        .expect("failed to write test moon.mod.json");

        let source: ModuleSource = "example/pkg@0.2.1"
            .parse()
            .expect("failed to parse test module source");
        let stub = MoonMod {
            name: "example/pkg".to_string(),
            version: Some(source.version().clone()),
            ..Default::default()
        };
        let (resolved_env, id) = ResolvedEnv::only_one_module(source, stub);
        let mut dirs = DirSyncResult::default();
        dirs.insert(id, dir.clone());

        let discovered =
            discover_packages(&resolved_env, &dirs, &UserLog::new(log::LevelFilter::Error))
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
            dir.join("moon.mod.json"),
            r#"{
  "name": "example/pkg",
  "version": "0.2.1",
  "source": ".src"
}
"#,
        )
        .expect("failed to write test moon.mod.json");
        std::fs::create_dir_all(dir.join(".src/.hidden"))
            .expect("failed to create test package directories");
        std::fs::write(dir.join(".src/moon.pkg.json"), "{}")
            .expect("failed to write root package manifest");
        std::fs::write(dir.join(".src/.hidden/moon.pkg.json"), "{}")
            .expect("failed to write hidden package manifest");

        let source: ModuleSource = "example/pkg@0.2.1"
            .parse()
            .expect("failed to parse test module source");
        let stub = MoonMod {
            name: "example/pkg".to_string(),
            version: Some(source.version().clone()),
            ..Default::default()
        };
        let (resolved_env, id) = ResolvedEnv::only_one_module(source, stub);
        let mut dirs = DirSyncResult::default();
        dirs.insert(id, dir.clone());

        let discovered =
            discover_packages(&resolved_env, &dirs, &UserLog::new(log::LevelFilter::Error))
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
        let manifest_path = dir.join("moon.pkg.json");
        std::fs::write(
            &manifest_path,
            r#"{
  "virtual": { "has-default": false },
  "pre-build": [{
    "input": "contract.txt",
    "output": "./pkg.mbti",
    "command": "generate"
  }]
}"#,
        )
        .expect("failed to write package manifest");
        std::fs::write(dir.join("virtual.mbti"), "legacy contract")
            .expect("failed to write legacy contract");

        let source: ModuleSource = "example/pkg@0.2.1"
            .parse()
            .expect("failed to parse test module source");
        let package = PackagePath::new_from_rel_path(RelativePath::new("virtual"))
            .expect("failed to create package path");
        let fqn = PackageFQN::new(source, package);
        let (pkg, _) = read_package_desc_file_from_path_with_supported_targets_decl(
            &manifest_path,
            &UserLog::new(log::LevelFilter::Error),
        )
        .expect("failed to read package manifest");

        assert_eq!(
            discover_virtual_mbti_files(&pkg, &fqn, &dir),
            [dir.join("virtual.mbti")]
        );

        std::fs::write(dir.join("pkg.mbti"), "canonical contract")
            .expect("failed to write canonical contract");
        assert_eq!(
            discover_virtual_mbti_files(&pkg, &fqn, &dir),
            [dir.join("pkg.mbti"), dir.join("virtual.mbti")]
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
            dir.join("nested-module/moon.mod.json"),
            r#"{"name":"nested/module"}"#,
        )?;
        std::fs::write(dir.join("nested-module/foreign.h"), "foreign")?;

        std::fs::create_dir_all(dir.join("nested-package"))?;
        std::fs::write(dir.join("nested-package/moon.pkg.json"), "{}")?;
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
