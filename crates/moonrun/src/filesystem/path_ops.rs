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

//! Synchronous filesystem effects shared by import families.
//!
//! These operations keep authorization and the effect together. WASI passes
//! absolute paths interpreted from its dirfd, so Runtime cwd is not applied
//! again. Core callers may pass Runtime-relative paths. Descriptor rights and
//! ABI encoding stay with the caller; std and `native` supply the mechanisms.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use super::HostFs;
use crate::async_host::AsyncHostError;
use crate::policy::FsIntents;

/// Preserve policy denial separately from OS errors for each caller's ABI.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PathError {
    #[error("filesystem access denied by policy")]
    Denied,
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl From<AsyncHostError> for PathError {
    fn from(error: AsyncHostError) -> Self {
        match error {
            AsyncHostError::PermissionDenied => Self::Denied,
            _ => Self::Io(io::Error::from_raw_os_error(error.errno())),
        }
    }
}

/// One source for both native open options and their authorization intent.
pub(crate) struct FileOpenOptions {
    pub(crate) read: bool,
    pub(crate) write: bool,
    pub(crate) append: bool,
    pub(crate) create: bool,
    pub(crate) create_new: bool,
    pub(crate) truncate: bool,
}

impl HostFs {
    pub(crate) fn open_file(
        &self,
        path: &Path,
        options: FileOpenOptions,
    ) -> Result<fs::File, PathError> {
        self.authorize_path(
            path.as_os_str(),
            FsIntents::new(
                options.read,
                options.write
                    || options.append
                    || options.create
                    || options.create_new
                    || options.truncate,
            ),
        )?;
        Ok(fs::OpenOptions::new()
            .read(options.read)
            .write(options.write)
            .append(options.append)
            .create(options.create)
            .create_new(options.create_new)
            .truncate(options.truncate)
            .open(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn metadata(&self, path: &Path) -> Result<fs::Metadata, PathError> {
        self.authorize_read(path.as_os_str())?;
        Ok(fs::metadata(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn read_link(&self, path: &Path) -> Result<PathBuf, PathError> {
        self.authorize_entry_metadata(path.as_os_str())?;
        Ok(fs::read_link(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn read_directory(&self, path: &Path) -> Result<fs::ReadDir, PathError> {
        self.authorize_read(path.as_os_str())?;
        Ok(fs::read_dir(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn create_directory(&self, path: &Path) -> Result<(), PathError> {
        self.authorize_write_entry(path.as_os_str())?;
        Ok(fs::create_dir(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn rename(&self, source: &Path, target: &Path) -> Result<(), PathError> {
        self.authorize_rename(source.as_os_str(), target.as_os_str())?;
        Ok(super::native::rename(
            self.resolve_path(source.as_os_str()),
            self.resolve_path(target.as_os_str()),
            true,
            super::native::CrossDevice::Reject,
        )?)
    }

    pub(crate) fn remove_empty_directory(&self, path: &Path) -> Result<(), PathError> {
        self.authorize_remove(path.as_os_str())?;
        Ok(fs::remove_dir(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn unlink_file(&self, path: &Path) -> Result<(), PathError> {
        self.authorize_remove(path.as_os_str())?;
        Ok(fs::remove_file(self.resolve_path(path.as_os_str()))?)
    }
}
