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
use std::path::{Component, Path, PathBuf};

use anyhow::Context;

use crate::async_host::{AsyncHostError, AsyncHostResult};

use super::config::FsConfig;

/// Restricts async filesystem operations to native host roots.
///
/// This is not a virtual filesystem: relative runtime paths are resolved using
/// the process current directory, and configured roots are resolved using the
/// host platform's path rules.
#[derive(Clone, Debug, Default)]
pub(super) struct FsPolicy {
    read_roots: Vec<FsRoot>,
    write_roots: Vec<FsRoot>,
}

#[derive(Clone, Debug)]
enum FsRoot {
    Any,
    Path(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FsIntents {
    read: bool,
    write: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuntimePathBase<'a> {
    CurrentDirectory,
    PolicyPath(&'a Path),
    Untracked,
}

impl FsPolicy {
    pub(super) fn from_config(config: FsConfig, config_dir: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            read_roots: resolve_roots(config.read, config_dir)?,
            write_roots: resolve_roots(config.write, config_dir)?,
        })
    }

    pub(super) fn allows(
        &self,
        base: RuntimePathBase<'_>,
        path: &OsStr,
        intents: FsIntents,
    ) -> AsyncHostResult<()> {
        let path = Path::new(path);
        let path = resolve_runtime_path(base, path)?;
        let path = canonicalize_existing_prefix(&path)?;

        self.allows_resolved(&path, intents)
    }

    pub(super) fn allows_entry(
        &self,
        base: RuntimePathBase<'_>,
        path: &OsStr,
        intents: FsIntents,
    ) -> AsyncHostResult<()> {
        let path = Path::new(path);
        let path = resolve_runtime_path(base, path)?;
        let path = canonicalize_entry_path(&path)?;

        self.allows_resolved(&path, intents)
    }

    fn allows_resolved(&self, path: &Path, intents: FsIntents) -> AsyncHostResult<()> {
        if intents.read && !self.allows_read(path) {
            return Err(AsyncHostError::PermissionDenied);
        }
        if intents.write && !self.allows_write(path) {
            return Err(AsyncHostError::PermissionDenied);
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
    pub(super) fn read() -> Self {
        Self {
            read: true,
            write: false,
        }
    }

    pub(super) fn write() -> Self {
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

    pub(super) fn for_open(access: i32, create_mode: i32, append: bool) -> Self {
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

    pub(super) fn for_access_check(access: i32) -> Self {
        if access == 2 {
            Self::write()
        } else {
            Self::read()
        }
    }
}

fn resolve_roots(roots: Vec<PathBuf>, config_dir: &Path) -> anyhow::Result<Vec<FsRoot>> {
    roots
        .into_iter()
        .map(|root| {
            if root.as_os_str() == OsStr::new("*") {
                return Ok(FsRoot::Any);
            }
            let path = if root.is_absolute() {
                root
            } else {
                config_dir.join(root)
            };
            let path = std::fs::canonicalize(&path)
                .with_context(|| format!("failed to resolve async fs root {}", path.display()))?;
            Ok(FsRoot::Path(path))
        })
        .collect()
}

fn resolve_runtime_path(base: RuntimePathBase<'_>, path: &Path) -> AsyncHostResult<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    match base {
        RuntimePathBase::CurrentDirectory => std::env::current_dir()
            .map(|current_dir| current_dir.join(path))
            .map_err(|_| AsyncHostError::PermissionDenied),
        RuntimePathBase::PolicyPath(base) => Ok(base.join(path)),
        RuntimePathBase::Untracked => Err(AsyncHostError::PermissionDenied),
    }
}

fn canonicalize_existing_prefix(path: &Path) -> AsyncHostResult<PathBuf> {
    for ancestor in path.ancestors() {
        if let Ok(prefix) = std::fs::canonicalize(ancestor) {
            let suffix = path
                .strip_prefix(ancestor)
                .map_err(|_| AsyncHostError::PermissionDenied)?;
            return Ok(normalize_path(prefix.join(suffix)));
        }
    }
    Err(AsyncHostError::PermissionDenied)
}

fn canonicalize_entry_path(path: &Path) -> AsyncHostResult<PathBuf> {
    let parent = path.parent().ok_or(AsyncHostError::PermissionDenied)?;
    let file_name = path.file_name().ok_or(AsyncHostError::PermissionDenied)?;
    Ok(normalize_path(
        canonicalize_existing_prefix(parent)?.join(file_name),
    ))
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(read: Vec<PathBuf>, write: Vec<PathBuf>, config_dir: &Path) -> FsPolicy {
        FsPolicy::from_config(FsConfig { read, write }, config_dir).unwrap()
    }

    #[test]
    fn allows_missing_file_under_relative_root() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy = policy(Vec::new(), vec![PathBuf::from("allowed")], tmp.path());

        policy
            .allows(
                RuntimePathBase::CurrentDirectory,
                allowed.join("new.txt").as_os_str(),
                FsIntents::for_open(1, 0, false),
            )
            .unwrap();
    }

    #[test]
    fn denies_paths_outside_roots_after_normalization() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let policy = policy(Vec::new(), vec![allowed], tmp.path());

        let error = policy
            .allows(
                RuntimePathBase::CurrentDirectory,
                denied.join("new.txt").as_os_str(),
                FsIntents::write(),
            )
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn read_roots_do_not_permit_write_operations() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy = policy(vec![PathBuf::from("allowed")], Vec::new(), tmp.path());
        let path = allowed.join("new.txt");

        policy
            .allows(
                RuntimePathBase::CurrentDirectory,
                path.as_os_str(),
                FsIntents::read(),
            )
            .unwrap();
        let error = policy
            .allows(
                RuntimePathBase::CurrentDirectory,
                path.as_os_str(),
                FsIntents::write(),
            )
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn write_roots_do_not_permit_read_operations() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy = policy(Vec::new(), vec![PathBuf::from("allowed")], tmp.path());

        let error = policy
            .allows(
                RuntimePathBase::CurrentDirectory,
                allowed.join("new.txt").as_os_str(),
                FsIntents::read(),
            )
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn create_mode_requires_write_even_for_read_access() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy = policy(vec![PathBuf::from("allowed")], Vec::new(), tmp.path());

        let error = policy
            .allows(
                RuntimePathBase::CurrentDirectory,
                allowed.join("new.txt").as_os_str(),
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
            .allows(
                RuntimePathBase::Untracked,
                OsStr::new("file.txt"),
                FsIntents::read(),
            )
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn wildcard_root_allows_any_host_path() {
        let tmp = tempfile::tempdir().unwrap();
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&denied).unwrap();
        let policy = policy(Vec::new(), vec![PathBuf::from("*")], tmp.path());

        policy
            .allows(
                RuntimePathBase::CurrentDirectory,
                denied.join("new.txt").as_os_str(),
                FsIntents::write(),
            )
            .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn entry_checks_use_link_path_not_symlink_target() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let allowed_file = allowed.join("target.txt");
        std::fs::write(&allowed_file, "target").unwrap();
        let denied_link = denied.join("link.txt");
        std::os::unix::fs::symlink(&allowed_file, &denied_link).unwrap();
        let policy = policy(Vec::new(), vec![PathBuf::from("allowed")], tmp.path());

        policy
            .allows(
                RuntimePathBase::CurrentDirectory,
                denied_link.as_os_str(),
                FsIntents::write(),
            )
            .unwrap();
        let error = policy
            .allows_entry(
                RuntimePathBase::CurrentDirectory,
                denied_link.as_os_str(),
                FsIntents::write(),
            )
            .unwrap_err();

        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[cfg(unix)]
    #[test]
    fn entry_checks_allow_allowed_link_without_target_write() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let denied_file = denied.join("target.txt");
        std::fs::write(&denied_file, "target").unwrap();
        let allowed_link = allowed.join("link.txt");
        std::os::unix::fs::symlink(&denied_file, &allowed_link).unwrap();
        let policy = policy(Vec::new(), vec![PathBuf::from("allowed")], tmp.path());

        policy
            .allows_entry(
                RuntimePathBase::CurrentDirectory,
                allowed_link.as_os_str(),
                FsIntents::write(),
            )
            .unwrap();
        let error = policy
            .allows(
                RuntimePathBase::CurrentDirectory,
                allowed_link.as_os_str(),
                FsIntents::write(),
            )
            .unwrap_err();

        assert_eq!(error, AsyncHostError::PermissionDenied);
    }
}
