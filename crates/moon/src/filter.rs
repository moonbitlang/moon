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

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use moonbuild_rupes_recta::{ResolveOutput, model::PackageId};
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

/// Resolve package identifiers (or other entities) by name using exact or fuzzy matching.
///
/// Returns a list of matches ordered by relevance, without duplicates. The `name_map`
/// should contain the full package names as keys and the desired return value (e.g.
/// `PackageId`, `String`) as values.
pub fn fuzzy_match_by_name<T>(needle: &str, name_map: &HashMap<String, T>) -> SmallVec<[T; 1]>
where
    T: Clone + PartialEq,
{
    if let Some(value) = name_map.get(needle) {
        let mut out = SmallVec::new();
        out.push(value.clone());
        return out;
    }

    let mut result = SmallVec::new();

    if let Some(matches) =
        moonutil::fuzzy_match::fuzzy_match(needle, name_map.keys().map(|k| k.as_str()))
    {
        for m in matches {
            if let Some(value) = name_map.get(m.as_str())
                && result.iter().all(|existing| existing != value)
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

    let packages = resolve_output
        .pkg_dirs
        .packages_for_module(main_module_id)
        .expect("Cannot find the local module!");

    let name_map: HashMap<String, PackageId> = packages
        .values()
        .map(|&pkg_id| {
            let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
            (pkg.fqn.to_string(), pkg_id)
        })
        .collect();

    fuzzy_match_by_name(needle, &name_map)
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
