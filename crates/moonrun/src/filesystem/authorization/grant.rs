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

//! Opaque authority retained with an acquired file.

use std::path::PathBuf;

/// File authorization context retained with a Resource.
///
/// Stdio is explicitly granted by the host. Other files retain pathname-based
/// policy: the saved opening path is re-resolved for each protected operation.
/// That path is neither stable object identity nor a snapshot of capabilities.
/// Callers must not depend on its representation or inspect path availability.
#[derive(Debug)]
pub(crate) struct FileGrant(pub(super) GrantSource);

#[derive(Debug)]
pub(super) enum GrantSource {
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
}
