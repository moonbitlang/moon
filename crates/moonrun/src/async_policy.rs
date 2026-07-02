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

use std::ffi::{OsStr, OsString};
use std::path::Path;

use crate::async_host::AsyncHostResult;

use self::config::PolicyConfig;
use self::env::EnvPolicy;
pub(crate) use self::fs::RuntimePathBase;
use self::fs::{FsIntents, FsPolicy};
use self::net::{NetOperation, NetPolicy};

#[derive(Clone, Debug)]
pub(crate) struct AsyncPolicy {
    fs: Option<FsPolicy>,
    net: Option<NetPolicy>,
    env: Option<EnvPolicy>,
}

impl AsyncPolicy {
    pub(crate) fn allow_all() -> Self {
        Self {
            fs: None,
            net: None,
            env: None,
        }
    }

    pub(crate) fn from_file(path: &Path) -> anyhow::Result<Self> {
        let config = PolicyConfig::from_file(path)?;
        let config_dir = path.parent().unwrap_or_else(|| Path::new("."));
        Self::from_config(config, config_dir)
    }

    fn from_config(config: PolicyConfig, config_dir: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            fs: Some(FsPolicy::from_config(
                config.fs.unwrap_or_default(),
                config_dir,
            )?),
            net: Some(NetPolicy::from_config(config.net.unwrap_or_default())?),
            env: Some(EnvPolicy::from_config(config.env.unwrap_or_default())?),
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
        let Some(fs) = self.fs_policy() else {
            return Ok(());
        };
        fs.allows(base, path, FsIntents::read())
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
        let Some(fs) = self.fs_policy() else {
            return Ok(());
        };
        fs.allows(RuntimePathBase::CurrentDirectory, path, FsIntents::write())
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

    pub(crate) fn resolve_dns(&self, host: &OsStr) -> AsyncHostResult<()> {
        let Some(net) = self.net_policy() else {
            return Ok(());
        };
        net.resolve_dns(host)
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

    pub(crate) fn connect_socket(&self, addr: &[u8]) -> AsyncHostResult<()> {
        let Some(net) = self.net_policy() else {
            return Ok(());
        };
        net.allows_socket(NetOperation::Connect, addr)
    }

    pub(crate) fn bind_socket(&self, addr: &[u8]) -> AsyncHostResult<()> {
        let Some(net) = self.net_policy() else {
            return Ok(());
        };
        net.allows_socket(NetOperation::Bind, addr)
    }

    pub(crate) fn env_vars(&self) -> Vec<(String, String)> {
        self.env_policy()
            .map_or_else(|| std::env::vars().collect(), EnvPolicy::vars)
    }

    pub(crate) fn get_env_var(&self, name: &str) -> Option<String> {
        self.env_policy()
            .map_or_else(|| std::env::var(name).ok(), |env| env.get(name))
    }

    pub(crate) fn env_var_exists(&self, name: &str) -> bool {
        self.env_policy()
            .map_or_else(|| std::env::var(name).is_ok(), |env| env.contains(name))
    }

    pub(crate) fn env_var_os(&self, name: &str) -> Option<OsString> {
        self.env_policy().map_or_else(
            || std::env::var_os(name),
            |env| env.get(name).map(OsString::from),
        )
    }

    pub(crate) fn has_env_policy(&self) -> bool {
        self.env_policy().is_some()
    }

    pub(crate) fn set_env_var(&self, name: String, value: String) {
        if let Some(env) = self.env_policy() {
            env.set(name, value);
        } else {
            // TODO: Audit that the environment access only happens in single-threaded code.
            unsafe { std::env::set_var(name, value) };
        }
    }

    pub(crate) fn unset_env_var(&self, name: &str) {
        if let Some(env) = self.env_policy() {
            env.unset(name);
        } else {
            // TODO: Audit that the environment access only happens in single-threaded code.
            unsafe { std::env::remove_var(name) };
        }
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

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::net::Ipv4Addr;
    use std::path::PathBuf;

    use crate::async_host::AsyncHostError;

    use super::*;

    #[test]
    fn no_policy_leaves_fs_unrestricted() {
        let policy = AsyncPolicy::allow_all();

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
        let policy = AsyncPolicy::from_config(
            PolicyConfig {
                fs: None,
                net: Some(Default::default()),
                env: None,
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
        let policy = AsyncPolicy::from_config(
            PolicyConfig {
                fs: Some(Default::default()),
                net: None,
                env: None,
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
        let policy = AsyncPolicy::allow_all();

        policy
            .connect_socket(&ipv4_addr(Ipv4Addr::LOCALHOST, 443))
            .unwrap();
    }

    #[test]
    fn missing_net_section_denies_net_in_policy_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy = AsyncPolicy::from_config(
            PolicyConfig {
                fs: Some(config::FsConfig {
                    read: vec![PathBuf::from("allowed")],
                    write: vec![PathBuf::from("allowed")],
                }),
                net: None,
                env: None,
            },
            tmp.path(),
        )
        .unwrap();

        let error = policy
            .connect_socket(&ipv4_addr(Ipv4Addr::LOCALHOST, 443))
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn empty_net_section_denies_net() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = AsyncPolicy::from_config(
            PolicyConfig {
                fs: None,
                net: Some(Default::default()),
                env: None,
            },
            tmp.path(),
        )
        .unwrap();

        let error = policy
            .connect_socket(&ipv4_addr(Ipv4Addr::LOCALHOST, 443))
            .unwrap_err();
        assert_eq!(error, AsyncHostError::PermissionDenied);
    }

    #[test]
    fn missing_env_section_uses_empty_env_in_policy_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = AsyncPolicy::from_config(
            PolicyConfig {
                fs: None,
                net: None,
                env: None,
            },
            tmp.path(),
        )
        .unwrap();

        assert!(policy.env_vars().is_empty());
        assert!(!policy.env_var_exists("PATH"));
    }

    fn ipv4_addr(ip: Ipv4Addr, port: u16) -> Box<[u8]> {
        let mut addr = vec![0; crate::async_sys::socket::ipv4_addr_size() as usize];
        crate::async_sys::socket::init_ip_addr(&mut addr, u32::from(ip) as i32, i32::from(port))
            .unwrap();
        addr.into_boxed_slice()
    }
}
