//! A mock registry for testing purposes; currently only available in tests

use std::{
    collections::{BTreeMap, HashMap},
    rc::Rc,
};

use moonutil::{
    common::{DependencyInfo, Module},
    mooncakes::ModuleName,
};
use semver::{Version, VersionReq};

use super::Registry;

/// A mock registry, primarily used in tests.
pub struct MockRegistry {
    modules: HashMap<ModuleName, Rc<BTreeMap<Version, Rc<Module>>>>,
}

impl MockRegistry {
    pub fn new() -> Self {
        MockRegistry {
            modules: HashMap::new(),
        }
    }

    pub fn parse(&mut self, input: Vec<Vec<(&str, usize)>>) {
        for dep_item in input.iter() {
            let (name, v) = dep_item[0];
            let deps: Vec<(String, String)> = dep_item
                .iter()
                .skip(1)
                .map(|(name, v)| (format!("t/{}", name), format!("0.1.{}", v)))
                .collect();

            for (name, version) in &deps {
                if self.try_get_module(name, version).is_none() {
                    self.add_module_full(name, version, vec![]); // add placeholder
                }
            }

            self.add_module_full(
                &format!("t/{}", name),
                &format!("0.1.{}", v),
                deps.iter().map(|x| (x.0.as_str(), x.1.as_str())),
            );
        }

        let mut misses = vec![];

        for module in self.modules.iter() {
            for dep in module.1.iter().flat_map(|x| x.1.deps.iter()) {
                if !self
                    .modules
                    .contains_key(&dep.0.parse::<ModuleName>().unwrap())
                {
                    misses.push((
                        dep.0.clone(),
                        format!(
                            "{}.{}.{}",
                            dep.1.version.comparators[0].major,
                            dep.1.version.comparators[0].minor.unwrap(),
                            dep.1.version.comparators[0].patch.unwrap()
                        ),
                    ));
                }
            }
        }
        for (missing, v) in misses.iter() {
            self.add_module_full(missing, v, []);
        }
    }

    pub fn get_module(&self, name: &str, version: &str) -> Rc<Module> {
        self.try_get_module(name, version).unwrap()
    }

    pub fn try_get_module(&self, name: &str, version: &str) -> Option<Rc<Module>> {
        Some(
            self.modules
                .get(&name.parse().unwrap())?
                .get(&Version::parse(version).unwrap())?
                .clone(),
        )
    }

    /// Add a module to the mock registry. Only available when the mock registry
    /// is not used, since modifying a [`Rc`] is only possible when it is not
    /// shared.
    pub fn add_module(&mut self, module: Module) -> &mut Self {
        let name = module.name.parse().unwrap();
        let version = module.version.clone().unwrap();
        let entry = self.modules.entry(name).or_default();
        Rc::get_mut(entry)
            .expect("This mock registry is already shared")
            .insert(version, Rc::new(module));
        self
    }

    pub fn add_module_full<'a>(
        &mut self,
        name: &'a str,
        version: &'a str,
        deps: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> &mut Self {
        let module = create_mock_module(name, version, deps);
        self.add_module(module)
    }
}

pub fn create_mock_module<'a>(
    name: &'a str,
    version: &'a str,
    deps: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Module {
    Module {
        name: name.to_string(),
        version: Some(Version::parse(version).unwrap()),
        deps: deps
            .into_iter()
            .map(|(name, version)| {
                (
                    name.to_string(),
                    DependencyInfo {
                        version: VersionReq::parse(version).unwrap(),
                        ..Default::default()
                    },
                )
            })
            .collect(),
        ..Default::default()
    }
}

impl Default for MockRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry for MockRegistry {
    fn all_versions_of(
        &self,
        name: &ModuleName,
    ) -> anyhow::Result<std::rc::Rc<BTreeMap<Version, std::rc::Rc<Module>>>> {
        self.modules
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("module not found in mock registry"))
            .cloned()
    }

    fn install_to(
        &self,
        _name: &ModuleName,
        _version: &Version,
        _to: &std::path::Path,
        _quiet: bool,
    ) -> anyhow::Result<()> {
        panic!("Mock registry does not support installing")
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_mock_registry_add() {
        let mut registry = MockRegistry::new();
        registry
            .add_module_full("foo/bar", "0.2.0", [])
            .add_module_full("foo/bar", "0.1.0", [])
            .add_module_full("foo/bar", "0.1.2", []);
        let module = registry
            .all_versions_of(&"foo/bar".parse().unwrap())
            .unwrap();
        assert_eq!(
            module
                .keys()
                .cloned()
                .map(|x| x.to_string())
                .collect::<Vec<_>>(),
            vec!["0.1.0", "0.1.2", "0.2.0"]
        )
    }
}
