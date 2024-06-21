use anyhow::bail;

use super::UniversalFlags;

pub fn run_upgrade(cli: UniversalFlags) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not implemented for upgrade")
    }
    moonbuild::upgrade::upgrade()
}
