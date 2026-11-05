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

//! Interpret filesystem requests and select their authority in one place.
//! Callers supply operations and targets, never inferred policy requirements.

mod grant;

pub(crate) use grant::FileGrant;
use grant::GrantSource;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::host::{FileOpenOptions, FsError, HostFs};
use super::os::RawFile;
use super::path::{canonicalize_entry_path, canonicalize_existing_prefix, directory_path};

/// A pathname lookup uses either Runtime cwd or an acquired directory's base.
/// It never inherits the grant for operations on the directory handle itself.
#[derive(Clone, Copy)]
pub(crate) enum FsPath<'a> {
    Cwd(&'a OsStr),
    At {
        base: &'a Path,
        path: &'a OsStr,
    },
    /// The caller keeps this directory handle alive through authorization.
    /// Recover its pathname only when policy requires it.
    AtHandle {
        directory: RawFile,
        path: &'a OsStr,
    },
}

#[derive(Clone, Copy)]
pub(crate) enum FsTarget<'a> {
    Path(FsPath<'a>),
    File(&'a FileGrant),
}

/// Requested effects carry only targets on which they can be performed.
/// Metadata and kind queries share semantics across pathname and handle access;
/// entry changes require a path, while locking requires an acquired file.
pub(crate) enum FsOperation<'a> {
    Read(FsPath<'a>),
    Write(FsPath<'a>),
    Kind(FsTarget<'a>),
    Metadata(FsTarget<'a>),
    InspectEntry(FsPath<'a>),
    ChangeEntry(FsPath<'a>),
    Rename {
        source: FsPath<'a>,
        destination: FsPath<'a>,
    },
    WriteFile(FsPath<'a>),
    CreateDirectories(FsPath<'a>),
    /// Async ABI: access selects read/write/read-write/directory listing;
    /// append only changes positioning. Permissions precede scalar validation.
    OpenAsync {
        path: FsPath<'a>,
        access: i32,
        create_mode: i32,
        /// Additional metadata beyond the identity supplied by opening.
        read_metadata: bool,
    },
    /// std::OpenOptions contract: append implies write; creation/truncation
    /// without write, and append+truncate without create_new, are rejected after
    /// authorization. Used by WASI, separately from the async ABI.
    OpenWithOptions {
        path: FsPath<'a>,
        options: &'a FileOpenOptions,
    },
    AccessCheck {
        path: FsPath<'a>,
        access: i32,
    },
    Lock {
        file: &'a FileGrant,
        exclusive: bool,
    },
    Watch(FsTarget<'a>),
}

impl HostFs {
    /// The filesystem authorization entry point. Native handle validity and ABI
    /// descriptor rights remain the responsibility of the operation's caller.
    pub(crate) fn authorize(&self, operation: FsOperation<'_>) -> Result<(), FsError> {
        let Some(policy) = &self.policy else {
            return Ok(());
        };
        // Operation interpretation stays here; the policy layer only matches
        // resolved targets against read/write rules, in policy-group order.
        let object: fn(&Path) -> Result<PathBuf, FsError> = canonicalize_existing_prefix;
        let entry: fn(&Path) -> Result<PathBuf, FsError> = canonicalize_entry_path;
        let mut read_metadata = false;
        let (target, resolve, read, write) = match operation {
            FsOperation::Rename {
                source,
                destination,
            } => {
                self.authorize(FsOperation::ChangeEntry(source))?;
                return self.authorize(FsOperation::ChangeEntry(destination));
            }
            FsOperation::InspectEntry(path) => (FsTarget::Path(path), entry, true, false),
            FsOperation::ChangeEntry(path) => (FsTarget::Path(path), entry, false, true),
            FsOperation::Read(path) => (FsTarget::Path(path), object, true, false),
            // Composite writes and recursive creation retain their existing
            // followed-target check, without inspecting intermediate entries.
            FsOperation::Write(path)
            | FsOperation::WriteFile(path)
            | FsOperation::CreateDirectories(path) => (FsTarget::Path(path), object, false, true),
            FsOperation::Kind(FsTarget::File(_)) => return Ok(()),
            FsOperation::Kind(target) | FsOperation::Metadata(target) => {
                (target, object, true, false)
            }
            FsOperation::OpenAsync {
                path,
                access,
                create_mode,
                read_metadata: metadata,
            } => {
                read_metadata = metadata;
                // Preserve the permission check before invalid-option errors.
                // Append changes positioning, not access.
                let (read, write) = match access {
                    0 | 3 => (true, false),
                    1 => (false, true),
                    _ => (true, true),
                };
                (
                    FsTarget::Path(path),
                    object,
                    read,
                    write || create_mode != 0,
                )
            }
            FsOperation::OpenWithOptions { path, options } => (
                FsTarget::Path(path),
                object,
                options.read,
                options.write
                    || options.append
                    || options.create
                    || options.create_new
                    || options.truncate,
            ),
            FsOperation::AccessCheck { path, access } => {
                (FsTarget::Path(path), object, access != 2, access == 2)
            }
            FsOperation::Lock {
                exclusive: false, ..
            } => return Ok(()),
            FsOperation::Lock {
                file,
                exclusive: true,
            } => (FsTarget::File(file), object, false, true),
            // Windows handle watching retains its existing admission. Linux
            // inotify supplies a pathname; macOS kqueue supplies a file grant.
            #[cfg(windows)]
            FsOperation::Watch(FsTarget::File(_)) => return Ok(()),
            FsOperation::Watch(target) => (target, object, true, false),
        };
        let (path, diagnostic) = match target {
            FsTarget::Path(FsPath::Cwd(path)) => (
                self.working_directory
                    .resolve_absolute(Path::new(path))
                    .ok(),
                format!("{:?}", path.to_string_lossy()),
            ),
            FsTarget::Path(FsPath::At { base, path }) => (
                Some(base.join(path)),
                format!("{:?}", path.to_string_lossy()),
            ),
            FsTarget::Path(FsPath::AtHandle { directory, path }) => {
                // Query the directory even for an absolute child path, retaining
                // the existing directory-error precedence. Both open checks use
                // this same base, and no-policy calls never query it.
                let base = directory_path(directory)?;
                (
                    Some(base.join(path)),
                    format!("{:?}", path.to_string_lossy()),
                )
            }
            FsTarget::File(file) => match &file.0 {
                GrantSource::Stdio => return Ok(()),
                GrantSource::Path(path) => {
                    (Some(path.clone()), format!("{:?}", path.to_string_lossy()))
                }
                GrantSource::Untracked => (None, "\"<untracked resource>\"".to_owned()),
            },
        };
        let resolved = path.as_deref().and_then(|path| resolve(path).ok());
        policy
            .authorize(resolved.as_deref(), read, write, &diagnostic)
            .map_err(|_| FsError::Denied)?;
        if read_metadata {
            // Preserve the separate, subsequent read admission for extra open
            // metadata, including policy-group order and target re-resolution.
            let path = match target {
                FsTarget::Path(FsPath::Cwd(path)) => self
                    .working_directory
                    .resolve_absolute(Path::new(path))
                    .ok(),
                _ => path,
            };
            let resolved = path.as_deref().and_then(|path| object(path).ok());
            policy
                .authorize(resolved.as_deref(), true, false, &diagnostic)
                .map_err(|_| FsError::Denied)?;
        }
        Ok(())
    }
}
