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

//! Policy-file I/O and source-relative path resolution.

use anyhow::Context;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use super::{
    Policy,
    config::{FsRuleConfig, PolicyConfig},
    fs::{FsRule, FsRules},
    net::NetRules,
    process::ProcessRules,
};
use crate::runtime::EnvConfig;

/// One selected file's construction inputs. Only `policy` is inherited;
/// execution behavior and Env provisioning are not authorization rules.
pub(crate) struct LoadedPolicy {
    pub(crate) policy: Policy,
    pub(crate) environment: EnvConfig,
    pub(crate) virtualize: bool,
    pub(crate) ipc: bool,
}

impl std::fmt::Debug for LoadedPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedPolicy")
            .field("virtualize", &self.virtualize)
            .field("ipc", &self.ipc)
            .finish_non_exhaustive()
    }
}

pub(crate) fn read_file(path: &Path, source_dir: Option<&Path>) -> anyhow::Result<LoadedPolicy> {
    let config = PolicyConfig::from_file(path)?;
    let directory = source_dir.unwrap_or_else(|| path.parent().unwrap_or_else(|| Path::new(".")));
    resolve(config, directory)
}

pub(super) fn resolve(config: PolicyConfig, directory: &Path) -> anyhow::Result<LoadedPolicy> {
    // A bare policy filename has an empty parent path; it denotes cwd.
    let directory = if directory.as_os_str().is_empty() {
        Path::new(".")
    } else {
        directory
    };
    let environment = config.env.unwrap_or_default();
    let fs = config.fs.unwrap_or_default();
    let virtualize = config
        .process
        .as_ref()
        .is_some_and(|process| process.virtualize);
    let ipc = config.process.as_ref().is_some_and(|process| process.ipc);
    Ok(LoadedPolicy {
        policy: Policy::new(
            FsRules {
                read_rules: resolve_fs_rules(fs.read, directory)?,
                write_rules: resolve_fs_rules(fs.write, directory)?,
            },
            NetRules::from_config(config.net.unwrap_or_default())?,
            ProcessRules::from_config(config.process.unwrap_or_default())?,
        ),
        environment,
        virtualize,
        ipc,
    })
}

