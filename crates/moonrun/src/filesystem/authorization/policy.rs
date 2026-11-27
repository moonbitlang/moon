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

//! Built-in filesystem rules, including path resolution and acquired grants.

use super::grant::GrantSource;
use super::path::{canonicalize_entry_path, canonicalize_existing_prefix, directory_path};
#[cfg(unix)]
use super::system_reads;
use super::{FileGrant, FsAuthorizer, FsOperation, FsPath, FsTarget};
use crate::filesystem::{CreateMode, FsError, OpenAccess};
use crate::policy::{AccessRights, FsPolicy};
use crate::runtime::WorkingDirectory;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) struct PolicyAuthorizer {
    policy: Option<FsPolicy>,
    working_directory: Arc<WorkingDirectory>,
    additional: Option<Box<dyn FsAuthorizer>>,
}

impl PolicyAuthorizer {
    pub(crate) fn new(
        policy: Option<FsPolicy>,
        working_directory: Arc<WorkingDirectory>,
        additional: Option<Box<dyn FsAuthorizer>>,
    ) -> Self {
        Self {
            policy,
            working_directory,
            additional,
        }
    }

    // Internal checks for the two rename entries use only the built-in rules.
    // The additional authorizer receives the original request once.
    fn policy_access(&self, operation: FsOperation<'_>) -> Result<AccessRights, FsError> {
        let acquired = match operation {
            FsOperation::ReadResource(file)
            | FsOperation::WriteResource(file)
            | FsOperation::Flush(file)
            | FsOperation::ListDirectory(FsTarget::File(file))
            | FsOperation::Kind(FsTarget::File(file))
            | FsOperation::Metadata(FsTarget::File(file))
            | FsOperation::Watch(FsTarget::File(file))
            | FsOperation::Lock { file, .. } => Some(file),
            _ => None,
        };
        if let Some(FileGrant(GrantSource::Acquired(access))) = acquired {
            return if access.contains(operation.requirements().0) {
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
            | FsOperation::ListDirectory(FsTarget::File(_file))
            | FsOperation::Flush(_file) => return Ok(AccessRights::READ_WRITE),
            // The built-in policy requires both entries to be writable for all rename options.
            FsOperation::Rename {
                source,
                destination,
                options: _options,
            } => {
                self.target_access(policy, operation, FsTarget::Path(source), entry)?;
                (FsTarget::Path(destination), entry)
            }
            // These creation options are visible to other authorizers. Built-in
            // write admission applies to the new entry in either case.
            FsOperation::CreateDirectory { path, mode: _mode } => (FsTarget::Path(path), entry),
            FsOperation::CreateSymlink {
                path,
                target: _target,
                force_symlink: _force_symlink,
            } => (FsTarget::Path(path), entry),
            FsOperation::InspectEntry(path)
            | FsOperation::UnlinkFile(path)
            | FsOperation::RemoveDirectory(path)
            | FsOperation::RemoveEntry(path) => (FsTarget::Path(path), entry),
            FsOperation::Read(path) => (FsTarget::Path(path), object),
            // Composite writes and recursive creation retain their existing
            // followed-target check, without inspecting intermediate entries.
            FsOperation::Write(path)
            | FsOperation::Chmod(path)
            | FsOperation::WriteFile(path)
            | FsOperation::CreateDirectories(path) => (FsTarget::Path(path), object),
            FsOperation::Kind(FsTarget::File(_)) => return Ok(AccessRights::READ_WRITE),
            FsOperation::ListDirectory(target)
            | FsOperation::Kind(target)
            | FsOperation::Metadata(target) => (target, object),
            FsOperation::Open { path, .. } | FsOperation::AccessCheck { path, .. } => {
                (FsTarget::Path(path), object)
            }
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
        self.target_access(policy, operation, target, resolve)
    }

    fn target_access(
        &self,
        policy: &FsPolicy,
        operation: FsOperation<'_>,
        target: FsTarget<'_>,
        resolve: fn(&Path) -> Result<PathBuf, FsError>,
    ) -> Result<AccessRights, FsError> {
        let (required, read_metadata) = operation.requirements();
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
            let requested = match (self.working_directory.as_ref(), target) {
                (WorkingDirectory::Ambient, FsTarget::Path(FsPath::Cwd(name))) => {
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

impl FsAuthorizer for PolicyAuthorizer {
    fn authorize(&self, operation: FsOperation<'_>) -> Result<AccessRights, FsError> {
        let ceiling = self.policy_access(operation)?;
        let Some(authorizer) = &self.additional else {
            return Ok(ceiling);
        };
        let allowed = authorizer.authorize(operation)?;
        let granted = ceiling.intersection(allowed);
        operation.check_grant(granted)?;
        Ok(granted)
    }
}

impl FsOperation<'_> {
    pub(in crate::filesystem) fn check_grant(&self, granted: AccessRights) -> Result<(), FsError> {
        let (required, read_metadata) = self.requirements();
        if granted.contains(required) && (!read_metadata || granted.contains(AccessRights::READ)) {
            Ok(())
        } else {
            Err(FsError::Denied)
        }
    }

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
            | Self::CreateDirectory { .. }
            | Self::CreateSymlink { .. }
            | Self::UnlinkFile(_)
            | Self::RemoveDirectory(_)
            | Self::RemoveEntry(_)
            | Self::Rename { .. }
            | Self::Lock {
                exclusive: true, ..
            } => AccessRights::WRITE,
            Self::Open {
                access,
                create,
                append: _append,
                read_metadata,
                ..
            } => {
                let access = match access {
                    Some(
                        OpenAccess::Read
                        | OpenAccess::ListDirectory
                        | OpenAccess::TraverseDirectory
                        | OpenAccess::Attributes,
                    ) => AccessRights::READ,
                    Some(OpenAccess::Write | OpenAccess::Delete) => AccessRights::WRITE,
                    Some(OpenAccess::ReadWrite) | None => AccessRights::READ_WRITE,
                };
                let access = if *create == Some(CreateMode::Open) {
                    access
                } else {
                    access.union(AccessRights::WRITE)
                };
                return (access, *read_metadata);
            }
            Self::AccessCheck { access: 2, .. } => AccessRights::WRITE,
            Self::ReadResource(_)
            | Self::Read(_)
            | Self::ListDirectory(_)
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
