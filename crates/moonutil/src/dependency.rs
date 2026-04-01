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

use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};

/// Information about a specific dependency
#[derive(Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct SourceDependencyInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub version: Option<Version>,
    // Other optional fields...
    /// Local path to the dependency. Overrides the version requirement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Git repository URL. Overrides the version requirement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    /// Git branch to use.
    #[serde(skip_serializing_if = "Option::is_none", rename = "branch")]
    pub git_branch: Option<String>,
}

impl std::fmt::Debug for SourceDependencyInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_simple() {
            write!(
                f,
                "{}",
                self.version
                    .as_ref()
                    .expect("simple dependency should always have a version")
            )
        } else {
            f.debug_struct("SourceDependencyInfo")
                .field(
                    "version",
                    &self.version.as_ref().map(std::string::ToString::to_string),
                )
                .finish()
        }
    }
}

/// The JSON representation of a source dependency info
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SourceDependencyInfoJson {
    /// A simple version requirement
    #[schemars(with = "String")]
    Simple(Version),
    /// A detailed dependency info
    Detailed(SourceDependencyInfo),
}

impl SourceDependencyInfo {
    /// Check if the requirement is simple. That is, it only contains a version requirement
    fn is_simple(&self) -> bool {
        self.version.is_some()
            && self.path.is_none()
            && self.git.is_none()
            && self.git_branch.is_none()
    }

    #[allow(clippy::needless_update)] // More fields will be added later
    fn from_simple(version: Version) -> Self {
        Self {
            version: Some(version),
            ..Default::default()
        }
    }
}

impl From<SourceDependencyInfo> for SourceDependencyInfoJson {
    fn from(dep: SourceDependencyInfo) -> Self {
        if dep.is_simple() {
            SourceDependencyInfoJson::Simple(
                dep.version
                    .expect("simple dependency should always have a version"),
            )
        } else {
            SourceDependencyInfoJson::Detailed(dep)
        }
    }
}

impl From<SourceDependencyInfoJson> for SourceDependencyInfo {
    fn from(dep: SourceDependencyInfoJson) -> Self {
        match dep {
            SourceDependencyInfoJson::Simple(v) => SourceDependencyInfo::from_simple(v),
            SourceDependencyInfoJson::Detailed(d) => d,
        }
    }
}

impl FromStr for SourceDependencyInfo {
    type Err = semver::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SourceDependencyInfo::from_simple(Version::parse(s)?))
    }
}

/// The JSON representation of a binary dependency info
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BinaryDependencyInfoJson {
    /// A simple version requirement
    #[schemars(with = "String")]
    Simple(Version),
    /// A detailed dependency info
    Detailed(BinaryDependencyInfo),
}

/// Information about a specific dependency
#[derive(Clone, Serialize, Deserialize, Default, Debug, JsonSchema)]
pub struct BinaryDependencyInfo {
    #[serde(flatten)]
    pub common: SourceDependencyInfo,

    /// Binary packages to compile.
    #[serde(skip_serializing_if = "Option::is_none", alias = "bin-pkg")]
    pub bin_pkg: Option<Vec<String>>,
}

impl BinaryDependencyInfo {
    /// Check if the requirement is simple. That is, it only contains a version requirement
    fn is_simple(&self) -> bool {
        self.common.is_simple() && self.bin_pkg.is_none()
    }

    #[allow(clippy::needless_update)] // More fields will be added later
    fn from_simple(version: Version) -> Self {
        Self {
            common: SourceDependencyInfo::from_simple(version),
            ..Default::default()
        }
    }
}

impl From<BinaryDependencyInfo> for SourceDependencyInfoJson {
    fn from(dep: BinaryDependencyInfo) -> Self {
        if dep.is_simple() {
            SourceDependencyInfoJson::Simple(
                dep.common
                    .version
                    .expect("simple dependency should always have a version"),
            )
        } else {
            SourceDependencyInfoJson::Detailed(dep.into())
        }
    }
}

impl From<BinaryDependencyInfo> for SourceDependencyInfo {
    fn from(dep: BinaryDependencyInfo) -> Self {
        dep.common
    }
}

impl From<BinaryDependencyInfo> for BinaryDependencyInfoJson {
    fn from(dep: BinaryDependencyInfo) -> Self {
        if dep.is_simple() {
            BinaryDependencyInfoJson::Simple(
                dep.common
                    .version
                    .expect("simple dependency should always have a version"),
            )
        } else {
            BinaryDependencyInfoJson::Detailed(dep)
        }
    }
}

impl From<BinaryDependencyInfoJson> for BinaryDependencyInfo {
    fn from(dep: BinaryDependencyInfoJson) -> Self {
        match dep {
            BinaryDependencyInfoJson::Simple(v) => BinaryDependencyInfo::from_simple(v),
            BinaryDependencyInfoJson::Detailed(d) => d,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detailed_dependency_allows_missing_version() {
        let dep: SourceDependencyInfo =
            serde_json_lenient::from_str(r#"{"path":"../dep"}"#).unwrap();
        assert!(dep.version.is_none());
        assert_eq!(dep.path.as_deref(), Some("../dep"));
    }

    #[test]
    fn detailed_dependency_allows_null_version() {
        let dep: SourceDependencyInfo =
            serde_json_lenient::from_str(r#"{"version":null,"path":"../dep"}"#).unwrap();
        assert!(dep.version.is_none());
        assert_eq!(dep.path.as_deref(), Some("../dep"));
    }
}
