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

//! Compiled authorization rules for moonrun-owned Wasm operations.
//!
//! No policy file preserves existing moonrun behavior. Supplying a policy file
//! switches the supported host surfaces to deny-by-default mode. Runtime
//! construction supplies each Host Domain with its policy and retains the
//! effective Policy for child inheritance. Environment provisioning is consumed
//! separately during construction and does not become a runtime policy check.

#[cfg(test)]
mod composition_tests;
mod config;
mod fs;
mod net;
mod process;
mod source;
mod transfer;

pub(crate) use source::{LoadedPolicy, read_file};

#[cfg(test)]
use self::config::PolicyConfig;
pub(crate) use self::fs::{FsIntents, FsPolicy};
pub(crate) use self::net::{NetOperation, NetPolicy, ResolvedConnectRule};
pub(crate) use self::process::ProcessPolicy;
use self::{fs::FsRules, net::NetRules, process::ProcessRules};
use crate::async_host::{AsyncHostError, AsyncHostResult};
#[cfg(test)]
use crate::runtime::EnvProvisioning;
#[cfg(test)]
use std::path::Path;
use std::sync::Arc;

/// Effective authorization: every rule group must allow the operation.
/// Source loading and transfer decoding validate groups before construction.
/// An absent `Option<Policy>` is unrestricted; a policy document always adds a
/// complete group, with omitted domains denied.
#[derive(Clone, Debug)]
pub(crate) struct Policy(Arc<[Rules]>);

/// In-process children share the effective policy; external children decode it.
#[derive(Debug)]
pub(crate) enum InheritedPolicyInput {
    Resolved(Policy),
    Transfer(Vec<u8>),
}

/// One policy source's compiled rules, with filesystem roots already resolved.
#[derive(Clone, Debug)]
struct Rules {
    fs: Arc<FsRules>,
    net: Arc<NetRules>,
    process: Arc<ProcessRules>,
}

fn sandbox_denied(action: &str, target: Option<&str>) -> AsyncHostResult<()> {
    if let Some(target) = target {
        eprintln!("Sandbox policy blocked {action}: {target}");
    } else {
        eprintln!("Sandbox policy blocked {action}");
    }
    Err(AsyncHostError::PermissionDenied)
}

impl Policy {
    fn new(fs: FsRules, net: NetRules, process: ProcessRules) -> Self {
        Self(Arc::from([Rules {
            fs: Arc::new(fs),
            net: Arc::new(net),
            process: Arc::new(process),
        }]))
    }

    /// Intersect complete policies without I/O, parsing, or Env provisioning.
    pub(crate) fn intersect(self, additional: Self) -> Self {
        Self(self.0.iter().chain(additional.0.iter()).cloned().collect())
    }

    pub(crate) fn filesystem(&self) -> FsPolicy {
        FsPolicy::from_rule_groups(self.0.iter().map(|rules| Arc::clone(&rules.fs)))
    }

    pub(crate) fn network(&self) -> NetPolicy {
        NetPolicy::from_rule_groups(self.0.iter().map(|rules| Arc::clone(&rules.net)))
    }

    pub(crate) fn process(&self) -> ProcessPolicy {
        ProcessPolicy::from_rule_groups(self.0.iter().map(|rules| Arc::clone(&rules.process)))
    }
}

// These helpers exercise the complete construction path in domain tests.
#[cfg(test)]
pub(crate) fn load_file(path: &Path) -> anyhow::Result<(Policy, EnvProvisioning)> {
    load_file_with_source_dir(path, None, &crate::runtime::Env::ambient())
}

#[cfg(test)]
pub(crate) fn load_file_with_source_dir(
    path: &Path,
    source_dir: Option<&Path>,
    source: &crate::runtime::Env,
) -> anyhow::Result<(Policy, EnvProvisioning)> {
    let LoadedPolicy {
        policy,
        environment: env,
        ..
    } = read_file(path, source_dir)?;
    Ok((
        policy,
        EnvProvisioning::new(env.from_host, env.required_from_host, env.set, source)?,
    ))
}

