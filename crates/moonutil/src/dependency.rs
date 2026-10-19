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

/// Information about a specific dependency. This supports both simple string
/// syntax and detailed object syntax in `moon.mod.json`.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SourceDependencyInfo {
    #[schemars(with = "String")]
    Simple(Version),
    Local(SourceLocalDependencyInfo),
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
            SourceDependencyInfo::Registry(info) => info.version = version,
        }
    }

    pub fn path(&self) -> Option<&str> {
        match self {
            SourceDependencyInfo::Local(info) => Some(info.path.as_str()),
            _ => None,
        }
    }
}

impl std::fmt::Debug for SourceDependencyInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(version) = self.version()
            && self.path().is_none()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependencies_reject_git_and_branch_fields() {
        for json in [
            r#"{"git":"https://example.com/dep.git"}"#,
            r#"{"git":"https://example.com/dep.git","branch":"main"}"#,
            r#"{"version":"1.0.0","branch":"main"}"#,
            r#"{"version":"1.0.0","git":"https://example.com/dep.git"}"#,
        ] {
            assert!(
                serde_json_lenient::from_str::<SourceDependencyInfo>(json).is_err(),
                "accepted source dependency {json}"
            );
        }
    }
}
