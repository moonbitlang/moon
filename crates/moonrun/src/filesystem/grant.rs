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

//! Authorization of operations on an acquired file, independent of its caller.

use std::path::PathBuf;

use crate::policy::{FsIntents, FsPolicy};

use super::host::{FsError, canonicalize_existing_prefix};

/// The operation being requested, not the policy rule a caller thinks it needs.
#[derive(Clone, Copy, Debug)]
pub(crate) enum FileOperation {
    Kind,
    Metadata,
    Lock {
        exclusive: bool,
    },
    #[cfg(any(windows, target_os = "macos"))]
    Watch,
}

/// File authorization context retained with a Resource.
///
/// Stdio is explicitly granted by the host. Other files retain pathname-based
/// policy: the saved opening path is re-resolved for each protected operation.
/// That path is neither stable object identity nor a snapshot of capabilities.
/// Callers must not depend on its representation or inspect path availability.
#[derive(Debug)]
pub(crate) struct FileGrant(GrantSource);

#[derive(Debug)]
enum GrantSource {
    Stdio,
    Path(PathBuf),
    Untracked,
}

impl FileGrant {
    /// Inherited or explicitly bound standard streams are host-provided grants.
    /// Native access modes and operation-specific handle checks still apply.
    /// FIXME: Guest spawn also rebinds source resources as stdio grants. Revisit
    /// this when adding attenuation to resource delegation.
    pub(crate) const STDIO: Self = Self(GrantSource::Stdio);

    /// Pipes and other resources without recorded pathname provenance.
    /// Absence of a pathname does not itself imply permission or denial.
    pub(crate) const UNTRACKED: Self = Self(GrantSource::Untracked);

    pub(crate) fn from_opened_path(path: Option<PathBuf>) -> Self {
        path.and_then(|path| std::fs::canonicalize(path).ok())
            .map_or(Self::UNTRACKED, |path| Self(GrantSource::Path(path)))
    }

    pub(super) fn authorize(
        &self,
        policy: Option<&FsPolicy>,
        operation: FileOperation,
    ) -> Result<(), FsError> {
        let canonical_path = match &self.0 {
            GrantSource::Stdio => return Ok(()),
            GrantSource::Path(path) => Some(path),
            GrantSource::Untracked => None,
        };
        let intents = match operation {
            FileOperation::Kind | FileOperation::Lock { exclusive: false } => return Ok(()),
            FileOperation::Metadata => FsIntents::read(),
            FileOperation::Lock { exclusive: true } => FsIntents::write(),
            #[cfg(target_os = "macos")]
            FileOperation::Watch => FsIntents::read(),
            // Preserve Windows' existing handle-only watcher admission. Changing
            // that policy is separate from extracting this authorization seam.
            #[cfg(windows)]
            FileOperation::Watch => return Ok(()),
        };
        let Some(policy) = policy else { return Ok(()) };
        let (resolved, target) = match canonical_path {
            Some(path) => (
                canonicalize_existing_prefix(path).ok(),
                format!("{:?}", path.to_string_lossy()),
            ),
            None => (None, "\"<untracked resource>\"".to_owned()),
        };
        policy
            .authorize(resolved.as_deref(), intents, &target)
            .map_err(|_| FsError::Denied)
    }
}
