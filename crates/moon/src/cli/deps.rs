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

use anyhow::{Context as _, bail};
use colored::Colorize;
use moonbuild_rupes_recta::{ResolveConfig, intent::UserIntent, model::BuildPlanNode};
use mooncake::pkg::{
    add::AddSubcommand, install::InstallSubcommand, remove::RemoveSubcommand, tree::TreeSubcommand,
};
use mooncake::registry::{OnlineRegistry, Registry};
use moonutil::{
    common::{BUILD_DIR, RunMode, TargetBackend, read_module_desc_file_in_dir},
    dirs::PackageDirs,
    moon_dir,
    mooncakes::{ModuleName, RegistryConfig, sync::AutoSyncFlags},
};
use semver::Version;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::UniversalFlags;
use crate::cli::BuildFlags;
use crate::rr_build::{self, BuildConfig, plan_build_from_resolved, preconfig_compile};

pub fn install_cli(cli: UniversalFlags, cmd: InstallSubcommand) -> anyhow::Result<i32> {
    if let Some(package_path) = cmd.package_path {
        return install_package(cli, &package_path);
    }

    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;
    eprintln!(
        "{}: `moon install` without arguments is deprecated; use `moon build` instead.",
        "Warning".yellow().bold()
    );
    mooncake::pkg::install::install(&source_dir, &target_dir, cli.quiet, cli.verbose, true)
}

fn install_package(cli: UniversalFlags, package_path: &str) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("--dry-run is not supported for `moon install <package>`");
    }

    let (pkg_name, version) = resolve_package_version(&cli, package_path)?;
    let temp_dir = TempDir::new().context("failed to create temp directory for install")?;
    let pkg_dir = temp_dir.path().join("pkg");

    let registry = OnlineRegistry::mooncakes_io();
    registry.install_to(&pkg_name, &version, &pkg_dir, cli.quiet)?;

    let moon_mod = read_module_desc_file_in_dir(&pkg_dir)?;
    match moon_mod.preferred_target {
        Some(preferred) if preferred != TargetBackend::Native => {
            bail!(
                "package {} prefers target `{}`, but `moon install` only supports native",
                pkg_name,
                preferred.to_flag()
            );
        }
        _ => {}
    }

    let mut failures = Vec::new();
    let install_dir = moon_dir::bin();
    let target_dir = pkg_dir.join(BUILD_DIR);
    std::fs::create_dir_all(&target_dir)
        .context("failed to create target directory for install")?;

    let resolve_cfg = ResolveConfig::new_with_load_defaults(false, false, false);
    let resolve_output = moonbuild_rupes_recta::resolve(&resolve_cfg, &pkg_dir)?;
    let local_modules = resolve_output.local_modules();
    let &[main_module_id] = local_modules else {
        bail!(
            "expected exactly one main module when installing {}",
            pkg_name
        );
    };
    let packages = resolve_output
        .pkg_dirs
        .packages_for_module(main_module_id)
        .ok_or_else(|| anyhow::anyhow!("cannot find packages for module {}", pkg_name))?;

    let mut main_pkgs = Vec::new();
    for &pkg_id in packages.values() {
        let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
        if pkg.raw.is_main {
            main_pkgs.push(pkg_id);
        }
    }
    if main_pkgs.is_empty() {
        bail!("no `is_main` packages found in {}", pkg_name);
    }

    for pkg_id in main_pkgs {
        if let Err(err) =
            build_and_install_pkg(&cli, &resolve_output, pkg_id, &target_dir, &install_dir)
        {
            let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
            eprintln!(
                "{}: failed to install {}: {err}",
                "Warning".yellow().bold(),
                pkg.fqn
            );
            failures.push(pkg_id);
        }
    }

    if failures.is_empty() {
        if !cli.quiet {
            println!(
                "{}: installed {} to {}",
                "Success".green().bold(),
                pkg_name,
                install_dir.display()
            );
        }
        Ok(0)
    } else {
        bail!("one or more packages failed to install");
    }
}

