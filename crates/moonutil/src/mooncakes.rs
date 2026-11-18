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
    convert::Infallible,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, LazyLock},
};

use arcstr::ArcStr;
use clap::Subcommand;
use semver::Version;
use serde::{Deserialize, Serialize};
use slotmap::SecondaryMap;
use sync::AutoSyncFlags;

use crate::module::MoonMod;

slotmap::new_key_type! {pub struct ModuleId;}

pub type DirSyncResult = SecondaryMap<ModuleId, PathBuf>;

/// The name of a module.
///
/// This type is cheaply clonable.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleName {
    /// The username part of the module name
    pub username: ArcStr,
    /// The unqualified name part of the module name
    pub unqual: ArcStr,
}

impl ModuleName {
    /// The canonical major-version suffix in `user/module/vN`.
    /// `/v0` and `/v1` are invalid; those versions use the unsuffixed name.
    pub fn major_version_suffix(&self) -> Result<Option<u64>, ModuleVersionError> {
        let major = self.unqual.split_once('/').and_then(|(_, suffix)| {
            let major: u64 = suffix.strip_prefix('v')?.parse().ok()?;
            // Integer parsing also accepts leading zeroes and a `+` sign.
            (suffix == format!("v{major}")).then_some(major)
        });
        match major {
            Some(0 | 1) => Err(ModuleVersionError::InvalidSuffix {
                module: self.clone(),
            }),
            _ => Ok(major),
        }
    }

    /// Check the path suffix, and its agreement with a version when supplied.
    /// Local development modules may omit their version.
    pub fn validate_version(&self, version: Option<&Version>) -> Result<(), ModuleVersionError> {
        if let Some(major) = self.major_version_suffix()?
            && let Some(version) = version
            && major != version.major
        {
            return Err(ModuleVersionError::Mismatch {
                module: self.clone(),
                major,
                version: version.clone(),
            });
        }
        Ok(())
    }

    /// Return the last segment of the name, that may be used as a short name
    /// of a package.
    pub fn last_segment(&self) -> &str {
        if let Some((_, r)) = self.unqual.rsplit_once('/') {
            r
        } else {
            &self.unqual
        }
    }

    pub fn last_segment_owned(&self) -> arcstr::Substr {
        self.unqual.substr_from(self.last_segment())
    }

    /// Return segments of the module name.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        std::iter::once(&*self.username).chain(self.unqual.split('/'))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleVersionError {
    #[error("module `{module}` major-version suffix must be /v2 or higher")]
    InvalidSuffix { module: ModuleName },
    #[error("module `{module}` requires major version {major}, but got {version}")]
    Mismatch {
        module: ModuleName,
        major: u64,
        version: Version,
    },
}

impl std::fmt::Debug for ModuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.username.is_empty() {
            f.write_fmt(format_args!("{}", self.unqual))
        } else {
            f.write_fmt(format_args!("{}/{}", self.username, self.unqual))
        }
    }
}

impl std::fmt::Display for ModuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.username.is_empty() {
            f.write_fmt(format_args!("{}", self.unqual))
        } else {
            f.write_fmt(format_args!("{}/{}", self.username, self.unqual))
        }
    }
}

impl From<&str> for ModuleName {
    fn from(value: &str) -> Self {
        match value.split_once('/') {
            Some((username, pkgname)) => ModuleName {
                username: username.into(),
                unqual: pkgname.into(),
            },
            None => ModuleName {
                username: ArcStr::new(),
                unqual: value.into(),
            },
        }
    }
}

impl FromStr for ModuleName {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(s.into())
    }
}

impl PartialEq<(&str, &str)> for ModuleName {
    fn eq(&self, other: &(&str, &str)) -> bool {
        self.username == other.0 && self.unqual == other.1
    }
}

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

pub mod result {
    use std::{collections::HashMap, convert::Infallible, str::FromStr, sync::Arc};

    use petgraph::graphmap::DiGraphMap;
    use slotmap::{SecondaryMap, SlotMap};

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

    /// Selected input modules and the declarations used to begin resolution.
    /// Their IDs remain valid when this collection becomes a dependency graph.
    #[derive(Debug, Clone, Default)]
    pub struct ResolvedRootModules {
        sources: SlotMap<ModuleId, ModuleSource>,
        manifests: SecondaryMap<ModuleId, Arc<MoonMod>>,
    }

    impl ResolvedRootModules {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn insert(&mut self, source: ModuleSource, manifest: Arc<MoonMod>) -> ModuleId {
            let id = self.sources.insert(source);
            self.manifests.insert(id, manifest);
            id
        }

        pub fn iter(&self) -> impl Iterator<Item = (ModuleId, &ModuleSource, &Arc<MoonMod>)> {
            self.sources
                .iter()
                .map(|(id, source)| (id, source, &self.manifests[id]))
        }

        pub fn len(&self) -> usize {
            self.sources.len()
        }

        pub fn is_empty(&self) -> bool {
            self.sources.is_empty()
        }

