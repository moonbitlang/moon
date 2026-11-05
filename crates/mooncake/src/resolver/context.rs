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

use moonutil::{
    manifest::{MoonMod, read_module_desc_file_in_dir},
    resolution::ModuleName,
};
use semver::Version;

use crate::registry::{Registry, RegistryModuleNotFound};

use super::ModuleResolutionError;

/// Registry access, manifest caches, and diagnostics for module resolution.
pub(crate) struct ModuleResolutionContext<'a, R: Registry> {
    registry: &'a R,
    errors: Vec<super::ModuleResolutionError>,
    registry_modules: HashMap<ModuleName, HashMap<Version, RegistryModule>>,
    local_module_cache: HashMap<PathBuf, Arc<MoonMod>>,
}

/// The manifest and advisory status come from the same registry observation.
struct RegistryModule {
    manifest: Arc<MoonMod>,
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

    /// Load an exact registry version once per resolution, sharing its manifest
    /// across dependants. Failed reads retain the requesting dependency context.
    pub(crate) fn get_registry_module(
        &mut self,
        name: &ModuleName,
        version: &Version,
        dependant: &ModuleName,
    ) -> Result<Arc<MoonMod>, ModuleResolutionError> {
        if let Some(module) = self
            .registry_modules
            .get(name)
            .and_then(|versions| versions.get(version))
        {
            return Ok(Arc::clone(&module.manifest));
        }

        let info = self.registry.version_info(name, version).map_err(|error| {
            if error.is::<RegistryModuleNotFound>() {
                ModuleResolutionError::ModuleMissing {
                    dependency: name.clone(),
                    dependant: dependant.clone(),
                }
            } else {
                ModuleResolutionError::RegistryReadFailed {
                    dependency: name.clone(),
                    dependant: dependant.clone(),
                    required: version.clone(),
                    source: error,
                }
            }
        })?;
        let info = info.ok_or_else(|| ModuleResolutionError::VersionMissing {
            dependency: name.clone(),
            dependant: dependant.clone(),
            required: version.clone(),
        })?;
        let manifest = Arc::new(MoonMod {
            name: name.to_string(),
            version: Some(version.clone()),
            deps: info.deps,
            ..Default::default()
        });
        self.registry_modules
            .entry(name.clone())
            .or_default()
            .insert(
                version.clone(),
                RegistryModule {
                    manifest: Arc::clone(&manifest),
                    deprecation_reason: info.yanked.then(|| info.yanked_reason.unwrap_or_default()),
                },
            );
        Ok(manifest)
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
    pub(crate) fn resolve_local_module(
        &mut self,
        path: &Path,
    ) -> Result<Arc<MoonMod>, ModuleResolutionError> {
        if let Some(module) = self.local_module_cache.get(path) {
            return Ok(Arc::clone(module));
        }

        let module = read_module_desc_file_in_dir(path).map_err(ModuleResolutionError::Other)?;
        let rc_module = Arc::new(module);
        self.local_module_cache
            .insert(path.to_owned(), Arc::clone(&rc_module));
        Ok(rc_module)
    }
}
