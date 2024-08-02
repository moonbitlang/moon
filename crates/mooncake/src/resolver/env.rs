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
    rc::Rc,
};

use anyhow::Context;
use moonutil::{
    common::{read_module_desc_file_in_dir, MOON_MOD_JSON},
    module::MoonMod,
    mooncakes::{GitSource, ModuleName, ModuleSource, ModuleSourceKind},
};
use semver::Version;
use walkdir::WalkDir;

use crate::registry::RegistryList;

use super::ResolverError;

pub struct ResolverEnv<'a> {
    registries: &'a RegistryList,
    errors: Vec<super::ResolverError>,
    local_module_cache: HashMap<PathBuf, Rc<MoonMod>>,
    git_module_cache: HashMap<GitSource, HashMap<ModuleName, (PathBuf, Rc<MoonMod>)>>,
}

impl<'a> ResolverEnv<'a> {
    pub fn new(registries: &'a RegistryList) -> Self {
        ResolverEnv {
            registries,
            errors: Vec::new(),
            local_module_cache: HashMap::new(),
            git_module_cache: HashMap::new(),
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
    ) -> Option<Rc<BTreeMap<Version, Rc<MoonMod>>>> {
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
    ) -> Option<Rc<MoonMod>> {
        self.registries
            .get_registry(registry)?
            .get_module_version(name, version)
    }

    pub fn get(&mut self, ms: &ModuleSource) -> Option<Rc<MoonMod>> {
        match &ms.source {
            ModuleSourceKind::Registry(reg) => {
                self.get_module_version(&ms.name, &ms.version, reg.as_deref())
            }
            ModuleSourceKind::Git(_) => todo!("Resolve git module"),
            ModuleSourceKind::Local(path) => self.resolve_local_module(path).ok(),
        }
    }

    /// Resolve a local module from its **canonical** path.
    pub fn resolve_local_module(&mut self, path: &Path) -> Result<Rc<MoonMod>, ResolverError> {
        if let Some(module) = self.local_module_cache.get(path) {
            return Ok(Rc::clone(module));
        }

        let module = read_module_desc_file_in_dir(path).map_err(ResolverError::Other)?;
        let rc_module = Rc::new(module);
        self.local_module_cache
            .insert(path.to_owned(), Rc::clone(&rc_module));
        Ok(rc_module)
    }

    pub fn resolve_git_module(
        &mut self,
        git_info: &GitSource,
        expected_name: &ModuleName,
    ) -> Result<Rc<MoonMod>, ResolverError> {
        // Check cache
        if let Some(mods) = self.git_module_cache.get(git_info) {
            if let Some((_, module)) = mods.get(expected_name) {
                return Ok(module.clone());
            }
        }

        let checkout = super::git::resolve(git_info)
            .with_context(|| format!("Failed to resolve git source {}", git_info))
            .map_err(ResolverError::Other)?;
        let mods = recursively_scan_for_moon_mods(&checkout)
            .with_context(|| format!("Failed to scan for moon mods in {}", checkout.display()))
            .map_err(ResolverError::Other)?;

        // populate cache
        let mut mods_map = HashMap::new();
        for (path, module) in mods {
            mods_map.insert(
                module.name.parse().map_err(|e| {
                    ResolverError::Other(anyhow::anyhow!("Failed to parse module name: {}", e))
                })?,
                (path, module.clone()),
            );
        }
        let entry = self
            .git_module_cache
            .entry(git_info.clone())
            .or_insert(mods_map);

        entry
            .get(expected_name)
            .map(|(_, module)| module.clone())
            .ok_or_else(|| ResolverError::ModuleMissing(expected_name.clone()))
    }
}

fn recursively_scan_for_moon_mods(path: &Path) -> anyhow::Result<Vec<(PathBuf, Rc<MoonMod>)>> {
    let mut mods = Vec::new();
    for entry in WalkDir::new(path) {
        let entry = entry?;
        if entry.file_name() == MOON_MOD_JSON {
            let module = read_module_desc_file_in_dir(entry.path())?;
            mods.push((entry.path().to_owned(), Rc::new(module)));
        }
    }
    Ok(mods)
}
