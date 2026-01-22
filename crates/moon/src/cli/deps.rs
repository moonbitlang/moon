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
use mooncake::pkg::{
    add::AddSubcommand, install::InstallSubcommand, remove::RemoveSubcommand, tree::TreeSubcommand,
};
use mooncake::registry::Registry;
use moonutil::{
    dirs::PackageDirs,
    moon_dir,
    mooncakes::{ModuleName, RegistryConfig},
};

use super::UniversalFlags;
use super::install_binary::{
    GitRef, install_binary, install_from_git, install_from_local, is_git_url, is_local_path,
    parse_package_spec,
};

pub fn install_cli(cli: UniversalFlags, cmd: InstallSubcommand) -> anyhow::Result<i32> {
    // If no package path and no local path, use legacy behavior
    if cmd.package_path.is_none() && cmd.path.is_none() {
        eprintln!(
            "{}: `moon install` without arguments is deprecated and will be removed in a future version. \
             Use `moon install <package>` to install binaries globally, or use `moon build` to build your project.",
            "Warning".yellow().bold()
        );
        let PackageDirs {
            source_dir,
            target_dir,
        } = cli.source_tgt_dir.try_into_package_dirs()?;
        return mooncake::pkg::install::install(
            &source_dir,
            &target_dir,
            cli.quiet,
            cli.verbose,
            true,
        );
    }

    let install_dir = cmd.bin.unwrap_or_else(moon_dir::bin);
    let has_git_ref = cmd.rev.is_some() || cmd.branch.is_some() || cmd.tag.is_some();

    // Explicit --path takes priority
    if let Some(local_path) = cmd.path {
        return install_from_local(&cli, &local_path, &install_dir);
    }

    let package_path = cmd.package_path.unwrap();

    // Local path install
    // These checks can't be done in clap because we need to inspect the value of package_path
    // to determine whether it's a local path, git URL, or registry path.
    if is_local_path(&package_path) {
        if has_git_ref {
            anyhow::bail!("--rev, --branch, and --tag can only be used with git URLs");
        }
        if cmd.package_path_in_repo.is_some() {
            anyhow::bail!("Package path in repo can only be used with git URLs");
        }
        return install_from_local(&cli, package_path.as_ref(), &install_dir);
    }

    // Git URL install
    if is_git_url(&package_path) {
        let git_ref = if let Some(rev) = cmd.rev.as_deref() {
            GitRef::Rev(rev)
        } else if let Some(branch) = cmd.branch.as_deref() {
            GitRef::Branch(branch)
        } else if let Some(tag) = cmd.tag.as_deref() {
            GitRef::Tag(tag)
        } else {
            GitRef::Default
        };
        return install_from_git(
            &cli,
            &package_path,
            git_ref,
            cmd.package_path_in_repo.as_deref(),
            &install_dir,
        );
    }

    // Registry install
    if has_git_ref {
        anyhow::bail!("--rev, --branch, and --tag can only be used with git URLs");
    }
    if cmd.package_path_in_repo.is_some() {
        anyhow::bail!("Package path in repo can only be used with git URLs");
    }
    let spec = parse_package_spec(&package_path)?;
    install_binary(&cli, &spec, &install_dir)
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

    if cmd.package_paths.is_empty() {
        bail!("at least one package path must be provided");
    }

    // Update registry index by default (issue #963).
    // - `--no-update` keeps the previous behavior.
    // - If an index already exists, update failures are treated as warnings so users can proceed
    //   with the existing local index.
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

    // Parse all package paths
    let registry = mooncake::registry::OnlineRegistry::mooncakes_io();
    let mut packages = Vec::new();
    for package_path in &cmd.package_paths {
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

        let version = if parts.len() == 2 {
            parts[1].parse()?
        } else {
            // Get latest version
            registry
                .get_latest_version(&pkg_name)
                .ok_or_else(|| {
                    if index_updated {
                        anyhow::anyhow!("could not find the latest version of {}", pkg_name.to_string())
                    } else {
                        anyhow::anyhow!(
                            "could not find the latest version of {}. Please consider running `moon update` to update the index.",
                            pkg_name.to_string()
                        )
                    }
                })?
                .version
                .clone()
                .unwrap()
        };

        packages.push((pkg_name, version));
    }

    // Use batch add if multiple packages, otherwise use the original single add
    if packages.len() > 1 {
        mooncake::pkg::add::add_batch(
            &source_dir,
            &target_dir,
            &packages,
            cmd.bin,
            cli.quiet,
        )
    } else {
        let (pkg_name, version) = &packages[0];
        mooncake::pkg::add::add(
            &source_dir,
            &target_dir,
            pkg_name,
            cmd.bin,
            version,
            cli.quiet,
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
