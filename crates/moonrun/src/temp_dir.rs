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

//! Run-owned temporary-directory selection.

use std::ffi::OsString;
use std::sync::Arc;

use crate::async_host::AsyncHostResult;
use crate::policy::{Env, Policy};

/// Selects the temporary-directory base observed by one Run.
pub(crate) struct TempDir {
    environment: Arc<Env>,
    sandboxed: bool,
}

impl TempDir {
    pub(crate) fn new(environment: Arc<Env>, policy: &Policy) -> Self {
        Self {
            environment,
            sandboxed: policy.has_env_policy(),
        }
    }

    pub(crate) fn path(&self) -> AsyncHostResult<OsString> {
        platform::resolve(&self.environment, self.sandboxed)
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use crate::async_sys::fs::stub;

    pub(super) fn resolve(environment: &Env, _sandboxed: bool) -> AsyncHostResult<OsString> {
        stub::get_tmp_path_from_env(environment.get_os("TMPDIR"))
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use crate::async_host::AsyncHostError;
    use crate::async_sys::fs::stub;

    pub(super) fn resolve(environment: &Env, sandboxed: bool) -> AsyncHostResult<OsString> {
        match stub::get_tmp_path_from_env(environment.get_os("TMP"), environment.get_os("TEMP")) {
            Err(AsyncHostError::PermissionDenied) if !sandboxed => stub::get_tmp_path(),
            result => result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use crate::async_host::AsyncHostError;
    use crate::policy::InitialEnv;

    fn environment(
        policy: &Policy,
        entries: impl IntoIterator<Item = (&'static str, &'static str)>,
    ) -> Arc<Env> {
        Arc::new(
            policy
                .realize_env(InitialEnv::Explicit(
                    entries
                        .into_iter()
                        .map(|(name, value)| (name.into(), value.into()))
                        .collect(),
                ))
                .unwrap(),
        )
    }

    fn sandboxed_policy() -> Policy {
        let directory = tempfile::tempdir().unwrap();
        let policy_file = directory.path().join("policy.toml");
        std::fs::write(&policy_file, "").unwrap();
        Policy::from_file(&policy_file).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn temp_directory_reflects_run_environment_changes() {
        use std::os::unix::ffi::OsStrExt;

        let policy = Policy::allow_all();
        let environment = environment(&policy, [("TMPDIR", "/first")]);
        let temp_dir = TempDir::new(Arc::clone(&environment), &policy);

        assert_eq!(temp_dir.path().unwrap().as_bytes(), b"/first/");
        environment.set("TMPDIR".into(), "/second".into());
        assert_eq!(temp_dir.path().unwrap().as_bytes(), b"/second/");
        environment.unset("TMPDIR");
        assert_eq!(temp_dir.path().unwrap().as_bytes(), b"/tmp/");
    }

    #[cfg(all(unix, not(target_os = "android")))]
    #[test]
    fn sandboxed_unix_run_uses_constant_fallback_without_host_env() {
        use std::os::unix::ffi::OsStrExt;

        let policy = sandboxed_policy();
        let temp_dir = TempDir::new(environment(&policy, []), &policy);

        assert_eq!(temp_dir.path().unwrap().as_bytes(), b"/tmp/");
    }

    #[cfg(windows)]
    #[test]
    fn empty_tmp_falls_back_to_current_run_temp() {
        let policy = Policy::allow_all();
        let temp_dir = TempDir::new(
            environment(&policy, [("TMP", ""), ("TEMP", "C:/Fallback")]),
            &policy,
        );

        assert_eq!(temp_dir.path().unwrap().to_string_lossy(), "C:/Fallback\\");
    }

    #[cfg(windows)]
    #[test]
    fn unrestricted_windows_run_uses_native_fallback() {
        let policy = Policy::allow_all();
        let temp_dir = TempDir::new(environment(&policy, [("APP_ENV", "test")]), &policy);

        assert!(!temp_dir.path().unwrap().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn sandboxed_windows_run_observes_current_environment_without_native_fallback() {
        let policy = sandboxed_policy();
        let environment = environment(&policy, []);
        let temp_dir = TempDir::new(Arc::clone(&environment), &policy);

        assert_eq!(temp_dir.path(), Err(AsyncHostError::PermissionDenied));
        environment.set("TEMP".into(), "C:/Temp".into());
        assert_eq!(temp_dir.path().unwrap().to_string_lossy(), "C:/Temp\\");
        environment.unset("TEMP");
        assert_eq!(temp_dir.path(), Err(AsyncHostError::PermissionDenied));
    }
}
