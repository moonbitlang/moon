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

//! Synchronize the local Git index and coalesce overlapping synchronizations.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use moonutil::{
    MoonHomeLayout,
    git::{GitCommandError, Stdios},
    locks::lock_directory,
    registry::RegistryConfig,
    user_log::UserLog,
};
use serde::{Deserialize, Serialize};

use crate::registry::{replace_registry_directory, sha256_hex, stage_registry_cache_file};

/// Bookkeeping for successful Git synchronization, independent of index contents.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct GitIndexSyncState {
    index_url_hash: String,
    generation: String,
}

impl GitIndexSyncState {
    fn read(path: &Path) -> anyhow::Result<Option<Self>> {
        match std::fs::read(path) {
            Ok(data) => Ok(Some(serde_json::from_slice(&data)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write(&self, path: &Path) -> anyhow::Result<()> {
        let (mut staged, path) = stage_registry_cache_file(path)?;
        serde_json::to_writer(staged.as_file_mut(), self)?;
        staged.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
}

#[tracing::instrument(level = "debug", skip_all)]
pub(super) fn sync(
    home: &MoonHomeLayout,
    config: &RegistryConfig,
    user_log: &UserLog,
) -> anyhow::Result<RegistryIndexUpdate> {
    let state_path = home.registry_update_state_path();
    let observed = GitIndexSyncState::read(&state_path);
    let registry_dir = home.registry_dir();
    std::fs::create_dir_all(&registry_dir)
        .with_context(|| format!("failed to create `{}`", registry_dir.display()))?;
    let _lock = lock_directory(&registry_dir, user_log)
        .context("failed to lock registry update directory")?;
    let current = GitIndexSyncState::read(&state_path);
    let index_url_hash = sha256_hex(config.index.as_bytes());

    // A changed generation after taking the lock means another caller completed
    // this synchronization while we waited. Unreadable state disables reuse;
    // an earlier completed synchronization is never a freshness window.
    if let (Ok(observed), Ok(Some(current))) = (&observed, &current)
        && observed.as_ref() != Some(current)
        && current.index_url_hash == index_url_hash
    {
        return Ok(RegistryIndexUpdate::ConcurrentUpdateReused);
    }

    let outcome = update_registry_index(&home.registry_index_dir(), config, user_log)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let state = GitIndexSyncState {
        index_url_hash,
        generation: format!("{}-{nanos}", std::process::id()),
    };
    if let Err(error) = state.write(&state_path) {
        user_log.warn(format!(
            "failed to record completed Git index update: {error:#}"
        ));
    }
    Ok(outcome)
}

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
pub(super) enum RegistryIndexUpdate {
    Cloned,
    Updated,
    Recloned,
    ConcurrentUpdateReused,
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

    let parent = std::fs::canonicalize(parent).map_err(|error| UpdateError {
        source: UpdateErrorKind::IO(error),
    })?;
    let staged = tempfile::Builder::new()
        .prefix(".registry-index-")
        .tempdir_in(parent)
        .map_err(|error| UpdateError {
            source: UpdateErrorKind::IO(error),
        })?;
    clone_registry_index(registry_config, staged.path()).map_err(|error| UpdateError {
        source: UpdateErrorKind::CloneRegistryIndexError(error),
    })?;
    replace_registry_directory(staged, target_dir, user_log).map_err(|error| UpdateError {
        source: UpdateErrorKind::DirectoryReplacement(error),
    })
}

fn fetch_and_reset_registry_index(target_dir: &Path) -> Result<(), RegistryGitError> {
    // Only the current index snapshot is needed. Select main explicitly and
    // allow rewritten history without depending on the user's pull policy.
    run_registry_git(
        target_dir,
        &[
            "fetch",
            "--depth=1",
            "--no-tags",
            "--no-recurse-submodules",
            "origin",
            "+refs/heads/main:refs/remotes/origin/main",
        ],
    )?;
    // This is a managed cache: replace its tracked state, but only after a
    // successful fetch so a failure cannot reset it to a stale FETCH_HEAD.
    run_registry_git(target_dir, &["reset", "--hard", "--quiet", "FETCH_HEAD"])?;
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
    DirectoryReplacement(#[from] anyhow::Error),

    #[error(transparent)]
    CloneRegistryIndexError(#[from] CloneRegistryIndexError),

    #[error(transparent)]
    RegistryGitError(#[from] RegistryGitError),

    #[error(transparent)]
    IO(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
#[error("failed to access registry index")]
struct RegistryGitError {
    #[source]
    source: RegistryGitErrorKind,
}

#[derive(Debug, thiserror::Error)]
enum RegistryGitErrorKind {
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

fn run_registry_git(target_dir: &Path, args: &[&str]) -> Result<CommandOutput, RegistryGitError> {
    let mut command = vec!["-C", target_dir.to_str().unwrap()];
    command.extend_from_slice(args);
    let output = moonutil::git::git_command(&command, Stdios::npp())
        .map_err(|e| RegistryGitError {
            source: RegistryGitErrorKind::GitCommandError(e),
        })?
        // Drain both pipes while waiting, even when a command is normally quiet.
        .wait_with_output()
        .map_err(|e| RegistryGitError {
            source: RegistryGitErrorKind::IO(e),
        })?;
    if !output.status.success() {
        return Err(RegistryGitError {
            source: RegistryGitErrorKind::NonZeroExitCode {
                command: command.join(" "),
                status: output.status,
                output: CommandOutput::from_output(&output),
            },
        });
    }
    Ok(CommandOutput::from_output(&output))
}

fn inspect_registry_index(target_dir: &Path) -> Result<RegistryIndexState, RegistryGitError> {
    if !target_dir
        .join(".git")
        .try_exists()
        .map_err(|e| RegistryGitError {
            source: RegistryGitErrorKind::IO(e),
        })?
    {
        return Ok(RegistryIndexState::NotGitRepository);
    }

    let inside_work_tree = run_registry_git(target_dir, &["rev-parse", "--is-inside-work-tree"])?;
    if inside_work_tree.stdout != "true" {
        return Ok(RegistryIndexState::NotGitRepository);
    }

    let remotes = run_registry_git(target_dir, &["remote"])?;
    if !remotes.stdout.lines().any(|remote| remote == "origin") {
        return Ok(RegistryIndexState::MissingOrigin);
    }

    // Git fetch uses the first configured URL. Read all effective raw values
    // to preserve that ordering without applying `url.*.insteadOf` rewrites.
    let remote_urls =
        match run_registry_git(target_dir, &["config", "--get-all", "remote.origin.url"]) {
            Ok(output) => output,
            Err(error)
                if matches!(
                    &error.source,
                    RegistryGitErrorKind::NonZeroExitCode { status, .. }
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

fn update_registry_index(
    target_dir: &Path,
    registry_config: &RegistryConfig,
    user_log: &UserLog,
) -> Result<RegistryIndexUpdate, UpdateError> {
    if target_dir.try_exists().map_err(|e| UpdateError {
        source: UpdateErrorKind::IO(e),
    })? {
        let state = inspect_registry_index(target_dir).map_err(|e| UpdateError {
            source: UpdateErrorKind::RegistryGitError(e),
        })?;
        let reason = match state {
            RegistryIndexState::Ready { remote_url } if remote_url == registry_config.index => {
                // Command failures do not establish that the cache is invalid.
                // Preserve the original error instead of masking it with a clone.
                fetch_and_reset_registry_index(target_dir).map_err(|error| UpdateError {
                    source: UpdateErrorKind::RegistryGitError(error),
                })?;
                return Ok(RegistryIndexUpdate::Updated);
            }
            RegistryIndexState::Ready { .. } => {
                "Registry index remote does not match the configured URL"
            }
            RegistryIndexState::NotGitRepository => "Registry index is not a Git repository",
            RegistryIndexState::MissingOrigin => "Registry index has no origin remote",
        };
        user_log.status(format!("{reason}, re-cloning"));
        safe_reclone_registry_index(registry_config, target_dir, user_log)?;
        Ok(RegistryIndexUpdate::Recloned)
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
                api: index.clone(),
                index,
                download: String::new(),
                symbols: None,
                legacy_asset_urls: false,
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
                api: index.clone(),
                index,
                download: String::new(),
                symbols: None,
                legacy_asset_urls: false,
            },
        )
    }

    #[test]
    fn synchronization_reuses_only_overlapping_updates_of_the_same_index() {
        for same_index in [true, false] {
            let (_first_remote, first_config) = registry_with_history();
            let (_second_remote, mut second_config) = registry_with_history();
            if same_index {
                second_config.index.clone_from(&first_config.index);
            }
            let root = tempfile::tempdir().unwrap();
            let home = test_moon_home(&root);
            let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
            let lock = lock_directory(&home.registry_dir(), &quiet_user_log()).unwrap();
            std::thread::scope(|scope| {
                let first = scope.spawn(|| sync(&home, &first_config, &user_log));
                let second = scope.spawn(|| sync(&home, &second_config, &user_log));

                // Both callers must observe the old state before either can
                // synchronize. The lock's contention log confirms they wait.
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                let mut waiting = 0;
                while waiting < 2 && std::time::Instant::now() < deadline {
                    waiting += capture
                        .take()
                        .iter()
                        .filter(|entry| entry.message.starts_with("Blocking waiting for file lock"))
                        .count();
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                drop(lock);
                let outcomes = [
                    first.join().unwrap().unwrap(),
                    second.join().unwrap().unwrap(),
                ];
                assert_eq!(waiting, 2);
                assert_eq!(
                    outcomes
                        .iter()
                        .filter(|outcome| **outcome == RegistryIndexUpdate::ConcurrentUpdateReused)
                        .count(),
                    usize::from(same_index)
                );
            });
        }
    }

    #[test]
    fn index_sync_preserves_symbols_state_and_retries_failed_updates() {
        let (remote, config) = registry_with_history();
        let root = tempfile::tempdir().unwrap();
        let home = test_moon_home(&root);
        std::fs::write(home.registry_symbols_update_state_path(), "symbols state").unwrap();

        assert_eq!(
            sync(&home, &config, &quiet_user_log()).unwrap(),
            RegistryIndexUpdate::Cloned
        );
        // An earlier completed synchronization does not satisfy a later call.
        assert_eq!(
            sync(&home, &config, &quiet_user_log()).unwrap(),
            RegistryIndexUpdate::Updated
        );
        assert!(!home.registry_symbols_dir().exists());
        assert_eq!(
            std::fs::read_to_string(home.registry_symbols_update_state_path()).unwrap(),
            "symbols state"
        );
        let completed = std::fs::read(home.registry_update_state_path()).unwrap();

        std::fs::remove_dir_all(remote.path().join("index.git")).unwrap();
        for _ in 0..2 {
            assert!(sync(&home, &config, &quiet_user_log()).is_err());
            assert_eq!(
                std::fs::read(home.registry_update_state_path()).unwrap(),
                completed
            );
        }
    }

    #[test]
    fn registry_index_clone_is_shallow_and_single_branch() {
        let (_registry, config) = registry_with_history();
        let checkout = tempfile::tempdir().unwrap();
        let index = checkout.path().join("index");

        clone_registry_index(&config, &index).unwrap();

        let shallow = run_registry_git(&index, &["rev-parse", "--is-shallow-repository"]).unwrap();
        assert_eq!(shallow.stdout, "true");
        let commits = run_registry_git(&index, &["rev-list", "--count", "HEAD"]).unwrap();
        assert_eq!(commits.stdout, "1");
        let tag_option =
            run_registry_git(&index, &["config", "--get", "remote.origin.tagOpt"]).unwrap();
        assert_eq!(tag_option.stdout, "--no-tags");
        let branches = run_registry_git(
            &index,
            &["for-each-ref", "--format=%(refname:short)", "refs/remotes"],
        )
        .unwrap();
        assert!(!branches.stdout.contains("origin/side-branch"));
    }

    #[test]
    fn registry_index_update_keeps_full_and_shallow_clones_shallow() {
        let (registry, config) = registry_with_history();
        let checkout = tempfile::tempdir().unwrap();
        let source = registry.path().join("source");
        let source = source.to_str().unwrap();
        let bare = registry.path().join("index.git");
        let bare = bare.to_str().unwrap();
        let indexes = [
            checkout.path().join("full"),
            checkout.path().join("shallow"),
        ];
        run_git(&[
            "clone",
            "--quiet",
            &config.index,
            indexes[0].to_str().unwrap(),
        ]);
        clone_registry_index(&config, &indexes[1]).unwrap();
        for index in &indexes {
            run_git(&[
                "-C",
                index.to_str().unwrap(),
                "config",
                "pull.rebase",
                "true",
            ]);
            // Registry updates must select main even if the configured fetch
            // mapping names a different branch.
            run_git(&[
                "-C",
                index.to_str().unwrap(),
                "config",
                "remote.origin.fetch",
                "+refs/heads/side-branch:refs/remotes/origin/side-branch",
            ]);
        }

        run_git(&["-C", source, "mv", "index-version", "renamed-index-version"]);
        run_git(&["-C", source, "commit", "--quiet", "-m", "rename entry"]);
        run_git(&["-C", source, "tag", "new-tag"]);
        run_git(&["-C", source, "push", "--quiet", bare, "main", "new-tag"]);
        let expected = run_registry_git(Path::new(source), &["rev-parse", "HEAD"]).unwrap();

        for index in indexes {
            let outcome = update_registry_index(&index, &config, &quiet_user_log()).unwrap();
            assert_eq!(outcome, RegistryIndexUpdate::Updated);
            assert_eq!(
                run_registry_git(&index, &["rev-parse", "HEAD"])
                    .unwrap()
                    .stdout,
                expected.stdout
            );
            assert_eq!(
                run_registry_git(&index, &["rev-parse", "origin/main"])
                    .unwrap()
                    .stdout,
                expected.stdout
            );
            assert_eq!(
                run_registry_git(&index, &["rev-list", "--count", "HEAD"])
                    .unwrap()
                    .stdout,
                "1"
            );
            assert!(
                run_registry_git(&index, &["tag", "--list"])
                    .unwrap()
                    .stdout
                    .is_empty()
            );
            assert!(!index.join("index-version").exists());
            assert_eq!(
                std::fs::read_to_string(index.join("renamed-index-version")).unwrap(),
                "two"
            );
        }
    }

    #[test]
    fn registry_index_update_follows_rewritten_main_without_recloning() {
        let (registry, config) = registry_with_history();
        let checkout = tempfile::tempdir().unwrap();
        let index = checkout.path().join("index");
        clone_registry_index(&config, &index).unwrap();
        let source = registry.path().join("source");
        let source = source.to_str().unwrap();
        run_git(&["-C", source, "reset", "--hard", "HEAD~1"]);
        std::fs::write(Path::new(source).join("index-version"), "rewritten").unwrap();
        run_git(&["-C", source, "commit", "--quiet", "-am", "rewrite main"]);
        run_git(&[
            "-C",
            source,
            "push",
            "--quiet",
            "--force",
            registry.path().join("index.git").to_str().unwrap(),
            "main",
        ]);
        // Resetting the managed cache must not depend on a user's pull policy.
        run_git(&["-C", index.to_str().unwrap(), "config", "pull.ff", "only"]);

        let outcome = update_registry_index(&index, &config, &quiet_user_log()).unwrap();
        assert_eq!(outcome, RegistryIndexUpdate::Updated);
        assert_eq!(
            std::fs::read_to_string(index.join("index-version")).unwrap(),
            "rewritten"
        );
        assert_eq!(
            run_registry_git(&index, &["rev-list", "--count", "HEAD"])
                .unwrap()
                .stdout,
            "1"
        );
    }

    #[test]
    fn update_failure_preserves_the_git_error_without_recloning() {
        let (_registry, config) = registry_with_history();
        let checkout = tempfile::tempdir().unwrap();
        let index = checkout.path().join("index");
        clone_registry_index(&config, &index).unwrap();
        // A persistent checkout lock prevents reset; a fresh clone would succeed
        // and silently discard this checkout, masking the real update failure.
        std::fs::write(index.join(".git/index.lock"), "held by another operation").unwrap();
        std::fs::write(index.join("index-version"), "preserve local contents").unwrap();

        let error = update_registry_index(&index, &config, &quiet_user_log()).unwrap_err();
        assert!(matches!(error.source, UpdateErrorKind::RegistryGitError(_)));
        let message = format!("{error:?}");
        assert!(message.contains("reset --hard --quiet FETCH_HEAD"));
        assert!(message.contains("index.lock"));
        assert_eq!(
            std::fs::read_to_string(index.join("index-version")).unwrap(),
            "preserve local contents"
        );
        assert!(index.join(".git/index.lock").exists());
    }

    #[test]
    fn registry_index_update_preserves_checkout_when_fetch_fails() {
        let (registry, config) = registry_with_history();
        let checkout = tempfile::tempdir().unwrap();
        let index = checkout.path().join("index");
        clone_registry_index(&config, &index).unwrap();
        let original = run_registry_git(&index, &["rev-parse", "HEAD"])
            .unwrap()
            .stdout;
        // Seed a different FETCH_HEAD to catch a reset after a failed fetch.
        run_git(&[
            "-C",
            index.to_str().unwrap(),
            "fetch",
            "--unshallow",
            "origin",
            "main",
        ]);
        run_git(&["-C", index.to_str().unwrap(), "reset", "--hard", "HEAD~1"]);
        let previous = run_registry_git(&index, &["rev-parse", "HEAD"])
            .unwrap()
            .stdout;
        assert_ne!(previous, original);
        std::fs::write(index.join("index-version"), "preserve local contents").unwrap();
        run_git(&[
            "-C",
            index.to_str().unwrap(),
            "remote",
            "set-url",
            "origin",
            registry.path().join("missing.git").to_str().unwrap(),
        ]);

        assert!(fetch_and_reset_registry_index(&index).is_err());
        assert_eq!(
            run_registry_git(&index, &["rev-parse", "HEAD"])
                .unwrap()
                .stdout,
            previous
        );
        assert_eq!(
            std::fs::read_to_string(index.join("index-version")).unwrap(),
            "preserve local contents"
        );
    }

    #[test]
    fn registry_git_commands_drain_large_output() {
        let (registry, config) = registry_with_history();
        let checkout = tempfile::tempdir().unwrap();
        let index = checkout.path().join("index");
        clone_registry_index(&config, &index).unwrap();

        // A large diffstat exceeds the OS pipe buffer, so the output must be
        // drained while waiting even though index updates are normally quiet.
        let source = registry.path().join("source");
        for i in 0..2000 {
            std::fs::write(
                source.join(format!("package-with-a-long-enough-name-{i:04}")),
                "{}",
            )
            .unwrap();
        }
        run_git(&["-C", source.to_str().unwrap(), "add", "."]);
        run_git(&[
            "-C",
            source.to_str().unwrap(),
            "commit",
            "--quiet",
            "-m",
            "many packages",
        ]);
        let bare = registry.path().join("index.git");
        run_git(&[
            "-C",
            source.to_str().unwrap(),
            "push",
            "--quiet",
            bare.to_str().unwrap(),
            "main",
        ]);

        let (tx, rx) = std::sync::mpsc::channel();
        let update_index = index.clone();
        std::thread::spawn(move || {
            let output = fetch_and_reset_registry_index(&update_index).and_then(|()| {
                run_registry_git(&update_index, &["show", "--stat", "--oneline", "HEAD"])
            });
            tx.send(output).unwrap();
        });
        let output = rx
            .recv_timeout(std::time::Duration::from_secs(60))
            .expect("running a registry Git command deadlocked")
            .unwrap();
        assert!(output.stdout.len() > 65_536);
        assert!(index.join("package-with-a-long-enough-name-1999").exists());
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
    fn failed_reclone_preserves_the_old_directory_and_cleans_staging() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("index");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("cached-index"), "preserve me").unwrap();
        let config = RegistryConfig {
            index: root
                .path()
                .join("missing.git")
                .to_string_lossy()
                .into_owned(),
            ..RegistryConfig::default()
        };

        assert!(update_registry_index(&target, &config, &quiet_user_log()).is_err());
        assert_eq!(
            std::fs::read_to_string(target.join("cached-index")).unwrap(),
            "preserve me"
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[test]
    fn update_registry_index_reclones_non_git_directory() {
        let (_registry, config) = empty_registry();
        let registry_home = tempfile::tempdir().unwrap();
        let index = registry_home.path().join("index");
        std::fs::create_dir(&index).unwrap();
        std::fs::write(index.join("stale"), "stale index").unwrap();

        let (user_log, capture) = UserLog::captured(log::LevelFilter::Warn);
        let update = update_registry_index(&index, &config, &user_log).unwrap();

        assert_eq!(update, RegistryIndexUpdate::Recloned);
        assert_eq!(
            capture
                .take()
                .into_iter()
                .map(|entry| entry.message)
                .collect::<Vec<_>>(),
            ["Registry index is not a Git repository, re-cloning"]
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

        assert_eq!(update, RegistryIndexUpdate::Recloned);
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

        assert_eq!(update, RegistryIndexUpdate::Recloned);
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

        assert!(matches!(error.source, UpdateErrorKind::RegistryGitError(_)));
        assert_eq!(
            std::fs::read_to_string(index.join("cached-index")).unwrap(),
            "preserve me"
        );
    }
}
