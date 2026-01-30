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

use anyhow::{Context, bail};
use colored::Colorize;
use moonbuild_rupes_recta::{ResolveConfig, intent::UserIntent, model::BuildPlanNode};
use mooncake::pkg::install::InstallSubcommand;
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
    let InstallSubcommand {
        package_path,
        path,
        bin,
    } = cmd;
    let install_dir = bin.unwrap_or_else(|| moon_dir::home().join("mooncakes_bin"));

    if let Some(path) = path {
        return install_local_package(cli, &path, &install_dir);
    }
    if let Some(package_path) = package_path {
        return install_registry_package(cli, &package_path, &install_dir);
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

fn install_registry_package(
    cli: UniversalFlags,
    package_path: &str,
    install_dir: &Path,
) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("--dry-run is not supported for `moon install <package>`");
    }

    let target = parse_install_path(package_path)?;
    let version = resolve_package_version(&cli, &target.module, target.version)?;
    let temp_dir = TempDir::new().context("failed to create temp directory for install")?;
    let pkg_dir = temp_dir.path().join("pkg");

    let registry = OnlineRegistry::mooncakes_io();
    registry.install_to(&target.module, &version, &pkg_dir, cli.quiet)?;
    install_from_pkg_dir(&cli, &pkg_dir, &target.module, install_dir)
}

fn install_local_package(
    cli: UniversalFlags,
    path: &Path,
    install_dir: &Path,
) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("--dry-run is not supported for `moon install --path`");
    }

    let pkg_dir = dunce::canonicalize(path).context("failed to resolve install path")?;
    let moon_mod = read_module_desc_file_in_dir(&pkg_dir)?;
    let module_name = parse_module_name(&moon_mod.name).with_context(|| {
        format!(
            "local module name `{}` must be in the form of <author>/<package_name>",
            moon_mod.name
        )
    })?;

    install_from_pkg_dir(&cli, &pkg_dir, &module_name, install_dir)
}

fn install_from_pkg_dir(
    cli: &UniversalFlags,
    pkg_dir: &Path,
    module_name: &ModuleName,
    install_dir: &Path,
) -> anyhow::Result<i32> {
    let moon_mod = read_module_desc_file_in_dir(pkg_dir)?;
    match moon_mod.preferred_target {
        Some(preferred) if preferred != TargetBackend::Native => {
            bail!(
                "package {} prefers target `{}`, but `moon install` only supports native",
                module_name,
                preferred.to_flag()
            );
        }
        _ => {}
    }

    let mut failures = Vec::new();
    let target_dir = pkg_dir.join(BUILD_DIR);
    std::fs::create_dir_all(&target_dir)
        .context("failed to create target directory for install")?;

    let resolve_cfg = ResolveConfig::new_with_load_defaults(false, false, false);
    let resolve_output = moonbuild_rupes_recta::resolve(&resolve_cfg, pkg_dir)?;
    let local_modules = resolve_output.local_modules();
    let &[main_module_id] = local_modules else {
        bail!(
            "expected exactly one main module when installing {}",
            module_name
        );
    };

    let packages = resolve_output
        .pkg_dirs
        .packages_for_module(main_module_id)
        .ok_or_else(|| anyhow::anyhow!("cannot find packages for module {}", module_name))?;
    let mut selected_pkgs = Vec::new();
    for &pkg_id in packages.values() {
        let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
        if pkg.raw.is_main {
            selected_pkgs.push(pkg_id);
        }
    }
    if selected_pkgs.is_empty() {
        bail!("no `is_main` packages found in {}", module_name);
    }

    for pkg_id in selected_pkgs {
        if let Err(err) =
            build_and_install_pkg(cli, &resolve_output, pkg_id, &target_dir, install_dir)
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
                module_name,
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
    pkg_name: &ModuleName,
    version: Option<Version>,
) -> anyhow::Result<Version> {
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

    let registry = OnlineRegistry::mooncakes_io();
    let version = if let Some(version) = version {
        version
    } else {
        let latest_version = registry
            .get_latest_version(pkg_name)
            .and_then(|m| m.version.clone())
            .ok_or_else(|| {
                if index_updated {
                    anyhow::anyhow!("could not find the latest version of {pkg_name}")
                } else {
                    anyhow::anyhow!(
                        "could not find the latest version of {pkg_name} (registry index update failed)"
                    )
                }
            })?;
        if !cli.quiet {
            println!("Latest version of {pkg_name} is {latest_version}");
        }
        latest_version
    };

    Ok(version)
}

fn parse_install_path(input: &str) -> anyhow::Result<ParsedInstallPath> {
    let parts: Vec<&str> = input.splitn(2, '@').collect();
    let path_part = parts[0];
    let version = if parts.len() == 2 {
        Some(parts[1].parse()?)
    } else {
        None
    };

    let module: ModuleName = path_part.parse().map_err(|_| {
        anyhow::anyhow!("module path must be in the form of <author>/<module>[@<version>]")
    })?;
    if module.username.is_empty() || module.unqual.is_empty() || module.unqual.contains('/') {
        bail!("module path must be in the form of <author>/<module>[@<version>]");
    }
    Ok(ParsedInstallPath { module, version })
}

fn parse_module_name(input: &str) -> anyhow::Result<ModuleName> {
    let module: ModuleName = input
        .parse()
        .map_err(|_| anyhow::anyhow!("module name must be in the form of <author>/<module>"))?;
    if module.username.is_empty() || module.unqual.is_empty() || module.unqual.contains('/') {
        bail!("module name must be in the form of <author>/<module>");
    }
    Ok(module)
}

struct ParsedInstallPath {
    module: ModuleName,
    version: Option<Version>,
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
        bail!("module {} does not support native target", pkg.fqn);
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
    let has_exe_suffix = file_name.to_ascii_lowercase().ends_with(".exe");
    if cfg!(windows) {
        if !has_exe_suffix {
            file_name.push_str(".exe");
        }
    } else if has_exe_suffix {
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
