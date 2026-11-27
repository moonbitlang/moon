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

//! Owned directory bases and operations relative to their retained handles.

use super::{FsError, HostFs};
#[cfg(unix)]
use crate::filesystem::authorization::entry_before_directory_suffix;
use crate::filesystem::{FsOperation, FsPath, authorization, os};
use std::fs;
use std::path::{Path, PathBuf};

/// One source for both native open options and their authorization intent.
pub(crate) struct FileOpenOptions {
    pub(crate) read: bool,
    pub(crate) write: bool,
    pub(crate) append: bool,
    pub(crate) create: bool,
    pub(crate) create_new: bool,
    pub(crate) truncate: bool,
}

impl FileOpenOptions {
    fn openat_options(&self) -> std::io::Result<os::openat::OpenOptions> {
        use os::openat::{Access, CreateMode, OpenOptions, Sync, TypeConstraint};
        // Preserve std::OpenOptions validation separately from opening the file.
        // Async jobs retain their own scalar-option contract.
        let write = self.write || self.append;
        if (!write && (!self.read || self.create || self.create_new || self.truncate))
            || (self.append && self.truncate && !self.create_new)
        {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        Ok(OpenOptions {
            access: if self.read && write {
                Access::ReadWrite
            } else if write {
                Access::Write
            } else {
                Access::Read
            },
            type_constraint: TypeConstraint::Any,
            follow_symlink: true,
            is_async: false,
            create: if self.create_new {
                CreateMode::Exclusive
            } else {
                match (self.create, self.truncate) {
                    (false, false) => CreateMode::Open,
                    (false, true) => CreateMode::Truncate,
                    (true, false) => CreateMode::Create,
                    (true, true) => CreateMode::CreateOrTruncate,
                }
            },
            append: self.append,
            sync: Sync::None,
            permission: 0o666,
        })
    }
}

/// An acquired directory, independent of guest descriptor numbering and rights.
/// Its pathname is queried only for authorization and path-only operations.
pub(crate) struct Directory {
    file: fs::File,
}

impl Directory {
    /// Runtime configuration acquires a preopen without granting guest access.
    /// HostFs still authorizes each operation using its requested effects.
    pub(crate) fn preopen(path: &Path) -> std::io::Result<Self> {
        use os::{
            DirectIo,
            openat::{self, Access, CreateMode, OpenOptions, Sync, TypeConstraint},
        };
        Ok(Self {
            file: openat::open(
                None,
                path.as_os_str(),
                &OpenOptions {
                    access: Access::TraverseDirectory,
                    type_constraint: TypeConstraint::Directory,
                    follow_symlink: true,
                    is_async: false,
                    create: CreateMode::Open,
                    append: false,
                    sync: Sync::None,
                    permission: 0,
                },
                &DirectIo,
            )?,
        })
    }

    pub(crate) fn path(&self) -> Result<PathBuf, FsError> {
        authorization::directory_path(os::raw_file(&self.file))
    }
}

/// Keep Unix relative traversal intact. Absolute paths and Windows DOS paths
/// need conversion to a name relative to the acquired handle. Resolve only the
/// parent: following the final link here would change unlink and exclusive-open
/// semantics before their OS operation gets to interpret the entry.
fn relative_path(base: &Path, path: &Path) -> Result<PathBuf, FsError> {
    #[cfg(unix)]
    if !path.is_absolute() {
        return Ok(if path.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            path.to_owned()
        });
    }
    #[cfg(windows)]
    let host_path = std::path::absolute(dunce::simplified(base).join(path))?;
    #[cfg(unix)]
    let host_path = path.to_owned();
    let resolved = if cfg!(unix) && host_path.starts_with(base) {
        host_path
    } else if dunce::simplified(&host_path) == dunce::simplified(base) {
        base.to_owned()
    } else if let Some(name) = host_path.file_name() {
        let parent = host_path.parent().ok_or(FsError::Denied)?;
        fs::canonicalize(parent)?.join(name)
    } else {
        // A terminal `..` names a directory, not a removable entry.
        fs::canonicalize(&host_path)?
    };
    let relative = resolved
        .strip_prefix(base)
        .map_err(|_| FsError::Denied)?
        .to_path_buf();
    #[cfg(unix)]
    let relative = {
        let mut relative = relative;
        if relative.as_os_str().is_empty() {
            relative.push(".");
        } else if entry_before_directory_suffix(path).is_some() {
            // strip_prefix discards directory suffixes. Preserve traversal and
            // a final dot (even in /./), which rmdir must not turn into deletion.
            let trailing_dot = path
                .as_os_str()
                .as_encoded_bytes()
                .rsplit(|byte| *byte == b'/')
                .find(|component| !component.is_empty())
                .is_some_and(|component| component == b".");
            relative.push(if trailing_dot { "." } else { "" });
        }
        relative
    };
    Ok(relative)
}

