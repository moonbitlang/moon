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

use moonutil::user_log::UserLog;
use moonutil::{
    module_name::ModuleName,
    resolution::{
        ModuleDependencyGraph, ModuleId, ModuleSource, ModuleSourceKind, ResolvedRootModules,
    },
};
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
        "`bin-deps` is no longer supported in module `{module}`; remove it and run portable Wasm tools with `moonx` instead"
    )]
    RemovedBinaryDependencies { module: String },
    #[error(
        "Failed to resolve registry dependency `{dependency}` for module `{dependant}`: module was not found in the registry"
    )]
    ModuleMissing {
        dependency: ModuleName,
        dependant: ModuleName,
    },
    #[error(
        "Failed to resolve registry dependency `{dependency}` for module `{dependant}`: version `{required}` was not found in the registry"
    )]
    VersionMissing {
        dependency: ModuleName,
        dependant: ModuleName,
        required: Version,
    },
    #[error(
        "Failed to read registry dependency `{dependency}@{required}` for module `{dependant}`: {source:#}"
    )]
    RegistryReadFailed {
        dependency: ModuleName,
        dependant: ModuleName,
        required: Version,
        #[source]
        source: anyhow::Error,
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
pub(crate) trait ModuleResolver<R: Registry> {
    /// Select module sources and versions, extending `module_graph` from its
    /// existing root modules with their dependencies.
    ///
    /// If the dependencies cannot be resolved, this function should return
    /// `false`. The errors should be emitted in `context`.
    async fn resolve(
        &mut self,
        context: &mut ModuleResolutionContext<'_, R>,
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

/// Metadata access and advisory policy for selecting module dependencies.
pub(crate) struct ModuleResolutionConfig<'a, R: Registry> {
    pub(crate) registry: &'a R,
    /// Explicit refresh reports fresh status after resolution.
    pub(crate) warn_deprecated: bool,
}

/// Validate root declarations before project preparation performs external IO.
/// The graph retains the input root IDs; callers may add an already prepared
/// standard library before asking the resolver to expand dependencies.
pub(crate) fn prepare_roots(
    root: ResolvedRootModules,
) -> Result<ModuleDependencyGraph, ModuleResolutionErrors> {
    // Historical dependency manifests may still contain bin-deps. Only roots
    // used to execute them; reject those before any resolution or acquisition.
    let removed_bin_deps = root
        .iter()
        .map(|(_, root)| root.module_info())
        .filter(|module| {
            module.ext.get("bin-deps").is_some_and(|value| {
                !value.is_null() && !value.as_object().is_some_and(|deps| deps.is_empty())
            })
        })
        .map(|module| ModuleResolutionError::RemovedBinaryDependencies {
            module: module.name.clone(),
        })
        .collect::<Vec<_>>();
    if !removed_bin_deps.is_empty() {
        return Err(ModuleResolutionErrors(removed_bin_deps));
    }

    Ok(ModuleDependencyGraph::from_root_modules(root))
}

pub(crate) async fn resolve_modules_with_solver<R: Registry>(
    config: &ModuleResolutionConfig<'_, R>,
    resolver: &mut impl ModuleResolver<R>,
    mut module_graph: ModuleDependencyGraph,
    user_log: &UserLog,
) -> Result<ModuleDependencyGraph, ModuleResolutionErrors> {
    let mut context = ModuleResolutionContext::new(config.registry);
    let status = resolver
        .resolve(&mut context, &mut module_graph, user_log)
        .await;
    if context.any_errors() {
        Err(ModuleResolutionErrors(context.into_errors()))
    } else {
        if !status {
            panic!("The resolver should not return `false` when no errors are found");
        }
        assert_no_duplicate_module_names(&module_graph)?;
        if config.warn_deprecated {
            warn_deprecated_dependencies(&module_graph, &context, user_log);
        }
        Ok(module_graph)
    }
}

fn warn_deprecated_dependencies<R: Registry>(
    module_graph: &ModuleDependencyGraph,
    context: &ModuleResolutionContext<'_, R>,
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
        let Some(reason) = context.registry_deprecation_reason(source.name(), source.version())
        else {
            continue;
        };
        // Registry-authored reasons may contain terminal control sequences.
        // Strip those while preserving ordinary newlines and spacing.
        let reason = anstream::adapter::strip_str(reason).to_string();
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

#[tracing::instrument(level = "debug", skip_all)]
pub(crate) fn resolve_modules<R: Registry>(
    config: &ModuleResolutionConfig<'_, R>,
    module_graph: ModuleDependencyGraph,
    user_log: &UserLog,
) -> Result<ModuleDependencyGraph, ModuleResolutionErrors> {
    // The synchronous entry point drives the computation; the solver only
    // describes it. Source acquisition starts after this boundary returns.
    // TODO: Move runtime ownership to command orchestration when resolution
    // and test execution share a runtime.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .map_err(|error| {
            ModuleResolutionErrors(vec![ModuleResolutionError::Other(error.into())])
        })?;
    tokio::task::LocalSet::new().block_on(
        &runtime,
        resolve_modules_with_solver(config, &mut MvsSolver, module_graph, user_log),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use moonutil::resolution::ResolvedModule;

    use super::*;
    use crate::registry::mock::{MockRegistry, create_mock_module};

    #[test]
    fn resolution_retains_prepared_stdlib_and_root_identity() {
        let root = Arc::new(create_mock_module(
            "app/main",
            "0.1.0",
            [("dep/library", "1.0.0")],
        ));
        let source =
            ModuleSource::from_version("app/main".into(), "0.1.0".parse().unwrap()).unwrap();
        let (roots, root_id) = ResolvedModule::only_one_module(source, root);
        let mut graph = prepare_roots(roots).unwrap();

        // There is no core manifest on disk. The supplied module is authoritative;
        // resolution must neither reread it nor replace it with the installed core.
        let dir = tempfile::tempdir().unwrap();
        let core_path = dir.path().join("unread-core");
        let core = Arc::new(create_mock_module("moonbitlang/core", "1.9.0", []));
        let core_source = ModuleSource::from_stdlib(&core, &core_path).unwrap();
        let core_id = graph.add_module(core_source.clone(), Arc::clone(&core));
        graph.register_stdlib(core_id);

        let mut registry = MockRegistry::new();
        registry.add_module_full("dep/library", "1.0.0", []);
        let config = ModuleResolutionConfig {
            registry: &registry,
            warn_deprecated: false,
        };
        let resolved =
            resolve_modules(&config, graph, &UserLog::new(log::LevelFilter::Error)).unwrap();

        assert_eq!(resolved.input_module_ids(), &[root_id]);
        assert_eq!(resolved.stdlib(), Some(core_id));
        assert_eq!(resolved.module_source(core_id), &core_source);
        assert!(Arc::ptr_eq(resolved.module_info(core_id), &core));
        assert_eq!(resolved.module_count(), 3);
        for (id, _) in resolved.all_modules_and_id() {
            if id != core_id {
                assert!(resolved.deps(id).any(|dependency| dependency == core_id));
            }
        }
    }
}
