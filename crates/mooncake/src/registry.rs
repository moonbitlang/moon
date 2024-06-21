#[cfg(test)]
pub mod mock;
pub mod online;

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    rc::Rc,
};

use moonutil::{common::Module, mooncakes::ModuleName};
pub use online::*;
use semver::Version;

pub trait Registry {
    /// Get all versions of a module.
    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Rc<BTreeMap<Version, Rc<Module>>>>;

    fn get_module_version(&self, name: &ModuleName, version: &Version) -> Option<Rc<Module>> {
        let all_versions = self.all_versions_of(name).ok()?;
        all_versions.get(version).cloned()
    }

    fn get_latest_version(&self, name: &ModuleName) -> Option<Rc<Module>> {
        let all_versions = self.all_versions_of(name).ok()?;
        all_versions.values().last().cloned()
    }

    fn install_to(
        &self,
        name: &ModuleName,
        version: &Version,
        to: &Path,
        quiet: bool,
    ) -> anyhow::Result<()>;
}

impl<R> Registry for &mut R
where
    R: Registry,
{
    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Rc<BTreeMap<Version, Rc<Module>>>> {
        (**self).all_versions_of(name)
    }

    fn install_to(
        &self,
        name: &ModuleName,
        version: &Version,
        to: &Path,
        quiet: bool,
    ) -> anyhow::Result<()> {
        (**self).install_to(name, version, to, quiet)
    }

    fn get_module_version(&self, name: &ModuleName, version: &Version) -> Option<Rc<Module>> {
        (**self).get_module_version(name, version)
    }

    fn get_latest_version(&self, name: &ModuleName) -> Option<Rc<Module>> {
        (**self).get_latest_version(name)
    }
}

pub struct RegistryList {
    default_registry: String,
    registries: HashMap<String, Box<dyn Registry>>,
}

impl RegistryList {
    pub fn with_default_registry() -> Self {
        Self::with_registry(Box::new(OnlineRegistry::mooncakes_io()))
    }

    pub fn with_registry(registry: Box<dyn Registry>) -> Self {
        let mut registries = HashMap::new();
        let default_registry_name = "default";
        registries.insert(default_registry_name.to_owned(), registry);

        Self {
            registries,
            default_registry: default_registry_name.into(),
        }
    }

    pub fn from_registries(
        registries: impl Iterator<Item = (String, Box<dyn Registry>)>,
        default: String,
    ) -> Self {
        let registries: HashMap<_, _> = registries.collect();
        assert!(
            registries.contains_key(&default),
            "Registries must contain the default registry"
        );
        Self {
            registries,
            default_registry: default,
        }
    }

    pub fn set_default_registry(&mut self, registry: String) {
        self.default_registry = registry
    }

    pub fn add_registry(&mut self, name: String, registry: Box<dyn Registry>) {
        self.registries.insert(name, registry);
    }

    pub fn get_registry(&self, name: Option<&str>) -> Option<&dyn Registry> {
        self.registries
            .get(name.unwrap_or(&self.default_registry))
            .map(|refbox| &**refbox)
    }
}