impl HostFs {
    pub(crate) fn open_file_at(
        &self,
        directory: &Directory,
        path: &Path,
        options: FileOpenOptions,
    ) -> Result<fs::File, FsError> {
        use os::{DirectIo, openat};
        let base = directory.path()?;
        let path = relative_path(&base, path)?;
        self.authorize(FsOperation::OpenWithOptions {
            path: FsPath::At {
                base: &base,
                path: path.as_os_str(),
            },
            options: &options,
        })?;
        let options = options.openat_options()?;
        Ok(openat::open(
            Some(os::raw_file(&directory.file)),
            path.as_os_str(),
            &options,
            &DirectIo,
        )?)
    }

    pub(crate) fn open_directory_at(
        &self,
        directory: &Directory,
        path: &Path,
    ) -> Result<Directory, FsError> {
        use os::{
            DirectIo,
            openat::{self, Access, CreateMode, OpenOptions, Sync, TypeConstraint},
        };
        let base = directory.path()?;
        let path = relative_path(&base, path)?;
        self.authorize(FsOperation::Read(FsPath::At {
            base: &base,
            path: path.as_os_str(),
        }))?;
        // NtCreateFile does not perform DOS dot-component normalization. The
        // normalized path is empty when opening the acquired directory itself.
        let file = if path.as_os_str().is_empty() {
            directory.file.try_clone()?
        } else {
            openat::open(
                Some(os::raw_file(&directory.file)),
                path.as_os_str(),
                &OpenOptions {
                    access: Access::TraverseDirectory,
                    type_constraint: TypeConstraint::Directory,
                    follow_symlink: true,
                    is_async: false,
                    create: CreateMode::Open,
                    append: false,
                    sync: Sync::None,
                    permission: 0,
                },
                &DirectIo,
            )?
        };
        Ok(Directory { file })
    }

    pub(crate) fn remove_directory_at(
        &self,
        directory: &Directory,
        path: &Path,
    ) -> Result<(), FsError> {
        let base = directory.path()?;
        let path = relative_path(&base, path)?;
        self.authorize(FsOperation::RemoveDirectory(FsPath::At {
            base: &base,
            path: path.as_os_str(),
        }))?;
        Ok(os::unlinkat::remove_directory_at(
            Some(os::raw_file(&directory.file)),
            path.as_os_str(),
            &os::DirectIo,
        )?)
    }

    pub(crate) fn unlink_file_at(&self, directory: &Directory, path: &Path) -> Result<(), FsError> {
        let base = directory.path()?;
        let path = relative_path(&base, path)?;
        self.authorize(FsOperation::UnlinkFile(FsPath::At {
            base: &base,
            path: path.as_os_str(),
        }))?;
        Ok(os::unlinkat::unlink_file_at(
            Some(os::raw_file(&directory.file)),
            path.as_os_str(),
            &os::DirectIo,
        )?)
    }
}
