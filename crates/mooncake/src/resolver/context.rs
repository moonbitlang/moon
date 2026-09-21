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
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

use moonutil::{
    manifest::{MoonMod, read_module_desc_file_in_dir},
    resolution::{ModuleName, ModuleSource, ModuleSourceKind},
};
use semver::Version;

use crate::registry::{Registry, RegistryVersionInfo};

use super::ResolverError;

/// Registry access, manifest caches, and diagnostics for module resolution.
pub(crate) struct ModuleResolutionContext<'a> {
    registry: &'a dyn Registry,
    errors: Vec<super::ResolverError>,
    local_module_cache: HashMap<PathBuf, Arc<MoonMod>>,
    stdlib: Option<Arc<MoonMod>>,
}

impl<'a> ModuleResolutionContext<'a> {
    pub(crate) fn new(registry: &'a dyn Registry) -> Self {
        ModuleResolutionContext {
            registry,
            errors: Vec::new(),
            local_module_cache: HashMap::new(),
            stdlib: None,
        }
    }

    pub(crate) fn into_errors(self) -> Vec<super::ResolverError> {
        self.errors
    }

    pub(crate) fn report_error(&mut self, error: super::ResolverError) {
        self.errors.push(error);
    }

    pub(crate) fn any_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub(crate) fn all_versions_of(
        &mut self,
        name: &ModuleName,
    ) -> Option<Arc<BTreeMap<Version, RegistryVersionInfo>>> {
        self.registry.all_versions_of(name).ok()
    }

    pub(crate) fn get(&mut self, ms: &ModuleSource) -> Option<Arc<MoonMod>> {
        match ms.source() {
            ModuleSourceKind::Registry => {
                let version_info = self.registry.all_versions_of(ms.name()).ok()?;
                let deps = version_info.get(ms.version())?.deps.clone();
                Some(Arc::new(MoonMod {
                    name: ms.name().to_string(),
                    version: Some(ms.version().clone()),
                    deps,
                    ..Default::default()
                }))
            }
            ModuleSourceKind::Local(path) => self.resolve_local_module(path).ok(),
            ModuleSourceKind::Stdlib(_) => self.stdlib.clone(),
            ModuleSourceKind::SingleFile(path) => panic!(
                "Single file module source should already be manually resolved: {}",
                path.display()
            ),
        }
    }

    /// Resolve a local module from its **canonical** path.
    pub(crate) fn resolve_local_module(
        &mut self,
        path: &Path,
    ) -> Result<Arc<MoonMod>, ResolverError> {
        if let Some(module) = self.local_module_cache.get(path) {
            return Ok(Arc::clone(module));
        }

        let module = read_module_desc_file_in_dir(path).map_err(ResolverError::Other)?;
        let rc_module = Arc::new(module);
        self.local_module_cache
            .insert(path.to_owned(), Arc::clone(&rc_module));
        Ok(rc_module)
    }
}
