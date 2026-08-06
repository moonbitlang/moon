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
    git::{GitCommandError, Stdios},
    registry::RegistryConfig,
    user_log::UserLog,
};
use reqwest::header::USER_AGENT;

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
pub enum RegistryIndexRecloneReason {
    PullFailed,
    RemoteMismatch,
    NotGitRepository,
    MissingOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryIndexUpdate {
    Cloned,
    Updated,
    Recloned(RegistryIndexRecloneReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateOutcome {
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

    let child = moonutil::git::git_command(
        &[
            "clone",
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

fn download_symbols_zip() -> anyhow::Result<bytes::Bytes> {
    let client = reqwest::blocking::Client::new();
    let data = client
        .get(SYMBOLS_URL)
        .header(
            USER_AGENT,
            format!("mooncake/{}", env!("CARGO_PKG_VERSION")),
        )
        .send()
        .context("failed to fetch symbols.zip")?
        .error_for_status()
        .context("symbols.zip download returned error status")?
        .bytes()
        .context("failed to read symbols.zip response body")?;
    Ok(data)
}

fn update_symbols(registry_dir: &Path) -> anyhow::Result<()> {
    let data = download_symbols_zip()?;

    std::fs::create_dir_all(registry_dir)
        .with_context(|| format!("failed to create `{}`", registry_dir.display()))?;

    let tmp_dir = unique_sibling_dir(registry_dir, ".symbols.tmp")
        .context("failed to create temp directory for symbols")?;
    std::fs::create_dir_all(&tmp_dir)?;

    if let Err(e) = extract_zip_to_dir(&tmp_dir, std::io::Cursor::new(data)) {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(e);
    }

    let target_dir = registry_dir.join("symbols");
    if target_dir.exists() {
        let backup_dir = unique_sibling_dir(registry_dir, ".symbols.old")
            .context("failed to create backup dir")?;
        std::fs::rename(&target_dir, &backup_dir)
            .context("failed to move existing symbols dir to backup")?;

        if let Err(e) = std::fs::rename(&tmp_dir, &target_dir) {
            let _ = std::fs::rename(&backup_dir, &target_dir);
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
        std::fs::rename(&tmp_dir, &target_dir)
            .context("failed to move symbols directory into place")?;
    }

    Ok(())
}

pub fn update(
    target_dir: &Path,
    registry_config: &RegistryConfig,
    user_log: &UserLog,
) -> anyhow::Result<UpdateOutcome> {
    let registry_index = update_registry_index(target_dir, registry_config, user_log)?;

    let registry_dir = target_dir
        .parent()
        .context("registry index directory has no parent")?;
    let symbols_updated = match update_symbols(registry_dir) {
        Ok(()) => true,
        Err(e) => {
            user_log.warn(format!("failed to update symbols: {e:#}"));
            false
        }
    };

    Ok(UpdateOutcome {
        registry_index,
        symbols_updated,
    })
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
            },
        )
    }

    fn quiet_user_log() -> UserLog {
        UserLog::new(log::LevelFilter::Off)
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
