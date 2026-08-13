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

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use moonutil::{
    MoonHomeLayout,
    git::{GitCommandError, Stdios},
    locks::FileLock,
    registry::RegistryConfig,
    user_log::UserLog,
};
use reqwest::{
    StatusCode,
    header::{ETAG, HeaderValue, IF_NONE_MATCH, USER_AGENT},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::zip_util::extract_zip_to_dir;

const SYMBOLS_URL: &str = "https://download.mooncakes.io/symbols.zip";

#[derive(Debug)]
struct CommandOutput {
    stdout: String,
    stderr: String,
}

impl CommandOutput {
    fn from_output(output: &std::process::Output) -> Self {
        Self {
            stdout: String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string(),
            stderr: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string(),
        }
    }
}

impl std::fmt::Display for CommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.stdout.is_empty() {
            write!(f, "\ngit stdout:\n{}", self.stdout)?;
        }
        if !self.stderr.is_empty() {
            write!(f, "\ngit stderr:\n{}", self.stderr)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegistryIndexRecloneReason {
    PullFailed,
    RemoteMismatch,
    NotGitRepository,
    MissingOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegistryIndexUpdate {
    Cloned,
    Updated,
    Recloned(RegistryIndexRecloneReason),
    ConcurrentUpdateReused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UpdateOutcome {
    pub registry_index: RegistryIndexUpdate,
    /// Whether the best-effort symbols update succeeded.
    ///
    /// A failure is reported through `UserLog` and does not make the registry
    /// index update fail.
    pub symbols_updated: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("failed to clone registry index")]
struct CloneRegistryIndexError {
    #[source]
    source: CloneRegistryIndexErrorKind,
}

#[derive(Debug, thiserror::Error)]
enum CloneRegistryIndexErrorKind {
    #[error(transparent)]
    GitCommandError(#[from] GitCommandError),

    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error("non-zero exit code: {status}{output}")]
    NonZeroExitCode {
        status: std::process::ExitStatus,
        output: CommandOutput,
    },
}

fn clone_registry_index(
    registry_config: &RegistryConfig,
    target_dir: &Path,
) -> Result<(), CloneRegistryIndexError> {
    // Ensure parent directory exists (e.g. `$MOON_HOME/registry`).
    // `git clone <url> <target_dir>` does not create intermediate directories.
    let Some(parent) = target_dir.parent() else {
        return Err(CloneRegistryIndexError {
            source: CloneRegistryIndexErrorKind::IO(std::io::Error::other(
                "registry index directory has no parent",
            )),
        });
    };
    std::fs::create_dir_all(parent).map_err(|e| CloneRegistryIndexError {
        source: CloneRegistryIndexErrorKind::IO(e),
    })?;

    // Registry index servers must support smart Git transports and shallow
    // clones. Do not fall back to a full-history clone for dumb/static HTTP.
    let child = moonutil::git::git_command(
        &[
            "clone",
            "--depth",
            "1",
            "--single-branch",
            "--no-tags",
            &registry_config.index,
            target_dir.to_str().unwrap(),
        ],
        Stdios::npp(),
    )
    .map_err(|e| CloneRegistryIndexError {
        source: CloneRegistryIndexErrorKind::GitCommandError(e),
    })?;

    let output = child
        .wait_with_output()
        .map_err(|e| CloneRegistryIndexError {
            source: CloneRegistryIndexErrorKind::IO(e),
        })?;
    if !output.status.success() {
        return Err(CloneRegistryIndexError {
            source: CloneRegistryIndexErrorKind::NonZeroExitCode {
                status: output.status,
                output: CommandOutput::from_output(&output),
            },
        });
    }
    Ok(())
}

/// Create a unique sibling directory name under `parent`.
///
/// NOTE: We intentionally avoid using `tempfile` here to keep dependencies minimal.
fn unique_sibling_dir(parent: &Path, prefix: &str) -> std::io::Result<PathBuf> {
    // SAFETY/ROBUSTNESS:
    // - Use pid + timestamp to minimize collision risk.
    // - Retry a few times if a collision happens (e.g. parallel processes).
    let pid = std::process::id();
    for _ in 0..10 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let p = parent.join(format!("{prefix}.{pid}.{nanos}"));
        if !p.exists() {
            return Ok(p);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "failed to create a unique temp directory name",
    ))
}

/// Re-clone the registry index without risking data loss.
///
/// The old index directory is kept until the new clone succeeds, then swapped in.
fn safe_reclone_registry_index(
    registry_config: &RegistryConfig,
    target_dir: &Path,
    user_log: &UserLog,
) -> Result<(), UpdateError> {
    // Determine parent directory so we can `rename` within the same filesystem.
    let Some(parent) = target_dir.parent() else {
        return Err(UpdateError {
            source: UpdateErrorKind::IO(std::io::Error::other(
                "registry index directory has no parent",
            )),
        });
    };

    // Clone into a fresh sibling directory first.
    let tmp_dir = unique_sibling_dir(parent, ".registry-index.tmp").map_err(|e| UpdateError {
        source: UpdateErrorKind::IO(e),
    })?;
    let clone_res = clone_registry_index(registry_config, &tmp_dir).map_err(|e| UpdateError {
        source: UpdateErrorKind::CloneRegistryIndexError(e),
    });
    if let Err(e) = clone_res {
        // Best effort cleanup; ignore errors.
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(e);
    }

    // Swap: move old -> backup, move tmp -> target, then delete backup.
    let backup_dir =
        unique_sibling_dir(parent, ".registry-index.old").map_err(|e| UpdateError {
            source: UpdateErrorKind::IO(e),
        })?;
    std::fs::rename(target_dir, &backup_dir).map_err(|e| UpdateError {
        source: UpdateErrorKind::IO(e),
    })?;

    if let Err(e) = std::fs::rename(&tmp_dir, target_dir) {
        // Best effort rollback: restore original index.
        let _ = std::fs::rename(&backup_dir, target_dir);
        // Best effort cleanup.
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(UpdateError {
            source: UpdateErrorKind::IO(e),
        });
    }

    if let Err(e) = std::fs::remove_dir_all(&backup_dir) {
        user_log.warn(format!(
            "failed to remove old registry index at `{}`: {e}",
            backup_dir.display()
        ));
    }

    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("failed to pull latest registry index")]
struct PullLatestRegistryIndexError {
    #[source]
    source: PullLatestRegistryIndexErrorKind,
}

#[derive(Debug, thiserror::Error)]
enum PullLatestRegistryIndexErrorKind {
    #[error(transparent)]
    GitCommandError(GitCommandError),

    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error("non-zero exit code: {0}")]
    NonZeroExitCode(std::process::ExitStatus),
}

fn pull_latest_registry_index(target_dir: &Path) -> Result<(), PullLatestRegistryIndexError> {
    let mut child = moonutil::git::git_command(
        &["-C", target_dir.to_str().unwrap(), "pull", "origin", "main"],
        Stdios::npp(),
    )
    .map_err(|e| PullLatestRegistryIndexError {
        source: PullLatestRegistryIndexErrorKind::GitCommandError(e),
    })?;
    let status = child.wait().map_err(|e| PullLatestRegistryIndexError {
        source: PullLatestRegistryIndexErrorKind::IO(e),
    })?;
    if !status.success() {
        return Err(PullLatestRegistryIndexError {
            source: PullLatestRegistryIndexErrorKind::NonZeroExitCode(status),
        });
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
#[error("update failed")]
struct UpdateError {
    #[source]
    source: UpdateErrorKind,
}

#[derive(Debug, thiserror::Error)]
enum UpdateErrorKind {
    #[error(transparent)]
    CloneRegistryIndexError(#[from] CloneRegistryIndexError),

    #[error(transparent)]
    PullLatestRegistryIndexError(#[from] PullLatestRegistryIndexError),

    #[error(transparent)]
    InspectRegistryIndexError(#[from] InspectRegistryIndexError),

    #[error(transparent)]
    IO(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
#[error("failed to inspect registry index")]
struct InspectRegistryIndexError {
    #[source]
    source: InspectRegistryIndexErrorKind,
}

#[derive(Debug, thiserror::Error)]
enum InspectRegistryIndexErrorKind {
    #[error(transparent)]
    GitCommandError(#[from] GitCommandError),

    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error("`git {command}` exited with {status}{output}")]
    NonZeroExitCode {
        command: String,
        status: std::process::ExitStatus,
        output: CommandOutput,
    },
}

#[derive(Debug, PartialEq, Eq)]
enum RegistryIndexState {
    NotGitRepository,
    MissingOrigin,
    Ready { remote_url: String },
}

fn run_git_query(
    target_dir: &Path,
    args: &[&str],
) -> Result<CommandOutput, InspectRegistryIndexError> {
    let mut command = vec!["-C", target_dir.to_str().unwrap()];
    command.extend_from_slice(args);
    let output = moonutil::git::git_command(&command, Stdios::npp())
        .map_err(|e| InspectRegistryIndexError {
            source: InspectRegistryIndexErrorKind::GitCommandError(e),
        })?
        .wait_with_output()
        .map_err(|e| InspectRegistryIndexError {
            source: InspectRegistryIndexErrorKind::IO(e),
        })?;
    if !output.status.success() {
        return Err(InspectRegistryIndexError {
            source: InspectRegistryIndexErrorKind::NonZeroExitCode {
                command: command.join(" "),
                status: output.status,
                output: CommandOutput::from_output(&output),
            },
        });
    }
    Ok(CommandOutput::from_output(&output))
}

fn inspect_registry_index(
    target_dir: &Path,
) -> Result<RegistryIndexState, InspectRegistryIndexError> {
    if !target_dir
        .join(".git")
        .try_exists()
        .map_err(|e| InspectRegistryIndexError {
            source: InspectRegistryIndexErrorKind::IO(e),
        })?
    {
        return Ok(RegistryIndexState::NotGitRepository);
    }

    let inside_work_tree = run_git_query(target_dir, &["rev-parse", "--is-inside-work-tree"])?;
    if inside_work_tree.stdout != "true" {
        return Ok(RegistryIndexState::NotGitRepository);
    }

    let remotes = run_git_query(target_dir, &["remote"])?;
    if !remotes.stdout.lines().any(|remote| remote == "origin") {
        return Ok(RegistryIndexState::MissingOrigin);
    }

    // Git fetch uses the first configured URL. Read all effective raw values
    // to preserve that ordering without applying `url.*.insteadOf` rewrites.
    let remote_urls = match run_git_query(target_dir, &["config", "--get-all", "remote.origin.url"])
    {
        Ok(output) => output,
        Err(error)
            if matches!(
                &error.source,
                InspectRegistryIndexErrorKind::NonZeroExitCode { status, .. }
                    if status.code() == Some(1)
            ) =>
        {
            return Ok(RegistryIndexState::MissingOrigin);
        }
        Err(error) => return Err(error),
    };
    let Some(remote_url) = remote_urls.stdout.lines().next() else {
        return Ok(RegistryIndexState::MissingOrigin);
    };
    Ok(RegistryIndexState::Ready {
        remote_url: remote_url.to_owned(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SymbolsEtag {
    url: String,
    value: String,
}

#[derive(Debug)]
enum SymbolsDownload {
    Modified {
        data: bytes::Bytes,
        etag: Option<SymbolsEtag>,
    },
    NotModified {
        etag: SymbolsEtag,
    },
}

#[tracing::instrument(
    level = "debug",
    skip_all,
    fields(has_previous_etag = previous_etag.is_some())
)]
fn download_symbols_zip(
    symbols_url: &str,
    previous_etag: Option<&SymbolsEtag>,
) -> anyhow::Result<SymbolsDownload> {
    let client = reqwest::blocking::Client::new();
    let conditional_etag = previous_etag.and_then(|etag| {
        (etag.url == symbols_url)
            .then(|| {
                HeaderValue::from_bytes(etag.value.as_bytes())
                    .ok()
                    .map(|value| (etag, value))
            })
            .flatten()
    });
    if previous_etag.is_some() && conditional_etag.is_none() {
        tracing::debug!("Ignoring invalid or mismatched symbols.zip ETag");
    }

    let mut request = client.get(symbols_url).header(
        USER_AGENT,
        format!("mooncake/{}", env!("CARGO_PKG_VERSION")),
    );
    if let Some((_, value)) = &conditional_etag {
        request = request.header(IF_NONE_MATCH, value.clone());
    }
    let response = request.send().context("failed to fetch symbols.zip")?;
    let response_etag = response
        .headers()
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(|value| SymbolsEtag {
            url: symbols_url.to_owned(),
            value: value.to_owned(),
        });
    tracing::debug!(
        status = %response.status(),
        has_etag = response_etag.is_some(),
        "Received symbols.zip response"
    );
    if response.status() == StatusCode::NOT_MODIFIED {
        let etag = response_etag
            .or_else(|| conditional_etag.map(|(etag, _)| etag.clone()))
            .context("symbols.zip returned 304 without a usable ETag")?;
        return Ok(SymbolsDownload::NotModified { etag });
    }

    let data = response
        .error_for_status()
        .context("symbols.zip download returned error status")?
        .bytes()
        .context("failed to read symbols.zip response body")?;
    Ok(SymbolsDownload::Modified {
        data,
        etag: response_etag,
    })
}

fn update_symbols_from_url(
    registry_dir: &Path,
    target_dir: &Path,
    symbols_url: &str,
    previous_etag: Option<&SymbolsEtag>,
) -> anyhow::Result<Option<SymbolsEtag>> {
    std::fs::create_dir_all(registry_dir)
        .with_context(|| format!("failed to create `{}`", registry_dir.display()))?;

    let previous_etag = target_dir.is_dir().then_some(previous_etag).flatten();
    let download = download_symbols_zip(symbols_url, previous_etag)?;
    let (data, etag) = match download {
        SymbolsDownload::Modified { data, etag } => (data, etag),
        SymbolsDownload::NotModified { etag } if target_dir.is_dir() => return Ok(Some(etag)),
        SymbolsDownload::NotModified { .. } => match download_symbols_zip(symbols_url, None)? {
            SymbolsDownload::Modified { data, etag } => (data, etag),
            SymbolsDownload::NotModified { .. } => {
                anyhow::bail!("symbols.zip returned 304 but local symbols are missing")
            }
        },
    };

    let tmp_dir = unique_sibling_dir(registry_dir, ".symbols.tmp")
        .context("failed to create temp directory for symbols")?;
    std::fs::create_dir_all(&tmp_dir)?;

    if let Err(e) = extract_zip_to_dir(&tmp_dir, std::io::Cursor::new(data)) {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(e);
    }

    if target_dir.exists() {
        let backup_dir = unique_sibling_dir(registry_dir, ".symbols.old")
            .context("failed to create backup dir")?;
        std::fs::rename(target_dir, &backup_dir)
            .context("failed to move existing symbols dir to backup")?;

        if let Err(e) = std::fs::rename(&tmp_dir, target_dir) {
            let _ = std::fs::rename(&backup_dir, target_dir);
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(anyhow::Error::from(e).context("failed to replace symbols directory"));
        }

        std::fs::remove_dir_all(&backup_dir).with_context(|| {
            format!(
                "failed to remove old symbols directory at `{}`",
                backup_dir.display()
            )
        })?;
    } else {
        std::fs::rename(&tmp_dir, target_dir)
            .context("failed to move symbols directory into place")?;
    }

    Ok(etag)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RegistryUpdateState {
    registry_identity: String,
    generation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    symbols_etag: Option<SymbolsEtag>,
}

#[derive(Debug, Clone)]
enum RegistryUpdateObservation {
    Observed(Option<RegistryUpdateState>),
    Unavailable,
}

fn observe_registry_update_state(path: &Path) -> RegistryUpdateObservation {
    match std::fs::read(path) {
        Ok(data) => match serde_json::from_slice(&data) {
            Ok(state) => RegistryUpdateObservation::Observed(Some(state)),
            Err(error) => {
                tracing::debug!(
                    path = %path.display(),
                    error = %error,
                    "Ignoring invalid registry update state"
                );
                RegistryUpdateObservation::Unavailable
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            RegistryUpdateObservation::Observed(None)
        }
        Err(error) => {
            tracing::debug!(
                path = %path.display(),
                error = %error,
                "Ignoring unreadable registry update state"
            );
            RegistryUpdateObservation::Unavailable
        }
    }
}

fn write_registry_update_state(
    registry_dir: &Path,
    state_path: &Path,
    registry_identity: &str,
    symbols_etag: Option<SymbolsEtag>,
) -> anyhow::Result<()> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let state = RegistryUpdateState {
        registry_identity: registry_identity.to_owned(),
        generation: format!("{}-{nanos}", std::process::id()),
        symbols_etag,
    };
    let mut staged = tempfile::NamedTempFile::new_in(registry_dir)?;
    serde_json::to_writer(staged.as_file_mut(), &state)?;
    staged.persist(state_path).map_err(|error| error.error)?;
    Ok(())
}

#[tracing::instrument(level = "debug", skip_all)]
fn run_registry_update_locked(
    home: &MoonHomeLayout,
    registry_identity: &str,
    observed: RegistryUpdateObservation,
    user_log: &UserLog,
    perform_update: impl FnOnce(
        Option<&SymbolsEtag>,
    ) -> anyhow::Result<(UpdateOutcome, Option<SymbolsEtag>)>,
) -> anyhow::Result<UpdateOutcome> {
    let registry_dir = home.registry_dir();
    let state_path = home.registry_update_state_path();
    let _lock = {
        let _span = tracing::debug_span!("acquire_registry_update_lock").entered();
        FileLock::lock_file_with_user_log(&home.registry_update_lock_path(), user_log)
            .context("failed to lock registry update directory")?
    };

    // The observation was captured before waiting for the lock. A changed
    // generation therefore represents work completed by a concurrent caller,
    // not a freshness window for a later update.
    let current = observe_registry_update_state(&state_path);
    let reuse = match (&observed, &current) {
        (
            RegistryUpdateObservation::Observed(observed),
            RegistryUpdateObservation::Observed(Some(current)),
        ) => observed.as_ref() != Some(current) && current.registry_identity == registry_identity,
        _ => false,
    };
    if reuse {
        tracing::debug!("Reusing registry update completed by another process");
        return Ok(UpdateOutcome {
            registry_index: RegistryIndexUpdate::ConcurrentUpdateReused,
            symbols_updated: true,
        });
    }

    let previous_etag = match &current {
        RegistryUpdateObservation::Observed(Some(state)) => state.symbols_etag.as_ref(),
        _ => None,
    };
    tracing::debug!("Performing registry update");
    let (outcome, symbols_etag) = perform_update(previous_etag)?;
    if outcome.symbols_updated
        && let Err(error) =
            write_registry_update_state(&registry_dir, &state_path, registry_identity, symbols_etag)
    {
        user_log.warn(format!(
            "failed to record completed registry update: {error:#}"
        ));
    }
    tracing::debug!(
        symbols_updated = outcome.symbols_updated,
        "Registry update completed"
    );
    Ok(outcome)
}

pub(crate) fn sync(
    home: &MoonHomeLayout,
    registry_config: &RegistryConfig,
    user_log: &UserLog,
) -> anyhow::Result<UpdateOutcome> {
    let registry_dir = home.registry_dir();
    let target_dir = home.registry_index_dir();
    let symbols_dir = home.registry_symbols_dir();
    std::fs::create_dir_all(&registry_dir)
        .with_context(|| format!("failed to create `{}`", registry_dir.display()))?;
    let observed = observe_registry_update_state(&home.registry_update_state_path());
    let symbols_url = registry_config.symbols.as_deref().unwrap_or(SYMBOLS_URL);
    let registry_identity = registry_identity(&registry_config.index, symbols_url);

    run_registry_update_locked(
        home,
        &registry_identity,
        observed,
        user_log,
        |previous_etag| {
            let registry_index = update_registry_index(&target_dir, registry_config, user_log)?;
            let (symbols_updated, symbols_etag) = match update_symbols_from_url(
                &registry_dir,
                &symbols_dir,
                symbols_url,
                previous_etag,
            ) {
                Ok(etag) => (true, etag),
                Err(e) => {
                    user_log.warn(format!("failed to update symbols: {e:#}"));
                    (false, None)
                }
            };

            Ok((
                UpdateOutcome {
                    registry_index,
                    symbols_updated,
                },
                symbols_etag,
            ))
        },
    )
}

fn registry_identity(index_url: &str, symbols_url: &str) -> String {
    let mut identity = Sha256::new();
    identity.update(index_url.as_bytes());
    identity.update([0]);
    identity.update(symbols_url.as_bytes());
    format!("{:x}", identity.finalize())
}

fn update_registry_index(
    target_dir: &Path,
    registry_config: &RegistryConfig,
    user_log: &UserLog,
) -> Result<RegistryIndexUpdate, UpdateError> {
    if target_dir.try_exists().map_err(|e| UpdateError {
        source: UpdateErrorKind::IO(e),
    })? {
        let state = inspect_registry_index(target_dir).map_err(|e| UpdateError {
            source: UpdateErrorKind::InspectRegistryIndexError(e),
        })?;
        let reclone_reason = match state {
            RegistryIndexState::Ready { remote_url } if remote_url == registry_config.index => {
                if pull_latest_registry_index(target_dir).is_ok() {
                    return Ok(RegistryIndexUpdate::Updated);
                }
                RegistryIndexRecloneReason::PullFailed
            }
            RegistryIndexState::Ready { .. } => RegistryIndexRecloneReason::RemoteMismatch,
            RegistryIndexState::NotGitRepository => RegistryIndexRecloneReason::NotGitRepository,
            RegistryIndexState::MissingOrigin => RegistryIndexRecloneReason::MissingOrigin,
        };
        safe_reclone_registry_index(registry_config, target_dir, user_log)?;
        Ok(RegistryIndexUpdate::Recloned(reclone_reason))
    } else {
        clone_registry_index(registry_config, target_dir).map_err(|e| UpdateError {
            source: UpdateErrorKind::CloneRegistryIndexError(e),
        })?;
        Ok(RegistryIndexUpdate::Cloned)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Cursor, Read, Write},
        net::TcpListener,
        sync::{
            Arc, Barrier,
            atomic::{AtomicUsize, Ordering},
        },
        thread::JoinHandle,
    };

    use super::*;

    fn run_git(args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn empty_registry() -> (tempfile::TempDir, RegistryConfig) {
        let base = tempfile::tempdir().unwrap();
        let index = base.path().join("index.git");
        run_git(&["init", "--bare", "--quiet", index.to_str().unwrap()]);
        let index = index.to_str().unwrap().to_owned();
        (
            base,
            RegistryConfig {
                registry: index.clone(),
                index,
                symbols: None,
            },
        )
    }

    fn quiet_user_log() -> UserLog {
        UserLog::new(log::LevelFilter::Off)
    }

    fn test_moon_home(root: &tempfile::TempDir) -> MoonHomeLayout {
        let home = MoonHomeLayout::new(root.path().to_path_buf());
        std::fs::create_dir_all(home.registry_dir()).unwrap();
        home
    }

    fn registry_with_history() -> (tempfile::TempDir, RegistryConfig) {
        let base = tempfile::tempdir().unwrap();
        let source = base.path().join("source");
        run_git(&["init", "--quiet", source.to_str().unwrap()]);
        run_git(&[
            "-C",
            source.to_str().unwrap(),
            "checkout",
            "--quiet",
            "-b",
            "main",
        ]);
        run_git(&[
            "-C",
            source.to_str().unwrap(),
            "config",
            "user.name",
            "Moon Test",
        ]);
        run_git(&[
            "-C",
            source.to_str().unwrap(),
            "config",
            "user.email",
            "moon-test@example.invalid",
        ]);
        for version in ["one", "two"] {
            std::fs::write(source.join("index-version"), version).unwrap();
            run_git(&["-C", source.to_str().unwrap(), "add", "index-version"]);
            run_git(&[
                "-C",
                source.to_str().unwrap(),
                "commit",
                "--quiet",
                "-m",
                version,
            ]);
        }
        run_git(&["-C", source.to_str().unwrap(), "branch", "side-branch"]);

        // Model the remote as a bare repository; the checkout under test is
        // still a regular worktree created by `clone_registry_index`.
        let bare = base.path().join("index.git");
        run_git(&[
            "clone",
            "--bare",
            "--quiet",
            source.to_str().unwrap(),
            bare.to_str().unwrap(),
        ]);
        let path = dunce::canonicalize(&bare)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let index = if cfg!(windows) {
            format!("file:///{path}")
        } else {
            format!("file://{path}")
        };
        (
            base,
            RegistryConfig {
                registry: index.clone(),
                index,
                symbols: None,
            },
        )
    }

    fn successful_update() -> UpdateOutcome {
        UpdateOutcome {
            registry_index: RegistryIndexUpdate::Updated,
            symbols_updated: true,
        }
    }

    #[test]
    fn registry_update_identity_covers_index_and_symbols() {
        let identity = registry_identity("https://registry.invalid/index", "https://a/symbols");
        assert_ne!(
            identity,
            registry_identity("https://other.invalid/index", "https://a/symbols")
        );
        assert_ne!(
            identity,
            registry_identity("https://registry.invalid/index", "https://b/symbols")
        );
    }

    struct TestHttpResponse {
        status: &'static str,
        headers: Vec<(&'static str, &'static str)>,
        body: Vec<u8>,
    }

    fn serve_http(responses: Vec<TestHttpResponse>) -> (String, JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            responses
                .into_iter()
                .map(|response| {
                    let (mut stream, _) = listener.accept().unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0; 1024];
                    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                        let read = stream.read(&mut buffer).unwrap();
                        assert_ne!(read, 0, "request ended before its headers");
                        request.extend_from_slice(&buffer[..read]);
                    }

                    write!(
                        stream,
                        "HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n",
                        response.status,
                        response.body.len()
                    )
                    .unwrap();
                    for (name, value) in response.headers {
                        write!(stream, "{name}: {value}\r\n").unwrap();
                    }
                    stream.write_all(b"\r\n").unwrap();
                    stream.write_all(&response.body).unwrap();
                    String::from_utf8(request).unwrap()
                })
                .collect()
        });
        (format!("http://{address}/symbols.zip"), server)
    }

    fn symbols_zip(contents: &str) -> Vec<u8> {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file("symbol.txt", zip::write::FileOptions::default())
            .unwrap();
        archive.write_all(contents.as_bytes()).unwrap();
        archive.finish().unwrap().into_inner()
    }

    #[test]
    fn symbols_update_reuses_an_etag_on_not_modified() {
        let root = tempfile::tempdir().unwrap();
        let home = test_moon_home(&root);
        let (url, server) = serve_http(vec![
            TestHttpResponse {
                status: "200 OK",
                headers: vec![("ETag", "\"symbols-v1\"")],
                body: symbols_zip("downloaded"),
            },
            TestHttpResponse {
                status: "304 Not Modified",
                headers: Vec::new(),
                body: Vec::new(),
            },
        ]);

        let etag = update_symbols_from_url(
            &home.registry_dir(),
            &home.registry_symbols_dir(),
            &url,
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(etag.url, url);
        assert_eq!(etag.value, "\"symbols-v1\"");
        std::fs::write(home.registry_symbols_dir().join("symbol.txt"), "local").unwrap();

        let reused = update_symbols_from_url(
            &home.registry_dir(),
            &home.registry_symbols_dir(),
            &url,
            Some(&etag),
        )
        .unwrap()
        .unwrap();

        assert_eq!(reused, etag);
        assert_eq!(
            std::fs::read_to_string(home.registry_symbols_dir().join("symbol.txt")).unwrap(),
            "local"
        );
        let requests = server.join().unwrap();
        assert!(!requests[0].to_ascii_lowercase().contains("if-none-match"));
        assert!(
            requests[1]
                .to_ascii_lowercase()
                .contains("if-none-match: \"symbols-v1\"")
        );
    }

    #[test]
    fn symbols_update_ignores_an_etag_when_local_symbols_are_missing() {
        let root = tempfile::tempdir().unwrap();
        let home = test_moon_home(&root);
        let (url, server) = serve_http(vec![TestHttpResponse {
            status: "200 OK",
            headers: vec![("ETag", "\"symbols-v2\"")],
            body: symbols_zip("restored"),
        }]);
        let etag = SymbolsEtag {
            url: url.clone(),
            value: "\"symbols-v1\"".to_owned(),
        };

        let updated = update_symbols_from_url(
            &home.registry_dir(),
            &home.registry_symbols_dir(),
            &url,
            Some(&etag),
        )
        .unwrap()
        .unwrap();

        assert_eq!(updated.value, "\"symbols-v2\"");
        assert_eq!(
            std::fs::read_to_string(home.registry_symbols_dir().join("symbol.txt")).unwrap(),
            "restored"
        );
        let requests = server.join().unwrap();
        assert!(!requests[0].to_ascii_lowercase().contains("if-none-match"));
    }

    #[test]
    fn symbols_update_ignores_an_invalid_etag() {
        let root = tempfile::tempdir().unwrap();
        let home = test_moon_home(&root);
        std::fs::create_dir(home.registry_symbols_dir()).unwrap();
        let (url, server) = serve_http(vec![TestHttpResponse {
            status: "200 OK",
            headers: Vec::new(),
            body: symbols_zip("updated"),
        }]);
        let etag = SymbolsEtag {
            url: url.clone(),
            value: "invalid\netag".to_owned(),
        };

        let updated = update_symbols_from_url(
            &home.registry_dir(),
            &home.registry_symbols_dir(),
            &url,
            Some(&etag),
        )
        .unwrap();

        assert_eq!(updated, None);
        let requests = server.join().unwrap();
        assert!(!requests[0].to_ascii_lowercase().contains("if-none-match"));
    }

    #[test]
    fn registry_update_state_without_an_etag_remains_readable() {
        let state: RegistryUpdateState =
            serde_json::from_str(r#"{"registry_identity":"registry-a","generation":"one"}"#)
                .unwrap();

        assert_eq!(state.symbols_etag, None);
    }

    #[test]
    fn registry_index_clone_is_shallow_and_single_branch() {
        let (_registry, config) = registry_with_history();
        let checkout = tempfile::tempdir().unwrap();
        let index = checkout.path().join("index");

        clone_registry_index(&config, &index).unwrap();

        let shallow = run_git_query(&index, &["rev-parse", "--is-shallow-repository"]).unwrap();
        assert_eq!(shallow.stdout, "true");
        let commits = run_git_query(&index, &["rev-list", "--count", "HEAD"]).unwrap();
        assert_eq!(commits.stdout, "1");
        let tag_option =
            run_git_query(&index, &["config", "--get", "remote.origin.tagOpt"]).unwrap();
        assert_eq!(tag_option.stdout, "--no-tags");
        let branches = run_git_query(
            &index,
            &["for-each-ref", "--format=%(refname:short)", "refs/remotes"],
        )
        .unwrap();
        assert!(!branches.stdout.contains("origin/side-branch"));
    }

    #[test]
    fn concurrent_registry_updates_are_coalesced() {
        let root = tempfile::tempdir().unwrap();
        let home = Arc::new(test_moon_home(&root));
        let barrier = Arc::new(Barrier::new(2));
        let performed = Arc::new(AtomicUsize::new(0));
        let observed = observe_registry_update_state(&home.registry_update_state_path());
        let mut threads = Vec::new();

        for _ in 0..2 {
            let home = Arc::clone(&home);
            let barrier = Arc::clone(&barrier);
            let performed = Arc::clone(&performed);
            let observed = observed.clone();
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                run_registry_update_locked(&home, "registry-a", observed, &quiet_user_log(), |_| {
                    performed.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    Ok((successful_update(), None))
                })
                .unwrap()
            }));
        }

        let outcomes = threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(performed.load(Ordering::SeqCst), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| {
                    outcome.registry_index == RegistryIndexUpdate::ConcurrentUpdateReused
                })
                .count(),
            1
        );
    }

    #[test]
    fn completed_registry_update_is_not_a_ttl() {
        let root = tempfile::tempdir().unwrap();
        let home = test_moon_home(&root);
        let performed = AtomicUsize::new(0);
        let etag = SymbolsEtag {
            url: "https://example.invalid/symbols.zip".to_owned(),
            value: "\"symbols-v1\"".to_owned(),
        };
        let perform = |previous_etag: Option<&SymbolsEtag>| {
            let attempt = performed.fetch_add(1, Ordering::SeqCst);
            if attempt == 0 {
                assert_eq!(previous_etag, None);
            } else {
                assert_eq!(previous_etag, Some(&etag));
            }
            Ok((successful_update(), Some(etag.clone())))
        };

        run_registry_update_locked(
            &home,
            "registry-a",
            observe_registry_update_state(&home.registry_update_state_path()),
            &quiet_user_log(),
            perform,
        )
        .unwrap();
        let observed = observe_registry_update_state(&home.registry_update_state_path());
        run_registry_update_locked(&home, "registry-a", observed, &quiet_user_log(), perform)
            .unwrap();

        assert_eq!(performed.load(Ordering::SeqCst), 2);
        let RegistryUpdateObservation::Observed(Some(state)) =
            observe_registry_update_state(&home.registry_update_state_path())
        else {
            panic!("registry update state was not written");
        };
        assert_eq!(state.symbols_etag, Some(etag));
    }

    #[test]
    fn concurrent_update_for_a_different_registry_is_not_reused() {
        let root = tempfile::tempdir().unwrap();
        let home = test_moon_home(&root);
        let performed = AtomicUsize::new(0);
        for registry_identity in ["registry-a", "registry-b"] {
            run_registry_update_locked(
                &home,
                registry_identity,
                RegistryUpdateObservation::Observed(None),
                &quiet_user_log(),
                |_| {
                    performed.fetch_add(1, Ordering::SeqCst);
                    Ok((successful_update(), None))
                },
            )
            .unwrap();
        }

        assert_eq!(performed.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn update_with_failed_symbols_is_not_reused() {
        let root = tempfile::tempdir().unwrap();
        let home = test_moon_home(&root);
        let performed = AtomicUsize::new(0);
        for _ in 0..2 {
            run_registry_update_locked(
                &home,
                "registry-a",
                RegistryUpdateObservation::Observed(None),
                &quiet_user_log(),
                |_| {
                    performed.fetch_add(1, Ordering::SeqCst);
                    Ok((
                        UpdateOutcome {
                            registry_index: RegistryIndexUpdate::Updated,
                            symbols_updated: false,
                        },
                        None,
                    ))
                },
            )
            .unwrap();
        }

        assert_eq!(performed.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn inspect_registry_index_does_not_expand_instead_of_rewrites() {
        let repo = tempfile::tempdir().unwrap();
        run_git(&["-C", repo.path().to_str().unwrap(), "init", "--quiet"]);

        let registry_url = "https://registry.invalid/git/index";
        run_git(&[
            "-C",
            repo.path().to_str().unwrap(),
            "remote",
            "add",
            "origin",
            registry_url,
        ]);
        run_git(&[
            "-C",
            repo.path().to_str().unwrap(),
            "config",
            "url.https://mirror.example/.insteadOf",
            "https://registry.invalid/",
        ]);

        assert_eq!(
            inspect_registry_index(repo.path()).unwrap(),
            RegistryIndexState::Ready {
                remote_url: registry_url.to_owned()
            }
        );
    }

    #[test]
    fn inspect_registry_index_uses_primary_origin_url() {
        let repo = tempfile::tempdir().unwrap();
        run_git(&["-C", repo.path().to_str().unwrap(), "init", "--quiet"]);

        let primary_url = "https://primary.invalid/index";
        run_git(&[
            "-C",
            repo.path().to_str().unwrap(),
            "remote",
            "add",
            "origin",
            primary_url,
        ]);
        run_git(&[
            "-C",
            repo.path().to_str().unwrap(),
            "remote",
            "set-url",
            "--add",
            "origin",
            "https://secondary.invalid/index",
        ]);

        assert_eq!(
            inspect_registry_index(repo.path()).unwrap(),
            RegistryIndexState::Ready {
                remote_url: primary_url.to_owned()
            }
        );
    }

    #[test]
    fn inspect_registry_index_reads_worktree_origin_url() {
        let repo = tempfile::tempdir().unwrap();
        run_git(&["-C", repo.path().to_str().unwrap(), "init", "--quiet"]);
        run_git(&[
            "-C",
            repo.path().to_str().unwrap(),
            "config",
            "extensions.worktreeConfig",
            "true",
        ]);

        let registry_url = "https://worktree.invalid/index";
        run_git(&[
            "-C",
            repo.path().to_str().unwrap(),
            "config",
            "--worktree",
            "remote.origin.url",
            registry_url,
        ]);

        assert_eq!(
            inspect_registry_index(repo.path()).unwrap(),
            RegistryIndexState::Ready {
                remote_url: registry_url.to_owned()
            }
        );
    }

    #[test]
    fn inspect_registry_index_recognizes_non_git_directory() {
        let not_a_repo = tempfile::tempdir().unwrap();

        assert_eq!(
            inspect_registry_index(not_a_repo.path()).unwrap(),
            RegistryIndexState::NotGitRepository
        );
    }

    #[test]
    fn update_registry_index_reclones_non_git_directory() {
        let (_registry, config) = empty_registry();
        let registry_home = tempfile::tempdir().unwrap();
        let index = registry_home.path().join("index");
        std::fs::create_dir(&index).unwrap();
        std::fs::write(index.join("stale"), "stale index").unwrap();

        let update = update_registry_index(&index, &config, &quiet_user_log()).unwrap();

        assert_eq!(
            update,
            RegistryIndexUpdate::Recloned(RegistryIndexRecloneReason::NotGitRepository)
        );
        assert!(!index.join("stale").exists());
        assert_eq!(
            inspect_registry_index(&index).unwrap(),
            RegistryIndexState::Ready {
                remote_url: config.index
            }
        );
    }

    #[test]
    fn update_registry_index_reclones_repository_without_origin() {
        let (_registry, config) = empty_registry();
        let registry_home = tempfile::tempdir().unwrap();
        let index = registry_home.path().join("index");
        std::fs::create_dir(&index).unwrap();
        run_git(&["-C", index.to_str().unwrap(), "init", "--quiet"]);

        let update = update_registry_index(&index, &config, &quiet_user_log()).unwrap();

        assert_eq!(
            update,
            RegistryIndexUpdate::Recloned(RegistryIndexRecloneReason::MissingOrigin)
        );
        assert_eq!(
            inspect_registry_index(&index).unwrap(),
            RegistryIndexState::Ready {
                remote_url: config.index
            }
        );
    }

    #[test]
    fn update_registry_index_reclones_origin_without_url() {
        let (_registry, config) = empty_registry();
        let registry_home = tempfile::tempdir().unwrap();
        let index = registry_home.path().join("index");
        std::fs::create_dir(&index).unwrap();
        run_git(&["-C", index.to_str().unwrap(), "init", "--quiet"]);
        run_git(&[
            "-C",
            index.to_str().unwrap(),
            "config",
            "remote.origin.fetch",
            "+refs/heads/*:refs/remotes/origin/*",
        ]);

        let update = update_registry_index(&index, &config, &quiet_user_log()).unwrap();

        assert_eq!(
            update,
            RegistryIndexUpdate::Recloned(RegistryIndexRecloneReason::MissingOrigin)
        );
        assert_eq!(
            inspect_registry_index(&index).unwrap(),
            RegistryIndexState::Ready {
                remote_url: config.index
            }
        );
    }

    #[test]
    fn update_registry_index_preserves_directory_when_inspection_fails() {
        let (_registry, config) = empty_registry();
        let registry_home = tempfile::tempdir().unwrap();
        let index = registry_home.path().join("index");
        std::fs::create_dir(&index).unwrap();
        run_git(&["-C", index.to_str().unwrap(), "init", "--quiet"]);
        std::fs::write(index.join("cached-index"), "preserve me").unwrap();
        std::fs::write(index.join(".git").join("config"), "[invalid").unwrap();

        let error = update_registry_index(&index, &config, &quiet_user_log()).unwrap_err();

        assert!(matches!(
            error.source,
            UpdateErrorKind::InspectRegistryIndexError(_)
        ));
        assert_eq!(
            std::fs::read_to_string(index.join("cached-index")).unwrap(),
            "preserve me"
        );
    }
}
