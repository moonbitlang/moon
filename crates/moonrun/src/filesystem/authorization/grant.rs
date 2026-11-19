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

use crate::policy::AccessRights;

/// Authority captured at acquisition, independent of the file's later pathname.
/// OS access modes and ABI descriptor rights remain additional restrictions.
#[derive(Debug)]
pub(crate) struct FileGrant(pub(super) GrantSource);

#[derive(Debug)]
pub(super) enum GrantSource {
    Stdio,
    Acquired(AccessRights),
    Untracked,
}

impl FileGrant {
    /// Inherited or explicitly bound standard streams are host-provided grants.
    /// FIXME: Guest spawn also rebinds source resources as stdio grants. Revisit
    /// this when adding attenuation to resource delegation.
    pub(crate) const STDIO: Self = Self(GrantSource::Stdio);

    /// Pipes and other resources acquired outside filesystem path admission.
    pub(crate) const UNTRACKED: Self = Self(GrantSource::Untracked);

    pub(in crate::filesystem) fn acquired(access: AccessRights) -> Self {
        Self(GrantSource::Acquired(access))
    }
}
