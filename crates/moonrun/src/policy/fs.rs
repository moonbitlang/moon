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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::async_host::AsyncHostResult;

use super::{sandbox_denied, transfer};
use anyhow::Context;
use globset::{GlobBuilder, GlobMatcher};

/// Checks filesystem operations against every source's rules.
///
/// This is not a virtual filesystem. `HostFs` resolves runtime paths before
/// asking these immutable rules whether the resulting target is allowed.
#[derive(Clone, Debug)]
pub(crate) struct FsPolicy {
    rule_groups: Vec<Arc<FsRules>>,
}

impl Default for FsPolicy {
    fn default() -> Self {
        Self::from_rule_groups([Arc::new(FsRules::default())])
    }
}

impl FsPolicy {
    pub(super) fn from_rule_groups(rule_groups: impl IntoIterator<Item = Arc<FsRules>>) -> Self {
        Self {
            rule_groups: rule_groups.into_iter().collect(),
        }
    }

    pub(crate) fn authorize(
        &self,
        path: Option<&Path>,
        read: bool,
        write: bool,
        target: &str,
    ) -> AsyncHostResult<()> {
        self.rule_groups
            .iter()
            .try_for_each(|group| group.authorize(path, read, write, target))
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct FsRules {
    pub(super) read_rules: Vec<FsRule>,
    pub(super) write_rules: Vec<FsRule>,
}

#[derive(Clone, Debug)]
pub(super) enum FsRule {
    Any,
    Path(PathBuf),
    Glob {
        base: PathBuf,
        matcher: GlobMatcher,
        exclude: bool,
    },
}

impl FsRules {
    pub(super) fn to_transfer(&self) -> transfer::Filesystem {
        let rules = |rules: &[FsRule]| {
            rules
                .iter()
                .map(|rule| match rule {
                    FsRule::Any => transfer::FsRule::Any,
                    FsRule::Path(path) => transfer::FsRule::Path(path.as_os_str().to_owned()),
                    FsRule::Glob {
                        base,
                        matcher,
                        exclude: false,
                    } => transfer::FsRule::Glob {
                        base: base.as_os_str().to_owned(),
                        pattern: matcher.glob().glob().to_owned(),
                    },
                    FsRule::Glob {
                        base,
                        matcher,
                        exclude: true,
                    } => transfer::FsRule::ExcludeGlob {
                        base: base.as_os_str().to_owned(),
                        pattern: matcher.glob().glob().to_owned(),
                    },
                })
                .collect()
        };
        transfer::Filesystem {
            read: rules(&self.read_rules),
            write: rules(&self.write_rules),
        }
    }

    pub(super) fn from_transfer(value: transfer::Filesystem) -> anyhow::Result<Self> {
        let rules = |rules: Vec<transfer::FsRule>| {
            rules
                .into_iter()
                .map(|rule| match rule {
                    transfer::FsRule::Any => Ok(FsRule::Any),
                    transfer::FsRule::Path(path) => {
                        let path = PathBuf::from(path);
                        validate_base(&path)?;
                        Ok(FsRule::Path(path))
                    }
                    transfer::FsRule::Glob { base, pattern } => {
                        FsRule::glob(base.into(), &pattern, false)
                    }
                    transfer::FsRule::ExcludeGlob { base, pattern } => {
                        FsRule::glob(base.into(), &pattern, true)
                    }
                })
                .collect::<anyhow::Result<Vec<_>>>()
        };
        Ok(Self {
            read_rules: rules(value.read)?,
            write_rules: rules(value.write)?,
        })
    }

    pub(crate) fn authorize(
        &self,
        resolved_path: Option<&Path>,
        read: bool,
        write: bool,
        target: &str,
    ) -> AsyncHostResult<()> {
        let Some(path) = resolved_path else {
            return sandbox_denied(
                match (read, write) {
                    (true, false) => "file read",
                    (false, true) => "file write",
                    _ => "file access",
                },
                Some(target),
            );
        };
        if read && !Self::allows(&self.read_rules, path) {
            return sandbox_denied("file read", Some(target));
        }
        if write && !Self::allows(&self.write_rules, path) {
            return sandbox_denied("file write", Some(target));
        }
        Ok(())
    }

    fn allows(rules: &[FsRule], path: &Path) -> bool {
        let mut allowed = false;
        for rule in rules {
            if rule.matches(path) {
                // Exclusions override every grant in this group, independent of
                // order. A list containing only exclusions grants nothing.
                if matches!(rule, FsRule::Glob { exclude: true, .. }) {
                    return false;
                }
                allowed = true;
            }
        }
        allowed
    }
}

impl FsRule {
    /// Compile a source-resolved pattern without observing the filesystem.
    pub(super) fn glob(base: PathBuf, pattern: &str, exclude: bool) -> anyhow::Result<Self> {
        validate_base(&base)?;
        anyhow::ensure!(
            !pattern.is_empty()
                && !pattern.contains('\0')
                && Path::new(pattern)
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_))),
            "filesystem glob must be a nonempty relative pattern without parent traversal"
        );
        let matcher = GlobBuilder::new(pattern)
            .literal_separator(true)
            .backslash_escape(false)
            .case_insensitive(false)
            .build()
            .with_context(|| format!("invalid filesystem glob {pattern:?}"))?
            .compile_matcher();
        Ok(Self::Glob {
            base,
            matcher,
            exclude,
        })
    }

    fn matches(&self, path: &Path) -> bool {
        match self {
            Self::Any => true,
            Self::Path(root) => path == root || path.starts_with(root),
            Self::Glob { base, matcher, .. } => path.strip_prefix(base).is_ok_and(|path| {
                // Candidate::new performs lossy conversion on Windows.
                // OsStr's encoded bytes preserve names while satisfying the
                // matcher's ASCII-compatible encoding requirement.
                matcher.is_match_candidate(&globset::Candidate::from_bytes(
                    path.as_os_str().as_encoded_bytes(),
                ))
            }),
        }
    }
}

