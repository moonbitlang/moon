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
#[cfg(unix)]
mod system_reads;

pub(crate) use grant::FileGrant;
use grant::GrantSource;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::host::{FileOpenOptions, FsError, HostFs};
use super::os::RawFile;
use super::path::{canonicalize_entry_path, canonicalize_existing_prefix, directory_path};
use crate::policy::AccessRights;

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
    ReadResource(&'a FileGrant),
    WriteResource(&'a FileGrant),
    Flush(&'a FileGrant),
    Chmod(FsPath<'a>),
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

impl FsOperation<'_> {
    /// Paths and acquired files use the same operation-to-access interpretation.
    /// Extra open metadata retains its separate read admission.
    fn requirements(&self) -> (AccessRights, bool) {
        let access = match self {
            // Opening already supplied basic identity, independently of read.
            Self::Kind(FsTarget::File(_)) => AccessRights::NONE,
            Self::WriteResource(_)
            | Self::Flush(_)
            | Self::Write(_)
            | Self::Chmod(_)
            | Self::WriteFile(_)
            | Self::CreateDirectories(_)
            | Self::ChangeEntry(_)
            | Self::Rename { .. }
            | Self::Lock {
                exclusive: true, ..
            } => AccessRights::WRITE,
            Self::OpenAsync {
                access,
                create_mode,
                read_metadata,
                ..
            } => {
                let access = match access {
                    0 | 3 => AccessRights::READ,
                    1 => AccessRights::WRITE,
                    _ => AccessRights::READ_WRITE,
                };
                return (
                    if *create_mode != 0 {
                        access.union(AccessRights::WRITE)
                    } else {
                        access
                    },
                    *read_metadata,
                );
            }
            Self::OpenWithOptions { options, .. } => {
                let access = if options.read {
                    AccessRights::READ
                } else {
                    AccessRights::NONE
                };
                if options.write
                    || options.append
                    || options.create
                    || options.create_new
                    || options.truncate
                {
                    access.union(AccessRights::WRITE)
                } else {
                    access
                }
            }
            Self::AccessCheck { access: 2, .. } => AccessRights::WRITE,
            Self::ReadResource(_)
            | Self::Read(_)
            | Self::Kind(FsTarget::Path(_))
            | Self::Metadata(_)
            | Self::InspectEntry(_)
            | Self::Watch(_)
            | Self::Lock {
                exclusive: false, ..
            }
            | Self::AccessCheck { .. } => AccessRights::READ,
        };
        (access, false)
    }
}

impl HostFs {
    /// Authorize the operation and return the target's complete policy access.
    /// File acquisition retains this access; other callers only need admission.
    /// OS access modes and ABI descriptor rights remain additional restrictions.
    pub(crate) fn authorize(&self, operation: FsOperation<'_>) -> Result<AccessRights, FsError> {
        let (required, read_metadata) = operation.requirements();
        let acquired = match operation {
            FsOperation::ReadResource(file)
            | FsOperation::WriteResource(file)
            | FsOperation::Flush(file)
            | FsOperation::Kind(FsTarget::File(file))
            | FsOperation::Metadata(FsTarget::File(file))
            | FsOperation::Watch(FsTarget::File(file))
            | FsOperation::Lock { file, .. } => Some(file),
            _ => None,
        };
        if let Some(FileGrant(GrantSource::Acquired(access))) = acquired {
            return if access.contains(required) {
                Ok(*access)
            } else {
                Err(FsError::Denied)
            };
        }
        let Some(policy) = &self.policy else {
            return Ok(AccessRights::READ_WRITE);
        };
        // Operation interpretation stays here; the policy layer only matches
        // resolved targets against read/write rules, in policy-group order.
        let object: fn(&Path) -> Result<PathBuf, FsError> = canonicalize_existing_prefix;
        let entry: fn(&Path) -> Result<PathBuf, FsError> = canonicalize_entry_path;
        let (target, resolve) = match operation {
            // Preserve admission of stdio and non-filesystem resources (pipes,
            // sockets). Acquired file grants were checked above.
            FsOperation::ReadResource(_file)
            | FsOperation::WriteResource(_file)
            | FsOperation::Flush(_file) => return Ok(AccessRights::READ_WRITE),
            FsOperation::Rename {
                source,
                destination,
            } => {
                self.authorize(FsOperation::ChangeEntry(source))?;
                return self.authorize(FsOperation::ChangeEntry(destination));
            }
            FsOperation::InspectEntry(path) => (FsTarget::Path(path), entry),
            FsOperation::ChangeEntry(path) => (FsTarget::Path(path), entry),
            FsOperation::Read(path) => (FsTarget::Path(path), object),
            // Composite writes and recursive creation retain their existing
            // followed-target check, without inspecting intermediate entries.
            FsOperation::Write(path)
            | FsOperation::Chmod(path)
            | FsOperation::WriteFile(path)
            | FsOperation::CreateDirectories(path) => (FsTarget::Path(path), object),
            FsOperation::Kind(FsTarget::File(_)) => return Ok(AccessRights::READ_WRITE),
            FsOperation::Kind(target) | FsOperation::Metadata(target) => (target, object),
            FsOperation::OpenAsync { path, .. }
            | FsOperation::OpenWithOptions { path, .. }
            | FsOperation::AccessCheck { path, .. } => (FsTarget::Path(path), object),
            FsOperation::Lock {
                exclusive: false, ..
            } => return Ok(AccessRights::READ_WRITE),
            FsOperation::Lock {
                file,
                exclusive: true,
            } => (FsTarget::File(file), object),
            // Windows handle watching retains its existing admission. Linux
            // inotify supplies a pathname; macOS kqueue supplies a file grant.
            #[cfg(windows)]
            FsOperation::Watch(FsTarget::File(_)) => return Ok(AccessRights::READ_WRITE),
            FsOperation::Watch(target) => (target, object),
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
                GrantSource::Stdio => return Ok(AccessRights::READ_WRITE),
                GrantSource::Acquired(_) => unreachable!("acquired grant was checked above"),
                GrantSource::Untracked => (None, "\"<untracked resource>\"".to_owned()),
            },
        };
        let resolved = path.as_deref().and_then(|path| resolve(path).ok());
        #[cfg(unix)]
        {
            // A directory handle or a fixed cwd anchors a relative name. With an
            // ambient cwd, only an absolute name stays valid until a worker runs.
            let requested = match (&*self.working_directory, target) {
                (crate::runtime::WorkingDirectory::Ambient, FsTarget::Path(FsPath::Cwd(name))) => {
                    Some(Path::new(name))
                }
                _ => path.as_deref(),
            };
            if let (Some(requested), Some(resolved)) = (requested, resolved.as_deref())
                && system_reads::allows(&operation, requested, resolved)
            {
                return Ok(policy
                    .granted_access(Some(resolved))
                    .union(AccessRights::READ));
            }
        }
        policy
            .authorize(resolved.as_deref(), required, &diagnostic)
            .map_err(|_| FsError::Denied)?;
        if read_metadata {
            // Preserve the subsequent read check and policy-group error order,
            // using the same resolved target for admission and captured access.
            policy
                .authorize(resolved.as_deref(), AccessRights::READ, &diagnostic)
                .map_err(|_| FsError::Denied)?;
        }
        Ok(policy.granted_access(resolved.as_deref()))
    }
}
