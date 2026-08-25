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

//! Host-owned access policy for moonrun-owned wasm boundaries.
//!
//! No policy file preserves existing moonrun behavior. Supplying a policy file
//! switches the supported host surfaces to deny-by-default mode.

mod config;
mod env;
mod fs;
mod net;
mod process;

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::async_host::{AsyncHostError, AsyncHostResult};
use crate::env::Env;

use self::config::PolicyConfig;
use self::env::EnvPolicy;
pub(crate) use self::fs::RuntimePathBase;
use self::fs::{FsIntents, FsPolicy};
use self::net::{NetOperation, NetPolicy};
use self::process::ProcessPolicy;

#[derive(Clone, Debug)]
pub(crate) struct Policy {
    fs: Option<FsPolicy>,
    net: Option<NetPolicy>,
    env: Option<EnvPolicy>,
    process: Option<ProcessPolicy>,
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
    pub(crate) fn allow_all() -> Self {
        Self {
            fs: None,
            net: None,
            env: None,
            process: None,
        }
    }

    pub(crate) fn from_file(path: &Path) -> anyhow::Result<Self> {
        let config = PolicyConfig::from_file(path)?;
        let config_dir = path.parent().unwrap_or_else(|| Path::new("."));
        Self::from_config(config, config_dir)
    }

    fn from_config(config: PolicyConfig, config_dir: &Path) -> anyhow::Result<Self> {
        let config = canonicalize(config, config_dir)?;
        Ok(Self {
            fs: Some(FsPolicy::from_canonical_config(
                config.fs.unwrap_or_default(),
            )),
            net: Some(NetPolicy::from_config(config.net.unwrap_or_default())?),
            env: Some(EnvPolicy::from_config(config.env.unwrap_or_default())?),
            process: Some(ProcessPolicy::from_config(
                config.process.unwrap_or_default(),
            )?),
        })
    }

    pub(crate) fn open_path(
        &self,
        base: RuntimePathBase<'_>,
        path: &OsStr,
        access: i32,
        create_mode: i32,
        append: bool,
    ) -> AsyncHostResult<()> {
        let Some(fs) = self.fs_policy() else {
            return Ok(());
        };
        fs.allows(base, path, FsIntents::for_open(access, create_mode, append))
    }

    pub(crate) fn stat_path(&self, base: RuntimePathBase<'_>, path: &OsStr) -> AsyncHostResult<()> {
        self.read_path(base, path)
    }

    pub(crate) fn read_path(&self, base: RuntimePathBase<'_>, path: &OsStr) -> AsyncHostResult<()> {
        let Some(fs) = self.fs_policy() else {
            return Ok(());
        };
        fs.allows(base, path, FsIntents::read())
    }

    pub(crate) fn write_path(
        &self,
        base: RuntimePathBase<'_>,
        path: &OsStr,
    ) -> AsyncHostResult<()> {
        let Some(fs) = self.fs_policy() else {
            return Ok(());
        };
        fs.allows(base, path, FsIntents::write())
    }

    pub(crate) fn stat_entry_path(
        &self,
        base: RuntimePathBase<'_>,
        path: &OsStr,
    ) -> AsyncHostResult<()> {
        let Some(fs) = self.fs_policy() else {
            return Ok(());
        };
        fs.allows_entry(base, path, FsIntents::read())
    }

    pub(crate) fn access_path(&self, path: &OsStr, access: i32) -> AsyncHostResult<()> {
        let Some(fs) = self.fs_policy() else {
            return Ok(());
        };
        fs.allows(
            RuntimePathBase::CurrentDirectory,
            path,
            FsIntents::for_access_check(access),
        )
    }

    pub(crate) fn chmod_path(&self, path: &OsStr) -> AsyncHostResult<()> {
        self.write_path(RuntimePathBase::CurrentDirectory, path)
    }

    pub(crate) fn remove_path(&self, path: &OsStr) -> AsyncHostResult<()> {
        let Some(fs) = self.fs_policy() else {
            return Ok(());
        };
        fs.allows_entry(RuntimePathBase::CurrentDirectory, path, FsIntents::write())
    }

