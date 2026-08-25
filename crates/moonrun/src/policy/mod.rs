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
mod snapshot;

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::sync::Mutex;

use crate::async_host::{AsyncHostError, AsyncHostResult};

use self::config::PolicyConfig;
use self::env::EnvPolicy;
pub(crate) use self::fs::RuntimePathBase;
use self::fs::{FsIntents, FsPolicy};
use self::net::{NetOperation, NetPolicy};
use self::process::ProcessPolicy;
pub(crate) use self::snapshot::PolicySnapshot;
use self::snapshot::SnapshotTemplate;

#[derive(Debug)]
pub(crate) struct Policy {
    fs: Option<FsPolicy>,
    net: Option<NetPolicy>,
    env: Option<EnvPolicy>,
    process: Option<ProcessPolicy>,
    snapshot_template: Option<SnapshotTemplate>,
    snapshot_leases: Mutex<Vec<PolicySnapshot>>,
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
            snapshot_template: None,
            snapshot_leases: Mutex::default(),
        }
    }

    pub(crate) fn from_file(path: &Path) -> anyhow::Result<Self> {
        let config = PolicyConfig::from_file(path)?;
        let config_dir = path.parent().unwrap_or_else(|| Path::new("."));
        Self::from_config(config, config_dir)
    }

    fn from_config(config: PolicyConfig, config_dir: &Path) -> anyhow::Result<Self> {
        let fs_config = config.fs.unwrap_or_default();
        let net_config = config.net.unwrap_or_default();
        let env_config = config.env.unwrap_or_default();
        let process_config = config.process.unwrap_or_default();
        let fs = FsPolicy::from_config(fs_config, config_dir)?;
        let snapshot_template = SnapshotTemplate::new(
            fs.config_for_snapshot(),
            net_config.clone(),
            process_config.clone(),
        );
        Ok(Self {
            fs: Some(fs),
            net: Some(NetPolicy::from_config(net_config)?),
            env: Some(EnvPolicy::from_config(env_config)?),
            process: Some(ProcessPolicy::from_config(process_config)?),
            snapshot_template: Some(snapshot_template),
            snapshot_leases: Mutex::default(),
        })
    }

    pub(crate) fn from_inherited_snapshot(token: OsString) -> anyhow::Result<Self> {
        Self::from_config(PolicySnapshot::consume(token)?, Path::new("."))
    }

    pub(crate) fn snapshot_for_child(&self) -> anyhow::Result<Option<snapshot::PolicySnapshot>> {
        let Some(template) = self.snapshot_template.as_ref() else {
            return Ok(None);
        };
        let net = self
            .net_policy()
            .expect("policy-bearing runs always have a network policy");
        let env = self
            .env_policy()
            .expect("policy-bearing runs always have an environment policy");
        let fs = self
            .fs_policy()
            .expect("policy-bearing runs always have a filesystem policy");
        let snapshot = template.write(fs, net, env)?;
        let mut leases = self.snapshot_leases.lock().unwrap();
        leases.retain(|snapshot| !snapshot.is_consumed());
        leases.push(snapshot.clone());
        Ok(Some(snapshot))
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

        assert!(policy.env_vars().is_empty());
        assert!(!policy.env_var_exists("PATH"));
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

    #[test]
    fn snapshot_captures_effective_policy_without_reusing_the_source_file() {
        let tmp = tempfile::tempdir().unwrap();
        let allowed = tmp.path().join("allowed");
        std::fs::create_dir(&allowed).unwrap();
        let policy_file = tmp.path().join("policy.toml");
        std::fs::write(
            &policy_file,
            r#"
[fs]
read = ["allowed"]
write = ["allowed"]

[env.set]
INITIAL = "value"
MOONRUN_INHERITED_POLICY = "guest value"

[process]
spawn = true
"#,
        )
        .unwrap();
        let policy = Policy::from_file(&policy_file).unwrap();
        policy.unset_env_var("INITIAL");
        policy.set_env_var("CURRENT".to_owned(), "策略-🌙".to_owned());

        let snapshot = policy.snapshot_for_child().unwrap().unwrap();
        std::fs::write(&policy_file, "").unwrap();
        let snapshot_path = PathBuf::from(snapshot.transport_token());
        let inherited =
            Policy::from_inherited_snapshot(snapshot.transport_token().to_owned()).unwrap();
        assert!(!snapshot_path.exists());
        snapshot.handoff();

        assert_eq!(inherited.get_env_var("CURRENT").as_deref(), Some("策略-🌙"));
        assert!(!inherited.env_var_exists("INITIAL"));
        assert!(!inherited.env_var_exists(moonutil::constants::MOONRUN_INHERITED_POLICY));
        inherited
            .write_path(
                RuntimePathBase::CurrentDirectory,
                allowed.join("child.txt").as_os_str(),
            )
            .unwrap();
        #[cfg(unix)]
        inherited
            .spawn_process_unix(OsStr::new("program"), &[OsString::from("program")])
            .unwrap();
        #[cfg(windows)]
        inherited
            .spawn_process_windows(OsStr::new("program.exe"))
            .unwrap();
    }

    #[test]
    fn snapshot_transport_is_read_only_and_protected() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = Policy::from_config(
            PolicyConfig {
                fs: Some(config::FsConfig {
                    read: vec![PathBuf::from("*")],
                    write: vec![PathBuf::from("*")],
                }),
                ..PolicyConfig::default()
            },
            tmp.path(),
        )
        .unwrap();
        let snapshot = policy.snapshot_for_child().unwrap().unwrap();
        let path = PathBuf::from(snapshot.transport_token());
        assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err());
        assert_eq!(
            policy.read_path(RuntimePathBase::CurrentDirectory, path.as_os_str()),
            Err(AsyncHostError::PermissionDenied)
        );
        assert_eq!(
            policy.write_path(RuntimePathBase::CurrentDirectory, path.as_os_str()),
            Err(AsyncHostError::PermissionDenied)
        );
        assert_eq!(
            policy.remove_path(path.parent().unwrap().as_os_str()),
            Err(AsyncHostError::PermissionDenied)
        );
    }

    #[test]
    fn unrestricted_runs_do_not_create_policy_snapshots() {
        assert!(Policy::allow_all().snapshot_for_child().unwrap().is_none());
    }

    #[test]
    fn originating_policy_keeps_cleanup_lease_until_run_teardown() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = Policy::from_config(PolicyConfig::default(), tmp.path()).unwrap();
        let snapshot = policy.snapshot_for_child().unwrap().unwrap();
        let path = PathBuf::from(snapshot.transport_token());

        drop(snapshot);
        assert!(path.exists());
        drop(policy);
        assert!(!path.exists());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn snapshot_preserves_non_utf8_filesystem_roots() {
        use std::os::unix::ffi::OsStringExt;

        let tmp = tempfile::tempdir().unwrap();
        let root = tmp
            .path()
            .join(std::ffi::OsString::from_vec(b"allowed-\xff".to_vec()));
        std::fs::create_dir(&root).unwrap();
        let policy = Policy::from_config(
            PolicyConfig {
                fs: Some(config::FsConfig {
                    read: vec![root.clone()],
                    write: vec![root.clone()],
                }),
                ..PolicyConfig::default()
            },
            tmp.path(),
        )
        .unwrap();

        let snapshot = policy.snapshot_for_child().unwrap().unwrap();
        let inherited =
            Policy::from_inherited_snapshot(snapshot.transport_token().to_owned()).unwrap();
        snapshot.handoff();

        inherited
            .write_path(
                RuntimePathBase::CurrentDirectory,
                root.join("child.txt").as_os_str(),
            )
            .unwrap();
    }

    #[test]
    fn snapshot_preserves_resolved_network_authority() {
        let tmp = tempfile::tempdir().unwrap();
        let policy = Policy::from_config(
            PolicyConfig {
                net: Some(config::NetConfig {
                    dns: Vec::new(),
                    connect: vec!["api.example.com:443".to_owned()],
                    bind: Vec::new(),
                }),
                ..PolicyConfig::default()
            },
            tmp.path(),
        )
        .unwrap();
        let resolved = ipv4_addr(Ipv4Addr::LOCALHOST, 0);
        policy
            .register_dns_result(OsStr::new("api.example.com"), &[resolved])
            .unwrap();

        let snapshot = policy.snapshot_for_child().unwrap().unwrap();
        let inherited =
            Policy::from_inherited_snapshot(snapshot.transport_token().to_owned()).unwrap();
        snapshot.handoff();

        inherited
            .check_connect(&ipv4_addr(Ipv4Addr::LOCALHOST, 443))
            .unwrap();
    }

    fn ipv4_addr(ip: Ipv4Addr, port: u16) -> Box<[u8]> {
        let mut addr = vec![0; crate::async_sys::socket::ipv4_addr_size() as usize];
        crate::async_sys::socket::init_ip_addr(&mut addr, u32::from(ip), u32::from(port)).unwrap();
        addr.into_boxed_slice()
    }
}
