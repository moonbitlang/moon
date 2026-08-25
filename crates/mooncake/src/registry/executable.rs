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

use anyhow::{Context, bail};
use moonutil::{resolution::ModuleName, user_log::UserLog};
use semver::Version;

use super::{Registry, RegistryClient, path as registry_path};

/// One exact main package selected by an Executable Package Coordinate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedExecutablePackage {
    pub module_name: ModuleName,
    pub package_path: String,
    pub version: Version,
}

impl ResolvedExecutablePackage {
    pub fn artifact_name(&self, suffix: &str) -> String {
        let stem = if self.package_path.is_empty() {
            self.module_name.last_segment()
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
    Exact(Version),
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
        let version = match requested_version {
            ExecutablePackageVersionSelector::Exact(version) => version,
            ExecutablePackageVersionSelector::LocallyKnownLatest => self
                .resolve_latest_executable_version(
                    &module_name,
                    LatestVersionResolutionPolicy::PreferLocal,
                    user_log,
                )?,
            ExecutablePackageVersionSelector::RefreshLatest => self
                .resolve_latest_executable_version(
                    &module_name,
                    LatestVersionResolutionPolicy::Refresh,
                    user_log,
                )?,
        };
        Ok(ResolvedExecutablePackage {
            module_name,
            package_path,
            version,
        })
    }

    /// Resolve an Executable Package Coordinate and acquire its cached Wasm artifact.
    pub fn acquire_executable_wasm(
        &self,
        coordinate: &str,
        user_log: &UserLog,
    ) -> anyhow::Result<std::path::PathBuf> {
        let package = self.resolve_executable_package(coordinate, user_log)?;
        self.acquire_wasm_asset(
            &package.module_name,
            &package.version,
            &package.package_path,
            user_log,
        )
    }

    fn resolve_latest_executable_version(
        &self,
        module_name: &ModuleName,
        policy: LatestVersionResolutionPolicy,
        user_log: &UserLog,
    ) -> anyhow::Result<Version> {
        resolve_latest_version_with(
            module_name,
            user_log,
            self.has_cached_index(),
            policy,
            || latest_version_from_registry(self, module_name),
            || self.sync(user_log),
        )
    }
}

fn latest_version_from_registry(
    registry: &impl Registry,
    module_name: &ModuleName,
) -> LatestVersionLookup {
    let versions = match registry.all_versions_of(module_name) {
        Ok(versions) => versions,
        Err(_) => return LatestVersionLookup::NotFound,
    };
    versions
        .last_key_value()
        .map(|(version, _)| LatestVersionLookup::Found(version.clone()))
        .unwrap_or(LatestVersionLookup::NoVersionInformation)
}

fn resolve_latest_version_with(
    module_name: &ModuleName,
    user_log: &UserLog,
    had_index: bool,
    policy: LatestVersionResolutionPolicy,
    mut lookup_latest_version: impl FnMut() -> LatestVersionLookup,
    mut update_registry: impl FnMut() -> anyhow::Result<()>,
) -> anyhow::Result<Version> {
    if policy == LatestVersionResolutionPolicy::PreferLocal
        && let LatestVersionLookup::Found(version) = lookup_latest_version()
    {
        user_log.info(format!(
            "Resolved {module_name} latest version to {version}"
        ));
        return Ok(version);
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

    let version = match lookup_latest_version() {
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
    user_log.info(format!(
        "Resolved {module_name} latest version to {version}"
    ));
    Ok(version)
}

fn parse_executable_package_coordinate(
    input: &str,
) -> anyhow::Result<(ModuleName, String, ExecutablePackageVersionSelector)> {
    if input.ends_with("...") {
        bail!("Invalid package coordinate `{input}`: wildcard package paths are not supported");
    }

    if input.contains('@') {
        let parsed = if let Ok(parsed) = registry_path::parse_module_at_version_path(input) {
            parsed
        } else if let Ok(parsed) = registry_path::parse_package_at_version_path(input) {
            parsed
        } else {
            bail!("Invalid package coordinate `{input}`");
        };
        let version = if parsed.version == "latest" {
            ExecutablePackageVersionSelector::RefreshLatest
        } else {
            ExecutablePackageVersionSelector::Exact(Version::parse(&parsed.version).with_context(
                || format!("Invalid version `{}` in package coordinate", parsed.version),
            )?)
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
    fn parse_install_style_version() {
        let (module_name, package_path, version) = parse("moonbitlang/parser/cmd/moonfmt@0.3.3");
        assert_eq!(module_name.to_string(), "moonbitlang/parser");
        assert_eq!(package_path, "cmd/moonfmt");
        assert_eq!(
            version,
            ExecutablePackageVersionSelector::Exact("0.3.3".parse().unwrap())
        );
    }

    #[test]
    fn parse_module_version_alias() {
        let (module_name, package_path, version) = parse("moonbitlang/parser@0.3.3/cmd/moonfmt");
        assert_eq!(module_name.to_string(), "moonbitlang/parser");
        assert_eq!(package_path, "cmd/moonfmt");
        assert_eq!(
            version,
            ExecutablePackageVersionSelector::Exact("0.3.3".parse().unwrap())
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

        let version = resolve_latest_version_with(
            &module_name,
            &UserLog::new(log::LevelFilter::Warn),
            true,
            LatestVersionResolutionPolicy::PreferLocal,
            || LatestVersionLookup::Found("0.3.3".parse().unwrap()),
            || {
                update_called = true;
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(version.to_string(), "0.3.3");
        assert!(!update_called);
    }

    #[test]
    fn latest_resolution_updates_after_local_registry_miss() {
        let module_name = "moonbitlang/parser".parse::<ModuleName>().unwrap();
        let mut lookup_count = 0;
        let mut update_called = false;

        let version = resolve_latest_version_with(
            &module_name,
            &UserLog::new(log::LevelFilter::Warn),
            true,
            LatestVersionResolutionPolicy::PreferLocal,
            || {
                lookup_count += 1;
                if lookup_count > 1 {
                    LatestVersionLookup::Found("0.3.3".parse().unwrap())
                } else {
                    LatestVersionLookup::NotFound
                }
            },
            || {
                update_called = true;
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(version.to_string(), "0.3.3");
        assert_eq!(lookup_count, 2);
        assert!(update_called);
    }

    #[test]
    fn explicit_latest_refreshes_before_resolving() {
        let module_name = "moonbitlang/parser".parse::<ModuleName>().unwrap();
        let registry_updated = Cell::new(false);
        let lookup_count = Cell::new(0);

        let version = resolve_latest_version_with(
            &module_name,
            &UserLog::new(log::LevelFilter::Warn),
            true,
            LatestVersionResolutionPolicy::Refresh,
            || {
                lookup_count.set(lookup_count.get() + 1);
                LatestVersionLookup::Found(
                    if registry_updated.get() {
                        "0.4.0"
                    } else {
                        "0.3.3"
                    }
                    .parse()
                    .unwrap(),
                )
            },
            || {
                registry_updated.set(true);
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(version.to_string(), "0.4.0");
        assert!(registry_updated.get());
        assert_eq!(lookup_count.get(), 1);
    }

    #[test]
    fn explicit_latest_fails_when_refresh_fails() {
        let module_name = "moonbitlang/parser".parse::<ModuleName>().unwrap();
        let lookup_called = Cell::new(false);

        let error = resolve_latest_version_with(
            &module_name,
            &UserLog::new(log::LevelFilter::Warn),
            true,
            LatestVersionResolutionPolicy::Refresh,
            || {
                lookup_called.set(true);
                LatestVersionLookup::Found("0.3.3".parse().unwrap())
            },
            || Err(anyhow::anyhow!("offline")),
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

        let error = resolve_latest_version_with(
            &module_name,
            &UserLog::new(log::LevelFilter::Warn),
            true,
            LatestVersionResolutionPolicy::PreferLocal,
            || LatestVersionLookup::NoVersionInformation,
            || {
                update_called = true;
                Ok(())
            },
        )
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Module `moonbitlang/parser` has no version information"
        );
        assert!(update_called);
    }

    #[test]
    fn root_package_uses_module_last_segment_for_artifact_name() {
        let package = ResolvedExecutablePackage {
            module_name: "moonbitlang/parser".parse().unwrap(),
            package_path: String::new(),
            version: "0.3.3".parse().unwrap(),
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
