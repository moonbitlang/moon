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

//! Filter files for `moon watch`

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use tracing::warn;

/// Ephemeral struct to apply filters on file paths in a single run
pub struct FileFilterBuilder<'a> {
    /// The root path of the repository
    root_path: &'a Path,

    /// Directories whose `.gitignore` files have been processed
    handled_dirs: HashSet<PathBuf>,

    /// The gitignore builders
    ignores: Vec<Gitignore>,
}

impl<'a> FileFilterBuilder<'a> {
    /// Create a new instance.
    ///
    /// This will always ignore the `target/` and `.mooncakes/` directories.
    pub fn new(repo_path: &'a Path) -> Self {
        let mut ignore = GitignoreBuilder::new(repo_path);

        // Always ignore the target and .mooncakes directories
        ignore
            .add_line(Some(repo_path.to_path_buf()), "target/")
            .unwrap();
        ignore
            .add_line(Some(repo_path.to_path_buf()), ".mooncakes/")
            .unwrap();

        Self {
            root_path: repo_path,
            handled_dirs: HashSet::new(),
            ignores: vec![ignore.build().unwrap()],
        }
    }

    /// Add the gitignore file for all parent directories of `file_path`, and
    /// check if it should be ignored. Returns true if the file should be ignored.
    ///
    /// FIXME: This is not the most efficient way to do this.
    #[tracing::instrument(skip_all)]
    pub fn check_file(&mut self, file_path: &Path) -> bool {
        // This should not happen, but just in case
        if !file_path.starts_with(self.root_path) {
            warn!(
                "Watched file '{}' is outside of the repository root '{}'",
                file_path.display(),
                self.root_path.display()
            );
            return false;
        }

        let is_dir = file_path.is_dir();

        // Add gitignore files for all parent directories up to the root
        for p in file_path.ancestors().skip(1) {
            if self.handled_dirs.contains(p) {
                continue;
            }
            let gitignore_path = p.join(".gitignore");
            if gitignore_path.exists() {
                let mut builder = GitignoreBuilder::new(p);
                builder.add(gitignore_path);
                let built = builder.build().unwrap();
                self.ignores.push(built);
                self.handled_dirs.insert(p.to_path_buf());
            }

            if p == self.root_path {
                break;
            }
        }

        // Check if the file is ignored by any of the gitignore rules
        for gitignore in &self.ignores {
            if gitignore
                .matched_path_or_any_parents(file_path, is_dir)
                .is_ignore()
            {
                return true;
            }
        }

        false
    }
}

#[test]
fn test_file_filter_builder() {
    use std::fs;

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_path = temp_dir.path();

    // Create .gitignore files
    fs::create_dir_all(repo_path.join("dir/subdir")).unwrap();
    fs::write(repo_path.join(".gitignore"), "ignored_root_file.txt\n").unwrap();
    fs::write(repo_path.join("dir/.gitignore"), "ignored_dir_file.txt\n").unwrap();

    let mut builder = FileFilterBuilder::new(repo_path);

    // Add files to be watched
    assert!(builder.check_file(&repo_path.join("ignored_root_file.txt")));
    assert!(builder.check_file(&repo_path.join("dir/ignored_dir_file.txt")));
    assert!(!builder.check_file(&repo_path.join("dir/subdir/some_file.txt")));
}

#[test]
fn test_ignore_target_and_mooncakes() {
    use std::fs;

    let temp_dir = tempfile::tempdir().unwrap();
    let repo_path = temp_dir.path();

    // Create target and .mooncakes directories
    fs::create_dir_all(repo_path.join("target")).unwrap();
    fs::create_dir_all(repo_path.join(".mooncakes")).unwrap();
    fs::write(repo_path.join("target/some_file.txt"), "").unwrap();
    fs::write(repo_path.join(".mooncakes/another_file.txt"), "").unwrap();

    let mut builder = FileFilterBuilder::new(repo_path);

    assert!(builder.check_file(&repo_path.join("target/some_file.txt")));
    assert!(builder.check_file(&repo_path.join(".mooncakes/another_file.txt")));
}
