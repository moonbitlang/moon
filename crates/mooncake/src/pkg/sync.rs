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

//! Sync dependencies with mod/pkg definition

use std::path::Path;

use moonutil::mooncakes::{
    result::ResolvedEnv, sync::AutoSyncFlags, DirSyncResult, RegistryConfig,
};

use crate::dep_dir::resolve_dep_dirs;

pub fn auto_sync(
    source_dir: &Path,
    cli: &AutoSyncFlags,
    registry_config: &RegistryConfig,
    quiet: bool,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult)> {
    let (resolved_env, dep_dir) =
        super::install::install_impl(source_dir, registry_config, quiet, cli.dont_sync())?;
    let dir_sync_result = resolve_dep_dirs(&dep_dir, &resolved_env);
    log::debug!("Dir sync result: {:?}", dir_sync_result);
    Ok((resolved_env, dir_sync_result))
}
