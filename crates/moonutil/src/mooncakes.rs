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
    fs::File,
    io::BufReader,
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
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleName {
    /// The username part of the module name
    pub username: ArcStr,
    /// The unqualified name part of the module name
    pub unqual: ArcStr,
}

impl ModuleName {
    /// Returns whether the module's name is in legacy format, either having
    /// empty username or containing multiple segments in the unqualified name.
    pub fn is_legacy(&self) -> bool {
        if self.username.is_empty() {
            return true;
        }
        if self.unqual.contains('/') {
            return true;
        }
        false
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

    /// Return segments of the module name.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        std::iter::once(&*self.username).chain(self.unqual.split('/'))
    }
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

impl FromStr for ModuleName {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once('/') {
            Some((username, pkgname)) => Ok(ModuleName {
                username: username.into(),
                unqual: pkgname.into(),
            }),
            None => Ok(ModuleName {
                username: ArcStr::new(),
                unqual: s.into(),
            }),
        }
    }
}

impl PartialEq<(&str, &str)> for ModuleName {
    fn eq(&self, other: &(&str, &str)) -> bool {
        self.username == other.0 && self.unqual == other.1
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleSourceKind {
    /// Module comes from some registry. If param is `None`, it comes from the default
    /// registry. Otherwise it comes from a specific registry (unused for now).
    Registry(Option<String>), // Registry ID?
    /// Module comes from a git repository.
    // TODO: add branch/commit
    Git(String),
    /// Module comes from a local path. The path must be absolute.
    Local(PathBuf),

    /// This module is the standard library.
    ///
    /// Since the standard library is prebuilt during installation, it is
    /// handled specially. Setting this skips some default behaviors designed
    /// for regular modules.
    ///
    /// TODO: Evaluate if this design is sound
    Stdlib,
}

impl Default for ModuleSourceKind {
    fn default() -> Self {
        ModuleSourceKind::Registry(None)
    }
}

impl ModuleSourceKind {
    pub fn is_default(&self) -> bool {
        matches!(self, ModuleSourceKind::Registry(None))
    }
}

impl std::fmt::Display for ModuleSourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModuleSourceKind::Registry(None) => write!(f, "default registry"),
            ModuleSourceKind::Registry(Some(name)) => write!(f, "registry {name}"),
            ModuleSourceKind::Local(path) => write!(f, "local {}", path.display()),
            ModuleSourceKind::Git(url) => write!(f, "git {url}"),
            ModuleSourceKind::Stdlib => write!(f, "standard library"),
        }
    }
}

/// Represents the information that fully-qualifies a module.
///
/// This type is cheaply clonable.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleSource {
    // Note: Serialization & deserialization of `Arc` does not retain identity.
    // This is generally not an issue, but I'm writing it here to prevent confusion.
    /// The inner representation of the module source.
    inner: Arc<ModuleSourceInner>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

    pub fn new_full(name: ModuleName, version: Version, source: ModuleSourceKind) -> Self {
        Self::new_inner(ModuleSourceInner {
            name,
            version,
            source,
        })
    }

    pub fn from_version(name: ModuleName, version: Version) -> Self {
        Self::new_inner(ModuleSourceInner {
            name,
            version,
            source: Default::default(),
        })
    }

    pub fn from_registry_and_version(name: ModuleName, registry: &str, version: Version) -> Self {
        Self::new_inner(ModuleSourceInner {
            name,
            version,
            source: ModuleSourceKind::Registry(Some(registry.to_owned())),
        })
    }

    pub fn local_path(name: ModuleName, path: PathBuf, version: Version) -> Self {
        Self::new_inner(ModuleSourceInner {
            name,
            version,
            source: ModuleSourceKind::Local(path),
        })
    }

    pub fn from_local_module(module: &MoonMod, path: &Path) -> Option<Self> {
        Some(Self::new_inner(ModuleSourceInner {
            name: module.name.parse().ok()?,
            version: module
                .version
                .clone()
                .unwrap_or_else(|| DEFAULT_VERSION.clone()),
            source: ModuleSourceKind::Local(path.to_owned()),
        }))
    }

    pub fn git(name: ModuleName, url: String, version: Version) -> Self {
        Self::new_inner(ModuleSourceInner {
            name,
            version,
            source: ModuleSourceKind::Git(url),
        })
    }

    fn new_inner(inner: ModuleSourceInner) -> Self {
        ModuleSource {
            inner: Arc::new(inner),
        }
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
        ModuleSourceKind::Registry(None),
    )
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
        let name = parts.0.parse()?;
        Ok(ModuleSource::new_inner(ModuleSourceInner {
            name,
            version,
            source: Default::default(),
        }))
    }
}

/// The default version for modules that didn't specify a version.
pub static DEFAULT_VERSION: Version = Version::new(0, 0, 0);

pub mod result {
    use std::{collections::HashMap, sync::Arc};

    use petgraph::graphmap::DiGraphMap;
    use slotmap::SlotMap;

    use crate::{common::MOD_NAME_STDLIB, module::MoonMod};