    pub(crate) fn rename_path(&self, old_path: &OsStr, new_path: &OsStr) -> AsyncHostResult<()> {
        let Some(fs) = self.fs_policy() else {
            return Ok(());
        };
        fs.allows_entry(
            RuntimePathBase::CurrentDirectory,
            old_path,
            FsIntents::write(),
        )?;
        fs.allows_entry(
            RuntimePathBase::CurrentDirectory,
            new_path,
            FsIntents::write(),
        )
    }

    pub(crate) fn symlink_path(&self, path: &OsStr) -> AsyncHostResult<()> {
        let Some(fs) = self.fs_policy() else {
            return Ok(());
        };
        fs.allows_entry(RuntimePathBase::CurrentDirectory, path, FsIntents::write())
    }

    pub(crate) fn mkdir_path(&self, path: &OsStr) -> AsyncHostResult<()> {
        let Some(fs) = self.fs_policy() else {
            return Ok(());
        };
        fs.allows_entry(RuntimePathBase::CurrentDirectory, path, FsIntents::write())
    }

    pub(crate) fn rmdir_path(&self, path: &OsStr) -> AsyncHostResult<()> {
        let Some(fs) = self.fs_policy() else {
            return Ok(());
        };
        fs.allows_entry(RuntimePathBase::CurrentDirectory, path, FsIntents::write())
    }

    pub(crate) fn lock_path(
        &self,
        base: RuntimePathBase<'_>,
        path: &OsStr,
        exclusive: bool,
    ) -> AsyncHostResult<()> {
        if !exclusive {
            return Ok(());
        }
        let Some(fs) = self.fs_policy() else {
            return Ok(());
        };
        fs.allows(base, path, FsIntents::write())
    }

    pub(crate) fn check_dns(&self, host: &OsStr) -> AsyncHostResult<()> {
        let Some(net) = self.net_policy() else {
            return Ok(());
        };
        net.check_dns(host)
    }

    pub(crate) fn register_dns_result(
        &self,
        host: &OsStr,
        addrs: &[Box<[u8]>],
    ) -> AsyncHostResult<()> {
        let Some(net) = self.net_policy() else {
            return Ok(());
        };
        net.register_dns_result(host, addrs)
    }

    pub(crate) fn check_connect(&self, addr: &[u8]) -> AsyncHostResult<()> {
        let Some(net) = self.net_policy() else {
            return Ok(());
        };
        net.check_socket(NetOperation::Connect, addr)
    }

    pub(crate) fn check_bind(&self, addr: &[u8]) -> AsyncHostResult<()> {
        let Some(net) = self.net_policy() else {
            return Ok(());
        };
        net.check_socket(NetOperation::Bind, addr)
    }

    pub(crate) fn realize_env(&self) -> anyhow::Result<Env> {
        self.env_policy()
            .map_or_else(|| Ok(Env::ambient()), EnvPolicy::realize)
    }

    pub(crate) fn has_env_policy(&self) -> bool {
        self.env_policy().is_some()
    }

    #[cfg(unix)]
    pub(crate) fn spawn_process_unix(
        &self,
        program: &OsStr,
        argv: &[OsString],
    ) -> AsyncHostResult<()> {
        self.process
            .as_ref()
            .map_or(Ok(()), |process| process.allows_unix(program, argv))
    }

    #[cfg(windows)]
    pub(crate) fn spawn_process_windows(&self, command_line: &OsStr) -> AsyncHostResult<()> {
        self.process
            .as_ref()
            .map_or(Ok(()), |process| process.allows_windows(command_line))
    }

    pub(crate) fn has_process_policy(&self) -> bool {
        self.process.is_some()
    }

    #[inline]
    fn fs_policy(&self) -> Option<&FsPolicy> {
        self.fs.as_ref()
    }

    #[inline]
    fn net_policy(&self) -> Option<&NetPolicy> {
        self.net.as_ref()
    }

    #[inline]
    fn env_policy(&self) -> Option<&EnvPolicy> {
        self.env.as_ref()
    }
}

