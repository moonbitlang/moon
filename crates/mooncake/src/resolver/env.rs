use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    rc::Rc,
};

use moonutil::{
    common::{read_module_desc_file_in_dir, Module},
    mooncakes::{ModuleName, ModuleSource, ModuleSourceKind},
};
use semver::Version;

use crate::registry::RegistryList;

use super::ResolverError;

pub struct ResolverEnv<'a> {
    registries: &'a RegistryList,
    errors: Vec<super::ResolverError>,
    local_module_cache: HashMap<PathBuf, Rc<Module>>,
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
    ) -> Option<Rc<BTreeMap<Version, Rc<Module>>>> {
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
    ) -> Option<Rc<Module>> {
        self.registries
            .get_registry(registry)?
            .get_module_version(name, version)
    }

    pub fn get(&mut self, ms: &ModuleSource) -> Option<Rc<Module>> {
        match &ms.source {
            ModuleSourceKind::Registry(reg) => {
                self.get_module_version(&ms.name, &ms.version, reg.as_deref())
            }
            ModuleSourceKind::Git(_) => todo!("Resolve git module"),
            ModuleSourceKind::Local(path) => self.resolve_local_module(path).ok(),
        }
    }

    /// Resolve a local module from its **canonical** path.
    pub fn resolve_local_module(&mut self, path: &Path) -> Result<Rc<Module>, ResolverError> {
        if let Some(module) = self.local_module_cache.get(path) {
            return Ok(module.clone());
        }

        let module = read_module_desc_file_in_dir(path).map_err(ResolverError::Other)?;
        let rc_module = Rc::new(module);
        self.local_module_cache
            .insert(path.to_owned(), rc_module.clone());
        Ok(rc_module)
    }
}
