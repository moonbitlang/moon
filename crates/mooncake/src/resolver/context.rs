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
    rc::Rc,
    sync::Arc,
};

use moonutil::manifest::{MoonMod, read_module_desc_file_in_dir};

use crate::registry::Registry;

use super::ModuleResolutionError;

/// Registry access, manifest caches, and diagnostics for module resolution.
pub(crate) struct ModuleResolutionContext<R: Registry> {
    pub(super) registry: Rc<R>,
    errors: Vec<super::ModuleResolutionError>,
    local_module_cache: HashMap<PathBuf, Arc<MoonMod>>,
}

impl<R: Registry> ModuleResolutionContext<R> {
    pub(crate) fn new(registry: Rc<R>) -> Self {
        ModuleResolutionContext {
            registry,
            errors: Vec::new(),
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
