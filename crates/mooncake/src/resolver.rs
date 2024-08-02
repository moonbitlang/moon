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

use std::{collections::HashMap, rc::Rc};

use moonutil::module::MoonMod;
use moonutil::mooncakes::{result, ModuleName, ModuleSource};
use semver::{Version, VersionReq};
use thiserror::Error;

use crate::registry::RegistryList;

pub mod env;
pub mod git;
pub mod mvs;

pub use mvs::MvsSolver;

use self::env::ResolverEnv;

/// Each package's resolved dependencies.
pub type PackageResolveResult = HashMap<ModuleName, Version>;

/// Any error that may occur during dependency resolution.
#[derive(Debug, Error)]
pub enum ResolverError {
    #[error("Malformed module name found in dependency {0}: {1}")]
    MalformedModuleName(ModuleName, String),
    #[error("Unable to find module {0}")]
    ModuleMissing(ModuleName),
    #[error("No version of module {0} satisfies the requirement {1}")]
    NoSatisfiedVersion(ModuleName, VersionReq),
    #[error("When resolving local/git dependencies, the version of module {0} did not match the required version {1}")]
    LocalDepVersionMismatch(Box<ModuleSource>, VersionReq),
    /// Multiple versions of a package are required, but the build system cannot handle this.
    #[error("Multiple conflicting versions were found for module {0}: {1:?}")]
    ConflictingVersions(ModuleName, Vec<Version>),
    #[error("Error during resolution: {0}")]
    Other(anyhow::Error),
}

#[derive(Debug)]
pub struct ResolverErrors(pub Vec<ResolverError>);

impl std::fmt::Display for ResolverErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for error in &self.0 {
            writeln!(f, "{}", error)?;
        }
        Ok(())
    }
}

impl std::error::Error for ResolverErrors {}

/// The dependency resolver.
pub trait Resolver {
    /// Resolves the dependencies of a package using the given environment.
    ///
    /// If the dependencies cannot be resolved, this function should return `None`.
    fn resolve(
        &mut self,
        env: &mut ResolverEnv,
        root: &[(ModuleSource, Rc<MoonMod>)],
    ) -> Option<result::ResolvedEnv>;
}

/// Goes through the resolved environment and checks for any duplicate module names.
///
/// Since the build system is not yet able to handle multiple versions of the same module,
/// this function will return an error if any duplicate module names with different versions
/// (implying incompatible versions of the same module are resolved) are found.
fn assert_no_duplicate_module_names(result: &result::ResolvedEnv) -> Result<(), ResolverErrors> {
    let mut module_name_versions: HashMap<_, Vec<_>> = HashMap::new();
    for it in result.all_packages() {
        module_name_versions
            .entry(&it.name)
            .or_default()
            .push(&it.version);
    }
    let mut errs = vec![];
    for (name, versions) in module_name_versions {
        if versions.len() > 1 {
            let err = ResolverError::ConflictingVersions(
                name.clone(),
                versions.iter().cloned().cloned().collect(),
            );
            errs.push(err);
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(ResolverErrors(errs))
    }
}

pub fn resolve_with_default_env(
    registries: &RegistryList,
    resolver: &mut dyn Resolver,
    root: &[(ModuleSource, Rc<MoonMod>)],
) -> Result<result::ResolvedEnv, ResolverErrors> {
    let mut env = env::ResolverEnv::new(registries);
    let res = resolver.resolve(&mut env, root);
    if env.any_errors() {
        Err(ResolverErrors(env.into_errors()))
    } else {
        let res = res.expect("Resolver should not return None when no errors were found");
        assert_no_duplicate_module_names(&res)?;
        Ok(res)
    }
}

pub fn resolve_with_default_env_and_resolver(
    registries: &RegistryList,
    root: &[(ModuleSource, Rc<MoonMod>)],
) -> Result<result::ResolvedEnv, ResolverErrors> {
    let mut resolver = MvsSolver;
    resolve_with_default_env(registries, &mut resolver, root)
}

pub fn resolve_single_root_with_defaults(
    registries: &RegistryList,
    root_source: ModuleSource,
    root_module: Rc<MoonMod>,
) -> Result<result::ResolvedEnv, ResolverErrors> {
    resolve_with_default_env_and_resolver(registries, &[(root_source, root_module)])
}
