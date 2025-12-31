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

//! Path filtering operations
//!
//! This module contains the common path filtering logic for both legacy and RR backends.

use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};

use anyhow::Context;
use moonbuild_rupes_recta::{ResolveOutput, fmt::FmtResolveOutput, model::PackageId};
use moonutil::mooncakes::{DirSyncResult, result::ResolvedEnv};
use smallvec::SmallVec;

/// Canonicalize the given path, returning the directory it's referencing, and
/// an optional filename if the path is a file.
pub fn canonicalize_with_filename(path: &Path) -> anyhow::Result<(PathBuf, Option<String>)> {
    let input_path = dunce::canonicalize(path).with_context(|| {
        format!(
            "Failed to canonicalize input filter directory `{}`",
            path.display()
        )
    })?;
    if input_path.is_dir() {
        Ok((input_path, None))
    } else {
        let filename = input_path
            .file_name()
            .with_context(|| {
                format!(
                    "Failed to get filename from input filter path `{}`",
                    input_path.display()
                )
            })?
            .to_str()
            .with_context(|| {
                format!(
                    "Input filename is not valid UTF-8: {}",
                    input_path.display()
                )
            })?
            .to_owned();

        let mut parent = input_path;
        parent.pop();

        Ok((parent.to_path_buf(), Some(filename)))
    }
}

trait AsNameMap<T> {
    fn all_names(&self) -> impl Iterator<Item = impl AsRef<str>>;
    fn get(&self, name: &str) -> Option<T>;
}

/// Resolve package identifiers (or other entities) by name using exact or fuzzy matching.
///
/// Returns a list of matches ordered by relevance, without duplicates. The `name_map`
/// should contain the full package names as keys and the desired return value (e.g.
/// `PackageId`, `String`) as values.
fn fuzzy_match_by_name<T>(needle: &str, name_map: &impl AsNameMap<T>) -> SmallVec<[T; 1]>
where
    T: Clone + PartialEq,
{
    if let Some(value) = name_map.get(needle) {
        let mut out = SmallVec::new();
        out.push(value.clone());
        return out;
    }

    let mut result = SmallVec::new();

    if let Some(matches) = moonutil::fuzzy_match::fuzzy_match(needle, name_map.all_names()) {
        for m in matches {
            if let Some(value) = name_map.get(m.as_str())
                && result.iter().all(|existing| *existing != value)
            {
                result.push(value.clone());
            }
        }
    }

    result
}

/// Perform fuzzy matching over package names and return the matching package IDs.
pub fn match_packages_by_name_rr(
    resolve_output: &ResolveOutput,
    main_modules: &[moonutil::mooncakes::ModuleId],
    needle: &str,
) -> SmallVec<[PackageId; 1]> {
    let &[main_module_id] = main_modules else {
        panic!("No multiple main modules are supported");
    };

    let res = fuzzy_match_by_name(needle, &resolve_output.pkg_dirs);

    // Warn about non-local packages being matched
    for &pkg_id in &res {
        let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
        if pkg.module != main_module_id {
            log::warn!(
                "Package '{}' matched by name '{}' is not in the main module, it may not be accessible.",
                pkg.fqn,
                needle
            );
        }
    }
    res
}

impl AsNameMap<PackageId> for moonbuild_rupes_recta::discover::DiscoverResult {
    fn all_names(&self) -> impl Iterator<Item = impl AsRef<str>> {
        self.all_packages(true).map(|(_, pkg)| pkg.fqn.to_string())
    }

    fn get(&self, name: &str) -> Option<PackageId> {
        self.get_package_id_by_name(name)
    }
}

/// From a canonicalized, directory path, find the corresponding package ID.
///
/// When a package cannot be found, returns a descriptive error that can be
/// reported to the user.
pub fn filter_pkg_by_dir(resolve_output: &ResolveOutput, dir: &Path) -> anyhow::Result<PackageId> {
    let mut all_local_packages = resolve_output.local_modules().iter().flat_map(|&it| {
        resolve_output
            .pkg_dirs
            .packages_for_module(it)
            .unwrap()
            .values()
            .cloned()
    });

    all_local_packages
        .find(|&pkg_id| {
            let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
            pkg.root_path == dir
        })
        .ok_or_else(|| {
            report_package_not_found(
                dir,
                &resolve_output.module_rel,
                &resolve_output.module_dirs,
                resolve_output.local_modules(),
            )
        })
}

