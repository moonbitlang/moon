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
use colored::Colorize;
use moonutil::{
    git::{GitCommandError, Stdios},
    mooncakes::RegistryConfig,
};
use reqwest::header::USER_AGENT;

use crate::zip_util::extract_zip_to_dir;

const SYMBOLS_URL: &str = "https://download.mooncakes.io/symbols.zip";

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

    #[error("non-zero exit code: {0}")]
    NonZeroExitCode(std::process::ExitStatus),
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

    let mut child = moonutil::git::git_command(
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

    let status = child.wait().map_err(|e| CloneRegistryIndexError {
        source: CloneRegistryIndexErrorKind::IO(e),
    })?;
    if !status.success() {
        return Err(CloneRegistryIndexError {
            source: CloneRegistryIndexErrorKind::NonZeroExitCode(status),
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
        eprintln!(
            "{}: failed to remove old registry index at `{}`: {e}",
            "Warning".yellow().bold(),
            backup_dir.display()
        );
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

fn pull_latest_registry_index(
    _registry_config: &RegistryConfig,
    target_dir: &Path,
) -> Result<(), PullLatestRegistryIndexError> {
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
    GetRemoteUrlError(#[from] GetRemoteUrlError),

    #[error(transparent)]
    IO(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
#[error("failed to get remote url")]
struct GetRemoteUrlError {
    #[source]
    source: GetRemoteUrlErrorKind,
}

#[derive(Debug, thiserror::Error)]
enum GetRemoteUrlErrorKind {
    #[error(transparent)]
    GitCommandError(#[from] GitCommandError),

    #[error(transparent)]
    IO(#[from] std::io::Error),
}

fn get_remote_url(target_dir: &Path) -> Result<String, GetRemoteUrlError> {
    let output = moonutil::git::git_command(
        &[
            "-C",
            target_dir.to_str().unwrap(),
            "remote",
            "get-url",
            "origin",
        ],
        Stdios::npp(),
    )
    .map_err(|e| GetRemoteUrlError {
        source: GetRemoteUrlErrorKind::GitCommandError(e),
    })?
    .wait_with_output()
    .map_err(|e| GetRemoteUrlError {
        source: GetRemoteUrlErrorKind::IO(e),
    })?;
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(url)
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
    std::fs::create_dir_all(registry_dir)
        .with_context(|| format!("failed to create `{}`", registry_dir.display()))?;

    let tmp_dir = unique_sibling_dir(registry_dir, ".symbols.tmp")
        .context("failed to create temp directory for symbols")?;
    std::fs::create_dir_all(&tmp_dir)?;

    let data = download_symbols_zip()?;
    if let Err(e) = extract_zip_to_dir(&tmp_dir, data) {
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

        if let Err(e) = std::fs::remove_dir_all(&backup_dir) {
            eprintln!(
                "{}: failed to remove old symbols directory at `{}`: {e}",
                "Warning".yellow().bold(),
                backup_dir.display()
            );
        }
    } else {
        std::fs::rename(&tmp_dir, &target_dir)
            .context("failed to move symbols directory into place")?;
    }

    eprintln!("{}", "Symbols updated successfully".bold().green());
    Ok(())
}

pub fn update(target_dir: &Path, registry_config: &RegistryConfig) -> anyhow::Result<i32> {
    if target_dir.exists() {
        let url = get_remote_url(target_dir).map_err(|e| UpdateError {
            source: UpdateErrorKind::GetRemoteUrlError(e),
        })?;
        if url == registry_config.index {
            let result = pull_latest_registry_index(registry_config, target_dir);
            match result {
                Err(_) => {
                    eprintln!(
                        "failed to update registry, {}",
                        "re-cloning".bold().yellow()
                    );
                    safe_reclone_registry_index(registry_config, target_dir)?;
                    eprintln!("{}", "Registry index re-cloned successfully".bold().green());
                }
                Ok(()) => {
                    eprintln!("{}", "Registry index updated successfully".bold().green());
                }
            }
        } else {
            eprintln!(
                "Registry index is not cloned from the same URL, {}",
                "re-cloning".yellow().bold()
            );
            safe_reclone_registry_index(registry_config, target_dir)?;
            eprintln!("{}", "Registry index re-cloned successfully".bold().green());
        }
    } else {
        clone_registry_index(registry_config, target_dir).map_err(|e| UpdateError {
            source: UpdateErrorKind::CloneRegistryIndexError(e),
        })?;
        eprintln!("{}", "Registry index cloned successfully".bold().green());
    }

    let registry_dir = target_dir
        .parent()
        .context("registry index directory has no parent")?;
    update_symbols(registry_dir)?;

    Ok(0)
}