/// Capture filesystem inputs only while loading a selected policy. Inheritance
/// carries these resolved bases; it must not repeat this filesystem observation.
fn resolve_fs_rules(rules: Vec<FsRuleConfig>, directory: &Path) -> anyhow::Result<Vec<FsRule>> {
    rules
        .into_iter()
        .map(|rule| match rule {
            FsRuleConfig::Path(root) => {
                if root.as_os_str() == OsStr::new("*") {
                    return Ok(FsRule::Any);
                }
                let path = directory.join(root);
                let root = std::fs::canonicalize(&path).with_context(|| {
                    format!(
                        "failed to resolve sandbox filesystem root {}",
                        path.display()
                    )
                })?;
                Ok(FsRule::Path(root))
            }
            FsRuleConfig::Glob { glob } => {
                // Negation belongs to policy syntax, before host path parsing.
                // Prefix a literal leading bang with ./ in a glob.
                let (pattern, exclude) = match glob.strip_prefix('!') {
                    Some(pattern) => (pattern, true),
                    None => (glob.as_str(), false),
                };
                let path = Path::new(pattern);
                // Only the leading root / ./ / ../ sequence determines the base.
                // Literal names inside the glob are matched against resolved target
                // names, not canonicalized as extra grants through symlinks.
                let anchor: PathBuf = path
                    .components()
                    .take_while(|part| !matches!(part, Component::Normal(_)))
                    .collect();
                anyhow::ensure!(
                    anchor.is_absolute()
                        || (!anchor.has_root()
                            && !anchor
                                .components()
                                .any(|part| matches!(part, Component::Prefix(_)))),
                    "filesystem glob cannot use a drive-relative or root-relative base"
                );
                let base = std::fs::canonicalize(directory.join(&anchor)).with_context(|| {
                    format!("failed to resolve filesystem glob base for {glob:?}")
                })?;
                let pattern = path.strip_prefix(anchor)?;
                // Glob separators are explicit. Backslash is a native separator on
                // Windows and a literal filename character on Unix, never an escape.
                let pattern = pattern
                    .components()
                    .map(|part| part.as_os_str().to_str().unwrap())
                    .collect::<Vec<_>>()
                    .join("/");
                FsRule::glob(base, &pattern, exclude)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_configuration_is_frozen_before_admission() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("policy.toml");
        std::fs::write(
            &path,
            r#"
[process]
virtualize = true
allow = [{ program = "moonx", args_prefix = ["tool.mbtx"] }]
[env]
required_from_host = ["VALUE"]
"#,
        )
        .unwrap();
        // Loading must not provision from the embedding process environment.
        let options = crate::RunOptions::default()
            .with_policy_file(&path)
            .unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(options.virtualize());
        let options = options.with_virtualize(false);
        assert!(!options.virtualize());
        let (policy, environment) = options
            .runtime
            .realize_policy(crate::runtime::Env::owned([("VALUE".into(), "child".into())]).unwrap())
            .unwrap();
        assert_eq!(environment.get(OsStr::new("VALUE")), Some("child".into()));
        let process = policy.unwrap().process();
        #[cfg(unix)]
        {
            process
                .allows_unix("moonx".as_ref(), &["moonx".into(), "tool.mbtx".into()])
                .unwrap();
            assert!(
                process
                    .allows_unix("moonx".as_ref(), &["moonx".into(), "other.mbtx".into()])
                    .is_err()
            );
        }
        #[cfg(windows)]
        {
            process
                .allows_windows("moonx.exe tool.mbtx".as_ref())
                .unwrap();
            assert!(
                process
                    .allows_windows("moonx.exe other.mbtx".as_ref())
                    .is_err()
            );
        }
    }

    #[test]
    fn inherited_ceiling_does_not_select_execution_or_suppress_child_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("policy.json");
        let parent = include_bytes!("../../tests/fixtures/deny_restrictions.json").to_vec();
        for virtualize in [false, true] {
            std::fs::write(
                &path,
                format!(r#"{{"process":{{"virtualize":{virtualize},"spawn":true}}}}"#),
            )
            .unwrap();
            for parent_first in [false, true] {
                let options = if parent_first {
                    crate::RunOptions::default()
                        .with_inherited_policy(parent.clone())
                        .with_policy_file(&path)
                        .unwrap()
                } else {
                    crate::RunOptions::default()
                        .with_policy_file(&path)
                        .unwrap()
                        .with_inherited_policy(parent.clone())
                };
                assert_eq!(options.virtualize(), virtualize);
                let (policy, _) = options
                    .runtime
                    .realize_policy(crate::runtime::Env::owned([]).unwrap())
                    .unwrap();
                let policy = policy.unwrap();
                let process = policy.process();
                #[cfg(unix)]
                assert!(
                    process
                        .allows_unix("moonx".as_ref(), &["moonx".into()])
                        .is_err()
                );
                #[cfg(windows)]
                assert!(process.allows_windows("moonx.exe".as_ref()).is_err());
                let transfer = policy.open_transfer().unwrap().read().unwrap();
                let child = crate::RunOptions::default().with_inherited_policy(transfer.clone());
                assert!(!child.virtualize());
                assert!(!String::from_utf8(transfer).unwrap().contains("virtualize"));
            }
        }
    }

    #[test]
    fn resolves_literal_roots_and_preserves_wildcards() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        std::fs::create_dir(&root).unwrap();
        let rules = resolve_fs_rules(
            vec![
                FsRuleConfig::Path("data".into()),
                FsRuleConfig::Path(root.clone()),
                FsRuleConfig::Path("*".into()),
            ],
            tmp.path(),
        )
        .unwrap();
        let canonical = std::fs::canonicalize(root).unwrap();
        assert!(matches!(&rules[0], FsRule::Path(path) if path == &canonical));
        assert!(matches!(&rules[1], FsRule::Path(path) if path == &canonical));
        assert!(matches!(rules[2], FsRule::Any));
    }

    #[cfg(unix)]
    #[test]
    fn resolves_literal_symlink_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        let alias = tmp.path().join("alias");
        std::fs::create_dir(&target).unwrap();
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        let canonical = std::fs::canonicalize(target).unwrap();
        let rules = resolve_fs_rules(vec![FsRuleConfig::Path(alias)], tmp.path()).unwrap();
        assert!(matches!(&rules[0], FsRule::Path(path) if path == &canonical));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn preserves_native_literal_path_bytes() {
        use std::os::unix::ffi::OsStringExt;
        let tmp = tempfile::tempdir().unwrap();
        let name = std::ffi::OsString::from_vec(b"allowed-\xff".to_vec());
        let root = tmp.path().join(&name);
        std::fs::create_dir(&root).unwrap();
        let rules = resolve_fs_rules(vec![FsRuleConfig::Path(name.into())], tmp.path()).unwrap();
        assert!(matches!(&rules[0], FsRule::Path(path) if path == &root));
    }

    #[test]
    fn reports_the_resolved_missing_literal_root() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("missing");
        let error =
            resolve_fs_rules(vec![FsRuleConfig::Path("missing".into())], tmp.path()).unwrap_err();
        assert!(error.to_string().contains(&missing.display().to_string()));
    }
}
