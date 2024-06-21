use anyhow::bail;
use moonutil::{
    dirs::PackageDirs,
    mooncakes::{ModuleName, RegistryConfig},
};

use super::UniversalFlags;

/// Install dependencies
#[derive(Debug, clap::Parser)]
pub struct InstallSubcommand {}

/// Remove a dependency
#[derive(Debug, clap::Parser)]
pub struct RemoveSubcommand {
    /// The package path to remove
    pub package_path: String,
}

/// Add a dependency
#[derive(Debug, clap::Parser)]
pub struct AddSubcommand {
    /// The package path to add
    pub package_path: String,
}

/// Display the dependency tree
#[derive(Debug, clap::Parser)]
pub struct TreeSubcommand {}

pub fn install_cli(cli: UniversalFlags, _cmd: InstallSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;
    let registry_config = RegistryConfig::load();
    mooncake::pkg::install::install(&source_dir, &target_dir, &registry_config, false)
}

pub fn remove_cli(cli: UniversalFlags, cmd: RemoveSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;
    let package_path = cmd.package_path;
    let parts: Vec<&str> = package_path.splitn(2, '/').collect();
    if parts.len() != 2 {
        bail!("package path must be in the form of <author>/<package_name>");
    }
    let username = parts[0];
    let pkgname = parts[1];
    let registry_config = RegistryConfig::load();
    mooncake::pkg::remove::remove(
        &source_dir,
        &target_dir,
        username,
        pkgname,
        &registry_config,
    )
}

pub fn add_cli(cli: UniversalFlags, cmd: AddSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;
    let package_path = cmd.package_path;

    let parts: Vec<&str> = package_path.splitn(2, '@').collect();
    if parts.len() == 2 {
        let version: &str = parts[1];
        let version = version.parse()?;

        let parts: Vec<&str> = parts[0].splitn(2, '/').collect();
        if parts.len() != 2 {
            bail!("package path must be in the form of <author>/<package_name>[@<version>]");
        }
        let username = parts[0];
        let pkgname = parts[1];

        let registry_config = RegistryConfig::load();
        let name = ModuleName {
            username: username.to_string(),
            pkgname: pkgname.to_string(),
        };
        mooncake::pkg::add::add(
            &source_dir,
            &target_dir,
            &name,
            &version,
            &registry_config,
            false,
        )
    } else {
        let parts: Vec<&str> = parts[0].splitn(2, '/').collect();
        if parts.len() < 2 {
            bail!("package path must be in the form of <author>/<package_name>[@<version>]");
        }
        let username = parts[0];
        let pkgname = parts[1];

        let registry_config = RegistryConfig::load();
        mooncake::pkg::add::add_latest(
            &source_dir,
            &target_dir,
            username,
            pkgname,
            &registry_config,
            false,
        )
    }
}

pub fn tree_cli(cli: UniversalFlags, _cmd: TreeSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;
    mooncake::pkg::tree::tree(&source_dir, &target_dir)
}
