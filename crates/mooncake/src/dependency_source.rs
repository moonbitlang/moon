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
    child_process::ManagedChildRunner,
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
/// hidden behind the same [`DependencySource`] seam. The project implementation
/// receives the runner needed to preserve its legacy postadd behavior; the
/// immutable implementation never receives or executes that hook.
pub(crate) fn select<'a>(
    project_dir: impl Into<PathBuf>,
    cache: &'a CacheRoot,
    resolved: &ResolvedEnv,
    child: &'a ManagedChildRunner,
) -> anyhow::Result<Box<dyn DependencySource + 'a>> {
    let has_registry_sources = resolved
        .all_modules()
        .any(|module| matches!(module.source(), ModuleSourceKind::Registry) && !module.is_core());
    if has_registry_sources && let Some(root) = cache.initialize()? {
        return Ok(Box::new(ImmutableDependencySource::new(root)));
    }

    Ok(Box::new(ProjectDependencySource::new(project_dir, child)))
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
        child_process::{ChildOutputMode, ManagedChildRunner},
        manifest::MoonMod,
        resolution::{ModuleName, ModuleSource, ModuleSourceKind, ResolvedEnv},
        user_log::UserLog,
    };
    use semver::Version;

    use super::{
        global::{ImmutableDependencySource, SOURCE_ARCHIVE_CHECKSUM_FILE},
        select,
    };
    use crate::registry::{Registry, RegistryVersionInfo};

    const FIRST_CHECKSUM: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const SECOND_CHECKSUM: &str =
        "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    struct TestRegistry {
        version: Version,
        checksum: String,
        checksum_after_first_read: Option<String>,
        checksum_reads: AtomicUsize,
        postadd: bool,
        installations: AtomicUsize,
    }

    impl TestRegistry {
        fn new(postadd: bool) -> Self {
            Self {
                version: Version::new(1, 2, 3),
                checksum: FIRST_CHECKSUM.to_string(),
                checksum_after_first_read: None,
                checksum_reads: AtomicUsize::new(0),
                postadd,
                installations: AtomicUsize::new(0),
            }
        }

        fn with_checksum(mut self, checksum: &str) -> Self {
            self.checksum = checksum.to_string();
            self
        }

        fn with_checksum_after_first_read(mut self, checksum: &str) -> Self {
            self.checksum_after_first_read = Some(checksum.to_string());
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
                r#"
options(
  "scripts": {
    "postadd": "must-not-run",
  },
)
"#
            } else {
                ""
            };
            std::fs::write(
                to.join("moon.mod"),
                format!("name = \"{name}\"\nversion = \"{version}\"\n{scripts}"),
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

        fn acquire_source_to(
            &self,
            name: &ModuleName,
            version: &Version,
            expected_checksum: &str,
            to: &Path,
            _user_log: &UserLog,
        ) -> anyhow::Result<()> {
            std::fs::create_dir_all(to)?;
            self.install_source(name, version, to)?;
            std::fs::write(to.join("acquired-archive-checksum"), expected_checksum)?;
            Ok(())
        }

        fn source_archive_checksum(
            &self,
            _name: &ModuleName,
            _version: &Version,
        ) -> anyhow::Result<String> {
            let read = self.checksum_reads.fetch_add(1, Ordering::SeqCst);
            Ok(if read == 0 {
                &self.checksum
            } else {
                self.checksum_after_first_read
                    .as_ref()
                    .unwrap_or(&self.checksum)
            }
            .clone())
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

    fn managed_child() -> ManagedChildRunner {
        ManagedChildRunner::new(ChildOutputMode::Inherit, &user_log())
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
            store.source_dir(resolved.module_source(module)),
            cache.path().join("v1/sources/test/module/1.2.3")
        );
    }

    #[test]
    fn source_is_published_once_and_reused() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let cache = global_cache(&sandbox.path().join("cache"));
        let registry = TestRegistry::new(false);
        let (resolved, module) = test_env();
        let child = managed_child();
        let source = select(sandbox.path().join(".mooncakes"), &cache, &resolved, &child).unwrap();

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
        assert_eq!(
            std::fs::read_to_string(first[module].join(SOURCE_ARCHIVE_CHECKSUM_FILE)).unwrap(),
            FIRST_CHECKSUM
        );
    }

    #[test]
    fn postadd_is_rejected_before_publication() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let cache_dir = sandbox.path().join("cache");
        let cache = global_cache(&cache_dir);
        let registry = TestRegistry::new(true);
        let (resolved, module) = test_env();
        let child = managed_child();
        let source = select(sandbox.path().join(".mooncakes"), &cache, &resolved, &child).unwrap();
        let directory =
            ImmutableDependencySource::new(&cache_dir).source_dir(resolved.module_source(module));

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
        let child = managed_child();
        let source = select(sandbox.path().join(".mooncakes"), &cache, &resolved, &child).unwrap();

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
                let child = managed_child();
                let source = select(&project_dir, &cache, &resolved, &child).unwrap();
                source
                    .ensure(registry.as_ref(), &resolved, false, &user_log())
                    .unwrap();
            });
            let second = scope.spawn(|| {
                let child = managed_child();
                let source = select(&project_dir, &cache, &resolved, &child).unwrap();
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
    fn changed_checksum_is_rejected_without_replacing_the_source() {
        // `lijunchen/hello18@0.1.30` was historically replaced after
        // publication. Registry versions are nevertheless immutable from the
        // dependency source cache's perspective: accepting the replacement
        // requires an explicit cache clean.
        let sandbox = tempfile::TempDir::new().unwrap();
        let cache = global_cache(&sandbox.path().join("cache"));
        let first_registry = TestRegistry::new(false);
        let second_registry = TestRegistry::new(false).with_checksum(SECOND_CHECKSUM);
        let (resolved, module) = test_env();
        let child = managed_child();
        let source = select(sandbox.path().join(".mooncakes"), &cache, &resolved, &child).unwrap();

        let first = source
            .ensure(&first_registry, &resolved, false, &user_log())
            .unwrap();
        let sentinel = first[module].join("must-survive");
        std::fs::write(&sentinel, "original source").unwrap();

        let error = source
            .ensure(&second_registry, &resolved, false, &user_log())
            .unwrap_err();

        assert!(
            format!("{error:#}").contains("registry archive checksum changed"),
            "{error:#}"
        );
        assert!(format!("{error:#}").contains("moon clean --dep-cache"));
        assert_eq!(
            std::fs::read_to_string(sentinel).unwrap(),
            "original source"
        );
        assert_eq!(second_registry.installations.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn extraction_and_metadata_use_one_archive_checksum_read() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let cache_dir = sandbox.path().join("cache");
        let cache = global_cache(&cache_dir);
        let registry = TestRegistry::new(false).with_checksum_after_first_read(SECOND_CHECKSUM);
        let (resolved, module) = test_env();
        let child = managed_child();
        let source = select(sandbox.path().join(".mooncakes"), &cache, &resolved, &child).unwrap();

        let paths = source
            .ensure(&registry, &resolved, false, &user_log())
            .unwrap();

        assert_eq!(
            paths[module],
            ImmutableDependencySource::new(&cache_dir).source_dir(resolved.module_source(module))
        );
        assert_eq!(
            std::fs::read_to_string(paths[module].join("acquired-archive-checksum")).unwrap(),
            FIRST_CHECKSUM
        );
        assert_eq!(
            std::fs::read_to_string(paths[module].join(SOURCE_ARCHIVE_CHECKSUM_FILE)).unwrap(),
            FIRST_CHECKSUM
        );
        assert_eq!(registry.checksum_reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn source_with_missing_or_invalid_checksum_metadata_is_rejected() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let cache_dir = sandbox.path().join("cache");
        let cache = global_cache(&cache_dir);
        let registry = TestRegistry::new(false);
        let (resolved, module) = test_env();
        let child = managed_child();
        let source = select(sandbox.path().join(".mooncakes"), &cache, &resolved, &child).unwrap();
        let directory =
            ImmutableDependencySource::new(&cache_dir).source_dir(resolved.module_source(module));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("moon.mod"),
            "name = \"test/module\"\nversion = \"1.2.3\"\n",
        )
        .unwrap();

        let missing_error = source
            .ensure(&registry, &resolved, false, &user_log())
            .unwrap_err();

        assert!(
            format!("{missing_error:#}").contains("missing archive checksum metadata"),
            "{missing_error:#}"
        );
        assert!(format!("{missing_error:#}").contains("moon clean --dep-cache"));

        std::fs::write(directory.join(SOURCE_ARCHIVE_CHECKSUM_FILE), "not-a-sha256").unwrap();
        let invalid_error = source
            .ensure(&registry, &resolved, false, &user_log())
            .unwrap_err();

        assert!(
            format!("{invalid_error:#}").contains("invalid archive checksum metadata"),
            "{invalid_error:#}"
        );
        assert!(format!("{invalid_error:#}").contains("moon clean --dep-cache"));
        assert_eq!(registry.installations.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn disabled_cache_uses_project_sources() {
        let sandbox = tempfile::TempDir::new().unwrap();
        let project_dir = sandbox.path().join(".mooncakes");
        let cache = CacheRoot::Disabled;
        let registry = TestRegistry::new(false);
        let (resolved, module) = test_env();
        let user_log = user_log();
        let child = ManagedChildRunner::new(ChildOutputMode::Inherit, &user_log);
        let source = select(&project_dir, &cache, &resolved, &child).unwrap();

        let paths = source
            .ensure(&registry, &resolved, false, &user_log)
            .unwrap();

        assert_eq!(paths[module], project_dir.join("test/module"));
        assert!(paths[module].join("moon.mod").is_file());
        assert_eq!(registry.installations.load(Ordering::SeqCst), 1);
    }
}
