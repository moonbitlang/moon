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
