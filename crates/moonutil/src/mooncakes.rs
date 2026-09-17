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
    use slotmap::SlotMap;

    use crate::{constants::MOD_NAME_STDLIB, module::MoonMod};

    use super::{ModuleId, ModuleName, ModuleSource};

    /// The kind of dependency between modules.
    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub enum DependencyKind {
        Regular,
        Binary,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct DependencyEdge {
        pub name: ModuleName,
        pub kind: DependencyKind,
    }

    // Only used in tests
    impl FromStr for DependencyEdge {
        type Err = Infallible;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            let name = s.into();
            Ok(DependencyEdge {
                name,
                kind: DependencyKind::Regular,
            })
        }
    }

    impl std::fmt::Display for DependencyEdge {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.name)?;
            if self.kind == DependencyKind::Binary {
                write!(f, " (binary)")
            } else {
                Ok(())
            }
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

    /// The result of a dependency resolution.
    #[derive(Debug, Clone)]
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
        mapping: ResolvedRootModules,

        /// The real dependency graph. Edges are labelled with the key of the dependency.
        ///
        /// Edges should point from dependents (downstream) to dependencies (upstream).
        dep_graph: DiGraphMap<ModuleId, DependencyEdge>,
    }

    impl ResolvedEnv {
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

        pub fn only_one_module(ms: ModuleSource, module: MoonMod) -> (ResolvedEnv, ModuleId) {
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
                        kind: DependencyKind::Regular,
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
                            kind: DependencyKind::Regular,
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

#[derive(Debug)]
pub struct RegistryConfig {
    pub api: String,
    pub index: String,
    pub download: String,
    pub symbols: Option<String>,
    /// TODO: Remove after support for `registry`-format configurations is dropped.
    /// Old configurations use the registry endpoint for prebuilt wasm assets.
    pub legacy_asset_urls: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RegistryConfigFile {
    Split {
        api: String,
        index: String,
        download: String,
        #[serde(default)]
        symbols: Option<String>,
    },
    Legacy {
        registry: String,
        index: String,
        #[serde(default)]
        symbols: Option<String>,
    },
}

impl From<RegistryConfigFile> for RegistryConfig {
    fn from(config: RegistryConfigFile) -> Self {
        match config {
            RegistryConfigFile::Split {
                api,
                index,
                download,
                symbols,
            } => Self {
                api,
                index,
                download,
                symbols,
                legacy_asset_urls: false,
            },
            RegistryConfigFile::Legacy {
                registry,
                index,
                symbols,
            } => {
                let download = if registry.trim_end_matches('/') == "https://mooncakes.io" {
                    "https://download.mooncakes.io".to_owned()
                } else {
                    registry.clone()
                };
                Self {
                    api: registry.clone(),
                    index,
                    download,
                    symbols,
                    legacy_asset_urls: true,
                }
            }
        }
    }
}

impl RegistryConfig {
    pub fn new() -> Self {
        if let Ok(v) = std::env::var("MOONCAKES_REGISTRY") {
            RegistryConfig {
                index: format!("{v}/git/index"),
                symbols: Some(format!("{v}/symbols.zip")),
                download: v.clone(),
                api: v,
                legacy_asset_urls: true,
            }
        } else {
            RegistryConfig {
                api: "https://mooncakes.io".into(),
                index: "https://mooncakes.io/git/index".into(),
                download: "https://download.mooncakes.io".into(),
                symbols: None,
                legacy_asset_urls: false,
            }
        }
    }

    pub fn load() -> Self {
        let config_path = crate::MOON_HOME.config_path();
        if !config_path.exists() {
            return Self::new();
        }
        let file = File::open(config_path).unwrap();
        let reader = BufReader::new(file);
        let config: RegistryConfigFile = serde_json_lenient::from_reader(reader).unwrap();
        config.into()
    }
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod registry_config_tests {
    use super::RegistryConfigFile;

    #[test]
    fn split_registry_config_uses_independent_endpoints() {
        let config: RegistryConfigFile = serde_json_lenient::from_str(
            r#"{
                "api": "https://api.example",
                "index": "https://index.example",
                "download": "https://download.example"
            }"#,
        )
        .unwrap();
        let config: super::RegistryConfig = config.into();

        assert_eq!(config.api, "https://api.example");
        assert_eq!(config.index, "https://index.example");
        assert_eq!(config.download, "https://download.example");
        assert!(!config.legacy_asset_urls);
    }

    #[test]
    fn legacy_registry_config_preserves_its_endpoint_contract() {
        let config: RegistryConfigFile = serde_json_lenient::from_str(
            r#"{
                "registry": "https://registry.example",
                "index": "https://index.example",
                "symbols": "https://registry.example/symbols.zip"
            }"#,
        )
        .unwrap();
        let config: super::RegistryConfig = config.into();

        assert_eq!(config.api, "https://registry.example");
        assert_eq!(config.download, "https://registry.example");
        assert!(config.legacy_asset_urls);
        assert_eq!(
            config.symbols.as_deref(),
            Some("https://registry.example/symbols.zip")
        );
    }

    #[test]
    fn legacy_official_registry_keeps_its_download_service() {
        let config: RegistryConfigFile = serde_json_lenient::from_str(
            r#"{
                "registry": "https://mooncakes.io",
                "index": "https://mooncakes.io/git/index"
            }"#,
        )
        .unwrap();
        let config: super::RegistryConfig = config.into();

        assert_eq!(config.api, "https://mooncakes.io");
        assert_eq!(config.download, "https://download.mooncakes.io");
        assert!(config.legacy_asset_urls);
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
/// The preview requires registry access but does not load publishing credentials.
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

/// Validate syntax needed to interpolate a username into a module manifest.
/// Registry-specific account rules, including length limits, belong to the
/// registry service.
pub fn validate_username(username: &str) -> anyhow::Result<(), String> {
    if username.is_empty() {
        return Err("Username must not be empty".to_string());
    }

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

#[cfg(test)]
mod tests {
    use super::{DEFAULT_VERSION, ModuleSource, MoonMod, validate_username};
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

    #[test]
    fn accepts_usernames_of_any_nonzero_length() {
        assert!(validate_username("a").is_ok());
        assert!(validate_username(&"a".repeat(40)).is_ok());
    }

    #[test]
    fn rejects_empty_and_manifest_unsafe_usernames() {
        assert!(validate_username("").is_err());
        assert!(validate_username("foo\nbar").is_err());
        assert!(validate_username("foo\"bar").is_err());
    }
}
