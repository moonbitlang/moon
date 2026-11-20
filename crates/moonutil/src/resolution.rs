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

//! Module sources and dependency-resolution result types.

pub use result::{DependencyEdge, ModuleDependencyGraph, ResolvedModule, ResolvedRootModules};

use std::{
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

use semver::Version;
use slotmap::SecondaryMap;

use crate::{
    module::MoonMod,
    module_name::{ModuleName, ModuleVersionError},
};

slotmap::new_key_type! {pub struct ModuleId;}

pub type DirSyncResult = SecondaryMap<ModuleId, PathBuf>;

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleSourceKind {
    /// Module comes from the registry.
    #[default]
    Registry,
    /// Module comes from a local path. The path must be absolute.
    Local(PathBuf),

    /// This module is the standard library.
    ///
    /// Since the standard library is prebuilt during installation, it is
    /// handled specially. Setting this skips some default behaviors designed
    /// for regular modules.
    ///
    /// TODO: Evaluate if this design is sound
    Stdlib(PathBuf),

    /// This module is from a single-file compilation.
    ///
    /// Setting this skips discovery and uses the given path as the source file.
    SingleFile(PathBuf),
}

impl ModuleSourceKind {
    pub fn is_default(&self) -> bool {
        matches!(self, ModuleSourceKind::Registry)
    }
}

impl std::fmt::Display for ModuleSourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModuleSourceKind::Registry => write!(f, "registry"),
            ModuleSourceKind::Local(path) => write!(f, "local {}", path.display()),
            ModuleSourceKind::Stdlib(_) => write!(f, "stdlib"),
            ModuleSourceKind::SingleFile(path) => write!(f, "single file {}", path.display()),
        }
    }
}

/// Represents the information that fully-qualifies a module.
///
/// Constructors check the module name against its declared or selected version.
/// Local modules may omit a version; their internal default is not a release.
/// This type is cheaply clonable.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleSource {
    /// The inner representation of the module source.
    inner: Arc<ModuleSourceInner>,
}

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct ModuleSourceInner {
    name: ModuleName,
    version: Version,
    source: ModuleSourceKind,
}

impl ModuleSource {
    pub fn name(&self) -> &ModuleName {
        &self.inner.name
    }

    pub fn version(&self) -> &Version {
        &self.inner.version
    }

    pub fn source(&self) -> &ModuleSourceKind {
        &self.inner.source
    }

    pub fn is_core(&self) -> bool {
        *self.name() == ("moonbitlang", "core")
    }

    pub fn new_full(
        name: ModuleName,
        version: Version,
        source: ModuleSourceKind,
    ) -> Result<Self, ModuleVersionError> {
        Self::new_inner(name, Some(version), source)
    }

    pub fn from_version(name: ModuleName, version: Version) -> Result<Self, ModuleVersionError> {
        Self::new_inner(name, Some(version), ModuleSourceKind::Registry)
    }

    pub fn local_path(
        name: ModuleName,
        path: PathBuf,
        version: Version,
    ) -> Result<Self, ModuleVersionError> {
        Self::new_inner(name, Some(version), ModuleSourceKind::Local(path))
    }

    pub fn from_local_module(module: &MoonMod, path: &Path) -> Result<Self, ModuleVersionError> {
        Self::new_inner(
            module.name.as_str().into(),
            module.version.clone(),
            ModuleSourceKind::Local(path.to_owned()),
        )
    }

    pub fn from_stdlib(module: &MoonMod, path: &Path) -> Result<Self, ModuleVersionError> {
        Self::new_inner(
            module.name.as_str().into(),
            module.version.clone(),
            ModuleSourceKind::Stdlib(path.to_owned()),
        )
    }

    pub fn single_file(module: &MoonMod, path: &Path) -> Result<Self, ModuleVersionError> {
        Self::new_inner(
            module.name.as_str().into(),
            module.version.clone(),
            ModuleSourceKind::SingleFile(path.to_owned()),
        )
    }

    fn new_inner(
        name: ModuleName,
        version: Option<Version>,
        source: ModuleSourceKind,
    ) -> Result<Self, ModuleVersionError> {
        // Validate the declared/selected version before filling in the internal
        // default for unversioned local modules. That default is not a release.
        name.validate_version(version.as_ref())?;
        Ok(ModuleSource {
            inner: Arc::new(ModuleSourceInner {
                name,
                version: version.unwrap_or_else(|| DEFAULT_VERSION.clone()),
                source,
            }),
        })
    }
}

/// The `ModuleSource` representing the core module.
pub static CORE_MODULE: LazyLock<ModuleSource> = LazyLock::new(|| {
    ModuleSource::new_full(
        ModuleName {
            username: "moonbitlang".into(),
            unqual: "core".into(),
        },
        Version::new(0, 0, 0),
        ModuleSourceKind::Registry,
    )
    .expect("core module has a valid version")
});