/// Given an invalid input path, report a helpful error message indicating why
/// no package could be found.
pub fn report_package_not_found(
    input_path: &Path,
    module_graph: &ResolvedEnv,
    module_dirs: &DirSyncResult,
    main_modules: &[moonutil::mooncakes::ModuleId],
) -> anyhow::Error {
    let m_def_and_dir = |id| {
        let module_dir = module_dirs.get(id).expect("Module should exist");
        let m_def = &**module_graph.module_info(id);
        (m_def, module_dir)
    };

    // Whether if the path is in a module's package directory
    let mut inside_module_pkgs = None;
    // Whether if the path is in a module's root directory
    let mut inside_module_root = None;

    // Find the most plausible module that the path belongs to
    for &mid in main_modules {
        let (m_def, module_dir) = m_def_and_dir(mid);
        let m_packages_root = module_dir.join(m_def.source.as_deref().unwrap_or(""));
        if input_path.starts_with(m_packages_root) {
            inside_module_pkgs = Some(mid);
            break;
        } else if input_path.starts_with(module_dir) {
            inside_module_root = Some(mid);
            break;
        }
    }

    // Report the hint on why it might not work
    let hint = if let Some(mid) = inside_module_pkgs {
        let (m_def, module_dir) = m_def_and_dir(mid);
        let m_packages_root = module_dir.join(m_def.source.as_deref().unwrap_or(""));

        format!(
            "The provided path `{}` is inside the packages directory of module `{}` at `{}`,
            but does not match any known package.",
            input_path.display(),
            m_def.name,
            m_packages_root.display()
        )
    } else if let Some(mid) = inside_module_root {
        let (m_def, module_dir) = m_def_and_dir(mid);

        format!(
            "The provided path `{}` is inside the root directory of module `{}` at `{}`, \
            but it is not in the path for searching packages, \
            thus it cannot be resolved to any known package.",
            input_path.display(),
            m_def.name,
            module_dir.display()
        )
    } else {
        format!(
            "The provided path `{}` is not inside any known module.",
            input_path.display()
        )
    };

    anyhow::anyhow!(
        "Cannot find package to build based on input path `{}`.\nHint: {}",
        input_path.display(),
        hint
    )
}

#[derive(Debug, Default)]
pub struct PackageMatchResult {
    pub matched: Vec<PackageId>,
    pub missing: Vec<String>,
}

/// Match package names with exact or fuzzy lookup, returning the corresponding package IDs.
///
/// Candidates are provided as a list of package IDs that belong to the current module. Names are
/// matched by their fully qualified names, preferring exact matches and falling back to fuzzy
/// suggestions. Results are deduplicated while preserving the order returned by the matcher.
pub fn match_packages_with_fuzzy<I, S>(
    resolve_output: &ResolveOutput,
    candidates: impl IntoIterator<Item = PackageId>,
    names: I,
) -> PackageMatchResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut name_map = BTreeMap::new();
    for pkg_id in candidates {
        let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
        name_map.insert(pkg.fqn.to_string(), pkg_id);
    }

    let mut matched = Vec::new();
    let mut missing = Vec::new();
    let mut seen = HashSet::new();

    for name in names.into_iter() {
        let needle = name.as_ref();
        if let Some(&pkg_id) = name_map.get(needle) {
            if seen.insert(pkg_id) {
                matched.push(pkg_id);
            }
            continue;
        }

        let haystack = name_map.keys().map(|k| k.as_str());
        match moonutil::fuzzy_match::fuzzy_match(needle, haystack) {
            Some(candidates) => {
                let mut found = false;
                for candidate in candidates {
                    if let Some(&pkg_id) = name_map.get(&candidate) {
                        if seen.insert(pkg_id) {
                            matched.push(pkg_id);
                        }
                        found = true;
                    }
                }
                if !found {
                    missing.push(needle.to_owned());
                }
            }
            None => missing.push(needle.to_owned()),
        }
    }

    PackageMatchResult { matched, missing }
}

/// From a canonicalized directory path, find the corresponding package ID in `FmtResolveOutput`.
///
/// This is a simpler version of `filter_pkg_by_dir` for the formatter case, which doesn't
/// have the full `ResolveOutput` available.
pub fn filter_pkg_by_dir_for_fmt(
    resolved: &FmtResolveOutput,
    dir: &Path,
) -> anyhow::Result<PackageId> {
    let all_packages = resolved
        .pkg_dirs
        .packages_for_module(resolved.main_module_id)
        .expect("Main module should have packages");

    all_packages
        .values()
        .find(|&&pkg_id| {
            let pkg = resolved.pkg_dirs.get_package(pkg_id);
            pkg.root_path == dir
        })
        .copied()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot find package to format at path `{}`.\n\
                 Hint: Make sure the path points to a package directory containing moon.pkg.json.",
                dir.display()
            )
        })
}
