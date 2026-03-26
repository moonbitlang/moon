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
use moonutil::common::{MOON_PKG, MOON_PKG_JSON, TargetBackend, is_moon_pkg_exist};
use moonutil::mooncakes::{DirSyncResult, result::ResolvedEnv};

use crate::user_diagnostics::UserDiagnostics;

/// Canonicalize the given path, returning the directory it's referencing, and
/// an optional filename if the path is a file.
pub(crate) fn canonicalize_with_filename(path: &Path) -> anyhow::Result<(PathBuf, Option<String>)> {
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

pub(crate) fn select_packages<I, F>(
    paths: I,
    output: UserDiagnostics,
    mut filter_pkg_by_dir: F,
) -> anyhow::Result<Vec<(PathBuf, PackageId)>>
where
    I: IntoIterator,
    I::Item: AsRef<Path>,
    F: FnMut(&Path) -> anyhow::Result<PackageId>,
{
    let mut selected = Vec::new();
    let mut seen = HashSet::new();

    for path in paths {
        let path = path.as_ref();
        let (dir, _) = canonicalize_with_filename(path)?;
        let Ok(pkg_id) = filter_pkg_by_dir(&dir) else {
            output.info(format!(
                "skipping path `{}` because it is not a package in the current work context.",
                path.display()
            ));
            continue;
        };
        if seen.insert(pkg_id) {
            selected.push((path.to_path_buf(), pkg_id));
        }
    }

    Ok(selected)
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
fn fuzzy_match_by_name<T>(needle: &str, name_map: &impl AsNameMap<T>) -> Vec<T>
where
    T: Clone + PartialEq,
{
    if let Some(value) = name_map.get(needle) {
        return vec![value.clone()];
    }

    let mut result = Vec::new();

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
pub(crate) fn match_packages_by_name_rr(
    resolve_output: &ResolveOutput,
    main_modules: &[moonutil::mooncakes::ModuleId],
    needle: &str,
    output: UserDiagnostics,
) -> Vec<PackageId> {
    let &[main_module_id] = main_modules else {
        panic!("No multiple main modules are supported");
    };

    let res = fuzzy_match_by_name(needle, &resolve_output.pkg_dirs);

    // Warn about non-local packages being matched
    for &pkg_id in &res {
        let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
        if pkg.module != main_module_id {
            output.warn(format!(
                "Package '{}' matched by name '{}' is not in the main module, it may not be accessible.",
                pkg.fqn,
                needle
            ));
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
pub(crate) fn filter_pkg_by_dir(
    resolve_output: &ResolveOutput,
    dir: &Path,
) -> anyhow::Result<PackageId> {
    let mut all_local_packages = resolve_output.local_modules().iter().flat_map(|&it| {
        resolve_output
            .pkg_dirs
            .packages_for_module(it)
            .into_iter()
            .flat_map(|packages| packages.values().cloned())
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
pub(crate) fn report_package_not_found(
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
    let missing_pkg_desc = !is_moon_pkg_exist(input_path);

    let hint = if let Some(mid) = inside_module_pkgs.or(inside_module_root) {
        let (m_def, module_dir) = m_def_and_dir(mid);
        let m_packages_root = module_dir.join(m_def.source.as_deref().unwrap_or(""));

        if missing_pkg_desc {
            let (scope, scope_path) = if inside_module_pkgs == Some(mid) {
                ("packages directory", m_packages_root.as_path())
            } else {
                ("root directory", module_dir.as_path())
            };

            format!(
                "The provided path `{}` is inside the {} of module `{}` at `{}`, \
                but the directory itself does not contain `{}` or `{}`, so it is not a package.",
                input_path.display(),
                scope,
                m_def.name,
                scope_path.display(),
                MOON_PKG,
                MOON_PKG_JSON,
            )
        } else if inside_module_pkgs == Some(mid) {
            format!(
                "The provided path `{}` is inside the packages directory of module `{}` at `{}`,
                but does not match any known package.",
                input_path.display(),
                m_def.name,
                m_packages_root.display()
            )
        } else {
            format!(
                "The provided path `{}` is inside the root directory of module `{}` at `{}`, \
                but it is not in the path for searching packages, \
                thus it cannot be resolved to any known package.",
                input_path.display(),
                m_def.name,
                module_dir.display()
            )
        }
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

pub(crate) fn format_supported_backends(
    resolve_output: &ResolveOutput,
    pkg_id: PackageId,
) -> String {
    let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
    let mut targets = pkg
        .effective_supported_targets
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    targets.sort();
    format!("[{}]", targets.join(", "))
}

pub(crate) fn package_supports_backend(
    resolve_output: &ResolveOutput,
    pkg_id: PackageId,
    target_backend: TargetBackend,
) -> bool {
    resolve_output
        .pkg_dirs
        .get_package(pkg_id)
        .effective_supported_targets
        .contains(&target_backend)
}

pub(crate) fn ensure_package_supports_backend(
    resolve_output: &ResolveOutput,
    pkg_id: PackageId,
    target_backend: TargetBackend,
) -> anyhow::Result<()> {
    if package_supports_backend(resolve_output, pkg_id, target_backend) {
        return Ok(());
    }

    let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
    anyhow::bail!(
        "Package '{}' does not support target backend '{}'. Supported backends: {}",
        pkg.fqn,
        target_backend,
        format_supported_backends(resolve_output, pkg_id),
    );
}

pub(crate) fn ensure_packages_support_backend<I>(
    resolve_output: &ResolveOutput,
    packages: I,
    target_backend: TargetBackend,
) -> anyhow::Result<()>
where
    I: IntoIterator<Item = PackageId>,
{
    let mut unsupported = Vec::new();

    for pkg_id in packages {
        if !package_supports_backend(resolve_output, pkg_id, target_backend) {
            unsupported.push(pkg_id);
        }
    }

    if unsupported.is_empty() {
        return Ok(());
    }

    let details = unsupported
        .iter()
        .map(|&pkg_id| {
            let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
            format!(
                "{} ({})",
                pkg.fqn,
                format_supported_backends(resolve_output, pkg_id)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    anyhow::bail!(
        "Selected package(s) do not support target backend '{}': {}",
        target_backend,
        details
    );
}

pub(crate) fn select_supported_packages<I>(
    resolve_output: &ResolveOutput,
    paths: I,
    target_backend: TargetBackend,
    output: UserDiagnostics,
) -> anyhow::Result<Vec<PackageId>>
where
    I: IntoIterator,
    I::Item: AsRef<Path>,
{
    let mut selected = Vec::new();
    let mut unsupported = Vec::new();

    for (path, pkg_id) in
        select_packages(paths, output, |dir| filter_pkg_by_dir(resolve_output, dir))?
    {
        if package_supports_backend(resolve_output, pkg_id, target_backend) {
            selected.push(pkg_id);
        } else {
            unsupported.push((path.to_path_buf(), pkg_id));
        }
    }

    if selected.is_empty() && !unsupported.is_empty() {
        if let [(_, pkg_id)] = unsupported.as_slice() {
            ensure_package_supports_backend(resolve_output, *pkg_id, target_backend)?;
        } else {
            ensure_packages_support_backend(
                resolve_output,
                unsupported.iter().map(|(_, pkg_id)| *pkg_id),
                target_backend,
            )?;
        }
    }

    for (path, pkg_id) in &unsupported {
        let pkg = resolve_output.pkg_dirs.get_package(*pkg_id);
        output.info(format!(
            "skipping path `{}` because package `{}` does not support target backend `{}`. Supported backends: {}",
            path.display(),
            pkg.fqn,
            target_backend,
            format_supported_backends(resolve_output, *pkg_id),
        ));
    }

    Ok(selected)
}

#[derive(Debug)]
pub(crate) struct PackageMatchResult {
    pub matched: Vec<PackageId>,
    pub missing: Vec<String>,
}

/// Match package names with exact or fuzzy lookup, returning the corresponding package IDs.
///
/// Candidates are provided as a list of package IDs that belong to the current module. Names are
/// matched by their fully qualified names, preferring exact matches and falling back to fuzzy
/// suggestions. Results are deduplicated while preserving the order returned by the matcher.
pub(crate) fn match_packages_with_fuzzy<I, S>(
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
pub(crate) fn filter_pkg_by_dir_for_fmt(
    resolved: &FmtResolveOutput,
    dir: &Path,
) -> anyhow::Result<PackageId> {
    resolved
        .root_module_ids
        .iter()
        .flat_map(|&module_id| {
            resolved
                .pkg_dirs
                .packages_for_module(module_id)
                .into_iter()
                .flat_map(|packages| packages.values().copied())
        })
        .find(|&pkg_id| {
            let pkg = resolved.pkg_dirs.get_package(pkg_id);
            pkg.root_path == dir
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot find package to format at path `{}`.\n\
                 Hint: Make sure the path points to a package directory containing `{}`.",
                dir.display(),
                MOON_PKG
            )
        })
}

#[cfg(test)]
mod tests {
    use super::select_supported_packages;
    use crate::user_diagnostics::UserDiagnostics;
    use moonbuild_rupes_recta::ResolveConfig;
    use moonutil::common::{MOON_MOD_JSON, MOON_PKG_JSON, MOON_WORK, TargetBackend};
    use std::path::{Path, PathBuf};

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn canonical(path: impl AsRef<Path>) -> PathBuf {
        dunce::canonicalize(path).unwrap()
    }

    fn resolve_output(source_dir: &Path) -> moonbuild_rupes_recta::ResolveOutput {
        let cfg = ResolveConfig::new_with_load_defaults(false, false, false);
        moonbuild_rupes_recta::resolve(&cfg, source_dir).unwrap()
    }

    #[test]
    fn select_supported_packages_skips_dangling_pkg_under_workspace_root() {
        let temp = tempfile::tempdir().unwrap();
        let workspace_root = temp.path().join("workspace");
        std::fs::create_dir_all(&workspace_root).unwrap();
        write_file(
            &workspace_root.join(MOON_WORK),
            "members = [\n  \"./app\",\n]\n",
        );
        write_file(
            &workspace_root.join("app").join(MOON_MOD_JSON),
            "{ \"name\": \"workspace/app\", \"version\": \"0.1.0\" }",
        );
        write_file(
            &workspace_root.join("dangling/pkg").join(MOON_PKG_JSON),
            "{ \"import\": [] }",
        );

        let workspace_root = canonical(workspace_root);
        let dangling_pkg = workspace_root.join("dangling/pkg");
        let resolved = resolve_output(&workspace_root);

        assert_eq!(
            select_supported_packages(
                &resolved,
                [&dangling_pkg],
                TargetBackend::default(),
                UserDiagnostics::default(),
            )
            .unwrap(),
            vec![]
        );
    }

    #[test]
    fn select_supported_packages_accepts_external_workspace_member() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let workspace_root = root.join("workspace");
        let external_module = root.join("external/app");
        std::fs::create_dir_all(&workspace_root).unwrap();
        std::fs::create_dir_all(&external_module).unwrap();

        write_file(
            &workspace_root.join(MOON_WORK),
            "members = [\n  \"../external/app\",\n]\n",
        );
        write_file(
            &external_module.join(MOON_MOD_JSON),
            "{ \"name\": \"external/app\", \"version\": \"0.1.0\" }",
        );
        write_file(
            &external_module.join("src/main").join(MOON_PKG_JSON),
            "{ \"is-main\": true }",
        );

        let workspace_root = canonical(workspace_root);
        let external_pkg = canonical(external_module.join("src/main"));
        let resolved = resolve_output(&workspace_root);
        let selected = select_supported_packages(
            &resolved,
            [&external_pkg],
            TargetBackend::default(),
            UserDiagnostics::default(),
        )
        .unwrap();

        assert_eq!(selected.len(), 1);
        assert_eq!(
            resolved.pkg_dirs.get_package(selected[0]).root_path,
            external_pkg
        );
    }
}
