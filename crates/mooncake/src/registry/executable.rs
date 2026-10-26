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

use anyhow::{Context, bail};
use moonutil::{
    resolution::{ModuleName, ModuleSource},
    user_log::UserLog,
};
use semver::Version;

use super::{Registry, RegistryClient, path as registry_path};

/// One exact main package selected by an Executable Package Coordinate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedExecutablePackage {
    pub module: ModuleSource,
    pub package_path: String,
}

impl ResolvedExecutablePackage {
    pub fn artifact_name(&self, suffix: &str) -> String {
        let stem = if self.package_path.is_empty() {
            self.module.name().last_segment()
        } else {
            self.package_path
                .rsplit('/')
                .next()
                .expect("non-empty package path must have a last segment")
        };
        format!("{stem}{suffix}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LatestVersionLookup {
    Found(Version),
    NoVersionInformation,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExecutablePackageVersionSelector {
    Exact(ModuleSource),
    LocallyKnownLatest,
    RefreshLatest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LatestVersionResolutionPolicy {
    PreferLocal,
    Refresh,
}

impl RegistryClient {
    /// Resolve an Executable Package Coordinate to one exact main package.
    pub fn resolve_executable_package(
        &self,
        coordinate: &str,
        user_log: &UserLog,
    ) -> anyhow::Result<ResolvedExecutablePackage> {
        let (module_name, package_path, requested_version) =
            parse_executable_package_coordinate(coordinate)?;
        let module = match requested_version {
            ExecutablePackageVersionSelector::Exact(module) => module,
            ExecutablePackageVersionSelector::LocallyKnownLatest => self
                .resolve_latest_executable_module(
                    &module_name,
                    LatestVersionResolutionPolicy::PreferLocal,
                    user_log,
                )?,
            ExecutablePackageVersionSelector::RefreshLatest => self
                .resolve_latest_executable_module(
                    &module_name,
                    LatestVersionResolutionPolicy::Refresh,
                    user_log,
                )?,
        };
        // Check ownership before returning a coordinate satisfied by a local
        // executable-artifact cache hit.
        self.is_private_release(module.name(), module.version())?;
        Ok(ResolvedExecutablePackage {
            module,
            package_path,
        })
    }

    /// Resolve an Executable Package Coordinate and acquire its cached Wasm artifact.
    pub fn acquire_executable_wasm(
        &self,
        coordinate: &str,
        user_log: &UserLog,
    ) -> anyhow::Result<std::path::PathBuf> {
        let package = self.resolve_executable_package(coordinate, user_log)?;
        self.acquire_wasm_asset(&package.module, &package.package_path, user_log)
    }

    fn resolve_latest_executable_module(
        &self,
        module_name: &ModuleName,
        policy: LatestVersionResolutionPolicy,
        user_log: &UserLog,
    ) -> anyhow::Result<ModuleSource> {
        resolve_latest_module_with(
            module_name,
            user_log,
            self.has_cached_index(),
            policy,
            || {
                Ok(self
                    .cached_latest_version(module_name)?
                    .map(LatestVersionLookup::Found)
                    .unwrap_or(LatestVersionLookup::NotFound))
            },
            || self.sync(user_log),
            || latest_version_from_registry(self, module_name),
        )
    }
}

fn latest_version_from_registry(
    registry: &impl Registry,
    module_name: &ModuleName,
) -> anyhow::Result<LatestVersionLookup> {
    match registry.latest_version(module_name) {
        Ok(Some(version)) => Ok(LatestVersionLookup::Found(version)),
        Ok(None) => Ok(LatestVersionLookup::NoVersionInformation),
        Err(error) if error.is::<super::RegistryModuleNotFound>() => {
            Ok(LatestVersionLookup::NotFound)
        }
        Err(error) => Err(error),
    }
}

fn resolve_latest_module_with(
    module_name: &ModuleName,
    user_log: &UserLog,
    had_index: bool,
    policy: LatestVersionResolutionPolicy,
    mut lookup_cached_version: impl FnMut() -> anyhow::Result<LatestVersionLookup>,
    mut update_registry: impl FnMut() -> anyhow::Result<()>,
    mut lookup_latest_version: impl FnMut() -> anyhow::Result<LatestVersionLookup>,
) -> anyhow::Result<ModuleSource> {
    if policy == LatestVersionResolutionPolicy::PreferLocal
        && let LatestVersionLookup::Found(version) = lookup_cached_version()?
    {
        let module = ModuleSource::from_version(module_name.clone(), version)?;
        user_log.info(format!(
            "Resolved {module_name} latest version to {}",
            module.version()
        ));
        return Ok(module);
    }

    match update_registry() {
        Ok(()) => {}
        Err(error) if policy == LatestVersionResolutionPolicy::Refresh => {
            return Err(error).context("Failed to update registry index");
        }
        Err(error) => {
            if had_index {
                user_log.warn(format!(
                    "Failed to update registry index, using cached index: {error}"
                ));
            } else {
                return Err(error).context("Failed to update registry index");
            }
        }
    }

    let version = match lookup_latest_version()? {
        LatestVersionLookup::Found(version) => version,
        LatestVersionLookup::NoVersionInformation => {
            bail!("Module `{module_name}` has no version information")
        }
        LatestVersionLookup::NotFound if had_index => {
            bail!("Module `{module_name}` not found in registry")
        }
        LatestVersionLookup::NotFound => {
            bail!("Module `{module_name}` not found in registry after updating the index")
        }
    };
    let module = ModuleSource::from_version(module_name.clone(), version)?;
    user_log.info(format!(
        "Resolved {module_name} latest version to {}",
        module.version()
    ));
    Ok(module)
}

fn parse_executable_package_coordinate(
    input: &str,
) -> anyhow::Result<(ModuleName, String, ExecutablePackageVersionSelector)> {
    if input.ends_with("...") {
        bail!("Invalid package coordinate `{input}`: wildcard package paths are not supported");
    }

    if let Some((_, version_tail)) = input.split_once('@') {
        let parsed = if version_tail.contains('/') {
            registry_path::parse_module_at_version_path(input)
        } else {
            registry_path::parse_package_at_version_path(input)
        }
        .with_context(|| format!("Invalid package coordinate `{input}`"))?;
        let version = if parsed.version.as_deref() == Some("latest") {
            ExecutablePackageVersionSelector::RefreshLatest
        } else {
            ExecutablePackageVersionSelector::Exact(
                parsed
                    .exact_module()
                    .with_context(|| format!("Invalid version in package coordinate `{input}`"))?
                    .expect("versioned path has a version"),
            )
        };
        return Ok((parsed.module, parsed.package, version));
    }

    let parsed = registry_path::parse_install_style_path(input)
        .with_context(|| format!("Invalid package coordinate `{input}`"))?;
    Ok((
        parsed.module,
        parsed.package,
        ExecutablePackageVersionSelector::LocallyKnownLatest,
    ))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn parse(input: &str) -> (ModuleName, String, ExecutablePackageVersionSelector) {
        parse_executable_package_coordinate(input).unwrap()
    }

    #[test]
    fn major_version_coordinates_select_the_full_module_name() {
        for coordinate in ["a/b/v2@2.1.0/tool", "a/b/v2/tool@2.1.0"] {
            assert_eq!(
                parse(coordinate),
                (
                    "a/b/v2".into(),
                    "tool".into(),
                    ExecutablePackageVersionSelector::Exact("a/b/v2@2.1.0".parse().unwrap()),
                )
            );
        }
        for coordinate in ["a/b/v2@latest/tool", "a/b/v2/tool@latest"] {
            assert_eq!(
                parse(coordinate),
                (
                    "a/b/v2".into(),
                    "tool".into(),
                    ExecutablePackageVersionSelector::RefreshLatest
                )
            );
        }
        assert_eq!(
            parse("a/b/v2/tool"),
            (
                "a/b/v2".into(),
                "tool".into(),
                ExecutablePackageVersionSelector::LocallyKnownLatest
            )
        );
    }

    #[test]
    fn major_version_coordinates_reject_mismatches() {
        for coordinate in ["a/b/v2@1.5.0", "a/b/v2@1.5.0/tool", "a/b/v2/tool@1.5.0"] {
            let error = parse_executable_package_coordinate(coordinate).unwrap_err();
            assert!(format!("{error:#}").contains("requires major version 2, but got 1.5.0"));
        }
        for policy in [
            LatestVersionResolutionPolicy::PreferLocal,
            LatestVersionResolutionPolicy::Refresh,
        ] {
            let error = resolve_latest_module_with(
                &"a/b/v2".into(),
                &UserLog::new(log::LevelFilter::Error),
                true,
                policy,
                || Ok(LatestVersionLookup::Found(Version::new(1, 5, 0))),
                || Ok(()),
                || Ok(LatestVersionLookup::Found(Version::new(1, 5, 0))),
            )
            .unwrap_err();
            assert_eq!(
                error.to_string(),
                "module `a/b/v2` requires major version 2, but got 1.5.0"
            );
        }
    }

    #[test]
    fn parse_install_style_version() {
        let (module_name, package_path, version) = parse("moonbitlang/parser/cmd/moonfmt@0.3.3");
        assert_eq!(module_name.to_string(), "moonbitlang/parser");
        assert_eq!(package_path, "cmd/moonfmt");
        assert_eq!(
            version,
            ExecutablePackageVersionSelector::Exact("moonbitlang/parser@0.3.3".parse().unwrap())
        );
    }

    #[test]
    fn parse_module_version_alias() {
        let (module_name, package_path, version) = parse("moonbitlang/parser@0.3.3/cmd/moonfmt");
        assert_eq!(module_name.to_string(), "moonbitlang/parser");
        assert_eq!(package_path, "cmd/moonfmt");
        assert_eq!(
            version,
            ExecutablePackageVersionSelector::Exact("moonbitlang/parser@0.3.3".parse().unwrap())
        );
    }

    #[test]
    fn parse_unpinned_coordinate() {
        let (module_name, package_path, version) = parse("moonbitlang/parser/cmd/moonfmt");
        assert_eq!(module_name.to_string(), "moonbitlang/parser");
        assert_eq!(package_path, "cmd/moonfmt");
        assert_eq!(
            version,
            ExecutablePackageVersionSelector::LocallyKnownLatest
        );
    }

    #[test]
    fn parse_explicit_latest_coordinates() {
        let (module_name, package_path, version) = parse("moonbitlang/parser/cmd/moonfmt@latest");
        assert_eq!(module_name.to_string(), "moonbitlang/parser");
        assert_eq!(package_path, "cmd/moonfmt");
        assert_eq!(version, ExecutablePackageVersionSelector::RefreshLatest);

        let (module_name, package_path, version) = parse("moonbitlang/parser@latest/cmd/moonfmt");
        assert_eq!(module_name.to_string(), "moonbitlang/parser");
        assert_eq!(package_path, "cmd/moonfmt");
        assert_eq!(version, ExecutablePackageVersionSelector::RefreshLatest);
    }

    #[test]
    fn latest_resolution_uses_local_registry_before_updating() {
        let module_name = "moonbitlang/parser".parse::<ModuleName>().unwrap();
        let mut update_called = false;

        let version = resolve_latest_module_with(
            &module_name,
            &UserLog::new(log::LevelFilter::Warn),
            true,
            LatestVersionResolutionPolicy::PreferLocal,
            || Ok(LatestVersionLookup::Found("0.3.3".parse().unwrap())),
            || {
                update_called = true;
                Ok(())
            },
            || panic!("a cached hit must not trigger an online lookup"),
        )
        .unwrap();

        assert_eq!(version.version().to_string(), "0.3.3");
        assert!(!update_called);
    }

    #[test]
    fn latest_resolution_updates_after_local_registry_miss() {
        let module_name = "moonbitlang/parser".parse::<ModuleName>().unwrap();
        let lookup_count = Cell::new(0);
        let mut update_called = false;

        let version = resolve_latest_module_with(
            &module_name,
            &UserLog::new(log::LevelFilter::Warn),
            true,
            LatestVersionResolutionPolicy::PreferLocal,
            || {
                lookup_count.set(lookup_count.get() + 1);
                Ok(LatestVersionLookup::NotFound)
            },
            || {
                update_called = true;
                Ok(())
            },
            || {
                lookup_count.set(lookup_count.get() + 1);
                Ok(LatestVersionLookup::Found("0.3.3".parse().unwrap()))
            },
        )
        .unwrap();

        assert_eq!(version.version().to_string(), "0.3.3");
        assert_eq!(lookup_count.get(), 2);
        assert!(update_called);
    }

    #[test]
    fn explicit_latest_refreshes_before_resolving() {
        let module_name = "moonbitlang/parser".parse::<ModuleName>().unwrap();
        let registry_updated = Cell::new(false);
        let lookup_count = Cell::new(0);

        let version = resolve_latest_module_with(
            &module_name,
            &UserLog::new(log::LevelFilter::Warn),
            true,
            LatestVersionResolutionPolicy::Refresh,
            || panic!("explicit latest must not use cached metadata"),
            || {
                registry_updated.set(true);
                Ok(())
            },
            || {
                lookup_count.set(lookup_count.get() + 1);
                Ok(LatestVersionLookup::Found(
                    if registry_updated.get() {
                        "0.4.0"
                    } else {
                        "0.3.3"
                    }
                    .parse()
                    .unwrap(),
                ))
            },
        )
        .unwrap();

        assert_eq!(version.version().to_string(), "0.4.0");
        assert!(registry_updated.get());
        assert_eq!(lookup_count.get(), 1);
    }

    #[test]
    fn explicit_latest_fails_when_refresh_fails() {
        let module_name = "moonbitlang/parser".parse::<ModuleName>().unwrap();
        let lookup_called = Cell::new(false);

        let error = resolve_latest_module_with(
            &module_name,
            &UserLog::new(log::LevelFilter::Warn),
            true,
            LatestVersionResolutionPolicy::Refresh,
            || panic!("explicit latest must not use cached metadata"),
            || Err(anyhow::anyhow!("offline")),
            || {
                lookup_called.set(true);
                Ok(LatestVersionLookup::Found("0.3.3".parse().unwrap()))
            },
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "Failed to update registry index");
        assert_eq!(error.source().unwrap().to_string(), "offline");
        assert!(!lookup_called.get());
    }

    #[test]
    fn latest_resolution_preserves_no_version_information_after_update() {
        let module_name = "moonbitlang/parser".parse::<ModuleName>().unwrap();
        let mut update_called = false;

        let error = resolve_latest_module_with(
            &module_name,
            &UserLog::new(log::LevelFilter::Warn),
            true,
            LatestVersionResolutionPolicy::PreferLocal,
            || Ok(LatestVersionLookup::NoVersionInformation),
            || {
                update_called = true;
                Ok(())
            },
            || Ok(LatestVersionLookup::NoVersionInformation),
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Module `moonbitlang/parser` has no version information"
        );
        assert!(update_called);
    }

    #[test]
    fn latest_resolution_preserves_online_denials_after_public_index_refresh() {
        let updated = Cell::new(false);
        let error = resolve_latest_module_with(
            &"alice/secret".into(),
            &UserLog::new(log::LevelFilter::Warn),
            true,
            LatestVersionResolutionPolicy::PreferLocal,
            || Ok(LatestVersionLookup::NotFound),
            || {
                updated.set(true);
                Ok(())
            },
            || {
                assert!(updated.get());
                Err(anyhow::anyhow!(
                    "Module `alice/secret` not found or access denied"
                ))
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("not found or access denied"));
    }

    #[test]
    fn root_package_uses_module_last_segment_for_artifact_name() {
        let package = ResolvedExecutablePackage {
            module: "moonbitlang/parser@0.3.3".parse().unwrap(),
            package_path: String::new(),
        };
        assert_eq!(package.artifact_name(".exe"), "parser.exe");
    }

    #[test]
    fn reject_invalid_coordinates() {
        assert!(parse_executable_package_coordinate("moonbitlang/parser@bad/cmd/moonfmt").is_err());
        assert!(parse_executable_package_coordinate("moonbitlang/parser/cmd/moonfmt@bad").is_err());
        assert!(parse_executable_package_coordinate("moonbitlang/parser@0.3.3/cmd@0.4.0").is_err());
        assert!(parse_executable_package_coordinate("moonbitlang/parser/0.3.3@0.4.0/cmd").is_err());
        assert!(parse_executable_package_coordinate("moonbitlang/parser/...").is_err());
        assert!(parse_executable_package_coordinate("moonbitlang/parser//cmd").is_err());
        assert!(parse_executable_package_coordinate("./moonbitlang/parser").is_err());
        assert!(parse_executable_package_coordinate("C:/moonbitlang/parser").is_err());
        assert!(parse_executable_package_coordinate("https://mooncakes.io/x").is_err());
    }
}
