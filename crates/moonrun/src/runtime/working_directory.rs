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

//! Working-directory behavior selected for one Runtime.
//!
//! The current implementation deliberately exposes only ambient process
//! behavior. This module is the boundary for adding other behaviors later;
//! selecting `Ambient` does not snapshot, canonicalize, or change the process
//! current directory.
//!
//! TODO: If an anchored mode is needed, use an open directory handle and `*at`
//! syscalls as its authority rather than a cached path. A file watcher may
//! provide rename/delete diagnostics, but must not define path resolution or
//! authorization; the API must also represent a directory with no recoverable
//! pathname after unlink.

use std::ffi::OsString;
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
}

impl WorkingDirectory {
    pub(crate) fn current_dir(&self) -> std::io::Result<PathBuf> {
        match self {
            Self::Ambient => std::env::current_dir(),
        }
    }

    pub(crate) fn resolve(&self, path: &Path) -> std::io::Result<PathBuf> {
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }
        self.current_dir().map(|current_dir| current_dir.join(path))
    }

    pub(crate) fn configure_child_cwd(&self, _cwd: &mut Option<OsString>) {
        match self {
            Self::Ambient => {}
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
        assert_eq!(WorkingDirectory::Ambient.resolve(path).unwrap(), expected);
    }

    #[test]
    fn resolving_an_absolute_path_does_not_observe_cwd() {
        let path = std::env::current_exe().unwrap();
        assert_eq!(WorkingDirectory::Ambient.resolve(&path).unwrap(), path);
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