        pub fn only_one_module(
            source: ModuleSource,
            module: Arc<MoonMod>,
        ) -> (ResolvedRootModules, ModuleId) {
            let mut roots = Self::new();
            let id = roots.insert(source, module);
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
        /// Selected sources for all modules; dependency metadata stays in the resolver.
        mapping: SlotMap<ModuleId, ModuleSource>,

        /// Original input declarations, retained for workspace edits and synthetic
        /// roots. Build consumers read complete source manifests during discovery.
        root_manifests: SecondaryMap<ModuleId, Arc<MoonMod>>,

        /// The real dependency graph. Edges are labelled with the key of the dependency.
        ///
        /// Edges should point from dependents (downstream) to dependencies (upstream).
        dep_graph: DiGraphMap<ModuleId, DependencyEdge>,
    }

    impl ModuleDependencyGraph {
        pub fn input_module_ids(&self) -> &[ModuleId] {
            &self.input_module_ids
        }

        pub fn module_source(&self, id: ModuleId) -> &ModuleSource {
            &self.mapping[id]
        }

        /// The declaration supplied for an input root before resolution.
        /// Dependencies, including the installed standard library, have no root manifest.
        pub fn root_manifest(&self, id: ModuleId) -> Option<&Arc<MoonMod>> {
            self.root_manifests.get(id)
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
            self.mapping.iter()
        }

        pub fn all_modules(&self) -> impl Iterator<Item = &ModuleSource> {
            self.mapping.values()
        }

        pub fn only_one_module(
            ms: ModuleSource,
            module: MoonMod,
        ) -> (ModuleDependencyGraph, ModuleId) {
            let (roots, id) = ResolvedRootModules::only_one_module(ms, Arc::new(module));
            (Self::from_root_modules(roots), id)
        }

        pub fn module_count(&self) -> usize {
            self.mapping.len()
        }

        pub fn from_root_modules(root_modules: ResolvedRootModules) -> Self {
            let ResolvedRootModules { sources, manifests } = root_modules;
            let input_module_ids = sources.keys().collect();
            let rev_map = sources
                .iter()
                .map(|(id, source)| (source.clone(), id))
                .collect();

            Self {
                input_module_ids,
                stdlib: None,
                mapping: sources,
                root_manifests: manifests,
                dep_graph: DiGraphMap::new(),
                rev_map,
            }
        }

        pub fn new() -> Self {
            Self {
                input_module_ids: Vec::new(),
                stdlib: None,
                mapping: SlotMap::with_key(),
                root_manifests: SecondaryMap::new(),
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

        pub fn add_module(&mut self, mod_source: ModuleSource) -> ModuleId {
            // check if it's already inserted
            if let Some(id) = self.rev_map.get(&mod_source) {
                *id
            } else {
                // Add module definition
                let id = self.mapping.insert(mod_source.clone());
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

pub mod sync {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, clap::Parser, Serialize, Deserialize, Clone)]
    #[clap(next_help_heading = "Manifest Options")]
    pub struct AutoSyncFlags {
        /// Do not sync dependencies, assuming local dependencies are up-to-date
        #[clap(long)]
        pub frozen: bool,
    }

    impl AutoSyncFlags {
        pub fn dont_sync(&self) -> bool {
            self.frozen
        }
    }
}

#[derive(Subcommand, Serialize, Deserialize)]
pub enum MooncakeSubcommands {
    Login(LoginSubcommand),
    Register(RegisterSubcommand),
    Publish(PublishSubcommand),
    Package(PackageSubcommand),
    Deprecate(DeprecateSubcommand),
}

/// Log in to your account
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct LoginSubcommand {}

/// Register an account at mooncakes.io
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct RegisterSubcommand {}

/// Publish the current module
///
/// The optional `private` field in moon.mod requests restricted
/// access to the module after publication when true. Omission or false requests
/// public visibility.
///
/// Publication permissions and visibility are checked by the registry.
/// With --dry-run, validate the publication without creating a release.
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct PublishSubcommand {
    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,
}

/// Package the current module
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct PackageSubcommand {
    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    #[clap(long)]
    pub list: bool,
}

/// Deprecate or restore all existing versions of a published module
///
/// Pass `--reason <REASON>` to deprecate or `--undo` to restore.
///
/// Specify the full module name without a version selector. This command works
/// outside a project and uses the same saved credentials as `moon publish`.
/// Versions published later start undeprecated. Restoring clears all reasons;
/// it does not recover previous per-version states.
///
/// Deprecation does not affect version selection. Commands that resolve
/// dependencies warn about every selected deprecated module, including
/// transitive dependencies, using the local registry index without refreshing
/// it. `--quiet` hides these warnings.
///
/// With `--dry-run`, fetch the module's currently published versions and print
/// the intended change for each release without modifying the registry.
/// The preview uses optional API-bound credentials for the read-only lookup.
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct DeprecateSubcommand {
    /// Full module name, without a version selector
    pub module: String,

    /// Deprecation reason, replacing any previous reasons
    #[arg(
        long,
        required_unless_present = "undo",
        conflicts_with = "undo",
        value_parser = clap::builder::NonEmptyStringValueParser::new()
    )]
    pub reason: Option<String>,

    /// Clear deprecation and reasons on all existing versions
    #[arg(long)]
    #[serde(default)]
    pub undo: bool,
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
