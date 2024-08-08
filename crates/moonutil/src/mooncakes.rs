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
    collections::HashMap,
    fmt::Display,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    str::FromStr,
};

use clap::Subcommand;
use semver::Version;
use serde::{Deserialize, Serialize};
use sync::AutoSyncFlags;

use crate::module::MoonMod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleId(u32);

impl ModuleId {
    pub fn new_usize(id: usize) -> Self {
        Self(id as u32)
    }

    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

pub type DirSyncResult = HashMap<ModuleId, PathBuf>;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleName {
    pub username: String,
    pub pkgname: String,
}

impl std::fmt::Debug for ModuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.username.is_empty() {
            f.write_fmt(format_args!("{}", self.pkgname))
        } else {
            f.write_fmt(format_args!("{}/{}", self.username, self.pkgname))
        }
    }
}

impl std::fmt::Display for ModuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.username.is_empty() {
            f.write_fmt(format_args!("{}", self.pkgname))
        } else {
            f.write_fmt(format_args!("{}/{}", self.username, self.pkgname))
        }
    }
}

impl FromStr for ModuleName {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.split_once('/') {
            Some((username, pkgname)) => Ok(ModuleName {
                username: username.to_string(),
                pkgname: pkgname.to_string(),
            }),
            None => Ok(ModuleName {
                username: String::new(),
                pkgname: s.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GitSource {
    pub url: String,
    pub branch: Option<String>,
    pub revision: Option<String>,
}

impl Display for GitSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.url)?;
        if self.branch.is_some() || self.revision.is_some() {
            write!(f, " (")?;
            let mut first = true;
            if let Some(branch) = &self.branch {
                write!(f, "branch: {}", branch)?;
                first = false;
            }
            if let Some(revision) = &self.revision {
                if !first {
                    write!(f, ", ")?;
                }
                write!(f, "revision: {}", revision)?;
            }
            write!(f, ")")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleSourceKind {
    /// Module comes from some registry. If param is `None`, it comes from the default
    /// registry. Otherwise it comes from a specific registry (unused for now).
    Registry(Option<String>), // Registry ID?
    /// Module comes from a git repository.
    Git(GitSource),
    /// Module comes from a local path. The path must be absolute.
    Local(PathBuf),
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
            ModuleSourceKind::Registry(Some(name)) => write!(f, "registry {}", name),
            ModuleSourceKind::Local(path) => write!(f, "local {}", path.display()),
            ModuleSourceKind::Git(url) => write!(f, "git {}", url),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleSource {
    pub name: ModuleName,
    pub version: Version,
    pub source: ModuleSourceKind,
}

impl ModuleSource {
    pub fn from_version(name: ModuleName, version: Version) -> Self {
        ModuleSource {
            name,
            version,
            source: Default::default(),
        }
    }

    pub fn from_registry_and_version(name: ModuleName, registry: &str, version: Version) -> Self {
        ModuleSource {
            name,
            version,
            source: ModuleSourceKind::Registry(Some(registry.to_owned())),
        }
    }

    pub fn local_path(name: ModuleName, path: PathBuf, version: Version) -> Self {
        ModuleSource {
            name,
            version,
            source: ModuleSourceKind::Local(path),
        }
    }

    pub fn from_local_module(module: &MoonMod, path: &Path) -> Option<Self> {
        Some(ModuleSource {
            name: module.name.parse().ok()?,
            version: module
                .version
                .clone()
                .unwrap_or_else(|| DEFAULT_VERSION.clone()),
            source: ModuleSourceKind::Local(path.to_owned()),
        })
    }

    pub fn git(name: ModuleName, details: GitSource, version: Version) -> Self {
        ModuleSource {
            name,
            version,
            source: ModuleSourceKind::Git(details),
        }
    }
}

impl std::fmt::Display for ModuleSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.name, self.version)?;
        if !self.source.is_default() {
            write!(f, " ({})", self.source)?;
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
        Ok(ModuleSource {
            name,
            version,
            source: Default::default(),
        })
    }
}

/// The default version for modules that didn't specify a version.
pub static DEFAULT_VERSION: Version = Version::new(0, 0, 0);

pub mod result {
    use std::rc::Rc;

    use indexmap::IndexSet;
    use petgraph::graphmap::DiGraphMap;

    use crate::module::MoonMod;

    use super::{ModuleId, ModuleName, ModuleSource};

    pub type DependencyKey = ModuleName;

    /// The result of a dependency resolution.
    #[derive(Debug)]
    pub struct ResolvedEnv {
        mapping: IndexSet<ModuleSource>,
        /// List of all modules in the environment.
        modules: Vec<Rc<MoonMod>>,
        /// The real dependency graph. Edges are labelled with the key of the dependency.
        // FIXME: Using module names for dependency keys is not very efficient, both
        // in terms of memory and speed. We should change the graph into a hashmap
        // or something similar.
        dep_graph: DiGraphMap<ModuleId, DependencyKey>,
    }

    impl ResolvedEnv {
        pub fn mod_name_from_id(&self, id: ModuleId) -> &ModuleSource {
            &self.mapping[id.as_usize()]
        }

        pub fn id_from_mod_name(&self, pkg: &ModuleSource) -> Option<ModuleId> {
            self.mapping
                .get_full(pkg)
                .map(|(id, _)| ModuleId::new_usize(id))
        }

        pub fn module_info(&self, id: ModuleId) -> &Rc<MoonMod> {
            &self.modules[id.as_usize()]
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

        pub fn all_packages_and_id(&self) -> impl Iterator<Item = (ModuleId, &ModuleSource)> {
            self.mapping
                .iter()
                .enumerate()
                .map(|(id, pkg)| (ModuleId::new_usize(id), pkg))
        }

        pub fn all_packages(&self) -> impl Iterator<Item = &ModuleSource> {
            self.mapping.iter()
        }

        pub fn builder() -> ResolvedEnvBuilder {
            ResolvedEnvBuilder::new()
        }

        pub fn only_one_module(ms: ModuleSource, module: MoonMod) -> ResolvedEnv {
            let mut builder = Self::builder();
            builder.add_module(ms, Rc::new(module));
            builder.build()
        }
    }

    pub struct ResolvedEnvBuilder {
        env: ResolvedEnv,
    }

    impl ResolvedEnvBuilder {
        pub fn new() -> Self {
            Self {
                env: ResolvedEnv {
                    mapping: IndexSet::new(),
                    modules: Vec::new(),
                    dep_graph: DiGraphMap::new(),
                },
            }
        }

        pub fn add_module(&mut self, pkg: ModuleSource, module: Rc<MoonMod>) -> ModuleId {
            let id = ModuleId::new_usize(self.env.mapping.len());
            self.env.mapping.insert(pkg);
            self.env.modules.push(module);
            assert_eq!(self.env.mapping.len(), self.env.modules.len());
            id
        }

        pub fn add_dependency(&mut self, from: ModuleId, to: ModuleId, key: &DependencyKey) {
            self.env.dep_graph.add_edge(from, to, key.to_owned());
        }

        pub fn build(self) -> ResolvedEnv {
            self.env
        }
    }

    impl Default for ResolvedEnvBuilder {
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
                index: format!("{}/git/index", v),
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
}

/// Log in to your account
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct LoginSubcommand {}

/// Register an account at mooncakes.io
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct RegisterSubcommand {}

/// Publish the current package
#[derive(Debug, clap::Parser, Serialize, Deserialize)]
pub struct PublishSubcommand {
    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,
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
