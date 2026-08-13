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
use mooncake::registry::RegistryClient;
use moonutil::user_log::UserLog;

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
    RegistryClient::configured().sync(user_log)?;
    Ok(0)
}
