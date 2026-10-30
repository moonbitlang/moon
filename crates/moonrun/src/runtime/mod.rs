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

//! Process-facing state selected for one Moonrun Runtime.

mod environment;
mod environment_provisioning;
mod executable;
mod handles;
mod stdio;
mod working_directory;

use std::cell::RefCell;
use std::ffi::OsString;
use std::path::PathBuf;
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
use crate::run_signal::SignalReceiver;
use crate::sqlite::SqliteHost;

pub(crate) use environment::Env;
pub(crate) use environment_provisioning::EnvProvisioning;
pub(crate) use executable::Executable;
pub(crate) use handles::Handles;
#[allow(
    unused_imports,
    reason = "shared interface may be unused by a partial backend"
)]
pub(crate) use stdio::Utf16Writer;
pub(crate) use stdio::{Stdio, StdioStream};
pub use working_directory::WorkingDirectory;

new_key_type! {
    pub(crate) struct HostKey;
}

/// Encoded null key, guaranteed by slotmap to be distinct from every live key.
/// This sentinel has no table entry. Guest code recognizes null results through
/// predicates; SQLite also exposes the sentinel value through an import.
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
/// The selected Module supplies immutable executable identity, while
/// `RunOptions` selects the other process-facing dependencies. `Runtime`
/// realizes them once, distributes domain policy together with shared Runtime
/// State, wires host domain state to one key namespace, and owns it until teardown.
pub(crate) struct Runtime {
    environment: Arc<Env>,
    executable: Executable,
    working_directory: Arc<WorkingDirectory>,
    stdio: Arc<Stdio>,
    filesystem: Arc<HostFs>,
    // Field order is significant: join workers and drop their SQLite payloads
    // before destroying the Database and Statement tables they refer to.
    async_host: AsyncHost,
    sqlite: SqliteHost,
}

/// Construction input shared by artifact preparation and its eventual Runtime.
/// Moving these inputs transfers ownership; only Runtime realizes the state
/// and loads the policy sources into domain authorization rules.
#[derive(Debug, Default)]
pub(crate) struct RuntimeConfig {
    pub(crate) policy_file: Option<PathBuf>,
    pub(crate) policy_source_dir: Option<PathBuf>,
    pub(crate) inherited_policy: Option<Vec<u8>>,
    pub(crate) working_directory: WorkingDirectory,
    pub(crate) stdio: Stdio,
    // Explicit policy provisioning takes precedence over the source environment.
    pub(crate) default_environment: Option<Vec<(OsString, OsString)>>,
    pub(crate) process_context: Option<crate::process_tree::ProcessContext>,
    #[cfg(unix)]
    pub(crate) preserved_child_signal_mask: Option<libc::sigset_t>,
}

impl RuntimeConfig {
    /// Capture missing host state before creating a virtual root.
    /// Guest children already carry state derived by their Host Process and
    /// must not use this host fallback. Policy is realized by Runtime later.
    pub(crate) fn capture_host_state(&mut self) -> std::io::Result<()> {
        self.default_environment
            .get_or_insert_with(|| std::env::vars_os().collect());
        if matches!(self.working_directory, WorkingDirectory::Ambient) {
            self.working_directory = WorkingDirectory::Fixed(std::env::current_dir()?);
        }
        Ok(())
    }
}

#[cfg(unix)]
pub(crate) fn current_signal_mask() -> anyhow::Result<libc::sigset_t> {
    let mut signal_mask = unsafe { std::mem::zeroed::<libc::sigset_t>() };
    let error =
        unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut signal_mask) };
    if error != 0 {
        return Err(std::io::Error::from_raw_os_error(error).into());
    }
    Ok(signal_mask)
}

