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

//! Working-directory behavior selected for one Runtime.
//!
//! `Ambient` observes the process current directory. `Fixed` resolves paths
//! against a caller-selected directory without changing process state. Neither
//! mode itself grants filesystem access or confines native descendants.
//!
//! TODO: If an anchored mode is needed, use an open directory handle and `*at`
//! syscalls as its authority rather than a cached path. A file watcher may
//! provide rename/delete diagnostics, but must not define path resolution or
//! authorization; the API must also represent a directory with no recoverable
//! pathname after unlink.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Selects how one run observes and inherits a working directory.
///
/// `Ambient` preserves moonrun's historical behavior: operations observe or
/// inherit the process current directory at the same points in execution as
/// they did before this abstraction was introduced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum WorkingDirectory {
    #[default]
    Ambient,
    /// Resolve relative paths and child directories against this directory.
    /// The Runtime canonicalizes it once at the start of each run.
    Fixed(PathBuf),
}

impl WorkingDirectory {
    pub(crate) fn current_dir(&self) -> std::io::Result<PathBuf> {
        match self {
            Self::Ambient => std::env::current_dir(),
            Self::Fixed(path) => Ok(path.clone()),
        }
    }

    /// Interpret a native operation's filename without capturing ambient cwd.
    pub(crate) fn resolve_path(&self, path: &OsStr) -> OsString {
        match self {
            Self::Fixed(root) if !path.is_empty() => root.join(path).into_os_string(),
            _ => path.to_owned(),
        }
    }

    /// Resolve a policy query's filename at the point where it is checked.
    pub(crate) fn resolve_absolute(&self, path: &Path) -> std::io::Result<PathBuf> {
        let path = PathBuf::from(self.resolve_path(path.as_os_str()));
        // An empty filename must retain its native semantics, not become cwd.
        if path.as_os_str().is_empty() || path.is_absolute() {
            return Ok(path);
        }
        std::env::current_dir().map(|current_dir| current_dir.join(path))
    }

    pub(crate) fn configure_child_cwd(&self, cwd: &mut Option<OsString>) {
        match self {
            Self::Ambient => {}
            Self::Fixed(root) => match cwd {
                Some(path) => *path = self.resolve_path(path),
                None => *cwd = Some(root.clone().into_os_string()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_observes_the_process_current_directory() {
        let working_directory = WorkingDirectory::Ambient;
        assert_eq!(
            working_directory.current_dir().unwrap(),
            std::env::current_dir().unwrap()
        );
    }

    #[test]
    fn ambient_resolves_relative_paths_at_observation_time() {
        let path = Path::new("relative/path");
        let expected = std::env::current_dir().unwrap().join(path);
        assert_eq!(
            WorkingDirectory::Ambient.resolve_absolute(path).unwrap(),
            expected
        );
    }

    #[test]
    fn resolving_an_absolute_path_does_not_observe_cwd() {
        let path = std::env::current_exe().unwrap();
        assert_eq!(
            WorkingDirectory::Ambient.resolve_absolute(&path).unwrap(),
            path
        );
    }

    #[test]
    fn ambient_does_not_override_child_cwd_behavior() {
        let mut inherited = None;
        WorkingDirectory::Ambient.configure_child_cwd(&mut inherited);
        assert_eq!(inherited, None);

        let mut cwd = Some(OsString::from("relative/child"));
        WorkingDirectory::Ambient.configure_child_cwd(&mut cwd);
        assert_eq!(cwd, Some(OsString::from("relative/child")));
    }
}
