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
use mooncake::pkg::{
    add::AddSubcommand, install::InstallSubcommand, remove::RemoveSubcommand, tree::TreeSubcommand,
};
use moonutil::{
    dirs::PackageDirs,
    mooncakes::{ModuleName, RegistryConfig},
};

use super::UniversalFlags;

pub fn install_cli(cli: UniversalFlags, _cmd: InstallSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;
    let registry_config = RegistryConfig::load();
    mooncake::pkg::install::install(
        &source_dir,
        &target_dir,
        &registry_config,
        cli.quiet,
        cli.verbose,
    )
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

    let author_pkg: Vec<&str> = parts[0].splitn(2, '/').collect();
    if author_pkg.len() != 2 {
        bail!("package path must be in the form of <author>/<package_name>[@<version>]");
    }
    let username = author_pkg[0];
    let pkgname = author_pkg[1];
    let pkg_name = ModuleName {
        username: username.to_string(),
        pkgname: pkgname.to_string(),
    };

    if parts.len() == 2 {
        let version: &str = parts[1];
        let version = version.parse()?;
        mooncake::pkg::add::add(
            &source_dir,
            &target_dir,
            &pkg_name,
            cmd.bin,
            &version,
            false,
        )
    } else {
        mooncake::pkg::add::add_latest(&source_dir, &target_dir, &pkg_name, cmd.bin, false)
    }
}

pub fn tree_cli(cli: UniversalFlags, _cmd: TreeSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;
    mooncake::pkg::tree::tree(&source_dir, &target_dir)
}
