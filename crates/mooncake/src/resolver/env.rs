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

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
};

use moonutil::{
    common::read_module_desc_file_in_dir,
    module::MoonMod,
    mooncakes::{ModuleName, ModuleSource, ModuleSourceKind},
};
use semver::Version;

use crate::registry::RegistryList;

use super::ResolverError;

pub struct ResolverEnv<'a> {
    registries: &'a RegistryList,
    errors: Vec<super::ResolverError>,
    local_module_cache: HashMap<PathBuf, Arc<MoonMod>>,
}

impl<'a> ResolverEnv<'a> {
    pub fn new(registries: &'a RegistryList) -> Self {
        ResolverEnv {
            registries,
            errors: Vec::new(),
            local_module_cache: HashMap::new(),
        }
    }

    pub fn into_errors(self) -> Vec<super::ResolverError> {
        self.errors
    }

    pub fn report_error(&mut self, error: super::ResolverError) {
        self.errors.push(error);
    }

    pub fn any_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn all_versions_of(
        &mut self,
        name: &ModuleName,
        registry: Option<&str>,
    ) -> Option<Arc<BTreeMap<Version, Arc<MoonMod>>>> {
        self.registries
            .get_registry(registry)?
            .all_versions_of(name)
            .ok()
    }

    pub fn get_module_version(
        &mut self,
        name: &ModuleName,
        version: &Version,
        registry: Option<&str>,
    ) -> Option<Arc<MoonMod>> {
        self.registries
            .get_registry(registry)?
            .get_module_version(name, version)
    }

    pub fn get(&mut self, ms: &ModuleSource) -> Option<Arc<MoonMod>> {
        match ms.source() {
            ModuleSourceKind::Registry(reg) => {
                self.get_module_version(ms.name(), ms.version(), reg.as_deref())
            }
            ModuleSourceKind::Git(_) => todo!("Resolve git module"),
            ModuleSourceKind::Local(path) => self.resolve_local_module(path).ok(),
        }
    }

    /// Resolve a local module from its **canonical** path.
    pub fn resolve_local_module(&mut self, path: &Path) -> Result<Arc<MoonMod>, ResolverError> {
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