fn resolve_package_version(
    cli: &UniversalFlags,
    package_path: &str,
) -> anyhow::Result<(ModuleName, Version)> {
    let index_dir = moon_dir::index();
    let mut index_updated = false;

    let registry_config = RegistryConfig::load();
    let had_index = index_dir.exists();
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

    let parts: Vec<&str> = package_path.splitn(2, '@').collect();
    let author_pkg: Vec<&str> = parts[0].splitn(2, '/').collect();
    if author_pkg.len() != 2 || author_pkg[0].is_empty() || author_pkg[1].is_empty() {
        bail!("package path must be in the form of <author>/<package_name>[@<version>]");
    }
    let pkg_name = ModuleName {
        username: author_pkg[0].into(),
        unqual: author_pkg[1].into(),
    };

    let registry = OnlineRegistry::mooncakes_io();
    let version = if parts.len() == 2 {
        parts[1].parse()?
    } else {
        let latest_version = registry
            .get_latest_version(&pkg_name)
            .ok_or_else(|| {
                if index_updated {
                    anyhow::anyhow!("could not find the latest version of {pkg_name}")
                } else {
                    anyhow::anyhow!(
                        "could not find the latest version of {pkg_name}. Please run `moon update` to refresh the index."
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

    Ok((pkg_name, version))
}

fn build_and_install_pkg(
    cli: &UniversalFlags,
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    pkg_id: moonbuild_rupes_recta::model::PackageId,
    target_dir: &Path,
    install_dir: &Path,
) -> anyhow::Result<()> {
    let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
    if !pkg.raw.supported_targets.contains(&TargetBackend::Native) {
        bail!("package {} does not support native target", pkg.fqn);
    }

    let build_flags = BuildFlags::default().with_target_backend(Some(TargetBackend::Native));
    let preconfig = preconfig_compile(
        &AutoSyncFlags { frozen: false },
        cli,
        &build_flags,
        target_dir,
        moonutil::cond_expr::OptLevel::Release,
        RunMode::Build,
    );
    let (build_meta, build_graph) = plan_build_from_resolved(
        preconfig,
        &cli.unstable_feature,
        target_dir,
        Box::new(|_, _| Ok(vec![UserIntent::Build(pkg_id)].into())),
        resolve_output.clone(),
    )?;

    rr_build::generate_all_pkgs_json(target_dir, &build_meta, RunMode::Build)?;
    let build_cfg = BuildConfig::from_flags(&build_flags, &cli.unstable_feature, cli.verbose);
    let result = rr_build::execute_build(&build_cfg, build_graph, target_dir)?;
    result.print_info(cli.quiet, "building")?;
    if !result.successful() {
        bail!("build failed");
    }

    let execs = collect_executables(&build_meta, pkg_id)?;
    let base_name = pkg
        .raw
        .bin_name
        .as_deref()
        .unwrap_or_else(|| pkg.fqn.short_alias());
    for exec_path in execs {
        install_executable(&exec_path, base_name, install_dir)?;
    }
    Ok(())
}

fn collect_executables(
    build_meta: &rr_build::BuildMeta,
    pkg_id: moonbuild_rupes_recta::model::PackageId,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut execs = Vec::new();
    for (node, arts) in &build_meta.artifacts {
        match node {
            BuildPlanNode::MakeExecutable(target) if target.package == pkg_id => {
                execs.extend(arts.artifacts.iter().cloned());
            }
            _ => {}
        }
    }
    if execs.is_empty() {
        bail!("no executable artifacts found for package");
    }
    Ok(execs)
}

fn install_executable(src: &Path, base_name: &str, install_dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(install_dir).context("failed to create install directory")?;

    let mut file_name = base_name.to_string();
    if let Some(ext) = src.extension().and_then(|s| s.to_str()) {
        let expected = format!(".{ext}");
        let keep_ext = !ext.eq_ignore_ascii_case("exe") || cfg!(windows);
        if keep_ext && !file_name.ends_with(&expected) {
            file_name.push_str(&expected);
        }
    }
    if !cfg!(windows) && file_name.to_ascii_lowercase().ends_with(".exe") {
        let new_len = file_name.len() - 4;
        file_name.truncate(new_len);
    }
    let dest = install_dir.join(file_name);
    std::fs::copy(src, &dest).with_context(|| {
        format!(
            "failed to copy executable from {} to {}",
            src.display(),
            dest.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
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

pub fn tree_cli(cli: UniversalFlags, _cmd: TreeSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;
    mooncake::pkg::tree::tree(&source_dir, &target_dir)
}
