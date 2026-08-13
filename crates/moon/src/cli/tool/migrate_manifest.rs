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

use std::path::PathBuf;

use anyhow::{Context, ensure};

/// Format a legacy JSON manifest, install its replacement, and remove the old file.
#[derive(Debug, clap::Parser)]
pub(crate) struct MigrateManifestSubcommand {
    /// Legacy manifest consumed by the formatter.
    #[clap(long)]
    old: PathBuf,

    /// New source manifest installed beside the legacy manifest.
    #[clap(long)]
    dest: PathBuf,
}

pub(crate) fn run_migrate_manifest(cmd: MigrateManifestSubcommand) -> anyhow::Result<i32> {
    ensure!(
        !cmd.dest.exists(),
        "refusing to overwrite existing manifest {}",
        cmd.dest.display()
    );
    let dest_dir = cmd
        .dest
        .parent()
        .context("migrated manifest destination should have a parent directory")?;
    let temp_dir = tempfile::Builder::new()
        .prefix(".moon-manifest-migration-")
        .tempdir_in(dest_dir)
        .context("failed to create temporary directory for manifest migration")?;
    let formatted = temp_dir.path().join(
        cmd.dest
            .file_name()
            .context("migrated manifest destination should have a file name")?,
    );
    let status = std::process::Command::new(&*moonutil::toolchain::BINARIES.moonfmt)
        .arg(&cmd.old)
        .arg("-o")
        .arg(&formatted)
        .status()
        .context("failed to run moonfmt while migrating a manifest")?;
    if !status.success() {
        return Ok(status.code().unwrap_or(1));
    }

    std::fs::rename(&formatted, &cmd.dest).with_context(|| {
        format!(
            "failed to install migrated manifest at {}",
            cmd.dest.display()
        )
    })?;
    if let Err(source) = std::fs::remove_file(&cmd.old) {
        let _ = std::fs::remove_file(&cmd.dest);
        return Err(source)
            .with_context(|| format!("failed to remove legacy manifest {}", cmd.old.display()));
    }
    Ok(0)
}