fn validate_base(path: &Path) -> anyhow::Result<()> {
    anyhow::ensure!(path.is_absolute(), "filesystem rule base must be absolute");
    anyhow::ensure!(
        !path.as_os_str().as_encoded_bytes().contains(&0)
            && !path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir)),
        "invalid filesystem rule base"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::config::{FsConfig, FsRuleConfig};
    use super::*;
    use crate::async_host::AsyncHostError;

    fn policy(read: Vec<PathBuf>, write: Vec<PathBuf>, config_dir: &Path) -> FsPolicy {
        let selected = crate::policy::source::resolve(
            super::super::config::PolicyConfig {
                fs: Some(FsConfig {
                    read: read.into_iter().map(FsRuleConfig::Path).collect(),
                    write: write.into_iter().map(FsRuleConfig::Path).collect(),
                }),
                ..Default::default()
            },
            config_dir,
        )
        .unwrap();
        selected.policy.filesystem()
    }

    fn authorize(policy: &FsPolicy, path: &Path, read: bool, write: bool) -> AsyncHostResult<()> {
        let parent = std::fs::canonicalize(path.parent().unwrap()).unwrap();
        let resolved = parent.join(path.file_name().unwrap());
        policy.authorize(
            Some(&resolved),
            read,
            write,
            &format!("{:?}", path.as_os_str()),
        )
    }

    #[test]
    fn allows_missing_file_under_relative_root() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy = policy(Vec::new(), vec![PathBuf::from("allowed")], tmp.path());

        authorize(&policy, &allowed.join("new.txt"), false, true).unwrap();
    }

    #[test]
    fn denies_paths_outside_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&denied).unwrap();
        let policy = policy(Vec::new(), vec![allowed], tmp.path());

        let error = authorize(&policy, &denied.join("new.txt"), false, true).unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn read_rules_do_not_permit_write_operations() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy = policy(vec![PathBuf::from("allowed")], Vec::new(), tmp.path());
        let path = allowed.join("new.txt");

        authorize(&policy, &path, true, false).unwrap();
        let error = authorize(&policy, &path, false, true).unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn write_rules_do_not_permit_read_operations() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy = policy(Vec::new(), vec![PathBuf::from("allowed")], tmp.path());

        let error = authorize(&policy, &allowed.join("new.txt"), true, false).unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn untracked_relative_paths_are_denied() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = policy(
            vec![PathBuf::from("*")],
            vec![PathBuf::from("*")],
            tmp.path(),
        );

        let error = policy
            .authorize(None, true, false, "\"<untracked resource>\"")
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn wildcard_root_allows_any_host_path() {
        let tmp = tempfile::tempdir().unwrap();
        let denied = tmp.path().join("denied");
        std::fs::create_dir(&denied).unwrap();
        let policy = policy(Vec::new(), vec![PathBuf::from("*")], tmp.path());

        authorize(&policy, &denied.join("new.txt"), false, true).unwrap();
    }
}
