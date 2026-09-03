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

//! Process-facing state selected for one Moonrun Runtime.

mod environment;
mod environment_provisioning;
mod stdio;
mod working_directory;

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Context;
use slotmap::Key;
use slotmap::{KeyData, SlotMap, new_key_type};

use crate::async_host::AsyncHost;
use crate::filesystem::HostFs;
use crate::network::HostNetwork;
use crate::policy::{self, Policy};
use crate::process::HostProcess;
use crate::sqlite::SqliteHost;

pub(crate) use environment::Env;
pub(crate) use environment_provisioning::EnvProvisioning;
pub(crate) use stdio::{Stdio, StdioStream};
pub use working_directory::WorkingDirectory;

new_key_type! {
    pub(crate) struct HostKey;
}

/// The one null Handle value shared by every Host resource kind.
pub(crate) fn null_handle() -> u64 {
    HostKey::null().data().as_ffi()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostResourceKind {
    Resource,
    Job,
    Poll,
    Worker,
    CBuffer,
    #[cfg(windows)]
    WindowsWatcherBuffer,
    #[cfg(unix)]
    ProcessArgv,
    ProcessEnv,
    ProcessEnvBuilder,
    AddrInfo,
    TlsConnection,
    #[cfg(windows)]
    IoResult,
    SqliteDatabase,
    SqliteDatabaseMutex,
    SqliteStatement,
}

/// The only identity mint for moonrun-owned opaque guest handles.
///
/// Domain state stores payloads in secondary maps keyed by `HostKey`. This
/// table records only liveness, generation, and kind, so adding another host
/// import family cannot accidentally create a colliding handle namespace.
#[derive(Default)]
pub(crate) struct HostKeys {
    kinds: SlotMap<HostKey, HostResourceKind>,
}

impl HostKeys {
    pub(crate) fn insert(&mut self, kind: HostResourceKind) -> HostKey {
        self.kinds.insert(kind)
    }

    pub(crate) fn key(&self, handle: u64, expected: HostResourceKind) -> Option<HostKey> {
        let key = HostKey::from(KeyData::from_ffi(handle));
        (self.kinds.get(key) == Some(&expected)).then_some(key)
    }

    pub(crate) fn kind(&self, key: HostKey) -> Option<HostResourceKind> {
        self.kinds.get(key).copied()
    }

    pub(crate) fn remove(&mut self, key: HostKey) -> Option<HostResourceKind> {
        self.kinds.remove(key)
    }
}

/// Backend-neutral composition root for one Moonrun virtual environment.
///
/// `RunOptions` selects the process-facing dependencies. `Runtime` realizes
/// them once, distributes domain policy together with shared Runtime State,
/// wires host domain state to one key namespace, and owns it until teardown.
pub(crate) struct Runtime {
    environment: Arc<Env>,
    working_directory: Arc<WorkingDirectory>,
    stdio: Arc<Stdio>,
    filesystem: Arc<HostFs>,
    async_host: AsyncHost,
    sqlite: SqliteHost,
}

impl Runtime {
    pub(crate) fn new(
        policy_file: Option<&Path>,
        policy_source_dir: Option<&Path>,
        inherited_policy: Option<&[u8]>,
        working_directory: WorkingDirectory,
    ) -> anyhow::Result<Self> {
        let (mut policy, env_provisioning) = match (inherited_policy, policy_file) {
            (Some(contents), _) => {
                let (policy, env) = policy::load_inherited_json(contents)
                    .context("failed to load inherited sandbox policy (experimental)")?;
                (policy, Some(env))
            }
            (None, Some(path)) => {
                let (policy, env) = policy::load_file_with_source_dir(path, policy_source_dir)
                    .context(
                    "failed to load sandbox policy (experimental); run `moonrun --help` for policy format notes",
                )?;
                (policy, Some(env))
            }
            (None, None) => (Policy::allow_all(), None),
        };
        let environment = Arc::new(
            env_provisioning
                .map_or_else(|| Ok(Env::ambient()), EnvProvisioning::realize)
                .context("failed to construct the Runtime environment")?,
        );
        let filesystem_policy = policy.take_filesystem_policy();
        let network_policy = policy.take_network_policy();
        let process_policy = policy.take_process_policy();
        let policy_inheritance = policy.take_policy_inheritance();
        let working_directory = Arc::new(working_directory);
        let stdio = Arc::new(Stdio::Ambient);
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let filesystem = Arc::new(HostFs::new(
            filesystem_policy,
            Arc::clone(&environment),
            Arc::clone(&working_directory),
        ));
        let network = HostNetwork::new(network_policy);
        let process = HostProcess::new(
            process_policy,
            policy_inheritance,
            Arc::clone(&working_directory),
            Arc::clone(&stdio),
        );
        let async_host = AsyncHost::new(
            Arc::clone(&environment),
            &stdio,
            Arc::clone(&filesystem),
            network,
            process,
            Rc::clone(&keys),
        );
        let sqlite = SqliteHost::new(Arc::clone(&filesystem), keys);
        Ok(Self {
            environment,
            working_directory,
            stdio,
            filesystem,
            async_host,
            sqlite,
        })
    }

    pub(crate) fn environment(&self) -> &Arc<Env> {
        &self.environment
    }

    pub(crate) fn filesystem(&self) -> &Arc<HostFs> {
        &self.filesystem
    }

    pub(crate) fn working_directory(&self) -> &WorkingDirectory {
        &self.working_directory
    }

    pub(crate) fn stdio(&self) -> &Arc<Stdio> {
        &self.stdio
    }

    pub(crate) fn async_host(&self) -> &AsyncHost {
        &self.async_host
    }

    pub(crate) fn null_handle(&self) -> u64 {
        null_handle()
    }

    pub(crate) fn sqlite(&self) -> &SqliteHost {
        &self.sqlite
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        // Keep the historical opt-in name for compatibility even though the
        // check now runs at the lifetime of the complete Runtime.
        if std::thread::panicking() || std::env::var_os("MOONBIT_ASYNC_CHECK_FD_LEAK").is_none() {
            return;
        }

        let mut leaks = Vec::new();
        if let Some(summary) = self.async_host.leak_summary() {
            leaks.push(format!("async({summary})"));
        }
        if let Some(summary) = self.sqlite.leak_summary() {
            leaks.push(format!("sqlite({summary})"));
        }
        if !leaks.is_empty() {
            panic!("moonrun Runtime leaked host state: {}", leaks.join(", "));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_typed_and_generational() {
        let mut keys = HostKeys::default();
        let job = keys.insert(HostResourceKind::Job);
        let poll = keys.insert(HostResourceKind::Poll);
        let job_handle = job.data().as_ffi();
        let poll_handle = poll.data().as_ffi();

        assert_eq!(
            HostKey::from(KeyData::from_ffi(null_handle())),
            HostKey::null()
        );
        assert_ne!(job_handle, null_handle());
        assert_eq!(keys.key(null_handle(), HostResourceKind::Job), None);
        assert_ne!(job_handle, poll_handle);
        assert_eq!(keys.key(job_handle, HostResourceKind::Job), Some(job));
        assert_eq!(keys.key(job_handle, HostResourceKind::Poll), None);

        assert_eq!(keys.remove(poll), Some(HostResourceKind::Poll));
        let replacement = keys.insert(HostResourceKind::Poll);
        assert_ne!(poll_handle, replacement.data().as_ffi());
        assert_eq!(keys.key(poll_handle, HostResourceKind::Poll), None);
    }
}
