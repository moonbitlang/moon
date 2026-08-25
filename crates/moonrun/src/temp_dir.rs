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

use std::ffi::{OsStr, OsString};
use std::sync::{Arc, OnceLock};

use crate::async_host::AsyncHostResult;
use crate::async_sys::fs::stub;
use crate::policy::Policy;

/// The temporary-directory base observed by one Run.
///
/// Cache both success and failure so the length and value imports observe one
/// stable path even if the guest mutates its environment between those calls.
pub(crate) struct TempDir {
    policy: Arc<Policy>,
    path: OnceLock<AsyncHostResult<OsString>>,
}

impl TempDir {
    pub(crate) fn new(policy: Arc<Policy>) -> Self {
        Self {
            policy,
            path: OnceLock::new(),
        }
    }

    pub(crate) fn path(&self) -> AsyncHostResult<&OsStr> {
        self.path
            .get_or_init(|| resolve(&self.policy))
            .as_ref()
            .map(OsString::as_os_str)
            .map_err(|error| *error)
    }
}

fn resolve(policy: &Policy) -> AsyncHostResult<OsString> {
    if !policy.has_env_policy() {
        return stub::get_tmp_path();
    }
    resolve_from_policy_env(policy)
}

#[cfg(unix)]
fn resolve_from_policy_env(policy: &Policy) -> AsyncHostResult<OsString> {
    stub::get_tmp_path_from_env(policy.env_var_os("TMPDIR"))
}

#[cfg(windows)]
fn resolve_from_policy_env(policy: &Policy) -> AsyncHostResult<OsString> {
    stub::get_tmp_path_from_env(policy.env_var_os("TMP"), policy.env_var_os("TEMP"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use crate::async_host::AsyncHostError;

    fn policy(contents: &str) -> Arc<Policy> {
        let dir = tempfile::tempdir().unwrap();
        let policy_file = dir.path().join("policy.toml");
        std::fs::write(&policy_file, contents).unwrap();
        Arc::new(Policy::from_file(&policy_file).unwrap())
    }

    #[cfg(unix)]
    #[test]
    fn policy_tmp_path_uses_policy_tmpdir() {
        use std::os::unix::ffi::OsStrExt;

        let temp_dir = TempDir::new(policy("[env.set]\nTMPDIR = \"/policy/tmp\"\n"));

        assert_eq!(temp_dir.path().unwrap().as_bytes(), b"/policy/tmp/");
    }

    #[cfg(unix)]
    #[test]
    fn first_tmp_path_result_is_cached() {
        use std::os::unix::ffi::OsStrExt;

        let policy = policy("[env.set]\nTMPDIR = \"/first\"\n");
        let temp_dir = TempDir::new(Arc::clone(&policy));

        assert_eq!(temp_dir.path().unwrap().as_bytes(), b"/first/");
        policy.set_env_var("TMPDIR".to_owned(), "/second".to_owned());
        assert_eq!(temp_dir.path().unwrap().as_bytes(), b"/first/");
    }

    #[cfg(all(unix, not(target_os = "android")))]
    #[test]
    fn policy_tmp_path_ignores_denied_host_tmpdir() {
        use std::os::unix::ffi::OsStrExt;

        let temp_dir = TempDir::new(policy(""));

        assert_eq!(temp_dir.path().unwrap().as_bytes(), b"/tmp/");
    }

    #[cfg(windows)]
    #[test]
    fn policy_tmp_path_requires_configured_windows_temp_env() {
        let policy = policy("");
        let temp_dir = TempDir::new(Arc::clone(&policy));

        assert_eq!(temp_dir.path(), Err(AsyncHostError::PermissionDenied));
        policy.set_env_var("TEMP".to_owned(), "C:/TooLate".to_owned());
        assert_eq!(temp_dir.path(), Err(AsyncHostError::PermissionDenied));

        let temp_dir = TempDir::new(policy("[env.set]\nTEMP = \"C:/Temp\"\n"));
        assert_eq!(temp_dir.path().unwrap().to_string_lossy(), "C:/Temp\\");
    }

    #[cfg(windows)]
    #[test]
    fn empty_policy_tmp_falls_back_to_temp() {
        let temp_dir = TempDir::new(policy("[env.set]\nTMP = \"\"\nTEMP = \"C:/Fallback\"\n"));

        assert_eq!(temp_dir.path().unwrap().to_string_lossy(), "C:/Fallback\\");
    }
}
