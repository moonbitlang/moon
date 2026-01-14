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

use anyhow::{Context, bail};
use moonutil::{
    cli::UniversalFlags,
    common::{FileLock, LEGACY_BUILD_DIR, MOON_MOD_JSON},
};

/// Remove the target directory
#[derive(Debug, clap::Parser)]
pub struct CleanSubcommand {}

pub fn run_clean(cli: &UniversalFlags) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not supported for clean");
    }

    let src_tgt = cli.source_tgt_dir.try_into_package_dirs()?;

    let _lock = FileLock::lock(&src_tgt.target_dir)?;

    if !moonutil::common::check_moon_mod_exists(&src_tgt.source_dir) {
        bail!("could not find `{}`", MOON_MOD_JSON);
    }

    if src_tgt.target_dir.is_dir() {
        std::fs::remove_dir_all(&src_tgt.target_dir)
            .context("failed to remove target directory")?;
    }

    // Also remove the legacy "target" symlink/junction/directory if it exists
    let legacy_target_path = src_tgt.source_dir.join(LEGACY_BUILD_DIR);
    #[cfg(unix)]
    if legacy_target_path.is_symlink() {
        std::fs::remove_file(&legacy_target_path)
            .context("failed to remove legacy target symlink")?;
    } else if legacy_target_path.is_dir() {
        // Handle case where "target" is an actual directory (e.g., from older moon versions)
        std::fs::remove_dir_all(&legacy_target_path)
            .context("failed to remove legacy target directory")?;
    }
    #[cfg(windows)]
    if legacy_target_path.is_symlink() {
        // Directory symlinks are removed with remove_dir on Windows
        std::fs::remove_dir(&legacy_target_path)
            .context("failed to remove legacy target symlink")?;
    } else if junction::exists(&legacy_target_path).unwrap_or(false) {
        // NTFS junctions are removed with junction::delete
        junction::delete(&legacy_target_path).context("failed to remove legacy target junction")?;
    } else if legacy_target_path.is_dir() {
        // Handle case where "target" is an actual directory (e.g., from older moon versions)
        std::fs::remove_dir_all(&legacy_target_path)
            .context("failed to remove legacy target directory")?;
    }

    Ok(0)
}
