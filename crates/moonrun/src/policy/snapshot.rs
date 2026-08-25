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

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use super::config::{EnvConfig, FsConfig, NetConfig, PolicyConfig, ProcessConfig};
use super::env::EnvPolicy;
use super::fs::FsPolicy;
use super::net::NetPolicy;

mod transport;

const SNAPSHOT_VERSION: u32 = 1;

#[derive(Debug)]
pub(super) struct SnapshotTemplate {
    fs: FsConfig,
    net: NetConfig,
    process: ProcessConfig,
}

impl SnapshotTemplate {
    pub(super) fn new(fs: FsConfig, net: NetConfig, process: ProcessConfig) -> Self {
        Self { fs, net, process }
    }

    pub(super) fn write(
        &self,
        fs_policy: &FsPolicy,
        net_policy: &NetPolicy,
        env_policy: &EnvPolicy,
    ) -> anyhow::Result<PolicySnapshot> {
        let mut net = self.net.clone();
        net.connect
            .extend(net_policy.resolved_connect_for_snapshot());
        let mut env = env_policy
            .vars()
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        env.retain(|name, _| {
            if cfg!(windows) {
                !name.eq_ignore_ascii_case(moonutil::constants::MOONRUN_INHERITED_POLICY)
            } else {
                name != moonutil::constants::MOONRUN_INHERITED_POLICY
            }
        });
        let document = SnapshotDocument {
            version: SNAPSHOT_VERSION,
            fs: SnapshotFsConfig::from_config(&self.fs),
            net,
            env: EnvConfig {
                set: env,
                ..EnvConfig::default()
            },
            process: self.process.clone(),
        };
        let contents =
            serde_json::to_vec(&document).context("failed to serialize policy snapshot")?;
        Ok(PolicySnapshot {
            transport: Arc::new(transport::SnapshotTransport::publish(&contents, fs_policy)?),
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PolicySnapshot {
    transport: Arc<transport::SnapshotTransport>,
}

impl PolicySnapshot {
    /// Return the opaque token that the spawn boundary must authenticate.
    pub(crate) fn transport_token(&self) -> &OsStr {
        self.transport.token()
    }

    /// Release the spawn Job's lease after a successful handoff.
    ///
    /// The originating Policy keeps a cleanup lease until the child consumes
    /// the token or the parent Run ends.
    pub(crate) fn handoff(self) {}

    pub(super) fn is_consumed(&self) -> bool {
        self.transport.is_consumed()
    }

    pub(super) fn consume(token: OsString) -> anyhow::Result<PolicyConfig> {
        let contents = transport::SnapshotTransport::consume(token)?;
        read(&contents)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SnapshotDocument {
    version: u32,
    fs: SnapshotFsConfig,
    net: NetConfig,
    env: EnvConfig,
    process: ProcessConfig,
}

impl SnapshotDocument {
    fn into_config(self) -> anyhow::Result<PolicyConfig> {
        anyhow::ensure!(
            self.version == SNAPSHOT_VERSION,
            "unsupported policy snapshot version {}",
            self.version
        );
        Ok(PolicyConfig {
            fs: Some(self.fs.into_config()),
            net: Some(self.net),
            env: Some(self.env),
            process: Some(self.process),
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SnapshotFsConfig {
    read: Vec<SnapshotPath>,
    write: Vec<SnapshotPath>,
}

impl SnapshotFsConfig {
    fn from_config(config: &FsConfig) -> Self {
        Self {
            read: config
                .read
                .iter()
                .map(|path| SnapshotPath::from_path(path.as_path()))
                .collect(),
            write: config
                .write
                .iter()
                .map(|path| SnapshotPath::from_path(path.as_path()))
                .collect(),
        }
    }

    fn into_config(self) -> FsConfig {
        FsConfig {
            read: self.read.into_iter().map(SnapshotPath::into_path).collect(),
            write: self
                .write
                .into_iter()
                .map(SnapshotPath::into_path)
                .collect(),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
enum SnapshotPath {
    #[cfg(unix)]
    Unix(Vec<u8>),
    #[cfg(windows)]
    Windows(Vec<u16>),
}

impl SnapshotPath {
    fn from_path(path: &Path) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;

            Self::Unix(path.as_os_str().as_bytes().to_vec())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;

            Self::Windows(path.as_os_str().encode_wide().collect())
        }
    }

    fn into_path(self) -> PathBuf {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;

            let Self::Unix(path) = self;
            PathBuf::from(std::ffi::OsString::from_vec(path))
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;

            let Self::Windows(path) = self;
            PathBuf::from(std::ffi::OsString::from_wide(&path))
        }
    }
}

pub(super) fn read(contents: &[u8]) -> anyhow::Result<PolicyConfig> {
    let document = serde_json::from_slice::<SnapshotDocument>(contents)
        .context("failed to parse policy snapshot")?;
    document.into_config()
}
