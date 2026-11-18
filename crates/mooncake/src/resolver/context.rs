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

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use indexmap::IndexMap;
use moonutil::{
    dependency::SourceDependencyInfo, manifest::read_module_desc_file_in_dir,
    resolution::ModuleName,
};
use semver::Version;

use crate::registry::{Registry, RegistryVersionInfo, VersionReader};

/// Requirements in declaration order, shared by traversal and graph construction.
pub(super) type ModuleDependencies = IndexMap<String, SourceDependencyInfo>;

/// Reading a local manifest supplies its actual version as well as its requirements.
pub(super) struct LocalModuleMetadata {
    pub(super) version: Version,
    pub(super) dependencies: Arc<ModuleDependencies>,
}

/// Registry access, dependency metadata caches, and diagnostics for module resolution.
pub(crate) struct ModuleResolutionContext<'a, R: Registry> {
    registry: &'a R,
    errors: Vec<super::ModuleResolutionError>,
    registry_modules: HashMap<ModuleName, HashMap<Version, RegistryModule>>,
    local_module_cache: HashMap<PathBuf, Arc<LocalModuleMetadata>>,
}

/// Requirements and advisory status come from the same registry observation.
struct RegistryModule {
    dependencies: Arc<ModuleDependencies>,
    /// `Some("")` means deprecated without a reason; `None` means not deprecated.
    deprecation_reason: Option<String>,
}

impl<'a, R: Registry> ModuleResolutionContext<'a, R> {
    pub(crate) fn new(registry: &'a R) -> Self {
        ModuleResolutionContext {
            registry,
            errors: Vec::new(),
            registry_modules: HashMap::new(),
            local_module_cache: HashMap::new(),
        }
    }

    pub(crate) fn into_errors(self) -> Vec<super::ModuleResolutionError> {
        self.errors
    }

    pub(crate) fn report_error(&mut self, error: super::ModuleResolutionError) {
        self.errors.push(error);
    }

    pub(crate) fn any_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Own the query session outside individual tasks, while keeping registry
    /// access encapsulated in the resolution context.
    pub(crate) fn version_reader(&self) -> impl VersionReader + 'static {
        self.registry.version_reader()
    }

    /// Accept a completed read. Only the resolver updates its dependency and
    /// advisory cache; tasks never borrow the mutable resolution context.
    pub(super) fn record_registry_module(
        &mut self,
        name: &ModuleName,
        version: &Version,
        info: RegistryVersionInfo,
    ) -> Arc<ModuleDependencies> {
        let module = self
            .registry_modules
            .entry(name.clone())
            .or_default()
            .entry(version.clone())
            .or_insert_with(|| RegistryModule {
                dependencies: Arc::new(info.deps),
                deprecation_reason: info.yanked.then(|| info.yanked_reason.unwrap_or_default()),
            });
        Arc::clone(&module.dependencies)
    }

    /// Advisory status observed during traversal, without another registry read.
    pub(crate) fn registry_deprecation_reason(
        &self,
        name: &ModuleName,
        version: &Version,
    ) -> Option<&str> {
        self.registry_modules
            .get(name)?
            .get(version)?
            .deprecation_reason
            .as_deref()
    }

    /// Resolve a local module from its **canonical** path.
    /// Return read errors unchanged; the caller adds dependency-edge context.
    pub(super) fn resolve_local_module(
        &mut self,
        path: &Path,
    ) -> anyhow::Result<Arc<LocalModuleMetadata>> {
        if let Some(module) = self.local_module_cache.get(path) {
            return Ok(Arc::clone(module));
        }

        // Parse and validate the full declaration before retaining only the
        // metadata needed for module resolution.
        let manifest = read_module_desc_file_in_dir(path)?;
        let module = Arc::new(LocalModuleMetadata {
            version: manifest.version.expect("Expected version in module"),
            dependencies: Arc::new(manifest.deps),
        });
        self.local_module_cache
            .insert(path.to_owned(), Arc::clone(&module));
        Ok(module)
    }
}
