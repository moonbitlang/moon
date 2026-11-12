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

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

use crate::test_dir::TestDir;

/// A test's sources and private writable workspace.
///
/// Use `new().with_files(...)` for inline inputs, `copied(source)` for a private
/// source tree, or `readonly(source)` to share sources with a private build dir.
/// Construct Moon commands through this fixture so its build isolation is kept.
/// Sources passed to `copied` and `readonly` are paths, not suite-relative names.
pub struct Fixture {
    cwd: PathBuf,
    // Shared sources always use a private target; writable sources use Moon's
    // default target selection. This also determines whether with_files is allowed.
    target_dir: Option<PathBuf>,
    workspace: TestDir,
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}

impl Fixture {
    /// Create an empty private source directory with normal build-path selection.
    pub fn new() -> Self {
        let workspace = TestDir::new_empty();
        Self {
            cwd: workspace.as_ref().to_owned(),
            target_dir: None,
            workspace,
        }
    }

    /// Copy a source directory, excluding `target` directories as in Moon's test suite.
    pub fn copied(source: impl AsRef<Path>) -> Self {
        let workspace = TestDir::from_case_root(source, "", true);
        Self {
            cwd: workspace.as_ref().to_owned(),
            target_dir: None,
            workspace,
        }
    }

    /// Share sources with a private build directory. Commands must not modify sources.
    /// This does not change filesystem permissions on the source tree.
    pub fn readonly(source: impl AsRef<Path>) -> Self {
        let workspace = TestDir::new_empty();
        Self {
            cwd: source.as_ref().to_owned(),
            target_dir: Some(workspace.join("_build")),
            workspace,
        }
    }

    /// Write inline inputs relative to the current working directory and time setup.
    /// Panics for shared sources, invalid file paths, or a failed write. Partial
    /// initialization is cleaned up or retained according to the fixture policy.
    pub fn with_files(
        mut self,
        files: impl IntoIterator<Item = (impl AsRef<Path>, impl AsRef<[u8]>)>,
    ) -> Self {
        assert!(
            self.target_dir.is_none(),
            "cannot write files into a readonly fixture"
        );
        let relative_cwd = self
            .cwd
            .strip_prefix(self.workspace.as_ref())
            .expect("writable fixture working directory belongs to its workspace")
            .to_owned();
        self.workspace = self.workspace.with_files(
            files
                .into_iter()
                .map(|(path, contents)| (relative_cwd.join(path), contents)),
        );
        self
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// The explicit target override for shared sources; `None` preserves Moon's defaults.
    pub fn target_dir(&self) -> Option<&Path> {
        self.target_dir.as_deref()
    }

    /// Resolve a path in the current source directory, not the private build directory.
    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.cwd.join(path)
    }

    /// Change the source working directory while preserving the workspace and target.
    pub fn in_subdir(mut self, sub: impl AsRef<Path>) -> Self {
        self.cwd.push(sub);
        self
    }

    /// Configure cwd and build isolation for the supplied Moon executable.
    /// The caller supplies toolchain and other command-specific environment settings.
    pub fn moon_process_cmd(&self, program: impl AsRef<OsStr>) -> Command {
        TestCommandDir::moon_process_cmd(self, program)
    }

    pub fn moon_cmd(&self, program: impl AsRef<OsStr>) -> snapbox::cmd::Command {
        snapbox::cmd::Command::from_std(self.moon_process_cmd(program))
    }
}

/// Compatibility for command helpers that still accept ordinary paths.
/// New scenarios should pass a `Fixture`, which keeps build isolation attached.
// TODO: Remove this path adapter when the Moon command helpers accept only Fixture.
#[doc(hidden)]
pub trait TestCommandDir {
    fn cwd(&self) -> &Path;
    fn target_dir(&self) -> Option<&Path> {
        None
    }

    fn moon_process_cmd(&self, program: impl AsRef<OsStr>) -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir(self.cwd());
        if let Some(target) = self.target_dir() {
            cmd.arg("--target-dir").arg(target);
        }
        cmd
    }
}

impl<T: AsRef<Path> + ?Sized> TestCommandDir for T {
    fn cwd(&self) -> &Path {
        self.as_ref()
    }
}

// Do not implement AsRef<Path> for Fixture: the blanket path adapter would lose
// its private target directory when passed to command helpers.
impl TestCommandDir for Fixture {
    fn cwd(&self) -> &Path {
        self.cwd()
    }

    fn target_dir(&self) -> Option<&Path> {
        self.target_dir()
    }
}

#[cfg(test)]
mod tests;
