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

use std::path::{Path, PathBuf};

pub fn copy_tree(src: &Path, dest: &Path, exclude_target: bool) -> anyhow::Result<()> {
    if src.is_dir() {
        if !dest.exists() {
            std::fs::create_dir_all(dest)?;
        }
        let mut walker = ignore::WalkBuilder::new(src);
        walker.hidden(false);
        walker.git_global(false);
        if exclude_target {
            walker.filter_entry(|x| x.file_name() != "target");
        }
        for entry in walker.build() {
            let entry = entry?;
            let path = entry.path();
            let relative_path = path.strip_prefix(src)?;
            let dest_path = dest.join(relative_path);
            if path.is_dir() {
                if !dest_path.exists() {
                    std::fs::create_dir_all(dest_path)?;
                }
            } else {
                std::fs::copy(path, dest_path)?;
            }
        }
    } else {
        std::fs::copy(src, dest)?;
    }
    Ok(())
}

pub struct TestDir {
    // tempfile::TempDir has a drop implementation that will remove the directory
    path: tempfile::TempDir,
}

impl TestDir {
    pub fn from_case_root(
        case_root: impl AsRef<Path>,
        sub: impl AsRef<Path>,
        exclude_target: bool,
    ) -> Self {
        let dir = case_root.as_ref().join(sub);
        let tmp_dir = tempfile::TempDir::new().expect("create temp dir for tests");
        copy_tree(&dir, tmp_dir.path(), exclude_target).expect("copy test case to temp dir");
        Self { path: tmp_dir }
    }

    pub fn new_empty() -> Self {
        let tmp_dir = tempfile::TempDir::new().expect("create empty temp dir for tests");
        Self { path: tmp_dir }
    }

    pub fn join(&self, sub: impl AsRef<Path>) -> PathBuf {
        self.path.path().join(sub.as_ref())
    }
}

impl AsRef<Path> for TestDir {
    fn as_ref(&self) -> &Path {
        self.path.path()
    }
}