impl std::fmt::Display for ModuleSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.name(), self.version())?;
        if !self.source().is_default() {
            write!(f, " ({})", self.source())?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for ModuleSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self, f)
    }
}

impl std::str::FromStr for ModuleSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s.split_once('@').ok_or("missing version")?;
        let version = Version::parse(parts.1).map_err(|e| e.to_string())?;
        let name = parts.0.into();
        ModuleSource::from_version(name, version).map_err(|error| error.to_string())
    }
}

/// The default version for modules that didn't specify a version.
pub static DEFAULT_VERSION: Version = Version::new(0, 0, 0);

mod result {
    use std::{collections::HashMap, convert::Infallible, str::FromStr, sync::Arc};

    use petgraph::graphmap::DiGraphMap;
    use slotmap::SlotMap;

    use crate::{constants::MOD_NAME_STDLIB, module::MoonMod};

    use super::{ModuleId, ModuleName, ModuleSource};

    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct DependencyEdge {
        pub name: ModuleName,
    }

    // Only used in tests
    impl FromStr for DependencyEdge {
        type Err = Infallible;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            let name = s.into();
            Ok(DependencyEdge { name })
        }
    }

    impl std::fmt::Display for DependencyEdge {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.name)
        }
    }

    #[derive(Debug, Clone)]
    pub struct ResolvedModule {
        source: ModuleSource,
        value: Arc<MoonMod>,
    }

    pub type ResolvedRootModules = SlotMap<ModuleId, ResolvedModule>;

    impl ResolvedModule {
        pub fn new(source: ModuleSource, value: Arc<MoonMod>) -> Self {
            Self { source, value }
        }

        pub fn source(&self) -> &ModuleSource {
            &self.source
        }

        pub fn module_info(&self) -> &Arc<MoonMod> {
            &self.value
        }

        pub fn only_one_module(
            source: ModuleSource,
            module: Arc<MoonMod>,
        ) -> (ResolvedRootModules, ModuleId) {
            let mut roots = SlotMap::with_key();
            let id = roots.insert(Self::new(source, module));
            (roots, id)
        }
    }

    /// Module identities, concrete sources and versions, and dependency edges.
    #[derive(Debug, Clone)]
    pub struct ModuleDependencyGraph {
        /// The list of module IDs that are provided as the input to the resolver.
        input_module_ids: Vec<ModuleId>,
        /// The module that is the standard library. `None` means the project is
        /// compiled without a standard library.
        stdlib: Option<ModuleId>,

        /// A reverse mapping to query the unique ID of a module from its source.
        rev_map: HashMap<ModuleSource, ModuleId>,
        /// The mapping from the unique IDs of modules to their source.
        ///
        /// Note that once we're out of the resolver, reverse-finding the ID
        /// from [`ModuleSource`]s is no longer needed, so this mapping is
        /// unidirectional (even though `ModuleSource`s are unique).
        mapping: ResolvedRootModules,

        /// The real dependency graph. Edges are labelled with the key of the dependency.
        ///
        /// Edges should point from dependents (downstream) to dependencies (upstream).
        dep_graph: DiGraphMap<ModuleId, DependencyEdge>,
    }

    impl ModuleDependencyGraph {
        pub fn input_module_ids(&self) -> &[ModuleId] {
            &self.input_module_ids
        }

        pub fn resolved_module(&self, id: ModuleId) -> &ResolvedModule {
            &self.mapping[id]
        }

        pub fn module_source(&self, id: ModuleId) -> &ModuleSource {
            self.resolved_module(id).source()
        }

        pub fn module_info(&self, id: ModuleId) -> &Arc<MoonMod> {
            self.resolved_module(id).module_info()
        }

        pub fn graph(&self) -> &DiGraphMap<ModuleId, DependencyEdge> {
            &self.dep_graph
        }

        pub fn stdlib(&self) -> Option<ModuleId> {
            self.stdlib
        }

        /// Get all resolved dependencies of a module
        pub fn deps(&self, id: ModuleId) -> impl Iterator<Item = ModuleId> + '_ {
            self.dep_graph
                .neighbors_directed(id, petgraph::Direction::Outgoing)
        }

        /// Get all resolved dependencies of a module along with their keys
        pub fn deps_keyed(
            &self,
            id: ModuleId,
        ) -> impl Iterator<Item = (ModuleId, &DependencyEdge)> + '_ {
            self.dep_graph
                .edges_directed(id, petgraph::Direction::Outgoing)
                .map(|(_s, t, k)| (t, k))
        }

        /// Get the module that `id` depends on, using `dep` as the key in the module manifest.
        pub fn dep_with_key(&self, id: ModuleId, dep: &DependencyEdge) -> Option<ModuleId> {
            // FIXME: This is not very efficient
            self.dep_graph
                .edges_directed(id, petgraph::Direction::Outgoing)
                .find_map(|(_s, t, k)| if k == dep { Some(t) } else { None })
        }

        pub fn dep_count(&self, id: ModuleId) -> usize {
            self.dep_graph
                .neighbors_directed(id, petgraph::Direction::Outgoing)
                .count()
        }

        pub fn all_modules_and_id(&self) -> impl Iterator<Item = (ModuleId, &ModuleSource)> {
            self.mapping.iter().map(|x| (x.0, &x.1.source))
        }

        pub fn all_modules(&self) -> impl Iterator<Item = &ModuleSource> {
            self.mapping.iter().map(|(_id, src)| &src.source)
        }

        pub fn only_one_module(
            ms: ModuleSource,
            module: MoonMod,
        ) -> (ModuleDependencyGraph, ModuleId) {
            let (roots, id) = ResolvedModule::only_one_module(ms, Arc::new(module));
            (Self::from_root_modules(roots), id)
        }

        pub fn module_count(&self) -> usize {
            self.mapping.len()
        }

        pub fn from_root_modules(root_modules: ResolvedRootModules) -> Self {
            let input_module_ids = root_modules.iter().map(|(id, _)| id).collect();
            let rev_map = root_modules
                .iter()
                .map(|(id, module)| (module.source().clone(), id))
                .collect();

            Self {
                input_module_ids,
                stdlib: None,
                mapping: root_modules,
                dep_graph: DiGraphMap::new(),
                rev_map,
            }
        }

        pub fn new() -> Self {
            Self {
                input_module_ids: Vec::new(),
                stdlib: None,
                mapping: SlotMap::with_key(),
                dep_graph: DiGraphMap::new(),
                rev_map: HashMap::new(),
            }
        }

        /// Register the given module ID as the standard library.
        ///
        /// All existing modules, and modules inserted afterwards, will
        /// automatically depend on this module.
        pub fn register_stdlib(&mut self, stdlib: ModuleId) {
            self.stdlib = Some(stdlib);
            for id in self.mapping.keys() {
                if id == stdlib {
                    continue;
                }
                self.dep_graph.add_edge(
                    id,
                    stdlib,
                    DependencyEdge {
                        name: MOD_NAME_STDLIB.clone(),
                    },
                );
            }
        }

        pub fn add_module(&mut self, mod_source: ModuleSource, module: Arc<MoonMod>) -> ModuleId {
            // check if it's already inserted
            if let Some(id) = self.rev_map.get(&mod_source) {
                *id
            } else {
                // Add module definition
                let val = ResolvedModule::new(mod_source.clone(), module);
                let id = self.mapping.insert(val);
                self.rev_map.insert(mod_source, id);

                // Add a dependency to the standard library module
                if let Some(stdlib) = self.stdlib {
                    self.dep_graph.add_edge(
                        id,
                        stdlib,
                        DependencyEdge {
                            name: MOD_NAME_STDLIB.clone(),
                        },
                    );
                }

                id
            }
        }

        pub fn add_dependency(&mut self, from: ModuleId, to: ModuleId, key: &DependencyEdge) {
            self.dep_graph.add_edge(from, to, key.to_owned());
        }
    }

    impl Default for ModuleDependencyGraph {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_VERSION, ModuleSource, MoonMod};
    use semver::Version;

    #[test]
    fn module_sources_reject_invalid_versions() {
        for (name, version) in [
            ("a/b/v0", "0.1.0"),
            ("a/b/v1", "1.0.0"),
            ("a/b/v2", "1.5.0"),
            ("a/b/v2", "3.0.0"),
            ("a/b", "2.0.0"),
            ("a/b", "2.0.0-rc.1"),
        ] {
            let coordinate = format!("{name}@{version}");
            assert!(
                coordinate.parse::<ModuleSource>().is_err(),
                "accepted {coordinate}"
            );
            assert!(ModuleSource::from_version(name.into(), version.parse().unwrap()).is_err());
        }
    }

    #[test]
    fn module_sources_accept_prereleases_of_the_matching_major() {
        let source =
            ModuleSource::from_version("a/b/v2".into(), "2.0.0-rc.1".parse().unwrap()).unwrap();
        assert_eq!(source.to_string(), "a/b/v2@2.0.0-rc.1");
    }

    #[test]
    fn local_module_sources_validate_before_defaulting_the_version() {
        let mut module = MoonMod {
            name: "a/b/v2".into(),
            ..Default::default()
        };
        let path = std::path::Path::new("workspace");
        let source = ModuleSource::from_local_module(&module, path).unwrap();
        assert_eq!(source.version(), &DEFAULT_VERSION);

        module.version = Some(Version::new(1, 5, 0));
        assert!(ModuleSource::from_local_module(&module, path).is_err());
        assert!(
            ModuleSource::local_path("a/b/v2".into(), path.into(), Version::new(1, 5, 0)).is_err()
        );

        module.name = "a/b/v1".into();
        module.version = None;
        assert!(ModuleSource::from_local_module(&module, path).is_err());
    }
}
