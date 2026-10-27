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

//! Freshness and fallback choices shared by registry-consuming commands.

use mooncake::registry::{
    Registry, RegistryClient, RegistryUnavailable,
    path::{ModuleNotFound, ModuleRequest},
};
use moonutil::{
    resolution::{ModuleName, ModuleSource},
    user_log::UserLog,
};
use semver::Version;

/// Select a module under the command's metadata-query policy. When remote queries
/// are allowed, unversioned requests query current latest, with a warned fallback
/// if unavailable. Disabling remote queries requires a cached latest answer.
pub(super) fn resolve_module(
    registry: &RegistryClient,
    request: ModuleRequest,
    allow_remote: bool,
    user_log: &UserLog,
) -> anyhow::Result<ModuleSource> {
    match request {
        ModuleRequest::Versioned(module) => {
            if allow_remote {
                let name = module.name();
                let version = module.version();
                match registry.refresh_version_info(name, version, user_log) {
                    Ok(Some(_)) => {}
                    Ok(None) => anyhow::bail!("Module `{name}` has no version {version}"),
                    Err(error) if error.is::<RegistryUnavailable>() => {
                        if registry.cached_version_info(name, version)?.is_none() {
                            return Err(error);
                        }
                        user_log.warn(format!(
                            "failed to refresh {name}@{version}, using cached metadata: {error:#}"
                        ));
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok(module)
        }
        ModuleRequest::Unversioned(name) => {
            let version = if allow_remote {
                query_latest_with_cached_fallback(registry, &name, user_log)?
            } else {
                registry
                    .cached_latest_version(&name)?
                    .ok_or_else(|| ModuleNotFound(name.clone()))?
            };
            Ok(ModuleSource::from_version(name, version)?)
        }
    }
}

/// These commands prefer current latest, but permit a known cached version
/// when the registry is unavailable. An authoritative result, including
/// absence or invalid metadata, must never be replaced with stale local data.
fn query_latest_with_cached_fallback(
    registry: &impl Registry,
    name: &ModuleName,
    user_log: &UserLog,
) -> anyhow::Result<Version> {
    match registry.query_latest_version(name, user_log) {
        Ok(version) => Ok(version),
        Err(error) if error.is::<RegistryUnavailable>() => {
            let Some(version) = registry.cached_latest_version(name)? else {
                return Err(error);
            };
            user_log.warn(format!(
                "failed to query latest version of {name}, using cached version {version}: {error:#}"
            ));
            Ok(version)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mooncake::registry::{RegistryModuleNotFound, RegistryVersionInfo};
    use std::cell::Cell;

    struct TestRegistry<Q, C> {
        query: Q,
        cached: C,
    }

    impl<Q, C> Registry for TestRegistry<Q, C>
    where
        Q: Fn() -> anyhow::Result<Version>,
        C: Fn() -> anyhow::Result<Option<Version>>,
    {
        fn version_info(
            &self,
            _name: &ModuleName,
            _version: &Version,
        ) -> anyhow::Result<Option<RegistryVersionInfo>> {
            panic!("latest selection must not query an exact release")
        }

        fn cached_latest_version(&self, _name: &ModuleName) -> anyhow::Result<Option<Version>> {
            (self.cached)()
        }

        fn query_latest_version(
            &self,
            _name: &ModuleName,
            _user_log: &UserLog,
        ) -> anyhow::Result<Version> {
            (self.query)()
        }
    }

    #[test]
    fn current_answer_does_not_read_cached_latest() {
        let queries = Cell::new(0);
        let registry = TestRegistry {
            query: || {
                queries.set(queries.get() + 1);
                Ok(Version::new(0, 2, 0))
            },
            cached: || panic!("a successful current query needs no fallback"),
        };
        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
        assert_eq!(
            query_latest_with_cached_fallback(&registry, &"test/tool".into(), &user_log).unwrap(),
            Version::new(0, 2, 0)
        );
        assert_eq!(queries.get(), 1);
        assert!(capture.take().is_empty());
    }

    #[test]
    fn unavailable_registry_warns_before_using_cached_latest() {
        let registry = TestRegistry {
            query: || Err(RegistryUnavailable(anyhow::anyhow!("offline")).into()),
            cached: || Ok(Some(Version::new(0, 1, 0))),
        };
        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
        assert_eq!(
            query_latest_with_cached_fallback(&registry, &"test/tool".into(), &user_log).unwrap(),
            Version::new(0, 1, 0)
        );
        let entries = capture.take();
        assert_eq!(entries.len(), 1);
        assert!(matches!(
            entries[0].level,
            moonutil::user_log::UserLogEntryLevel::Warning
        ));
        assert_eq!(
            entries[0].message,
            "failed to query latest version of test/tool, using cached version 0.1.0: offline"
        );
    }

    #[test]
    fn cache_miss_preserves_remote_failure() {
        let registry = TestRegistry {
            query: || Err(RegistryUnavailable(anyhow::anyhow!("offline")).into()),
            cached: || Ok(None),
        };
        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
        let error = query_latest_with_cached_fallback(&registry, &"test/tool".into(), &user_log)
            .unwrap_err();
        assert!(error.is::<RegistryUnavailable>());
        assert_eq!(error.to_string(), "offline");
        assert!(capture.take().is_empty());
    }

    #[test]
    fn authoritative_failure_does_not_fall_back_to_cached_latest() {
        for error in [
            anyhow::Error::new(RegistryModuleNotFound("test/tool".into())),
            anyhow::anyhow!("invalid metadata"),
        ] {
            let error = std::cell::RefCell::new(Some(error));
            let registry = TestRegistry {
                query: || Err(error.borrow_mut().take().unwrap()),
                cached: || panic!("an authoritative failure must not consult stale data"),
            };
            let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
            assert!(
                query_latest_with_cached_fallback(&registry, &"test/tool".into(), &user_log)
                    .is_err()
            );
            assert!(capture.take().is_empty());
        }
    }

    #[test]
    fn unreadable_cache_is_not_treated_as_a_miss() {
        let registry = TestRegistry {
            query: || Err(RegistryUnavailable(anyhow::anyhow!("offline")).into()),
            cached: || Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()),
        };
        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
        let error = query_latest_with_cached_fallback(&registry, &"test/tool".into(), &user_log)
            .unwrap_err();
        assert!(error.is::<std::io::Error>());
        assert!(capture.take().is_empty());
    }
}
