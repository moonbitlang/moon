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

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use anyhow::Context;
use moonutil::common::read_module_desc_file_in_dir;
use moonutil::moon_dir;
use moonutil::mooncakes::ModuleId;
use moonutil::mooncakes::result::{ResolvedEnv, ResolvedRootModules};
use moonutil::mooncakes::{ModuleName, ModuleSource, result};
use semver::{Version, VersionReq};
use thiserror::Error;

use crate::registry::Registry;

pub(crate) mod env;
pub(crate) mod mvs;

pub(crate) use mvs::MvsSolver;

use self::env::ResolverEnv;

/// Any error that may occur during dependency resolution.
#[derive(Debug, Error)]
pub(crate) enum ResolverError {
    #[error(
        "Failed to resolve registry dependency `{dependency}` for module `{dependant}`: module was not found in the registry"
    )]
    ModuleMissing {
        dependency: ModuleName,
        dependant: ModuleName,
    },
    #[error(
        "Failed to resolve registry dependency `{dependency}` for module `{dependant}`: no version satisfies requirement `{required}`"
    )]
    NoSatisfiedVersion {
        dependency: ModuleName,
        dependant: ModuleName,
        required: VersionReq,
    },
    #[error(
        "Failed to resolve local dependency `{dependency}` for module `{dependant}`: local module version `{actual}` does not satisfy requirement `{required}`"
    )]
    LocalDepVersionMismatch {
        dependant: ModuleName,
        dependency: ModuleName,
        actual: Version,
        required: VersionReq,
    },
    #[error("{}", format_version_conflict(.module, .conflicts))]
    ConflictingVersions {
        module: ModuleName,
        conflicts: Vec<VersionConflict>,
    },
    #[error("Cannot inject the standard library `moonbitlang/core`: {0}")]
    CannotInjectCore(#[source] anyhow::Error),
    #[error("Error during resolution: {0}")]
    Other(#[source] anyhow::Error),
}

#[derive(Debug)]
pub(crate) struct VersionConflict {
    pub selected: ModuleSource,
    pub chain: Option<Vec<ModuleSource>>,
}

#[derive(Debug, Error)]
#[error("{}", format_resolver_errors(.0))]
pub(crate) struct ResolverErrors(pub(crate) Vec<ResolverError>);

/// The dependency resolver.
pub(crate) trait Resolver {
    /// Resolves the dependencies of a package using the given environment. The
    /// function should write its results on `res`, which may be initialized
    /// with other existing data earlier.
    ///
    /// If the dependencies cannot be resolved, this function should return
    /// `false`. The errors should be emitted in `env`.
    fn resolve(&mut self, env: &mut ResolverEnv, res: &mut ResolvedEnv) -> bool;
}

/// Goes through the resolved environment and checks for any duplicate module names.
///
/// Since the build system is not yet able to handle multiple versions of the same module,
/// this function will return an error if any duplicate module names with different versions
/// (implying incompatible versions of the same module are resolved) are found.
fn assert_no_duplicate_module_names(result: &result::ResolvedEnv) -> Result<(), ResolverErrors> {
    let mut module_name_versions: HashMap<_, Vec<_>> = HashMap::new();
    for (id, it) in result.all_modules_and_id() {
        module_name_versions
            .entry(it.name().clone())
            .or_default()
            .push((id, it.clone()));
    }
    let mut errs = vec![];
    for (name, versions) in module_name_versions {
        if versions.len() > 1 {
            let err = ResolverError::ConflictingVersions {
                module: name.clone(),
                conflicts: collect_version_conflicts(&versions, result),
            };
            errs.push(err);
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(ResolverErrors(errs))
    }
}

fn collect_version_conflicts(
    versions: &[(ModuleId, ModuleSource)],
    result: &ResolvedEnv,
) -> Vec<VersionConflict> {
    let mut versions = versions.to_vec();
    versions.sort_by(|a, b| {
        a.1.version()
            .cmp(b.1.version())
            .then_with(|| a.1.source().cmp(b.1.source()))
    });

    versions
        .into_iter()
        .map(|(id, source)| VersionConflict {
            selected: source,
            chain: describe_dependency_chain(result, id),
        })
        .collect()
}

fn describe_dependency_chain(result: &ResolvedEnv, target: ModuleId) -> Option<Vec<ModuleSource>> {
    let mut queue = VecDeque::new();
    let mut prev = HashMap::<ModuleId, ModuleId>::new();

    for &root in result.input_module_ids() {
        queue.push_back(root);
        prev.insert(root, root);
    }

    while let Some(current) = queue.pop_front() {
        if current == target {
            break;
        }

        for dep in result.deps(current) {
            if prev.contains_key(&dep) {
                continue;
            }
            prev.insert(dep, current);
            queue.push_back(dep);
        }
    }

    if !prev.contains_key(&target) {
        return None;
    }

    let mut path = vec![target];
    let mut current = target;
    while let Some(parent) = prev.get(&current).copied() {
        if parent == current {
            break;
        }
        path.push(parent);
        current = parent;
    }
    path.reverse();

    Some(
        path.into_iter()
            .map(|id| result.module_source(id).clone())
            .collect(),
    )
}

fn format_version_conflict(module: &ModuleName, conflicts: &[VersionConflict]) -> String {
    let version_list = conflicts
        .iter()
        .map(|conflict| conflict.selected.version().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let mut lines = vec![format!(
        "Multiple conflicting versions were found for module `{}`: {}",
        module, version_list
    )];

    for conflict in conflicts {
        match &conflict.chain {
            Some(chain) => lines.push(format!(
                "  - `{}` is selected via {}",
                conflict.selected,
                chain
                    .iter()
                    .map(|source| format!("`{}`", source))
                    .collect::<Vec<_>>()
                    .join(" -> ")
            )),
            None => lines.push(format!(
                "  - `{}` was selected during resolution",
                conflict.selected
            )),
        }
    }

    lines.join("\n")
}

fn format_resolver_errors(errors: &[ResolverError]) -> String {
    errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) struct ResolveConfig {
    pub(crate) registry: Box<dyn Registry>,
    pub(crate) inject_std: bool,
}

pub(crate) fn resolve_with_default_env(
    config: &ResolveConfig,
    resolver: &mut dyn Resolver,
    root: ResolvedRootModules,
) -> Result<result::ResolvedEnv, ResolverErrors> {
    let mut env = env::ResolverEnv::new(config.registry.as_ref());
    let mut res = ResolvedEnv::from_root_modules(root);

    if config.inject_std {
        inject_std(&mut res)
            .map_err(|e| ResolverErrors(vec![ResolverError::CannotInjectCore(e)]))?;
    }

    let status = resolver.resolve(&mut env, &mut res);
    if env.any_errors() {
        Err(ResolverErrors(env.into_errors()))
    } else {
        if !status {
            panic!("The resolver should not return `false` when no errors are found");
        }
        assert_no_duplicate_module_names(&res)?;
        Ok(res)
    }
}

/// Inject the definition of `moonbitlang/core` in the installation directory
/// to the resolve graph, and mark it as the standard library.
fn inject_std(res: &mut ResolvedEnv) -> anyhow::Result<()> {
    let core_dir = moon_dir::core();
    let loaded_core =
        read_module_desc_file_in_dir(&core_dir).context("Cannot load the core file")?;
    let source = ModuleSource::from_stdlib(&loaded_core, &core_dir);
    let id = res.add_module(source, Arc::new(loaded_core));
    res.register_stdlib(id);

    Ok(())
}

pub(crate) fn resolve_with_default_env_and_resolver(
    config: &ResolveConfig,
    root: ResolvedRootModules,
) -> Result<result::ResolvedEnv, ResolverErrors> {
    let mut resolver = MvsSolver;
    resolve_with_default_env(config, &mut resolver, root)
}
