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
use moonutil::{
    dirs::PackageDirs,
    moon_dir,
    mooncakes::{ModuleName, RegistryConfig},
};
use std::{borrow::Cow, path::Path};

use super::UniversalFlags;
use super::install_binary::{
    GitRef, install_binary, install_from_git, install_from_local, is_git_url, is_local_path,
    parse_package_spec,
};

fn wildcard_base(s: &str) -> Option<&str> {
    s.strip_suffix("/...").or_else(|| s.strip_suffix("..."))
}

fn wildcard_local_base(source: &str) -> Option<Cow<'_, str>> {
    let base = wildcard_base(source)?;
    if base.is_empty() {
        if source.starts_with('/') {
            Some(Cow::Borrowed("/"))
        } else {
            Some(Cow::Borrowed("."))
        }
    } else if base.ends_with(':') && source.ends_with("/...") {
        // `C:/...` should resolve to `C:/` instead of drive-relative `C:`.
        Some(Cow::Owned(format!("{base}/")))
    } else {
        Some(Cow::Borrowed(base))
    }
}

pub(crate) fn install_cli(cli: UniversalFlags, cmd: InstallSubcommand) -> anyhow::Result<i32> {
    // If no package path and no local path, use legacy behavior
    if cmd.source.is_none() && cmd.path.is_none() {
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
        return install_from_local(&cli, &local_path, &install_dir, false);
    }

    let source = cmd.source.unwrap();

    // Local path install
    // These checks can't be done in clap because we need to inspect the value of source
    // to determine whether it's a local path, git URL, or registry path.
    if is_local_path(&source) {
        if has_git_ref {
            anyhow::bail!("--rev, --branch, and --tag can only be used with git URLs");
        }
        if cmd.path_in_repo.is_some() {
            anyhow::bail!("Path in repo can only be used with git URLs");
        }
        let (local_path, install_all) = if let Some(base) = wildcard_local_base(&source) {
            (base, true)
        } else {
            (Cow::Borrowed(source.as_str()), false)
        };
        return install_from_local(
            &cli,
            Path::new(local_path.as_ref()),
            &install_dir,
            install_all,
        );
    }

    // Git URL install
    if is_git_url(&source) {
        let install_all = cmd
            .path_in_repo
            .as_deref()
            .is_some_and(|s| wildcard_base(s).is_some());
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
            &source,
            git_ref,
            cmd.path_in_repo.as_deref(),
            &install_dir,
            install_all,
        );
    }

    // Registry install
    if has_git_ref {
        anyhow::bail!("--rev, --branch, and --tag can only be used with git URLs");
    }
    if cmd.path_in_repo.is_some() {
        anyhow::bail!("Path in repo can only be used with git URLs");
    }
    let spec = parse_package_spec(&source)?;
    let install_all = spec.is_wildcard;
    install_binary(&cli, &spec, &install_dir, install_all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_local_base_unix_root() {
        let got = wildcard_local_base("/...").unwrap();
        assert_eq!(got, "/");
    }

    #[test]
    fn test_wildcard_local_base_relative_current() {
        let got = wildcard_local_base("./...").unwrap();
        assert_eq!(got, ".");
    }

    #[test]
    fn test_wildcard_local_base_windows_drive_root() {
        let got = wildcard_local_base("C:/...").unwrap();
        assert_eq!(got, "C:/");
    }
}

pub(crate) fn remove_cli(cli: UniversalFlags, cmd: RemoveSubcommand) -> anyhow::Result<i32> {
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

pub(crate) fn add_cli(cli: UniversalFlags, cmd: AddSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

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

    if parts.len() == 2 {
        let version: &str = parts[1];
        let version = version.parse()?;
        mooncake::pkg::add::add(
            &source_dir,
            &target_dir,
            &pkg_name,
            cmd.bin,
            &version,
            cli.quiet,
        )
    } else {
        mooncake::pkg::add::add_latest(
            &source_dir,
            &target_dir,
            &pkg_name,
            cmd.bin,
            cli.quiet,
            index_updated,
        )
    }
}

pub(crate) fn tree_cli(cli: UniversalFlags, _cmd: TreeSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;
    mooncake::pkg::tree::tree(&source_dir, &target_dir)
}