impl Runtime {
    pub(crate) fn new(
        config: RuntimeConfig,
        executable: Executable,
        signals: SignalReceiver,
    ) -> anyhow::Result<Self> {
        #[cfg(unix)]
        let child_signal_mask = match config.preserved_child_signal_mask {
            Some(mask) => mask,
            None => current_signal_mask()?,
        };
        let source_environment = config
            .default_environment
            .map(Env::owned)
            .transpose()?
            .unwrap_or_else(Env::ambient);
        let (mut policy, env_provisioning) = match (
            config.inherited_policy.as_deref(),
            config.policy_file.as_deref(),
        ) {
            (Some(contents), _) => {
                let (policy, env) = policy::load_inherited_json(contents, &source_environment)
                    .context("failed to load inherited sandbox policy (experimental)")?;
                (policy, Some(env))
            }
            (None, Some(path)) => {
                let loaded = policy::load_file_with_source_dir(
                    path,
                    config.policy_source_dir.as_deref(),
                    &source_environment,
                );
                let (policy, env) = loaded.context(
                    "failed to load sandbox policy (experimental); run `moonrun --help` for policy format notes",
                )?;
                (policy, Some(env))
            }
            (None, None) => (Policy::allow_all(), None),
        };
        let environment = Arc::new(
            match env_provisioning {
                Some(provisioning) => provisioning.realize(),
                None => Ok(source_environment),
            }
            .context("failed to construct the Runtime environment")?,
        );
        let filesystem_policy = policy.take_filesystem_policy();
        let network_policy = policy.take_network_policy();
        let process_policy = policy.take_process_policy();
        let tty_allowed = policy.allows_tty();
        let policy_inheritance = policy.take_policy_inheritance();
        let working_directory = Arc::new(match config.working_directory {
            WorkingDirectory::Ambient => WorkingDirectory::Ambient,
            WorkingDirectory::Fixed(path) => {
                let path = dunce::canonicalize(path)
                    .context("failed to resolve fixed working directory")?;
                anyhow::ensure!(path.is_dir(), "fixed working directory must be a directory");
                WorkingDirectory::Fixed(path)
            }
        });
        let stdio = Arc::new(config.stdio);
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
            config.process_context,
            Arc::clone(&environment),
        );
        let async_host = AsyncHost::new(
            Arc::clone(&environment),
            &stdio,
            Arc::clone(&filesystem),
            network,
            process,
            tty_allowed,
            Rc::clone(&keys),
            signals,
            #[cfg(unix)]
            child_signal_mask,
        );
        let sqlite = SqliteHost::new(Arc::clone(&filesystem), keys);
        Ok(Self {
            environment,
            executable,
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

    pub(crate) fn executable(&self) -> &Executable {
        &self.executable
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
        // Async Host is dropped first and joins workers that can use SQLite.
        // Release guest-held recursive mutex entries before any worker join.
        self.sqlite.release_entered_mutexes();
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
    fn host_capture_preserves_explicit_empty_environment_and_fixed_directory() {
        let directory = PathBuf::from("unresolved-directory");
        let policy = PathBuf::from("unloaded-policy.json");
        let mut config = RuntimeConfig {
            default_environment: Some(Vec::new()),
            working_directory: WorkingDirectory::Fixed(directory.clone()),
            policy_file: Some(policy.clone()),
            ..Default::default()
        };

        config.capture_host_state().unwrap();

        assert_eq!(config.default_environment, Some(Vec::new()));
        assert_eq!(config.working_directory, WorkingDirectory::Fixed(directory));
        assert_eq!(config.policy_file, Some(policy));
    }

    #[test]
    fn host_capture_resolves_ambient_directory_without_replacing_environment() {
        let mut config = RuntimeConfig {
            default_environment: Some(vec![("RUN_ONLY".into(), "value".into())]),
            ..Default::default()
        };

        config.capture_host_state().unwrap();

        assert_eq!(
            config.working_directory,
            WorkingDirectory::Fixed(std::env::current_dir().unwrap())
        );
        assert_eq!(
            config.default_environment,
            Some(vec![("RUN_ONLY".into(), "value".into())])
        );
    }

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

    #[test]
    fn runtime_owns_the_selected_executable() {
        let path = std::env::current_exe().unwrap();
        let runtime = Runtime::new(
            crate::runtime::RuntimeConfig::default(),
            Executable::from_file(&path),
            crate::signal_channel().1,
        )
        .unwrap();

        assert_eq!(runtime.executable().path(), Ok(path.as_path()));
    }
}
