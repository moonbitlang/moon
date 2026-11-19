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

//! Shared filesystem scanning for formatting and build preparation.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

use ignore::{WalkBuilder, WalkState};
use moonutil::{
    constants::{MOON_MOD, MOON_MOD_JSON, MOON_PKG, MOON_PKG_JSON, is_ignored_directory_name},
    manifest::{
        ManifestFormat, MoonMod, preferred_manifest_in_dir,
        read_package_desc_file_from_path_with_supported_targets_decl, warn_known_shadowed_manifest,
    },
    module_name::ModuleName,
    package::{MoonPkg, SupportedTargetsDeclKind},
    resolution::ModuleSource,
    user_log::UserLog,
};
use relative_path::PathExt;

use super::DiscoverError;
use crate::pkg_name::{PackageFQN, PackagePath};

/// A package declaration and the files observed in its directory. It contains
/// no injected imports or derived build settings. Formatting consumes this
/// directly; build preparation consumes it to construct a DiscoveredPackage.
#[derive(Debug)]
pub(crate) struct PackageInput {
    pub root_path: PathBuf,
    pub fqn: PackageFQN,
    pub manifest_format: ManifestFormat,
    pub manifest: MoonPkg,
    pub supported_targets_decl: SupportedTargetsDeclKind,
    /// Immediate regular files and symlinks, sorted by path. Subdirectories are
    /// traversed separately. Source classification reuses this enumeration.
    pub files: Vec<PathBuf>,
}

pub(crate) fn warn_package_name(package: &PackageFQN, user_log: &UserLog) {
    // TODO: Remove this exception once core/v128 follows the major-version
    // package naming convention. It remains valid when building core itself.
    let core_v128 =
        *package.module().name() == ("moonbitlang", "core") && package.package().as_str() == "v128";
    if !core_v128
        && !package.module().name().username.is_empty()
        && !package.package().is_empty()
        && matches!(
            ModuleName::from(package.to_string().as_str()).major_version_suffix(),
            Ok(Some(_))
        )
    {
        user_log.warn(format!(
            "Package `{}` in module `{}` has the same import path as a major-version module. \
             Consider renaming package `{}` to avoid ambiguity.",
            package,
            package.module().name(),
            package.package(),
        ));
    }
}

#[derive(Default)]
struct Entries {
    packages: Vec<(usize, PackageInput)>,
    files: Vec<(usize, PathBuf)>,
    error: Option<DiscoverError>,
}

/// Publish one batch per worker, rather than synchronizing each visited entry.
struct WorkerEntries<'a> {
    entries: Entries,
    completed: &'a Mutex<Vec<Entries>>,
}

impl Drop for WorkerEntries<'_> {
    fn drop(&mut self) {
        self.completed
            .lock()
            .expect("worker result collection is not poisoned")
            .push(std::mem::take(&mut self.entries));
    }
}

