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

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceRegistryDependencyInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub version: Option<Version>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceLocalDependencyInfo {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub version: Option<Version>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceGitDependencyInfo {
    pub git: String,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "branch")]
    pub git_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub version: Option<Version>,
}

/// Information about a specific dependency. This supports both simple string
/// syntax and detailed object syntax in `moon.mod.json`.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SourceDependencyInfo {
    #[schemars(with = "String")]
    Simple(Version),
    Local(SourceLocalDependencyInfo),
    Git(SourceGitDependencyInfo),
    Registry(SourceRegistryDependencyInfo),
}

impl Default for SourceDependencyInfo {
    fn default() -> Self {
        Self::Registry(SourceRegistryDependencyInfo { version: None })
    }
}

impl SourceDependencyInfo {
    pub fn version(&self) -> Option<&Version> {
        match self {
            SourceDependencyInfo::Simple(version) => Some(version),
            SourceDependencyInfo::Local(info) => info.version.as_ref(),
            SourceDependencyInfo::Git(info) => info.version.as_ref(),
            SourceDependencyInfo::Registry(info) => info.version.as_ref(),
        }
    }

    pub fn set_version(&mut self, version: Option<Version>) {
        match self {
            SourceDependencyInfo::Simple(existing) => {
                if let Some(version) = version {
                    *existing = version;
                } else {
                    *self = SourceDependencyInfo::Registry(SourceRegistryDependencyInfo {
                        version: None,
                    });
                }
            }
            SourceDependencyInfo::Local(info) => info.version = version,
            SourceDependencyInfo::Git(info) => info.version = version,
            SourceDependencyInfo::Registry(info) => info.version = version,
        }
    }

    pub fn path(&self) -> Option<&str> {
        match self {
            SourceDependencyInfo::Local(info) => Some(info.path.as_str()),
            _ => None,
        }
    }

    pub fn git(&self) -> Option<&str> {
        match self {
            SourceDependencyInfo::Git(info) => Some(info.git.as_str()),
            _ => None,
        }
    }

    pub fn git_branch(&self) -> Option<&str> {
        match self {
            SourceDependencyInfo::Git(info) => info.git_branch.as_deref(),
            _ => None,
        }
    }

    /// Check if the requirement is simple. That is, it only contains a version requirement.
    fn is_simple(&self) -> bool {
        matches!(self, SourceDependencyInfo::Simple(_))
            || matches!(
                self,
                SourceDependencyInfo::Registry(SourceRegistryDependencyInfo { version: Some(_) })
            )
    }
}

impl std::fmt::Debug for SourceDependencyInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(version) = self.version()
            && self.path().is_none()
            && self.git().is_none()
            && self.git_branch().is_none()
        {
            return write!(f, "{version}");
        }

        match self {
            SourceDependencyInfo::Simple(version) => write!(f, "{version}"),
            SourceDependencyInfo::Local(info) => f
                .debug_struct("SourceDependencyInfo::Local")
                .field("path", &info.path)
                .field(
                    "version",
                    &info.version.as_ref().map(std::string::ToString::to_string),
                )
                .finish(),
            SourceDependencyInfo::Git(info) => f
                .debug_struct("SourceDependencyInfo::Git")
                .field("git", &info.git)
                .field("git_branch", &info.git_branch)
                .field(
                    "version",
                    &info.version.as_ref().map(std::string::ToString::to_string),
                )
                .finish(),
            SourceDependencyInfo::Registry(info) => f
                .debug_struct("SourceDependencyInfo::Registry")
                .field(
                    "version",
                    &info.version.as_ref().map(std::string::ToString::to_string),
                )
                .finish(),
        }
    }
}

impl FromStr for SourceDependencyInfo {
    type Err = semver::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SourceDependencyInfo::Simple(Version::parse(s)?))
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
            common: SourceDependencyInfo::Simple(version),
            ..Default::default()
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
                    .version()
                    .cloned()
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
        assert!(dep.version().is_none());
        assert_eq!(dep.path(), Some("../dep"));
    }

    #[test]
    fn detailed_dependency_allows_null_version() {
        let dep: SourceDependencyInfo =
            serde_json_lenient::from_str(r#"{"version":null,"path":"../dep"}"#).unwrap();
        assert!(dep.version().is_none());
        assert_eq!(dep.path(), Some("../dep"));
    }

    #[test]
    fn detailed_dependency_rejects_path_git_mix() {
        let err = serde_json_lenient::from_str::<SourceDependencyInfo>(
            r#"{"path":"../dep","git":"https://example.com/dep.git"}"#,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("did not match any variant"),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn binary_dependency_allows_local_source_with_bin_pkg() {
        let dep: BinaryDependencyInfoJson =
            serde_json_lenient::from_str(r#"{"path":"../dep","bin_pkg":["main"]}"#).unwrap();
        let BinaryDependencyInfoJson::Detailed(dep) = dep else {
            panic!("expected detailed dependency");
        };
        assert_eq!(dep.common.path(), Some("../dep"));
        assert_eq!(dep.bin_pkg, Some(vec!["main".to_string()]));
    }

    #[test]
    fn binary_dependency_allows_git_source_with_bin_pkg() {
        let dep: BinaryDependencyInfoJson = serde_json_lenient::from_str(
            r#"{"git":"https://example.com/dep.git","branch":"main","bin_pkg":["tool"]}"#,
        )
        .unwrap();
        let BinaryDependencyInfoJson::Detailed(dep) = dep else {
            panic!("expected detailed dependency");
        };
        assert_eq!(dep.common.git(), Some("https://example.com/dep.git"));
        assert_eq!(dep.common.git_branch(), Some("main"));
        assert_eq!(dep.bin_pkg, Some(vec!["tool".to_string()]));
    }
}
