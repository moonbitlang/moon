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

use anyhow::Context;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::common::{BUILD_DIR, MOON_MOD_JSON, MOON_WORK};

#[derive(Debug, Error)]
pub enum PackageDirsError {
    #[error(
        "not in a Moon project (no moon.mod.json or moon.work.json found starting from {0} or its ancestors)"
    )]
    NotInProject(PathBuf),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, clap::Parser, Serialize, Deserialize, Clone)]
pub struct SourceTargetDirs {
    // NOTE: This separates "working directory" vs "project root".
    //
    // - `-C` changes the process working directory early (like `cd DIR && moon ...`).
    // - `--manifest-path` pins the project root to a specific `moon.mod.json`
    //   without changing the working directory.
    /// Change to DIR before doing anything else (must appear before the subcommand). Relative paths in other options and arguments are interpreted relative to DIR. Example: `moon -C a run .` runs the same as invoking `moon run .` from within `a`.
    #[arg(short = 'C', value_name = "DIR")]
    pub cwd: Option<PathBuf>,

    /// Path to `moon.mod.json` to use as the project manifest (does not change the working directory).
    #[arg(long = "manifest-path", global = true, value_name = "PATH")]
    pub manifest_path: Option<PathBuf>,

    /// The target directory. Defaults to `<project-root>/_build`.
    #[clap(long, global = true)]
    pub target_dir: Option<PathBuf>,
}

impl SourceTargetDirs {
    pub fn try_into_package_dirs(&self) -> Result<PackageDirs, PackageDirsError> {
        get_src_dst_dir(self)
    }
}

pub struct PackageDirs {
    pub source_dir: PathBuf,
    pub target_dir: PathBuf,
}

pub fn check_moon_mod_exists(source_dir: &Path) -> bool {
    source_dir.join(MOON_MOD_JSON).exists()
}

pub fn check_moon_work_exists(source_dir: &Path) -> bool {
    source_dir.join(MOON_WORK).exists()
}

pub fn find_ancestor_with_mod(source_dir: &Path) -> Option<PathBuf> {
    source_dir
        .ancestors()
        .find(|dir| check_moon_mod_exists(dir))
        .map(|p| p.to_path_buf())
}

pub fn find_ancestor_with_work(source_dir: &Path) -> Option<PathBuf> {
    source_dir
        .ancestors()
        .find(|dir| check_moon_work_exists(dir))
        .map(|p| p.to_path_buf())
}

fn get_src_dst_dir(matches: &SourceTargetDirs) -> Result<PackageDirs, PackageDirsError> {
    let project_root = if let Some(manifest_path) = &matches.manifest_path {
        let manifest_path = dunce::canonicalize(manifest_path)
            .with_context(|| {
                format!(
                    "failed to resolve manifest path `{}`",
                    manifest_path.display()
                )
            })
            .map_err(PackageDirsError::from)?;

        if manifest_path.is_dir() {
            return Err(anyhow::anyhow!(
                "`--manifest-path` must point to `{}` (got directory `{}`)",
                MOON_MOD_JSON,
                manifest_path.display()
            )
            .into());
        }

        let file_name = manifest_path.file_name().and_then(|s| s.to_str());
        let is_module_manifest = file_name == Some(MOON_MOD_JSON);
        let is_workspace_manifest = file_name == Some(MOON_WORK);
        if !is_module_manifest && !is_workspace_manifest {
            return Err(anyhow::anyhow!(
                "`--manifest-path` must point to `{}` or `{}` (got `{}`)",
                MOON_MOD_JSON,
                MOON_WORK,
                manifest_path.display()
            )
            .into());
        }

        manifest_path
            .parent()
            .context("manifest path has no parent directory")
            .map_err(PackageDirsError::from)?
            .to_path_buf()
    } else {
        let start_dir = std::env::current_dir()
            .context("failed to get current directory")
            .map_err(PackageDirsError::from)?;
        let start_dir = dunce::canonicalize(start_dir)
            .context("failed to resolve current directory")
            .map_err(PackageDirsError::from)?;
        let project_root =
            find_ancestor_with_work(&start_dir).or_else(|| find_ancestor_with_mod(&start_dir));
        project_root.ok_or_else(|| PackageDirsError::NotInProject(start_dir.clone()))?
    };

    let target_dir = matches
        .target_dir
        .clone()
        .unwrap_or_else(|| project_root.join(BUILD_DIR));
    if !target_dir.exists() {
        std::fs::create_dir_all(&target_dir)
            .context("failed to create target directory")
            .map_err(PackageDirsError::from)?;
    }
    let target_dir = dunce::canonicalize(target_dir)
        .context("failed to set target directory")
        .map_err(PackageDirsError::from)?;

    Ok(PackageDirs {
        source_dir: project_root,
        target_dir,
    })
}
