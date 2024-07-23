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

#[cfg(test)]
pub mod mock;
pub mod online;

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    rc::Rc,
};

use moonutil::module::MoonMod;
use moonutil::mooncakes::ModuleName;
pub use online::*;
use semver::Version;

pub trait Registry {
    /// Get all versions of a module.
    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<Rc<BTreeMap<Version, Rc<MoonMod>>>>;

    fn get_module_version(&self, name: &ModuleName, version: &Version) -> Option<Rc<MoonMod>> {
        let all_versions = self.all_versions_of(name).ok()?;
        all_versions.get(version).cloned()
    }

    fn get_latest_version(&self, name: &ModuleName) -> Option<Rc<MoonMod>> {
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
    ) -> anyhow::Result<Rc<BTreeMap<Version, Rc<MoonMod>>>> {
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

    fn get_module_version(&self, name: &ModuleName, version: &Version) -> Option<Rc<MoonMod>> {
        (**self).get_module_version(name, version)
    }

    fn get_latest_version(&self, name: &ModuleName) -> Option<Rc<MoonMod>> {
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
