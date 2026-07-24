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

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use moonbuild_rupes_recta::model::RunBackend;
use mooncake::registry::{OnlineRegistry, Registry, path as registry_path};
use moonutil::{
    locks::FileLock, registry::RegistryConfig, resolution::ModuleName, user_log::UserLog,
};
use semver::Version;

use crate::rr_build;

pub(crate) enum RegistryRunTarget {
    Wasm {
        experimental_policy: Option<PathBuf>,
    },
    Native,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedExecutablePackage {
    pub module_name: ModuleName,
    pub package_path: String,
    pub version: Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LatestVersionLookup {
    Found(Version),
    NoVersionInformation,
    NotFound,
}

impl ResolvedExecutablePackage {
    pub(super) fn artifact_name(&self, suffix: &str) -> String {
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

    pub(super) fn cache_path(&self, suffix: &str) -> PathBuf {
        let mut cache_path = moonutil::registry::cache()
            .join("assets")
            .join(self.module_name.username.as_str());
        for segment in self.module_name.unqual.split('/') {
            cache_path.push(segment);
        }
        cache_path.push(self.version.to_string());
        for segment in self
            .package_path
            .split('/')
            .filter(|segment| !segment.is_empty())
        {
            cache_path.push(segment);
        }
        cache_path.push(self.artifact_name(suffix));
        cache_path
    }
}

fn resolve_registry_package(
    package: &str,
    user_log: &UserLog,
) -> anyhow::Result<ResolvedExecutablePackage> {
    let (module_name, package_path, requested_version) =
        parse_executable_package_coordinate(package)?;
    let version = match requested_version {
        Some(version) => version,
        None => resolve_latest_version(&module_name, user_log)?,
    };
    Ok(ResolvedExecutablePackage {
        module_name,
        package_path,
        version,
    })
}

fn resolve_latest_version(module_name: &ModuleName, user_log: &UserLog) -> anyhow::Result<Version> {
    let index_dir = moonutil::registry::index();
    let registry_config = RegistryConfig::load();
    let had_index = index_dir.exists();

    resolve_latest_version_with(
        module_name,
        user_log,
        had_index,
        || latest_version_from_local_registry(module_name),
        || {
            mooncake::update::update_with_output(
                &index_dir,
                &registry_config,
                mooncake::update::UpdateOutput::Quiet,
            )
            .map(|_| ())
        },
    )
}

fn latest_version_from_local_registry(module_name: &ModuleName) -> LatestVersionLookup {
    let registry = OnlineRegistry::mooncakes_io();
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
    mut lookup_latest_version: impl FnMut() -> LatestVersionLookup,
    mut update_registry: impl FnMut() -> anyhow::Result<()>,
) -> anyhow::Result<Version> {
    if let LatestVersionLookup::Found(version) = lookup_latest_version() {
        user_log.info(format!(
            "Resolved {module_name} latest version to {version}"
        ));
        return Ok(version);
    }

    match update_registry() {
        Ok(_) => user_log.info("Updated registry index"),
        Err(e) => {
            if had_index {
                user_log.warn(format!(
                    "Failed to update registry index, using cached index: {}",
                    e
                ));
            } else {
                return Err(e).context("Failed to update registry index");
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
) -> anyhow::Result<(ModuleName, String, Option<Version>)> {
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
        let version = Version::parse(&parsed.version).with_context(|| {
            format!("Invalid version `{}` in package coordinate", parsed.version)
        })?;
        return Ok((parsed.module, parsed.package, Some(version)));
    }

    let parsed = registry_path::parse_install_style_path(input)
        .with_context(|| format!("Invalid package coordinate `{input}`"))?;
    Ok((parsed.module, parsed.package, None))
}

pub(super) fn ensure_cached_file(
    cache_path: &Path,
    user_log: &UserLog,
    produce: impl FnOnce(&Path) -> anyhow::Result<()>,
) -> anyhow::Result<PathBuf> {
    if cache_path.exists() {
        user_log.info(format!("Using cached {}", cache_path.to_string_lossy()));
        return Ok(cache_path.to_path_buf());
    }

    let parent = cache_path
        .parent()
        .context("registry cache path has no parent")?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create registry cache directory {}",
            parent.display()
        )
    })?;
    let _lock = FileLock::lock(parent)
        .with_context(|| format!("failed to lock cache directory {}", parent.display()))?;

    if cache_path.exists() {
        user_log.info(format!("Using cached {}", cache_path.to_string_lossy()));
        return Ok(cache_path.to_path_buf());
    }

    let staged = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create cache file in {}", parent.display()))?;
    produce(staged.path())?;
    staged
        .as_file()
        .sync_all()
        .with_context(|| format!("failed to sync cache file {}", staged.path().display()))?;
    staged
        .persist(cache_path)
        .with_context(|| format!("failed to publish cached file to {}", cache_path.display()))?;
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(cache_path.to_path_buf())
}

pub(crate) fn run(
    package: String,
    target: RegistryRunTarget,
    args: Vec<String>,
    quiet: bool,
    verbose: bool,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    let package = resolve_registry_package(&package, user_log)?;
    match target {
        RegistryRunTarget::Wasm {
            experimental_policy,
        } => {
            let wasm_path = super::runwasm::cached_wasm_path(&package, user_log)?;
            run_artifact(
                RunBackend::Wasm,
                &wasm_path,
                experimental_policy.as_deref(),
                &args,
                user_log,
            )
        }
        RegistryRunTarget::Native => {
            let executable = cached_native_executable(&package, user_log, quiet, verbose)?;
            run_artifact(RunBackend::Native, &executable, None, &args, user_log)
        }
    }
}

fn cached_native_executable(
    package: &ResolvedExecutablePackage,
    user_log: &UserLog,
    quiet: bool,
    verbose: bool,
) -> anyhow::Result<PathBuf> {
    let cache_path = package.cache_path(".exe");

    ensure_cached_file(&cache_path, user_log, |staged| {
        super::install_binary::build_registry_native_executable_to(
            &package.module_name,
            &package.version,
            &package.package_path,
            staged,
            quiet,
            verbose,
            user_log,
        )
    })
}

fn run_artifact(
    backend: RunBackend,
    artifact: &Path,
    experimental_policy: Option<&Path>,
    args: &[String],
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    let mut run_cmd = crate::run::command_for_with_moonrun_policy(
        backend,
        None,
        artifact,
        None,
        experimental_policy,
    );
    run_cmd.args(args);

    user_log.info(rr_build::format_dry_run_command(&run_cmd, Path::new(".")));

    let status = super::process::delegate(&mut run_cmd)
        .context("failed to delegate to registry executable")?;
    status
        .code()
        .context("registry executable exited without a return code")
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    fn parse(input: &str) -> (ModuleName, String, Option<Version>) {
        parse_executable_package_coordinate(input).unwrap()
    }

    #[test]
    fn parse_install_style_version() {
        let (module_name, package_path, version) = parse("moonbitlang/parser/cmd/moonfmt@0.3.3");
        assert_eq!(module_name.to_string(), "moonbitlang/parser");
        assert_eq!(package_path, "cmd/moonfmt");
        assert_eq!(version.unwrap().to_string(), "0.3.3");
    }

    #[test]
    fn parse_module_version_alias() {
        let (module_name, package_path, version) = parse("moonbitlang/parser@0.3.3/cmd/moonfmt");
        assert_eq!(module_name.to_string(), "moonbitlang/parser");
        assert_eq!(package_path, "cmd/moonfmt");
        assert_eq!(version.unwrap().to_string(), "0.3.3");
    }

    #[test]
    fn parse_latest_coordinate() {
        let (module_name, package_path, version) = parse("moonbitlang/parser/cmd/moonfmt");
        assert_eq!(module_name.to_string(), "moonbitlang/parser");
        assert_eq!(package_path, "cmd/moonfmt");
        assert_eq!(version, None);
    }

    #[test]
    fn latest_resolution_uses_local_registry_before_updating() {
        let module_name = "moonbitlang/parser".parse::<ModuleName>().unwrap();
        let mut update_called = false;

        let version = resolve_latest_version_with(
            &module_name,
            &UserLog::new(log::LevelFilter::Warn),
            true,
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
    fn latest_resolution_preserves_no_version_information_after_update() {
        let module_name = "moonbitlang/parser".parse::<ModuleName>().unwrap();
        let mut update_called = false;

        let err = resolve_latest_version_with(
            &module_name,
            &UserLog::new(log::LevelFilter::Warn),
            true,
            || LatestVersionLookup::NoVersionInformation,
            || {
                update_called = true;
                Ok(())
            },
        )
        .unwrap_err();

        assert_eq!(
            err.to_string(),
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

    #[test]
    fn failed_production_does_not_publish_a_cache_entry() {
        let cache = tempfile::TempDir::new().unwrap();
        let final_path = cache.path().join("artifact");

        let error = ensure_cached_file(&final_path, &UserLog::new(log::LevelFilter::Warn), |_| {
            bail!("producer failed")
        })
        .unwrap_err();

        assert_eq!(error.to_string(), "producer failed");
        assert!(!final_path.exists());
    }

    #[test]
    fn concurrent_cache_misses_produce_once_and_recheck_after_locking() {
        let cache = tempfile::TempDir::new().unwrap();
        let final_path = Arc::new(cache.path().join("artifact"));
        let start = Arc::new(Barrier::new(3));
        let production_count = Arc::new(AtomicUsize::new(0));

        let threads = (0..2)
            .map(|_| {
                let final_path = Arc::clone(&final_path);
                let start = Arc::clone(&start);
                let production_count = Arc::clone(&production_count);
                std::thread::spawn(move || {
                    start.wait();
                    ensure_cached_file(
                        &final_path,
                        &UserLog::new(log::LevelFilter::Warn),
                        |staged| {
                            production_count.fetch_add(1, Ordering::SeqCst);
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            std::fs::write(staged, b"artifact")?;
                            Ok(())
                        },
                    )
                })
            })
            .collect::<Vec<_>>();
        start.wait();

        for thread in threads {
            assert_eq!(
                std::fs::read(thread.join().unwrap().unwrap()).unwrap(),
                b"artifact"
            );
        }
        assert_eq!(production_count.load(Ordering::SeqCst), 1);
    }
}
