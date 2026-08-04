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

//! Selection of dependency sources used by one resolution.

mod global;
mod project;

use std::path::PathBuf;

use moonutil::{
    cache::CacheRoot,
    resolution::{DirSyncResult, ModuleSourceKind, ResolvedEnv},
    user_log::UserLog,
};

use crate::registry::Registry;

use self::{global::ImmutableDependencySource, project::ProjectDependencySource};

pub(crate) trait DependencySource {
    /// Ensure that every resolved dependency has a source directory and return
    /// those directories indexed by module ID.
    fn ensure(
        &self,
        registry: &dyn Registry,
        resolved: &ResolvedEnv,
        frozen: bool,
        user_log: &UserLog,
    ) -> anyhow::Result<DirSyncResult>;
}

/// Select the dependency source adapter for one resolution.
///
/// The project-local `.mooncakes` directory and the shared immutable cache are
/// hidden behind the same [`DependencySource`] seam.
pub(crate) fn select<'a>(
    project_dir: impl Into<PathBuf>,
    cache: &'a CacheRoot,
    resolved: &ResolvedEnv,
) -> anyhow::Result<Box<dyn DependencySource + 'a>> {
    let has_registry_sources = resolved
        .all_modules()
        .any(|module| matches!(module.source(), ModuleSourceKind::Registry) && !module.is_core());
    if has_registry_sources && let Some(root) = cache.initialize()? {
        return Ok(Box::new(ImmutableDependencySource::new(root)));
    }

    Ok(Box::new(ProjectDependencySource::new(project_dir)))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        path::Path,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use moonutil::{
        cache::{CacheKind, CacheRoot},
        manifest::MoonMod,
        resolution::{ModuleName, ModuleSource, ModuleSourceKind, ResolvedEnv},
        user_log::UserLog,
    };
    use semver::Version;

    use super::{global::ImmutableDependencySource, select};
    use crate::registry::{Registry, RegistryVersionInfo};

    struct TestRegistry {
        version: Version,
        checksum: String,
        postadd: bool,
        installations: AtomicUsize,
    }

    impl TestRegistry {
        fn new(postadd: bool) -> Self {
            Self {
                version: Version::new(1, 2, 3),
                checksum: "0123456789abcdef".to_string(),
                postadd,
                installations: AtomicUsize::new(0),
            }
        }

        fn with_checksum(mut self, checksum: &str) -> Self {
            self.checksum = checksum.to_string();
            self
        }

        fn install_source(
            &self,
            name: &ModuleName,
            version: &Version,
            to: &Path,
        ) -> anyhow::Result<()> {
            self.installations.fetch_add(1, Ordering::SeqCst);
            let scripts = if self.postadd {
                r#", "scripts": {"postadd": "must-not-run"}"#
            } else {
                ""
            };
            std::fs::write(
                to.join("moon.mod.json"),
                format!(r#"{{"name":"{name}","version":"{version}"{scripts}}}"#),
            )?;
            Ok(())
        }
    }

    impl Registry for TestRegistry {
        fn all_versions_of(
            &self,
            _name: &ModuleName,
        ) -> anyhow::Result<Arc<BTreeMap<Version, RegistryVersionInfo>>> {
            Ok(Arc::new(BTreeMap::from([(
                self.version.clone(),
                RegistryVersionInfo {
                    deps: Default::default(),
                },
            )])))
        }

        fn install_to(
            &self,
            name: &ModuleName,
            version: &Version,
            to: &Path,
            _quiet: bool,
        ) -> anyhow::Result<()> {
            std::fs::create_dir_all(to)?;
            self.install_source(name, version, to)
        }

        fn extract_to(
            &self,
            name: &ModuleName,
            version: &Version,
            to: &Path,
            _quiet: bool,
        ) -> anyhow::Result<()> {
            self.install_source(name, version, to)
        }

        fn source_checksum(
            &self,
            _name: &ModuleName,
            _version: &Version,
        ) -> anyhow::Result<String> {
            Ok(self.checksum.clone())
        }
    }

    fn test_env() -> (ResolvedEnv, moonutil::resolution::ModuleId) {
        let version = Version::new(1, 2, 3);
        let source = ModuleSource::new_full(
            "test/module".into(),
            version.clone(),
            ModuleSourceKind::Registry,
        );
        ResolvedEnv::only_one_module(
            source,
            MoonMod {
                name: "test/module".to_string(),
                version: Some(version),
                ..Default::default()
            },
        )
    }

    fn user_log() -> UserLog {
        UserLog::new(log::LevelFilter::Error)
    }

    fn global_cache(path: &Path) -> CacheRoot {
        CacheRoot::Path {
            kind: CacheKind::DependencySources,
            path: path.to_path_buf(),
        }
    }

    #[test]
    fn source_layout_is_canonical() {
        let cache = tempfile::TempDir::new().unwrap();
        let store = ImmutableDependencySource::new(cache.path());
        let (resolved, module) = test_env();

        assert_eq!(
            store.source_dir(resolved.module_source(module), "0123456789abcdef"),
            cache
                .path()
                .join("v1/sources/test/module/1.2.3/0123456789abcdef")
        );
    }

    #[test]
    fn source_is_published_once_and_reused() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let cache = global_cache(&sandbox.path().join("cache"));
        let registry = TestRegistry::new(false);
        let (resolved, module) = test_env();
        let source = select(sandbox.path().join(".mooncakes"), &cache, &resolved).unwrap();

        let first = source
            .ensure(&registry, &resolved, false, &user_log())
            .unwrap();
        let sentinel = first[module].join("reuse-sentinel");
        std::fs::write(&sentinel, "keep").unwrap();
        let second = source
            .ensure(&registry, &resolved, false, &user_log())
            .unwrap();

        assert_eq!(first[module], second[module]);
        assert_eq!(registry.installations.load(Ordering::SeqCst), 1);
        assert_eq!(std::fs::read_to_string(sentinel).unwrap(), "keep");
    }

    #[test]
    fn postadd_is_rejected_before_publication() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let cache_dir = sandbox.path().join("cache");
        let cache = global_cache(&cache_dir);
        let registry = TestRegistry::new(true);
        let (resolved, module) = test_env();
        let source = select(sandbox.path().join(".mooncakes"), &cache, &resolved).unwrap();
        let directory = ImmutableDependencySource::new(&cache_dir)
            .source_dir(resolved.module_source(module), &registry.checksum);

        let error = source
            .ensure(&registry, &resolved, false, &user_log())
            .unwrap_err();

        assert!(
            format!("{error:#}").contains("scripts.postadd"),
            "{error:#}"
        );
        assert_eq!(registry.installations.load(Ordering::SeqCst), 1);
        assert!(!directory.exists());
    }

    #[test]
    fn frozen_mode_rejects_a_missing_source() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let cache = global_cache(&sandbox.path().join("cache"));
        let registry = TestRegistry::new(false);
        let (resolved, _) = test_env();
        let source = select(sandbox.path().join(".mooncakes"), &cache, &resolved).unwrap();

        let error = source
            .ensure(&registry, &resolved, true, &user_log())
            .unwrap_err();

        assert!(
            format!("{error:#}").contains("`frozen` is set"),
            "{error:#}"
        );
        assert_eq!(registry.installations.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn concurrent_publication_installs_once() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let project_dir = sandbox.path().join(".mooncakes");
        let cache = global_cache(&sandbox.path().join("cache"));
        let registry = Arc::new(TestRegistry::new(false));
        let (resolved, _) = test_env();

        std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                let source = select(&project_dir, &cache, &resolved).unwrap();
                source
                    .ensure(registry.as_ref(), &resolved, false, &user_log())
                    .unwrap();
            });
            let second = scope.spawn(|| {
                let source = select(&project_dir, &cache, &resolved).unwrap();
                source
                    .ensure(registry.as_ref(), &resolved, false, &user_log())
                    .unwrap();
            });
            first.join().unwrap();
            second.join().unwrap();
        });

        assert_eq!(registry.installations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn changed_checksum_publishes_a_distinct_source() {
        // `lijunchen/hello18@0.1.30` was historically replaced after
        // publication, so module and version alone are not a safe physical
        // cache identity even though registry versions should be immutable.
        let sandbox = tempfile::TempDir::new().unwrap();
        let cache = global_cache(&sandbox.path().join("cache"));
        let first_registry = TestRegistry::new(false);
        let second_registry = TestRegistry::new(false).with_checksum("fedcba9876543210");
        let (resolved, module) = test_env();
        let source = select(sandbox.path().join(".mooncakes"), &cache, &resolved).unwrap();

        let first = source
            .ensure(&first_registry, &resolved, false, &user_log())
            .unwrap();
        let second = source
            .ensure(&second_registry, &resolved, false, &user_log())
            .unwrap();

        assert_ne!(first[module], second[module]);
        assert!(first[module].join("moon.mod.json").is_file());
        assert!(second[module].join("moon.mod.json").is_file());
    }

    #[test]
    fn disabled_cache_uses_project_sources() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let project_dir = sandbox.path().join(".mooncakes");
        let cache = CacheRoot::Disabled;
        let registry = TestRegistry::new(false);
        let (resolved, module) = test_env();
        let source = select(&project_dir, &cache, &resolved).unwrap();

        let paths = source
            .ensure(&registry, &resolved, false, &user_log())
            .unwrap();

        assert_eq!(paths[module], project_dir.join("test/module"));
        assert!(paths[module].join("moon.mod.json").is_file());
        assert_eq!(registry.installations.load(Ordering::SeqCst), 1);
    }
}
