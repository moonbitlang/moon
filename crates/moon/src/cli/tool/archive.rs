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

//! Hidden archive runner for build-system generated commands.

use std::{ffi::OsString, path::Path, path::PathBuf, process::Command};

use anyhow::{Context, bail};
use moonutil::cli_support::UniversalFlags;

/// Create a static archive from scratch with an archiver that updates
/// existing archives in place, such as `ar -r`.
///
/// Such archivers keep members that are no longer listed, so this deletes the
/// archive before running the archiver.
#[derive(Debug, clap::Parser)]
pub(crate) struct ArchiveSubcommand {
    /// The archive that the archiver command creates.
    #[clap(long)]
    output: PathBuf,

    /// The archiver command and its arguments.
    #[clap(
        last = true,
        required = true,
        allow_hyphen_values = true,
        value_name = "ARCHIVER"
    )]
    archiver: Vec<OsString>,
}

pub(crate) fn run_archive(cli: &UniversalFlags, cmd: ArchiveSubcommand) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not supported for tool archive");
    }

    remove_stale_archive(&cmd.output)?;
    let (program, args) = cmd
        .archiver
        .split_first()
        .expect("clap requires an archiver command");
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("failed to run archiver {}", program.to_string_lossy()))?;
    // An archiver terminated by a signal has no exit code.
    Ok(status.code().unwrap_or(1))
}

fn remove_stale_archive(path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(error).with_context(|| format!("failed to remove stale archive {}", path.display()))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn archiver_arguments_follow_the_separator() {
        let cmd = ArchiveSubcommand::try_parse_from([
            "archive", "--output", "lib.a", "--", "ar", "-r", "-c", "-s", "lib.a", "a.o",
        ])
        .expect("archive arguments should parse");

        assert_eq!(cmd.output, PathBuf::from("lib.a"));
        assert_eq!(cmd.archiver, ["ar", "-r", "-c", "-s", "lib.a", "a.o"]);
    }

    #[test]
    #[allow(clippy::disallowed_methods)] // File writes only set up the stale archive.
    fn stale_archives_are_removed_and_missing_ones_ignored() {
        let dir = tempfile::tempdir().expect("create test directory");
        let stale = dir.path().join("stale.a");
        std::fs::write(&stale, "stale").expect("write stale archive");

        remove_stale_archive(&stale).expect("a stale archive should be removed");
        assert!(!stale.exists());
        remove_stale_archive(&stale).expect("a missing archive should be ignored");
    }
}
