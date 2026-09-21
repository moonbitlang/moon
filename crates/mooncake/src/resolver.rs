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

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use anyhow::Context;
use moonutil::manifest::read_module_desc_file_in_dir;
use moonutil::resolution::{
    ModuleDependencyGraph, ModuleId, ModuleName, ModuleSource, ModuleSourceKind,
    ResolvedRootModules,
};
use moonutil::toolchain;
use moonutil::user_log::UserLog;
use semver::Version;
use thiserror::Error;

use crate::registry::Registry;

pub(crate) mod context;
pub(crate) mod mvs;

pub(crate) use mvs::MvsSolver;

use self::context::ModuleResolutionContext;

/// A failure to select module sources, versions, or dependency relationships.
#[derive(Debug, Error)]
pub(crate) enum ModuleResolutionError {
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
        required: Version,
    },
    #[error(
        "Failed to resolve local dependency `{dependency}` for module `{dependant}`: local module version `{actual}` does not satisfy requirement `{required}`"
    )]
    LocalDepVersionMismatch {
        dependant: ModuleName,
        dependency: ModuleName,
        actual: Version,
        required: Version,
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
#[error("{}", format_module_resolution_errors(.0))]
pub(crate) struct ModuleResolutionErrors(pub(crate) Vec<ModuleResolutionError>);

/// The module dependency resolver.
pub(crate) trait ModuleResolver {
    /// Select module sources and versions, extending `module_graph` from its
    /// existing root modules with their dependencies.
    ///
    /// If the dependencies cannot be resolved, this function should return
    /// `false`. The errors should be emitted in `context`.
    fn resolve(
        &mut self,
        context: &mut ModuleResolutionContext,
        module_graph: &mut ModuleDependencyGraph,
        user_log: &UserLog,
    ) -> bool;
}

/// Check the module dependency graph for duplicate module names.
///
/// Since the build system is not yet able to handle multiple versions of the same module,
/// this function will return an error if any duplicate module names with different versions
/// (implying incompatible versions of the same module are resolved) are found.
fn assert_no_duplicate_module_names(
    module_graph: &ModuleDependencyGraph,
) -> Result<(), ModuleResolutionErrors> {
    let mut module_name_versions: HashMap<_, Vec<_>> = HashMap::new();
    for (id, it) in module_graph.all_modules_and_id() {
        module_name_versions
            .entry(it.name().clone())
            .or_default()
            .push((id, it.clone()));
    }
    let mut errs = vec![];
    for (name, versions) in module_name_versions {
        if versions.len() > 1 {
            let err = ModuleResolutionError::ConflictingVersions {
                module: name.clone(),
                conflicts: collect_version_conflicts(&versions, module_graph),
            };
            errs.push(err);
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(ModuleResolutionErrors(errs))
    }
}

fn collect_version_conflicts(
    versions: &[(ModuleId, ModuleSource)],
    module_graph: &ModuleDependencyGraph,
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
            chain: describe_dependency_chain(module_graph, id),
        })
        .collect()
}

fn describe_dependency_chain(
    module_graph: &ModuleDependencyGraph,
    target: ModuleId,
) -> Option<Vec<ModuleSource>> {
    let mut queue = VecDeque::new();
    let mut prev = HashMap::<ModuleId, ModuleId>::new();

    for &root in module_graph.input_module_ids() {
        queue.push_back(root);
        prev.insert(root, root);
    }

    while let Some(current) = queue.pop_front() {
        if current == target {
            break;
        }

        for dep in module_graph.deps(current) {
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
            .map(|id| module_graph.module_source(id).clone())
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

fn format_module_resolution_errors(errors: &[ModuleResolutionError]) -> String {
    errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Registry and standard-library policy for selecting module dependencies.
pub(crate) struct ModuleResolutionConfig<'a> {
    pub(crate) registry: &'a dyn Registry,
    pub(crate) inject_std: bool,
}

pub(crate) fn resolve_modules_with_solver(
    config: &ModuleResolutionConfig,
    resolver: &mut dyn ModuleResolver,
    root: ResolvedRootModules,
    user_log: &UserLog,
) -> Result<ModuleDependencyGraph, ModuleResolutionErrors> {
    let mut context = ModuleResolutionContext::new(config.registry);
    let mut module_graph = ModuleDependencyGraph::from_root_modules(root);

    if config.inject_std {
        inject_std(&mut module_graph).map_err(|e| {
            ModuleResolutionErrors(vec![ModuleResolutionError::CannotInjectCore(e)])
        })?;
    }

    let status = resolver.resolve(&mut context, &mut module_graph, user_log);
    if context.any_errors() {
        Err(ModuleResolutionErrors(context.into_errors()))
    } else {
        if !status {
            panic!("The resolver should not return `false` when no errors are found");
        }
        assert_no_duplicate_module_names(&module_graph)?;
        warn_deprecated_dependencies(&module_graph, config.registry, user_log);
        Ok(module_graph)
    }
}

fn warn_deprecated_dependencies(
    module_graph: &ModuleDependencyGraph,
    registry: &dyn Registry,
    user_log: &UserLog,
) {
    if !user_log.is_enabled(log::Level::Warn) {
        return;
    }

    // Only inspect the final graph: MVS can visit versions and dependencies that
    // it later discards. Each selected module appears once, even in a diamond.
    let mut modules = module_graph.all_modules_and_id().collect::<Vec<_>>();
    modules.sort_unstable_by(|(_, a), (_, b)| a.name().cmp(b.name()));
    for (id, source) in modules {
        if !matches!(source.source(), ModuleSourceKind::Registry) {
            continue;
        }
        // RegistryClient reuses the index metadata already read by resolution.
        let Ok(versions) = registry.all_versions_of(source.name()) else {
            continue;
        };
        let Some(release) = versions
            .get(source.version())
            .filter(|release| release.yanked)
        else {
            continue;
        };
        // Registry-authored reasons may contain terminal control sequences.
        // Strip those while preserving ordinary newlines and spacing.
        let reason = anstream::adapter::strip_str(release.yanked_reason.as_deref().unwrap_or(""))
            .to_string();
        let mut warning = if reason.trim().is_empty() {
            format!("Dependency `{source}` is deprecated.")
        } else {
            format!("Dependency `{source}` is deprecated: {reason}")
        };
        if let Some(chain) = describe_dependency_chain(module_graph, id)
            && chain.len() > 2
        {
            let path = chain
                .iter()
                .map(|module| format!("{}@{}", module.name(), module.version()))
                .collect::<Vec<_>>()
                .join(" -> ");
            warning.push_str(&format!("\n  required through {path}"));
        }
        user_log.warn(warning);
    }
}

/// Inject the definition of `moonbitlang/core` in the installation directory
/// to the resolve graph, and mark it as the standard library.
fn inject_std(module_graph: &mut ModuleDependencyGraph) -> anyhow::Result<()> {
    let core_dir = toolchain::core();
    let loaded_core =
        read_module_desc_file_in_dir(&core_dir).context("Cannot load the core file")?;
    let source = ModuleSource::from_stdlib(&loaded_core, &core_dir)?;
    let id = module_graph.add_module(source, Arc::new(loaded_core));
    module_graph.register_stdlib(id);

    Ok(())
}

pub(crate) fn resolve_modules(
    config: &ModuleResolutionConfig,
    root: ResolvedRootModules,
    user_log: &UserLog,
) -> Result<ModuleDependencyGraph, ModuleResolutionErrors> {
    let mut resolver = MvsSolver;
    resolve_modules_with_solver(config, &mut resolver, root, user_log)
}