/// Resolve source-relative policy values into a transport-independent form.
///
/// Environment materialization and runtime rule validation deliberately remain
/// part of Policy construction; only filesystem roots depend on the policy
/// source directory.
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
    use std::ffi::OsStr;
    use std::net::Ipv4Addr;
    use std::path::PathBuf;

    use crate::async_host::AsyncHostError;

    use super::*;

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

    #[test]
    fn no_policy_leaves_fs_unrestricted() {
        let policy = Policy::allow_all();

        policy
            .open_path(
                RuntimePathBase::CurrentDirectory,
                OsStr::new("missing-parent/new.txt"),
                1,
                4,
                false,
            )
            .unwrap();
    }

    #[test]
    fn missing_fs_section_denies_fs_in_policy_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = Policy::from_config(
            PolicyConfig {
                fs: None,
                net: Some(Default::default()),
                env: None,
                process: None,
            },
            tmp.path(),
        )
        .unwrap();

        let error = policy
            .open_path(
                RuntimePathBase::CurrentDirectory,
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
        let policy = Policy::from_config(
            PolicyConfig {
                fs: Some(Default::default()),
                net: None,
                env: None,
                process: None,
            },
            tmp.path(),
        )
        .unwrap();
        let denied = tmp.path().join("new.txt");

        let error = policy
            .open_path(
                RuntimePathBase::CurrentDirectory,
                denied.as_os_str(),
                1,
                4,
                false,
            )
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn no_policy_leaves_net_unrestricted() {
        let policy = Policy::allow_all();

        policy
            .check_connect(&ipv4_addr(Ipv4Addr::LOCALHOST, 443))
            .unwrap();
    }

    #[test]
    fn missing_net_section_denies_net_in_policy_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy = Policy::from_config(
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
        )
        .unwrap();

        let error = policy
            .check_connect(&ipv4_addr(Ipv4Addr::LOCALHOST, 443))
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn empty_net_section_denies_net() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = Policy::from_config(
            PolicyConfig {
                fs: None,
                net: Some(Default::default()),
                env: None,
                process: None,
            },
            tmp.path(),
        )
        .unwrap();

        let error = policy
            .check_connect(&ipv4_addr(Ipv4Addr::LOCALHOST, 443))
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn missing_env_section_uses_empty_env_in_policy_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = Policy::from_config(
            PolicyConfig {
                fs: None,
                net: None,
                env: None,
                process: None,
            },
            tmp.path(),
        )
        .unwrap();

        let environment = policy.realize_env().unwrap();
        assert!(environment.entries().is_empty());
        assert!(environment.get("PATH".as_ref()).is_none());
    }

    #[test]
    fn no_policy_leaves_process_spawning_unrestricted() {
        let policy = Policy::allow_all();
        #[cfg(unix)]
        policy
            .spawn_process_unix(OsStr::new("program"), &[OsString::from("program")])
            .unwrap();
        #[cfg(windows)]
        policy
            .spawn_process_windows(OsStr::new("program.exe"))
            .unwrap();
    }

    #[test]
    fn missing_process_section_denies_spawning_in_policy_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = Policy::from_config(PolicyConfig::default(), tmp.path()).unwrap();

        assert_eq!(
            {
                #[cfg(unix)]
                {
                    policy.spawn_process_unix(OsStr::new("program"), &[OsString::from("program")])
                }
                #[cfg(windows)]
                {
                    policy.spawn_process_windows(OsStr::new("program.exe"))
                }
            },
            Err(AsyncHostError::PermissionDenied)
        );
    }

    #[test]
    fn process_section_can_allow_spawning() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = Policy::from_config(
            PolicyConfig {
                process: Some(config::ProcessConfig {
                    spawn: true,
                    allow: Vec::new(),
                }),
                ..PolicyConfig::default()
            },
            tmp.path(),
        )
        .unwrap();

        #[cfg(unix)]
        policy
            .spawn_process_unix(OsStr::new("program"), &[OsString::from("program")])
            .unwrap();
        #[cfg(windows)]
        policy
            .spawn_process_windows(OsStr::new("program.exe"))
            .unwrap();
    }

    fn ipv4_addr(ip: Ipv4Addr, port: u16) -> Box<[u8]> {
        let mut addr = vec![0; crate::async_sys::socket::ipv4_addr_size() as usize];
        crate::async_sys::socket::init_ip_addr(&mut addr, u32::from(ip), u32::from(port)).unwrap();
        addr.into_boxed_slice()
    }
}