    use super::{ModuleId, ModuleName, ModuleSource};

    pub type DependencyKey = ModuleName;

    #[derive(Debug)]
    struct ResolvedModule {
        source: ModuleSource,
        value: Arc<MoonMod>,
    }

    /// The result of a dependency resolution.
    #[derive(Debug)]
    pub struct ResolvedEnv {
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
        mapping: SlotMap<ModuleId, ResolvedModule>,

        /// The real dependency graph. Edges are labelled with the key of the dependency.
        ///
        /// Edges should point from dependents (downstream) to dependencies (upstream).
        dep_graph: DiGraphMap<ModuleId, DependencyKey>,
    }

    impl ResolvedEnv {
        pub fn input_module_ids(&self) -> &[ModuleId] {
            &self.input_module_ids
        }

        pub fn mod_from_name(&self, ms: &ModuleSource) -> Option<ModuleId> {
            self.rev_map.get(ms).cloned()
        }

        pub fn mod_name_from_id(&self, id: ModuleId) -> &ModuleSource {
            &self.mapping[id].source
        }

        pub fn module_info(&self, id: ModuleId) -> &Arc<MoonMod> {
            &self.mapping[id].value
        }

        pub fn graph(&self) -> &DiGraphMap<ModuleId, DependencyKey> {
            &self.dep_graph
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
        ) -> impl Iterator<Item = (ModuleId, &DependencyKey)> + '_ {
            self.dep_graph
                .edges_directed(id, petgraph::Direction::Outgoing)
                .map(|(_s, t, k)| (t, k))
        }

        /// Get the module that `id` depends on, using `dep` as the key in the module manifest.
        pub fn dep_with_key(&self, id: ModuleId, dep: &DependencyKey) -> Option<ModuleId> {
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

        pub fn only_one_module(ms: ModuleSource, module: MoonMod) -> (ResolvedEnv, ModuleId) {
            let mut builder = Self::new();
            let id = builder.add_module(ms, Arc::new(module));
            (builder, id)
        }

        pub fn module_count(&self) -> usize {
            self.mapping.len()
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

        pub fn push_root_module(&mut self, id: ModuleId) {
            self.input_module_ids.push(id);
        }

        /// Set the given module ID as the standard library. All modules
        /// inserted afterwards will automatically depend on this module.
        pub fn set_stdlib(&mut self, stdlib: ModuleId) {
            self.stdlib = Some(stdlib)
        }

        pub fn add_module(&mut self, mod_source: ModuleSource, module: Arc<MoonMod>) -> ModuleId {
            // check if it's already inserted
            if let Some(id) = self.rev_map.get(&mod_source) {
                *id
            } else {
                // Add module definition
                let val = ResolvedModule {
                    source: mod_source.clone(),
                    value: module,
                };
                let id = self.mapping.insert(val);
                self.rev_map.insert(mod_source, id);

                // Add a dependency to the standard library module
                if let Some(stdlib) = self.stdlib {
                    self.dep_graph.add_edge(id, stdlib, MOD_NAME_STDLIB.clone());
                }

                id
            }
        }

        pub fn add_dependency(&mut self, from: ModuleId, to: ModuleId, key: &DependencyKey) {
            self.dep_graph.add_edge(from, to, key.to_owned());
        }
    }

    impl Default for ResolvedEnv {
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

#[derive(Serialize, Deserialize)]
pub struct Credentials {
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegistryConfig {
    pub registry: String,
    pub index: String,
}

impl RegistryConfig {
    pub fn new() -> Self {
        if let Ok(v) = std::env::var("MOONCAKES_REGISTRY") {
            RegistryConfig {
                index: format!("{v}/git/index"),
                registry: v,
            }
        } else {
            RegistryConfig {
                registry: "https://mooncakes.io".into(),
                index: "https://mooncakes.io/git/index".into(),
            }
        }
    }

    pub fn load() -> Self {
        let config_path = crate::moon_dir::config_json();
        if !config_path.exists() {
            return Self::new();
        }
        let file = File::open(config_path).unwrap();
        let reader = BufReader::new(file);
        let config: RegistryConfig = serde_json_lenient::from_reader(reader).unwrap();
        config
    }
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Subcommand, Serialize, Deserialize)]
pub enum MooncakeSubcommands {
    Login(LoginSubcommand),
    Register(RegisterSubcommand),
    Publish(PublishSubcommand),
    Package(PackageSubcommand),
}

/// Log in to your account
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct LoginSubcommand {}

/// Register an account at mooncakes.io
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct RegisterSubcommand {}

/// Publish the current module
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

// username rule
// at least 5 char, at most 39 char
// may contain [a-z] [0-9] [A-Z] '-' '_'
pub fn validate_username(username: &str) -> anyhow::Result<(), String> {
    // Check the length constraints
    if username.len() < 5 || username.len() > 39 {
        return Err("Username must be between 5 and 39 characters long".to_string());
    }

    // Check for valid characters
    if !username
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Usernames can only contain alphanumeric characters, dashes (-), and underscores (_)."
                .to_string(),
        );
    }

    Ok(())
}
