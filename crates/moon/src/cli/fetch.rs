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
use colored::Colorize;
use mooncake::registry::Registry;
use moonutil::mooncakes::{ModuleName, RegistryConfig};

use super::UniversalFlags;

#[derive(Debug, clap::Parser)]
pub struct FetchSubcommand {
    /// The package to fetch in the form of <author>/<package_name>[@<version>]
    pub package_path: String,

    /// Do not update the registry index before fetching
    #[clap(long)]
    pub no_update: bool,
}

pub fn fetch_cli(cli: UniversalFlags, cmd: FetchSubcommand) -> anyhow::Result<i32> {
    let index_dir = moonutil::moon_dir::index();
    let mut index_updated = false;

    if !cmd.no_update {
        let had_index = index_dir.exists();
        let registry_config = RegistryConfig::load();
        match mooncake::update::update(&index_dir, &registry_config) {
            Ok(_) => index_updated = true,
            Err(e) => {
                if had_index {
                    eprintln!(
                        "{}: failed to update registry index, continuing with existing index: {e}",
                        "Warning".yellow().bold(),
                    );
                } else {
                    return Err(e);
                }
            }
        }
    }

    let package_path = cmd.package_path;
    let parts: Vec<&str> = package_path.splitn(2, '@').collect();

    let author_pkg: Vec<&str> = parts[0].splitn(2, '/').collect();
    if author_pkg.len() != 2 {
        bail!("package path must be in the form of <author>/<package_name>[@<version>]");
    }
    let username = author_pkg[0];
    let pkgname = author_pkg[1];
    let pkg_name = ModuleName {
        username: username.into(),
        unqual: pkgname.into(),
    };

    let registry = mooncake::registry::OnlineRegistry::mooncakes_io();

    let version = if parts.len() == 2 {
        let version_str = parts[1];
        version_str.parse()?
    } else {
        let latest_version = registry
            .get_latest_version(&pkg_name)
            .ok_or_else(|| {
                if index_updated {
                    anyhow::anyhow!("could not find the latest version of {pkg_name}")
                } else {
                    anyhow::anyhow!(
                        "could not find the latest version of {pkg_name}. Please consider running `moon update` to update the index."
                    )
                }
            })?
            .version
            .clone()
            .unwrap();
        if !cli.quiet {
            println!("Latest version of {pkg_name} is {latest_version}");
        }
        latest_version
    };

    let repo_dir = std::env::current_dir()?.join(".repo");
    let pkg_dir = repo_dir
        .join(username)
        .join(pkgname)
        .join(version.to_string());

    if pkg_dir.exists() {
        if !cli.quiet {
            println!(
                "{}: {}@{version} already exists at {}",
                "Info".green().bold(),
                pkg_name,
                pkg_dir.display()
            );
        }
        return Ok(0);
    }

    if !cli.quiet {
        println!("Fetching {}@{version} to {}", pkg_name, pkg_dir.display());
    }

    registry.install_to(&pkg_name, &version, &pkg_dir, cli.quiet)?;

    if !cli.quiet {
        println!(
            "{}: Successfully fetched {}@{version} to {}",
            "Success".green().bold(),
            pkg_name,
            pkg_dir.display()
        );
    }

    Ok(0)
}
