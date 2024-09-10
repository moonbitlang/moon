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

use std::path::Path;

use colored::Colorize;
use moonutil::{
    git::{GitCommandError, Stdios},
    mooncakes::RegistryConfig,
};

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
                    std::fs::remove_dir_all(target_dir).map_err(|e| UpdateError {
                        source: UpdateErrorKind::IO(e),
                    })?;
                    clone_registry_index(registry_config, target_dir).map_err(|e| UpdateError {
                        source: UpdateErrorKind::CloneRegistryIndexError(e),
                    })?;
                    eprintln!("{}", "Registry index re-cloned successfully".bold().green());
                    Ok(0)
                }
                Ok(()) => {
                    eprintln!("{}", "Registry index updated successfully".bold().green());
                    Ok(0)
                }
            }
        } else {
            eprintln!(
                "Registry index is not cloned from the same URL, {}",
                "re-cloning".yellow().bold()
            );
            std::fs::remove_dir_all(target_dir).map_err(|e| UpdateError {
                source: UpdateErrorKind::IO(e),
            })?;
            clone_registry_index(registry_config, target_dir).map_err(|e| UpdateError {
                source: UpdateErrorKind::CloneRegistryIndexError(e),
            })?;
            eprintln!("{}", "Registry index re-cloned successfully".bold().green());
            Ok(0)
        }
    } else {
        clone_registry_index(registry_config, target_dir).map_err(|e| UpdateError {
            source: UpdateErrorKind::CloneRegistryIndexError(e),
        })?;
        eprintln!("{}", "Registry index cloned successfully".bold().green());
        Ok(0)
    }
}
