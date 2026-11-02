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
use std::path::{Path, PathBuf};

use super::{Policy, config::PolicyConfig, fs::FsRules, net::NetRules, process::ProcessRules};
use crate::runtime::EnvConfig;

/// One selected file's construction inputs. Only `policy` is inherited;
/// execution behavior and Env provisioning are not authorization rules.
pub(crate) struct LoadedPolicy {
    pub(crate) policy: Policy,
    pub(crate) environment: EnvConfig,
    pub(crate) virtualize: bool,
}

impl std::fmt::Debug for LoadedPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedPolicy")
            .field("virtualize", &self.virtualize)
            .finish_non_exhaustive()
    }
}

pub(crate) fn read_file(path: &Path, source_dir: Option<&Path>) -> anyhow::Result<LoadedPolicy> {
    let config = PolicyConfig::from_file(path)?;
    let directory = source_dir.unwrap_or_else(|| path.parent().unwrap_or_else(|| Path::new(".")));
    resolve(config, directory)
}

pub(super) fn resolve(config: PolicyConfig, directory: &Path) -> anyhow::Result<LoadedPolicy> {
    let mut config = canonicalize(config, directory)?;
    let environment = config.env.take().unwrap_or_default();
    let virtualize = config
        .process
        .as_ref()
        .is_some_and(|process| process.virtualize);
    Ok(LoadedPolicy {
        policy: Policy::new(
            FsRules::from_canonical_config(config.fs.unwrap_or_default()),
            NetRules::from_config(config.net.unwrap_or_default())?,
            ProcessRules::from_config(config.process.unwrap_or_default())?,
        ),
        environment,
        virtualize,
    })
}

/// Resolve source-relative policy values into a transport-independent form.
///
/// Environment provisioning and runtime rule validation deliberately remain
/// part of configuration realization; only filesystem roots depend on the
/// policy source directory.
fn canonicalize(mut config: PolicyConfig, config_dir: &Path) -> anyhow::Result<PolicyConfig> {
    if let Some(fs) = config.fs.as_mut() {
        fs.read = canonicalize_roots(std::mem::take(&mut fs.read), config_dir)?;
        fs.write = canonicalize_roots(std::mem::take(&mut fs.write), config_dir)?;
    }
    Ok(config)
}

fn canonicalize_roots(roots: Vec<PathBuf>, config_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    roots
        .into_iter()
        .map(|root| {
            if root.as_os_str() == OsStr::new("*") {
                return Ok(root);
            }
            let path = if root.is_absolute() {
                root
            } else {
                config_dir.join(root)
            };
            std::fs::canonicalize(&path).with_context(|| {
                format!(
                    "failed to resolve sandbox filesystem root {}",
                    path.display()
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::config;

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

    fn config_with_fs_roots(read: Vec<PathBuf>, write: Vec<PathBuf>) -> PolicyConfig {
        PolicyConfig {
            fs: Some(config::FsConfig { read, write }),
            ..Default::default()
        }
    }

    #[test]
    fn canonicalize_resolves_filesystem_roots_and_preserves_wildcards() {
        let tmp = tempfile::tempdir().unwrap();
        let read = tmp.path().join("read");
        let write = tmp.path().join("write");
        std::fs::create_dir(&read).unwrap();
        std::fs::create_dir(&write).unwrap();
        let canonical_read = std::fs::canonicalize(&read).unwrap();
        let canonical_write = std::fs::canonicalize(&write).unwrap();

        let config = canonicalize(
            PolicyConfig {
                fs: Some(config::FsConfig {
                    read: vec![PathBuf::from("read"), PathBuf::from("*")],
                    write: vec![write.clone()],
                }),
                net: Some(config::NetConfig {
                    dns: vec!["example.com".to_owned()],
                    ..Default::default()
                }),
                env: Some(config::EnvConfig {
                    from_host: vec!["PATH".to_owned()],
                    ..Default::default()
                }),
                process: Some(config::ProcessConfig {
                    spawn: true,
                    ..Default::default()
                }),
            },
            tmp.path(),
        )
        .unwrap();

        let fs = config.fs.unwrap();
        assert_eq!(fs.read, [canonical_read, PathBuf::from("*")]);
        assert_eq!(fs.write, [canonical_write]);
        assert_eq!(config.net.unwrap().dns, ["example.com"]);
        assert_eq!(config.env.unwrap().from_host, ["PATH"]);
        assert!(config.process.unwrap().spawn);
    }

    #[test]
    fn canonicalize_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let canonical_target = std::fs::canonicalize(&target).unwrap();

        let first = canonicalize(
            config_with_fs_roots(vec![PathBuf::from("target")], Vec::new()),
            tmp.path(),
        )
        .unwrap();
        let second = canonicalize(first, Path::new("unrelated-source-directory")).unwrap();

        assert_eq!(second.fs.unwrap().read, [canonical_target]);
    }

    #[cfg(unix)]
    #[test]
    fn canonicalize_resolves_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        let alias = tmp.path().join("alias");
        std::fs::create_dir(&target).unwrap();
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        let canonical_target = std::fs::canonicalize(&target).unwrap();

        let config =
            canonicalize(config_with_fs_roots(vec![alias], Vec::new()), tmp.path()).unwrap();

        assert_eq!(config.fs.unwrap().read, [canonical_target]);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn canonicalize_preserves_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;

        let tmp = tempfile::tempdir().unwrap();
        let name = std::ffi::OsString::from_vec(b"allowed-\xff".to_vec());
        let root = tmp.path().join(&name);
        std::fs::create_dir(&root).unwrap();

        let config = canonicalize(
            config_with_fs_roots(vec![PathBuf::from(name)], Vec::new()),
            tmp.path(),
        )
        .unwrap();

        assert_eq!(config.fs.unwrap().read, [root]);
    }

    #[test]
    fn canonicalize_reports_the_resolved_missing_root() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("missing");
        let error = match canonicalize(
            config_with_fs_roots(vec![PathBuf::from("missing")], Vec::new()),
            tmp.path(),
        ) {
            Ok(_) => panic!("expected a missing root to fail canonicalization"),
            Err(error) => error,
        };

        assert!(error.to_string().contains(&missing.display().to_string()));
    }
}
