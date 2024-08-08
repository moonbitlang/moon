// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Dependency info requires a detour to support both string and structured formats

use std::str::FromStr;

use semver::VersionReq;
use serde::{Deserialize, Serialize, Serializer};

/// Information about a specific dependency
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct DependencyInfo {
    #[serde(serialize_with = "serialize_version_req")]
    #[serde(default, skip_serializing_if = "version_is_default")]
    pub version: VersionReq,
    // Other optional fields...
    /// Local path to the dependency. Overrides the version requirement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Git repository URL. Overrides the version requirement unless the dependency comes from a
    /// registry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    /// Git branch to use.
    #[serde(skip_serializing_if = "Option::is_none", rename = "branch")]
    pub git_branch: Option<String>,
    /// Git revision to use.
    #[serde(skip_serializing_if = "Option::is_none", rename = "revision")]
    pub git_revision: Option<String>,
}

fn version_is_default(version: &VersionReq) -> bool {
    version.comparators.is_empty()
}

impl std::fmt::Debug for DependencyInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_simple() {
            write!(f, "{}", self.version)
        } else {
            f.debug_struct("DependencyInfo")
                .field("version", &format_args!("{}", self.version))
                .finish()
        }
    }
}

/// The JSON representation of a dependency info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencyInfoJson {
    /// A simple version requirement
    Simple(#[serde(serialize_with = "serialize_version_req")] VersionReq),
    /// A detailed dependency info
    Detailed(DependencyInfo),
}

impl DependencyInfo {
    /// Check if the requirement is simple. That is, it only contains a version requirement
    fn is_simple(&self) -> bool {
        self.path.is_none()
            && self.git.is_none()
            && self.git_branch.is_none()
            && self.git_revision.is_none()
    }

    #[allow(clippy::needless_update)] // More fields will be added later
    fn from_simple(version: VersionReq) -> Self {
        Self {
            version,
            ..Default::default()
        }
    }
}

impl From<DependencyInfo> for DependencyInfoJson {
    fn from(dep: DependencyInfo) -> Self {
        if dep.is_simple() {
            DependencyInfoJson::Simple(dep.version)
        } else {
            DependencyInfoJson::Detailed(dep)
        }
    }
}

impl From<DependencyInfoJson> for DependencyInfo {
    fn from(dep: DependencyInfoJson) -> Self {
        match dep {
            DependencyInfoJson::Simple(v) => DependencyInfo::from_simple(v),
            DependencyInfoJson::Detailed(d) => d,
        }
    }
}

fn serialize_version_req<S: Serializer>(v: &VersionReq, s: S) -> Result<S::Ok, S::Error> {
    if v.comparators.len() == 1 && v.comparators[0].op == semver::Op::Caret {
        // Format `^a.b.c` as `a.b.c`
        s.collect_str(&ComparatorFormatWrapper(&v.comparators[0]))
    } else {
        v.serialize(s)
    }
}

struct ComparatorFormatWrapper<'a>(&'a semver::Comparator);

impl<'a> std::fmt::Display for ComparatorFormatWrapper<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.major)?;
        if let Some(minor) = &self.0.minor {
            write!(f, ".{}", minor)?;
            if let Some(patch) = &self.0.patch {
                write!(f, ".{}", patch)?;
                if !self.0.pre.is_empty() {
                    write!(f, "-{}", self.0.pre)?;
                }
            }
        }
        Ok(())
    }
}

impl FromStr for DependencyInfo {
    type Err = semver::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DependencyInfo::from_simple(VersionReq::parse(s)?))
    }
}
