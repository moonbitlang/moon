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
use crate::policy::Policy;
use crate::runtime::Env;

/// Selects the temporary-directory base observed by one Runtime.
pub(crate) struct TempDir {
    environment: Arc<Env>,
    source: TempDirSource,
}

#[derive(Clone, Copy)]
enum TempDirSource {
    Native,
    RunEnvironment,
}

impl TempDir {
    pub(crate) fn new(environment: Arc<Env>, policy: &Policy) -> Self {
        Self {
            environment,
            source: if policy.has_env_policy() {
                TempDirSource::RunEnvironment
            } else {
                TempDirSource::Native
            },
        }
    }

    pub(crate) fn path(&self) -> AsyncHostResult<OsString> {
        match self.source {
            TempDirSource::Native => crate::async_sys::fs::stub::get_tmp_path(),
            TempDirSource::RunEnvironment => resolve_from_run_environment(&self.environment),
        }
    }
}

#[cfg(unix)]
fn resolve_from_run_environment(environment: &Env) -> AsyncHostResult<OsString> {
    crate::async_sys::fs::stub::get_tmp_path_from_env(environment.get("TMPDIR".as_ref()))
}

#[cfg(windows)]
fn resolve_from_run_environment(environment: &Env) -> AsyncHostResult<OsString> {
    crate::async_sys::fs::stub::get_tmp_path_from_env(
        environment.get("TMP".as_ref()),
        environment.get("TEMP".as_ref()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use crate::async_host::AsyncHostError;
    #[cfg(unix)]
    use crate::async_sys::fs::stub;

    fn load_temp_dir(contents: &str) -> (Arc<Env>, TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let policy_file = dir.path().join("policy.toml");
        std::fs::write(&policy_file, contents).unwrap();
        let policy = Policy::from_file(&policy_file).unwrap();
        let environment = Arc::new(policy.realize_env().unwrap());
        let temp_dir = TempDir::new(Arc::clone(&environment), &policy);
        (environment, temp_dir)
    }

    #[cfg(unix)]
    #[test]
    fn policy_tmp_path_uses_policy_tmpdir() {
        use std::os::unix::ffi::OsStrExt;

        let (_, temp_dir) = load_temp_dir("[env.set]\nTMPDIR = \"/policy/tmp\"\n");

        assert_eq!(temp_dir.path().unwrap().as_bytes(), b"/policy/tmp/");
    }

    #[cfg(unix)]
    #[test]
    fn tmp_path_reflects_policy_env_changes() {
        use std::os::unix::ffi::OsStrExt;

        let (environment, temp_dir) = load_temp_dir("[env.set]\nTMPDIR = \"/first\"\n");

        assert_eq!(temp_dir.path().unwrap().as_bytes(), b"/first/");
        environment.set("TMPDIR".into(), "/second".into()).unwrap();
        assert_eq!(temp_dir.path().unwrap().as_bytes(), b"/second/");
        environment.unset("TMPDIR".as_ref()).unwrap();
        assert_eq!(
            temp_dir.path().unwrap(),
            stub::get_tmp_path_from_env(None).unwrap()
        );
    }

    #[cfg(all(unix, not(target_os = "android")))]
    #[test]
    fn policy_tmp_path_ignores_denied_host_tmpdir() {
        use std::os::unix::ffi::OsStrExt;

        let (_, temp_dir) = load_temp_dir("");

        assert_eq!(temp_dir.path().unwrap().as_bytes(), b"/tmp/");
    }

    #[cfg(windows)]
    #[test]
    fn policy_tmp_path_requires_configured_windows_temp_env() {
        let (environment, temp_dir) = load_temp_dir("");

        assert_eq!(temp_dir.path(), Err(AsyncHostError::PermissionDenied));
        environment.set("TEMP".into(), "C:/Temp".into()).unwrap();
        assert_eq!(temp_dir.path().unwrap().to_string_lossy(), "C:/Temp\\");
        environment.unset("TEMP".as_ref()).unwrap();
        assert_eq!(temp_dir.path(), Err(AsyncHostError::PermissionDenied));
    }

    #[cfg(windows)]
    #[test]
    fn empty_policy_tmp_falls_back_to_temp() {
        let (_, temp_dir) = load_temp_dir("[env.set]\nTMP = \"\"\nTEMP = \"C:/Fallback\"\n");

        assert_eq!(temp_dir.path().unwrap().to_string_lossy(), "C:/Fallback\\");
    }
}
