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

//! Module names and their agreement with declared or selected versions.
//!
//! Shared by manifest validation and module resolution, without depending on
//! manifests, source acquisition, or the resolved dependency graph.

use std::{convert::Infallible, str::FromStr};

use arcstr::ArcStr;
use semver::Version;

/// The name of a module.
///
/// This type is cheaply clonable.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleName {
    /// The username part of the module name
    pub username: ArcStr,
    /// The unqualified name part of the module name
    pub unqual: ArcStr,
}

impl ModuleName {
    /// The canonical major-version suffix in `user/module/vN`.
    /// `/v0` and `/v1` are invalid; those versions use the unsuffixed name.
    pub fn major_version_suffix(&self) -> Result<Option<u64>, ModuleVersionError> {
        let major = self.unqual.split_once('/').and_then(|(_, suffix)| {
            let major: u64 = suffix.strip_prefix('v')?.parse().ok()?;
            // Integer parsing also accepts leading zeroes and a `+` sign.
            (suffix == format!("v{major}")).then_some(major)
        });
        match major {
            Some(0 | 1) => Err(ModuleVersionError::InvalidSuffix {
                module: self.clone(),
            }),
            _ => Ok(major),
        }
    }

    /// Check the path suffix, and its agreement with a version when supplied.
    /// Local development modules may omit their version.
    pub fn validate_version(&self, version: Option<&Version>) -> Result<(), ModuleVersionError> {
        if let Some(major) = self.major_version_suffix()?
            && let Some(version) = version
            && major != version.major
        {
            return Err(ModuleVersionError::Mismatch {
                module: self.clone(),
                major,
                version: version.clone(),
            });
        }
        Ok(())
    }

    /// Return the last segment of the name, that may be used as a short name
    /// of a package.
    pub fn last_segment(&self) -> &str {
        if let Some((_, r)) = self.unqual.rsplit_once('/') {
            r
        } else {
            &self.unqual
        }
    }

    pub fn last_segment_owned(&self) -> arcstr::Substr {
        self.unqual.substr_from(self.last_segment())
    }

    /// Return segments of the module name.
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        std::iter::once(&*self.username).chain(self.unqual.split('/'))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleVersionError {
    #[error("module `{module}` major-version suffix must be /v2 or higher")]
    InvalidSuffix { module: ModuleName },
    #[error("module `{module}` requires major version {major}, but got {version}")]
    Mismatch {
        module: ModuleName,
        major: u64,
        version: Version,
    },
}

impl std::fmt::Debug for ModuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.username.is_empty() {
            f.write_fmt(format_args!("{}", self.unqual))
        } else {
            f.write_fmt(format_args!("{}/{}", self.username, self.unqual))
        }
    }
}

impl std::fmt::Display for ModuleName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.username.is_empty() {
            f.write_fmt(format_args!("{}", self.unqual))
        } else {
            f.write_fmt(format_args!("{}/{}", self.username, self.unqual))
        }
    }
}

impl From<&str> for ModuleName {
    fn from(value: &str) -> Self {
        match value.split_once('/') {
            Some((username, pkgname)) => ModuleName {
                username: username.into(),
                unqual: pkgname.into(),
            },
            None => ModuleName {
                username: ArcStr::new(),
                unqual: value.into(),
            },
        }
    }
}

impl FromStr for ModuleName {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(s.into())
    }
}

impl PartialEq<(&str, &str)> for ModuleName {
    fn eq(&self, other: &(&str, &str)) -> bool {
        self.username == other.0 && self.unqual == other.1
    }
}
