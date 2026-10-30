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
            policy,
            || self.cached_latest_version(module_name),
            || self.query_latest_version(module_name, user_log),
        )
    }
}

fn resolve_latest_module_with(
    module_name: &ModuleName,
    user_log: &UserLog,
    policy: LatestVersionResolutionPolicy,
    mut cached_latest_version: impl FnMut() -> anyhow::Result<Option<Version>>,
    mut query_latest_version: impl FnMut() -> anyhow::Result<Version>,
) -> anyhow::Result<ModuleSource> {
    let version = match policy {
        LatestVersionResolutionPolicy::PreferLocal => match cached_latest_version()? {
            Some(version) => version,
            None => query_latest_version()?,
        },
        LatestVersionResolutionPolicy::Refresh => query_latest_version()?,
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
            let registry_path::ModuleRequest::Versioned(module) = parsed
                .module_request()
                .with_context(|| format!("Invalid version in package coordinate `{input}`"))?
            else {
                unreachable!("versioned path has a version");
            };
            ExecutablePackageVersionSelector::Exact(module)
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
                policy,
                || Ok(Some(Version::new(1, 5, 0))),
                || Ok(Version::new(1, 5, 0)),
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
    fn unpinned_coordinate_uses_cached_latest_without_querying() {
        let module = resolve_latest_module_with(
            &"moonbitlang/parser".into(),
            &UserLog::new(log::LevelFilter::Warn),
            LatestVersionResolutionPolicy::PreferLocal,
            || Ok(Some(Version::new(0, 3, 3))),
            || panic!("cached latest should satisfy an unpinned coordinate"),
        )
        .unwrap();
        assert_eq!(module.version(), &Version::new(0, 3, 3));
    }

    #[test]
    fn unpinned_coordinate_queries_latest_after_cache_miss() {
        let mut cached_reads = 0;
        let mut queries = 0;
        let module = resolve_latest_module_with(
            &"moonbitlang/parser".into(),
            &UserLog::new(log::LevelFilter::Warn),
            LatestVersionResolutionPolicy::PreferLocal,
            || {
                cached_reads += 1;
                Ok(None)
            },
            || {
                queries += 1;
                Ok(Version::new(0, 3, 3))
            },
        )
        .unwrap();
        assert_eq!(module.version(), &Version::new(0, 3, 3));
        assert_eq!(cached_reads, 1);
        assert_eq!(queries, 1);
    }

    #[test]
    fn explicit_latest_queries_without_reading_cached_latest() {
        let module = resolve_latest_module_with(
            &"moonbitlang/parser".into(),
            &UserLog::new(log::LevelFilter::Warn),
            LatestVersionResolutionPolicy::Refresh,
            || panic!("explicit latest must query the registry"),
            || Ok(Version::new(0, 4, 0)),
        )
        .unwrap();
        assert_eq!(module.version(), &Version::new(0, 4, 0));
    }

    #[test]
    fn explicit_latest_preserves_query_failure_without_stale_fallback() {
        let error = resolve_latest_module_with(
            &"moonbitlang/parser".into(),
            &UserLog::new(log::LevelFilter::Warn),
            LatestVersionResolutionPolicy::Refresh,
            || panic!("a failed query must not fall back to cached latest"),
            || Err(anyhow::anyhow!("offline").context("Failed to update registry index")),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "Failed to update registry index");
        assert_eq!(error.source().unwrap().to_string(), "offline");
    }

    #[test]
    fn unpinned_coordinate_preserves_query_errors_after_cache_miss() {
        for message in [
            "offline",
            "Module `moonbitlang/parser` has no version information",
            "Module `moonbitlang/parser` not found in registry",
        ] {
            let error = resolve_latest_module_with(
                &"moonbitlang/parser".into(),
                &UserLog::new(log::LevelFilter::Warn),
                LatestVersionResolutionPolicy::PreferLocal,
                || Ok(None),
                || Err(anyhow::anyhow!("{message}")),
            )
            .unwrap_err();
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn cached_read_error_does_not_trigger_a_query() {
        let error = resolve_latest_module_with(
            &"moonbitlang/parser".into(),
            &UserLog::new(log::LevelFilter::Warn),
            LatestVersionResolutionPolicy::PreferLocal,
            || Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()),
            || panic!("an unreadable cache is not a cache miss"),
        )
        .unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::PermissionDenied
        );
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
