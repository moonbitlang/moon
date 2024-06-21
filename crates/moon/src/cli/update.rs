use anyhow::bail;
use moonutil::mooncakes::RegistryConfig;

use super::UniversalFlags;

/// Update the package registry index
#[derive(Debug, clap::Parser)]
pub struct UpdateSubcommand {}

pub fn update_cli(cli: UniversalFlags, _cmd: UpdateSubcommand) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not implemented for update")
    }
    let registry_config = RegistryConfig::load();
    let target_dir = moonutil::moon_dir::index();
    mooncake::update::update(&target_dir, &registry_config)
}