#[cfg(test)]
pub(crate) fn load_json(
    contents: &[u8],
    source: &crate::runtime::Env,
) -> anyhow::Result<(Policy, EnvProvisioning)> {
    realize_config(serde_json::from_slice(contents)?, Path::new("."), source)
}

#[cfg(test)]
fn realize_config(
    config: config::PolicyConfig,
    directory: &Path,
    source: &crate::runtime::Env,
) -> anyhow::Result<(Policy, EnvProvisioning)> {
    let LoadedPolicy {
        policy,
        environment: env,
        ..
    } = source::resolve(config, directory)?;
    Ok((
        policy,
        EnvProvisioning::new(env.from_host, env.required_from_host, env.set, source)?,
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ffi::OsStr;
    #[cfg(unix)]
    use std::ffi::OsString;
    use std::net::Ipv4Addr;
    use std::path::PathBuf;

    use super::*;
    use crate::async_host::AsyncHostError;

    fn check_ambient_open(
        policy: Option<Policy>,
        path: &OsStr,
        access: i32,
        create_mode: i32,
        append: bool,
    ) -> AsyncHostResult<()> {
        policy
            .as_ref()
            .map(Policy::filesystem)
            .map_or(Ok(()), |policy| {
                policy.authorize(
                    Some(Path::new(path)),
                    FsIntents::for_open(access, create_mode, append),
                    &format!("{:?}", path),
                )
            })
    }

    fn check_network_connect(policy: Option<Policy>, addr: &[u8]) -> AsyncHostResult<()> {
        policy
            .as_ref()
            .map(Policy::network)
            .map_or(Ok(()), |policy| {
                policy.check_socket(NetOperation::Connect, addr, &[])
            })
    }

    #[cfg(unix)]
    fn check_process_spawn(
        policy: Option<Policy>,
        program: &OsStr,
        argv: &[OsString],
    ) -> AsyncHostResult<()> {
        policy
            .as_ref()
            .map(Policy::process)
            .map_or(Ok(()), |policy| policy.allows_unix(program, argv))
    }

    #[cfg(windows)]
    fn check_process_spawn(policy: Option<Policy>, command_line: &OsStr) -> AsyncHostResult<()> {
        policy
            .as_ref()
            .map(Policy::process)
            .map_or(Ok(()), |policy| policy.allows_windows(command_line))
    }

    #[test]
    fn embedded_policy_can_resolve_roots_from_its_logical_source_directory() {
        let source_dir = tempfile::tempdir().unwrap();
        let allowed = source_dir.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();

        let temporary_source_dir = tempfile::tempdir().unwrap();
        let temporary_source = temporary_source_dir.path().join("stdin.mbtx");
        std::fs::write(
            &temporary_source,
            r#"---
permission(fs: { "read": ["allowed"] })
---

fn main {}
"#,
        )
        .unwrap();

        let (policy, _) = load_file_with_source_dir(
            &temporary_source,
            Some(source_dir.path()),
            &crate::runtime::Env::ambient(),
        )
        .unwrap();
        let allowed = std::fs::canonicalize(allowed).unwrap();

        check_ambient_open(Some(policy), allowed.as_os_str(), 0, 0, false).unwrap();
    }

    #[test]
    fn inherited_policy_preserves_canonical_rules_and_uses_the_child_environment() {
        let tmp = tempfile::tempdir().unwrap();
        let read = tmp.path().join("read");
        let write = tmp.path().join("write");
        std::fs::create_dir(&read).unwrap();
        std::fs::create_dir(&write).unwrap();
        let canonical_read = std::fs::canonicalize(&read).unwrap();
        let canonical_write = std::fs::canonicalize(&write).unwrap();
        let secret = "value-that-must-not-be-serialized";

        let (policy, _) = realize_config(
            PolicyConfig {
                fs: Some(config::FsConfig {
                    read: vec![PathBuf::from("read")],
                    write: vec![PathBuf::from("write")],
                }),
                net: Some(config::NetConfig {
                    dns: vec!["example.com".to_owned()],
                    connect: vec!["example.com:443".to_owned()],
                    bind: Vec::new(),
                }),
                env: Some(config::EnvConfig {
                    from_host: vec!["PATH".to_owned()],
                    required_from_host: Vec::new(),
                    set: BTreeMap::from([("MOONRUN_COPY_SECRET".to_owned(), secret.to_owned())]),
                }),
                process: Some(config::ProcessConfig {
                    spawn: true,
                    allow: Vec::new(),
                    ..Default::default()
                }),
            },
            tmp.path(),
            &crate::runtime::Env::ambient(),
        )
        .unwrap();
        let contents = policy.open_transfer().unwrap().read().unwrap();

        assert!(!String::from_utf8_lossy(&contents).contains(secret));
        let inherited = Policy::from_transfer(&contents).unwrap();
        let fs = inherited.filesystem();
        fs.authorize(Some(&canonical_read), FsIntents::read(), "read")
            .unwrap();
        fs.authorize(Some(&canonical_write), FsIntents::write(), "write")
            .unwrap();
        assert!(
            fs.authorize(Some(&canonical_write), FsIntents::read(), "read")
                .is_err()
        );
        inherited
            .network()
            .check_dns("example.com".as_ref())
            .unwrap();
        #[cfg(unix)]
        check_process_spawn(Some(inherited), "program".as_ref(), &["program".into()]).unwrap();
    }

    #[test]
    fn inherited_policy_is_independent_of_later_source_file_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("policy.json");
        std::fs::write(&source, r#"{"process":{"spawn":false}}"#).unwrap();
        let (parent, _) = load_file(&source).unwrap();
        let contents = parent.open_transfer().unwrap().read().unwrap();

        std::fs::write(&source, r#"{"process":{"spawn":true}}"#).unwrap();
        let inherited = Policy::from_transfer(&contents).unwrap();

        #[cfg(unix)]
        assert_eq!(
            check_process_spawn(
                Some(inherited),
                OsStr::new("program"),
                &[OsString::from("program")]
            ),
            Err(AsyncHostError::PermissionDenied)
        );
        #[cfg(windows)]
        assert_eq!(
            check_process_spawn(Some(inherited), OsStr::new("program.exe")),
            Err(AsyncHostError::PermissionDenied)
        );
    }

    #[test]
    fn no_policy_leaves_fs_unrestricted() {
        let policy: Option<Policy> = None;

        check_ambient_open(policy, OsStr::new("missing-parent/new.txt"), 1, 4, false).unwrap();
    }

    #[test]
    fn missing_fs_section_denies_fs_in_policy_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let (policy, _) = realize_config(
            PolicyConfig {
                fs: None,
                net: Some(Default::default()),
                env: None,
                process: None,
            },
            tmp.path(),
            &crate::runtime::Env::ambient(),
        )
        .unwrap();

        let error = check_ambient_open(
            Some(policy),
            OsStr::new("missing-parent/new.txt"),
            1,
            4,
            false,
        )
        .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn empty_fs_section_denies_fs() {
        let tmp = tempfile::tempdir().unwrap();
        let (policy, _) = realize_config(
            PolicyConfig {
                fs: Some(Default::default()),
                net: None,
                env: None,
                process: None,
            },
            tmp.path(),
            &crate::runtime::Env::ambient(),
        )
        .unwrap();
        let denied = tmp.path().join("new.txt");

        let error = check_ambient_open(Some(policy), denied.as_os_str(), 1, 4, false).unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn no_policy_leaves_net_unrestricted() {
        let policy: Option<Policy> = None;

        check_network_connect(policy, &ipv4_addr(Ipv4Addr::LOCALHOST, 443)).unwrap();
    }

    #[test]
    fn missing_net_section_denies_net_in_policy_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let (policy, _) = realize_config(
            PolicyConfig {
                fs: Some(config::FsConfig {
                    read: vec![PathBuf::from("allowed")],
                    write: vec![PathBuf::from("allowed")],
                }),
                net: None,
                env: None,
                process: None,
            },
            tmp.path(),
            &crate::runtime::Env::ambient(),
        )
        .unwrap();

        let error =
            check_network_connect(Some(policy), &ipv4_addr(Ipv4Addr::LOCALHOST, 443)).unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn empty_net_section_denies_net() {
        let tmp = tempfile::tempdir().unwrap();
        let (policy, _) = realize_config(
            PolicyConfig {
                fs: None,
                net: Some(Default::default()),
                env: None,
                process: None,
            },
            tmp.path(),
            &crate::runtime::Env::ambient(),
        )
        .unwrap();

        let error =
            check_network_connect(Some(policy), &ipv4_addr(Ipv4Addr::LOCALHOST, 443)).unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn missing_env_section_uses_empty_env_in_policy_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let (_, provisioning) = realize_config(
            PolicyConfig {
                fs: None,
                net: None,
                env: None,
                process: None,
            },
            tmp.path(),
            &crate::runtime::Env::ambient(),
        )
        .unwrap();

        let environment = provisioning.realize().unwrap();
        assert!(environment.entries().is_empty());
        assert!(environment.get("PATH".as_ref()).is_none());
    }

    #[test]
    fn no_policy_leaves_process_spawning_unrestricted() {
        let policy: Option<Policy> = None;
        #[cfg(unix)]
        check_process_spawn(policy, OsStr::new("program"), &[OsString::from("program")]).unwrap();
        #[cfg(windows)]
        check_process_spawn(policy, OsStr::new("program.exe")).unwrap();
    }

    #[test]
    fn missing_process_section_denies_spawning_in_policy_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let (policy, _) = realize_config(
            PolicyConfig::default(),
            tmp.path(),
            &crate::runtime::Env::ambient(),
        )
        .unwrap();

        assert_eq!(
            {
                #[cfg(unix)]
                {
                    check_process_spawn(
                        Some(policy),
                        OsStr::new("program"),
                        &[OsString::from("program")],
                    )
                }
                #[cfg(windows)]
                {
                    check_process_spawn(Some(policy), OsStr::new("program.exe"))
                }
            },
            Err(AsyncHostError::PermissionDenied)
        );
    }

    #[test]
    fn process_section_can_allow_spawning() {
        let tmp = tempfile::tempdir().unwrap();
        let (policy, _) = realize_config(
            PolicyConfig {
                process: Some(config::ProcessConfig {
                    spawn: true,
                    allow: Vec::new(),
                    ..Default::default()
                }),
                ..PolicyConfig::default()
            },
            tmp.path(),
            &crate::runtime::Env::ambient(),
        )
        .unwrap();

        #[cfg(unix)]
        check_process_spawn(
            Some(policy),
            OsStr::new("program"),
            &[OsString::from("program")],
        )
        .unwrap();
        #[cfg(windows)]
        check_process_spawn(Some(policy), OsStr::new("program.exe")).unwrap();
    }

    fn ipv4_addr(ip: Ipv4Addr, port: u16) -> Box<[u8]> {
        let mut addr = vec![0; crate::async_sys::socket::ipv4_addr_size() as usize];
        crate::async_sys::socket::init_ip_addr(&mut addr, u32::from(ip), u32::from(port)).unwrap();
        addr.into_boxed_slice()
    }

    #[cfg(unix)]
    #[test]
    fn inherited_roots_do_not_follow_replaced_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        let allowed = directory.path().join("allowed");
        let outside = directory.path().join("outside");
        std::fs::create_dir(&allowed).unwrap();
        std::fs::create_dir(&outside).unwrap();
        let file = directory.path().join("policy.json");
        std::fs::write(&file, r#"{"fs":{"read":["allowed"]}}"#).unwrap();
        let (parent, _) = load_file(&file).unwrap();
        let contents = parent.open_transfer().unwrap().read().unwrap();
        std::fs::rename(&allowed, directory.path().join("moved")).unwrap();
        std::os::unix::fs::symlink(&outside, &allowed).unwrap();
        let child = Policy::from_transfer(&contents).unwrap();
        let target = std::fs::canonicalize(&outside).unwrap().join("secret");
        assert_eq!(
            child
                .filesystem()
                .authorize(Some(&target), FsIntents::read(), "secret"),
            Err(AsyncHostError::PermissionDenied)
        );
        std::fs::remove_file(&allowed).unwrap();
        Policy::from_transfer(&contents).unwrap();
    }
}
