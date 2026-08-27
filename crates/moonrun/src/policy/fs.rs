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

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::async_host::AsyncHostResult;

use super::{config::FsConfig, sandbox_denied};

/// Restricts async filesystem operations to native host roots.
///
/// This is not a virtual filesystem. `HostFs` resolves runtime paths before
/// asking these immutable rules whether the resulting target is allowed.
#[derive(Clone, Debug, Default)]
pub(crate) struct FsPolicy {
    read_roots: Vec<FsRoot>,
    write_roots: Vec<FsRoot>,
}

#[derive(Clone, Debug)]
enum FsRoot {
    Any,
    Path(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FsIntents {
    read: bool,
    write: bool,
}

impl FsPolicy {
    /// Construct runtime enforcement from roots canonicalized by `policy`.
    pub(super) fn from_canonical_config(config: FsConfig) -> Self {
        Self {
            read_roots: roots_from_config(config.read),
            write_roots: roots_from_config(config.write),
        }
    }

    pub(crate) fn authorize(
        &self,
        resolved_path: Option<&Path>,
        intents: FsIntents,
        target: &str,
    ) -> AsyncHostResult<()> {
        let Some(path) = resolved_path else {
            return sandbox_denied(intents.sandbox_action(), Some(target));
        };
        if intents.read && !self.allows_read(path) {
            return sandbox_denied("file read", Some(target));
        }
        if intents.write && !self.allows_write(path) {
            return sandbox_denied("file write", Some(target));
        }
        Ok(())
    }

    fn allows_read(&self, path: &Path) -> bool {
        self.read_roots.iter().any(|root| root.allows(path))
    }

    fn allows_write(&self, path: &Path) -> bool {
        self.write_roots.iter().any(|root| root.allows(path))
    }
}

impl FsRoot {
    fn allows(&self, path: &Path) -> bool {
        match self {
            Self::Any => true,
            Self::Path(root) => path == root || path.starts_with(root),
        }
    }
}

impl FsIntents {
    pub(crate) fn read() -> Self {
        Self {
            read: true,
            write: false,
        }
    }

    pub(crate) fn write() -> Self {
        Self {
            read: false,
            write: true,
        }
    }

    fn read_write() -> Self {
        Self {
            read: true,
            write: true,
        }
    }

    pub(crate) fn for_open(access: i32, create_mode: i32, append: bool) -> Self {
        let mut intents = match access {
            0 | 3 => Self::read(),
            1 => Self::write(),
            2 => Self::read_write(),
            _ => Self::read_write(),
        };
        if create_mode != 0 || append {
            intents.write = true;
        }
        intents
    }

    pub(crate) fn for_access_check(access: i32) -> Self {
        if access == 2 {
            Self::write()
        } else {
            Self::read()
        }
    }

    fn sandbox_action(self) -> &'static str {
        match (self.read, self.write) {
            (true, false) => "file read",
            (false, true) => "file write",
            _ => "file access",
        }
    }
}

fn roots_from_config(roots: Vec<PathBuf>) -> Vec<FsRoot> {
    roots
        .into_iter()
        .map(|root| {
            if root.as_os_str() == OsStr::new("*") {
                FsRoot::Any
            } else {
                FsRoot::Path(root)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_host::AsyncHostError;

    fn policy(read: Vec<PathBuf>, write: Vec<PathBuf>, config_dir: &Path) -> FsPolicy {
        let config = crate::policy::canonicalize(
            super::super::config::PolicyConfig {
                fs: Some(FsConfig { read, write }),
                ..Default::default()
            },
            config_dir,
        )
        .unwrap();
        FsPolicy::from_canonical_config(config.fs.unwrap())
    }

    fn authorize(policy: &FsPolicy, path: &Path, intents: FsIntents) -> AsyncHostResult<()> {
        let parent = std::fs::canonicalize(path.parent().unwrap()).unwrap();
        let resolved = parent.join(path.file_name().unwrap());
        policy.authorize(Some(&resolved), intents, &format!("{:?}", path.as_os_str()))
    }

    #[test]
    fn allows_missing_file_under_relative_root() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy = policy(Vec::new(), vec![PathBuf::from("allowed")], tmp.path());

        authorize(
            &policy,
            &allowed.join("new.txt"),
            FsIntents::for_open(1, 0, false),
        )
        .unwrap();
    }

    #[test]
    fn denies_paths_outside_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let policy = policy(Vec::new(), vec![allowed], tmp.path());

        let error = authorize(&policy, &denied.join("new.txt"), FsIntents::write()).unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn read_roots_do_not_permit_write_operations() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy = policy(vec![PathBuf::from("allowed")], Vec::new(), tmp.path());
        let path = allowed.join("new.txt");

        authorize(&policy, &path, FsIntents::read()).unwrap();
        let error = authorize(&policy, &path, FsIntents::write()).unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn write_roots_do_not_permit_read_operations() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy = policy(Vec::new(), vec![PathBuf::from("allowed")], tmp.path());

        let error = authorize(&policy, &allowed.join("new.txt"), FsIntents::read()).unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn create_mode_requires_write_even_for_read_access() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy = policy(vec![PathBuf::from("allowed")], Vec::new(), tmp.path());

        let error = authorize(
            &policy,
            &allowed.join("new.txt"),
            FsIntents::for_open(0, 1, false),
        )
        .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn untracked_relative_paths_are_denied() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = policy(
            vec![PathBuf::from("*")],
            vec![PathBuf::from("*")],
            tmp.path(),
        );

        let error = policy
            .authorize(None, FsIntents::read(), "\"<untracked resource>\"")
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn wildcard_root_allows_any_host_path() {
        let tmp = tempfile::tempdir().unwrap();
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&denied).unwrap();
        let policy = policy(Vec::new(), vec![PathBuf::from("*")], tmp.path());

        authorize(&policy, &denied.join("new.txt"), FsIntents::write()).unwrap();
    }
}
