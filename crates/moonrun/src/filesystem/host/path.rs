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

//! Operations whose targets are pathnames in Runtime Working Directory.

use super::{FsError, HostFs};
use crate::filesystem::{FsOperation, FsPath, FsTarget, os};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

impl HostFs {
    pub(crate) fn read_file(&self, path: &Path) -> Result<Vec<u8>, FsError> {
        self.authorize(FsOperation::Read(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::read(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn read_text(&self, path: &Path) -> Result<String, FsError> {
        self.authorize(FsOperation::Read(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::read_to_string(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn write_file(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        self.write_file_with(path, || Ok(contents))
    }

    pub(crate) fn write_file_with<E, B: AsRef<[u8]>>(
        &self,
        path: &Path,
        contents: impl FnOnce() -> Result<B, E>,
    ) -> Result<(), E>
    where
        E: From<FsError>,
    {
        // Authorize before decoding an untrusted payload. Decode before opening
        // the target so a conversion failure cannot create or truncate a file.
        self.authorize(FsOperation::WriteFile(FsPath::Cwd(path.as_os_str())))?;
        fs::write(self.resolve_path(path.as_os_str()), contents()?)
            .map_err(|error| FsError::Io(error).into())
    }

    pub(crate) fn create_directory_all(&self, path: &Path) -> Result<(), FsError> {
        self.authorize(FsOperation::CreateDirectories(FsPath::Cwd(
            path.as_os_str(),
        )))?;
        Ok(fs::create_dir_all(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn remove_directory_all(&self, path: &Path) -> Result<(), FsError> {
        self.authorize(FsOperation::ChangeEntry(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::remove_dir_all(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn current_directory(&self) -> Result<PathBuf, FsError> {
        self.authorize(FsOperation::Read(FsPath::Cwd(OsStr::new("."))))?;
        Ok(self.working_directory.current_dir()?)
    }

    pub(crate) fn metadata(&self, path: &Path) -> Result<fs::Metadata, FsError> {
        self.authorize(FsOperation::Metadata(FsTarget::Path(FsPath::Cwd(
            path.as_os_str(),
        ))))?;
        Ok(fs::metadata(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn read_link(&self, path: &Path) -> Result<PathBuf, FsError> {
        self.authorize(FsOperation::InspectEntry(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::read_link(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn read_directory(&self, path: &Path) -> Result<fs::ReadDir, FsError> {
        self.authorize(FsOperation::Read(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::read_dir(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn create_directory(&self, path: &Path) -> Result<(), FsError> {
        self.authorize(FsOperation::ChangeEntry(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::create_dir(self.resolve_path(path.as_os_str()))?)
    }

    pub(crate) fn rename(&self, source: &Path, target: &Path) -> Result<(), FsError> {
        let options = os::RenameOptions {
            replace: true,
            cross_device: os::CrossDevice::Reject,
        };
        self.authorize(FsOperation::Rename {
            source: FsPath::Cwd(source.as_os_str()),
            destination: FsPath::Cwd(target.as_os_str()),
            options,
        })?;
        Ok(os::rename(
            Path::new(&self.resolve_path(source.as_os_str())),
            Path::new(&self.resolve_path(target.as_os_str())),
            options,
        )?)
    }

    pub(crate) fn unlink_file(&self, path: &Path) -> Result<(), FsError> {
        self.authorize(FsOperation::ChangeEntry(FsPath::Cwd(path.as_os_str())))?;
        Ok(fs::remove_file(self.resolve_path(path.as_os_str()))?)
    }
}
