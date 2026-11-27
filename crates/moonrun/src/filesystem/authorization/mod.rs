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

//! Filesystem requests and the authorizer interface shared by executors.

mod grant;
mod path;
mod policy;
pub(super) use path::directory_path;
pub(crate) use path::entry_before_directory_suffix;
pub(crate) use policy::PolicyAuthorizer;
#[cfg(unix)]
mod system_reads;

pub(crate) use grant::FileGrant;

use std::ffi::OsStr;
use std::path::Path;

use super::os::RawFile;
use super::{CreateMode, FsError, OpenAccess};
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

/// Bound to a Run at construction. An error denies the operation. Async file
/// opens retain the returned rights for subsequent handle operations.
pub(crate) trait FsAuthorizer: Send + Sync {
    fn authorize(&self, operation: FsOperation<'_>) -> Result<AccessRights, FsError>;
}

/// Requested effects carry only targets on which they can be performed.
/// Metadata and kind queries share semantics across pathname and handle access;
/// entry changes require a path, while locking requires an acquired file.
#[derive(Clone, Copy)]
pub(crate) enum FsOperation<'a> {
    ReadResource(&'a FileGrant),
    WriteResource(&'a FileGrant),
    Flush(&'a FileGrant),
    Chmod(FsPath<'a>),
    Read(FsPath<'a>),
    Write(FsPath<'a>),
    Kind(FsTarget<'a>),
    Metadata(FsTarget<'a>),
    ListDirectory(FsTarget<'a>),
    InspectEntry(FsPath<'a>),
    CreateDirectory {
        path: FsPath<'a>,
        /// None selects the host default (the legacy ABI has no mode argument).
        mode: Option<u32>,
    },
    CreateSymlink {
        path: FsPath<'a>,
        /// Link text, interpreted relative to the link's parent if relative.
        target: &'a OsStr,
        force_symlink: bool,
    },
    UnlinkFile(FsPath<'a>),
    RemoveDirectory(FsPath<'a>),
    /// Async removal with native choice of entry kind (including empty directories for removeat).
    RemoveEntry(FsPath<'a>),
    Rename {
        source: FsPath<'a>,
        destination: FsPath<'a>,
        options: super::os::RenameOptions,
    },
    WriteFile(FsPath<'a>),
    CreateDirectories(FsPath<'a>),
    /// Unknown async access or creation modes stay absent until validation
    /// so that a policy denial retains precedence over invalid input.
    Open {
        path: FsPath<'a>,
        access: Option<OpenAccess>,
        create: Option<CreateMode>,
        /// Append positions writes. Write access is determined by `access`.
        append: bool,
        /// Metadata beyond the identity supplied by opening.
        read_metadata: bool,
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
