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

use anyhow::bail;
use mooncake::update::{RegistryIndexRecloneReason, RegistryIndexUpdate, UpdateOutcome};
use moonutil::{registry::RegistryConfig, user_log::UserLog};

use super::UniversalFlags;

/// Update the package registry index
#[derive(Debug, clap::Parser)]
pub(crate) struct UpdateSubcommand {}

pub(crate) fn update_cli(
    cli: UniversalFlags,
    _cmd: UpdateSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not supported for update")
    }
    let registry_config = RegistryConfig::load();
    let target_dir = moonutil::registry::index();
    let outcome = mooncake::update::update(&target_dir, &registry_config, user_log)?;
    log_registry_update(outcome, user_log);
    Ok(0)
}

pub(crate) fn log_registry_update(outcome: UpdateOutcome, user_log: &UserLog) {
    match outcome.registry_index {
        RegistryIndexUpdate::Cloned => user_log.status("Registry index cloned successfully"),
        RegistryIndexUpdate::Updated => user_log.status("Registry index updated successfully"),
        RegistryIndexUpdate::Recloned(reason) => {
            let reason = match reason {
                RegistryIndexRecloneReason::PullFailed => "Failed to update registry index",
                RegistryIndexRecloneReason::RemoteMismatch => {
                    "Registry index remote does not match the configured URL"
                }
                RegistryIndexRecloneReason::NotGitRepository => {
                    "Registry index is not a Git repository"
                }
                RegistryIndexRecloneReason::MissingOrigin => "Registry index has no origin remote",
            };
            user_log.status(format!("{reason}, re-cloning"));
            user_log.status("Registry index re-cloned successfully");
        }
        RegistryIndexUpdate::ConcurrentUpdateReused => {
            user_log.status("Registry update already completed by another process");
            return;
        }
    }
    if outcome.symbols_updated {
        user_log.status("Symbols updated successfully");
    }
}

#[cfg(test)]
mod tests {
    use log::LevelFilter;
    use moonutil::user_log::UserLogEntryLevel;

    use super::*;

    #[test]
    fn registry_update_status_respects_quiet_user_log() {
        let (user_log, capture) = UserLog::captured(LevelFilter::Error);

        log_registry_update(
            UpdateOutcome {
                registry_index: RegistryIndexUpdate::Cloned,
                symbols_updated: true,
            },
            &user_log,
        );

        assert!(capture.take().is_empty());
    }

    #[test]
    fn registry_update_status_describes_reclone() {
        let (user_log, capture) = UserLog::captured(LevelFilter::Warn);

        log_registry_update(
            UpdateOutcome {
                registry_index: RegistryIndexUpdate::Recloned(
                    RegistryIndexRecloneReason::NotGitRepository,
                ),
                symbols_updated: true,
            },
            &user_log,
        );

        let entries = capture.take();
        assert_eq!(entries.len(), 3);
        assert!(
            entries
                .iter()
                .all(|entry| matches!(entry.level, UserLogEntryLevel::Info))
        );
        assert_eq!(
            entries
                .into_iter()
                .map(|entry| entry.message)
                .collect::<Vec<_>>(),
            [
                "Registry index is not a Git repository, re-cloning",
                "Registry index re-cloned successfully",
                "Symbols updated successfully",
            ]
        );
    }

    #[test]
    fn registry_update_status_describes_concurrent_reuse() {
        let (user_log, capture) = UserLog::captured(LevelFilter::Warn);

        log_registry_update(
            UpdateOutcome {
                registry_index: RegistryIndexUpdate::ConcurrentUpdateReused,
                symbols_updated: true,
            },
            &user_log,
        );

        assert_eq!(
            capture
                .take()
                .into_iter()
                .map(|entry| entry.message)
                .collect::<Vec<_>>(),
            ["Registry update already completed by another process"]
        );
    }
}