/// Enumerate each directory once, then associate files with package declarations.
/// Serial and parallel walks share filtering, parsing, and result ordering.
#[tracing::instrument(level = "debug", skip_all, fields(workers))]
pub(super) fn scan_packages(
    modules: &[(&ModuleSource, &Path, &MoonMod, bool)],
    allow_legacy: bool,
    workers: usize,
    user_log: &UserLog,
) -> Result<Vec<Vec<PackageInput>>, DiscoverError> {
    let mut roots = HashMap::<PathBuf, Vec<usize>>::new();
    for (index, (source, dir, module, _)) in modules.iter().enumerate() {
        let root = std::fs::canonicalize(dir.join(module.source.as_deref().unwrap_or_default()))
            .map_err(|inner| DiscoverError::CantReadModulePackages {
                module: (*source).clone(),
                inner: inner.into(),
            })?;
        // Two module sources may point at the same directory. Walk it once,
        // but retain each source's identity and warning policy when parsing.
        roots.entry(root).or_default().push(index);
    }
    let mut paths = roots.keys();
    let Some(first) = paths.next() else {
        return Ok(Vec::new());
    };
    let mut walker = WalkBuilder::new(first);
    for path in paths {
        walker.add(path);
    }
    // Moon owns the exclusion rules; .ignore/.gitignore must not hide build
    // inputs. An explicit source root is visited even if its name is excluded.
    walker
        .threads(workers)
        .standard_filters(false)
        .follow_links(false)
        .filter_entry(|entry| {
            if entry.depth() == 0 || !entry.file_type().is_some_and(|kind| kind.is_dir()) {
                return true;
            }
            !is_ignored_directory_name(entry.file_name())
                && !entry.path().join(MOON_MOD).exists()
                && !entry.path().join(MOON_MOD_JSON).exists()
        });

    let span = tracing::Span::current();
    let dispatch = tracing::dispatcher::get_default(Clone::clone);
    let read_entry = |entry: ignore::DirEntry, collected: &mut Entries| {
        let path = entry.path();
        let root = path
            .ancestors()
            .nth(entry.depth())
            .expect("entry is under a scan root");
        let indexes = &roots[root];
        let kind = entry.file_type().expect("filesystem entry has a file type");
        if kind.is_dir() {
            tracing::dispatcher::with_default(&dispatch, || {
                let _entered = tracing::debug_span!(parent: &span, "scan_package_directory", path = %path.display()).entered();
                let relative = path.relative_to(root).expect("entry is under a scan root");
                let package = PackagePath::new_from_rel_path(&relative)
                    .expect("directory relative to module is a valid package path");
                for &index in indexes {
                    let (source, _, _, emit_warnings) = modules[index];
                    if let Some(input) = read_package_manifest(
                        source,
                        path,
                        package.clone(),
                        allow_legacy,
                        emit_warnings,
                        user_log,
                    )? {
                        collected.packages.push((index, input));
                    }
                }
                Ok::<_, DiscoverError>(())
            })?;
        } else if entry.depth() == 0 {
            return Err(DiscoverError::CantReadModulePackages {
                module: modules[indexes[0]].0.clone(),
                inner: anyhow::anyhow!(
                    "package source root `{}` is not a directory",
                    path.display()
                ),
            });
        } else if kind.is_file() || kind.is_symlink() {
            for &index in indexes {
                collected.files.push((index, path.to_owned()));
            }
        }
        Ok(())
    };

    let mut batches = if workers == 1 {
        // Formatting and -j1 stay on the calling thread.
        let mut collected = Entries::default();
        for entry in walker.build() {
            read_entry(entry?, &mut collected)?;
        }
        vec![collected]
    } else {
        let completed = Mutex::new(Vec::new());
        walker.build_parallel().run(|| {
            let mut collected = WorkerEntries {
                entries: Entries::default(),
                completed: &completed,
            };
            let read_entry = &read_entry;
            Box::new(move |entry| {
                let result = entry
                    .map_err(DiscoverError::from)
                    .and_then(|entry| read_entry(entry, &mut collected.entries));
                match result {
                    Ok(()) => WalkState::Continue,
                    Err(error) => {
                        collected.entries.error = Some(error);
                        WalkState::Quit
                    }
                }
            })
        });
        completed
            .into_inner()
            .expect("worker result collection is not poisoned")
    };
    let mut packages = (0..modules.len())
        .map(|_| HashMap::new())
        .collect::<Vec<_>>();
    for batch in &mut batches {
        if let Some(error) = batch.error.take() {
            return Err(error);
        }
        for (index, package) in batch.packages.drain(..) {
            packages[index].insert(package.root_path.clone(), package);
        }
    }
    // A directory's entries can be visited by different workers. Associate
    // files after joining, and discard files outside a declared package.
    for (index, path) in batches.into_iter().flat_map(|batch| batch.files) {
        if let Some(package) = packages[index].get_mut(path.parent().expect("entry has a parent")) {
            package.files.push(path);
        }
    }
    Ok(packages
        .into_iter()
        .map(|packages| {
            let mut packages = packages.into_values().collect::<Vec<_>>();
            packages.sort_unstable_by(|a, b| a.root_path.cmp(&b.root_path));
            for package in &mut packages {
                package.files.sort();
            }
            packages
        })
        .collect())
}

fn read_package_manifest(
    source: &ModuleSource,
    dir: &Path,
    package: PackagePath,
    allow_legacy: bool,
    emit_warnings: bool,
    user_log: &UserLog,
) -> Result<Option<PackageInput>, DiscoverError> {
    let Some((path, manifest_format)) = preferred_manifest_in_dir(dir, MOON_PKG, MOON_PKG_JSON)
    else {
        return Ok(None);
    };
    if emit_warnings
        && matches!(manifest_format, ManifestFormat::New)
        && dir.join(MOON_PKG_JSON).exists()
    {
        warn_known_shadowed_manifest(
            MOON_PKG_JSON,
            MOON_PKG,
            &format!("at package root '{}'", dir.display()),
            user_log,
        );
    }
    let (manifest, supported_targets_decl) = {
        let _span = tracing::debug_span!("read_package_manifest", path = %path.display()).entered();
        read_package_desc_file_from_path_with_supported_targets_decl(
            &path,
            user_log,
            allow_legacy,
            emit_warnings,
        )
        .map_err(|inner| DiscoverError::CantReadPackageFile {
            module: source.clone(),
            package: package.clone(),
            path: dir.to_owned(),
            inner,
        })?
    };
    Ok(Some(PackageInput {
        root_path: dir.to_owned(),
        fqn: PackageFQN::new(source.clone(), package),
        manifest_format,
        manifest,
        supported_targets_decl,
        files: Vec::new(),
    }))
}
