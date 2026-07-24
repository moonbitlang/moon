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

//! Utility to build and install a binary dependency module.
//!
//! This was originally `mooncake::pkg::install::build_and_install_bin_package`.
//! However, due to the sheer amount of hacks originally needed to make the
//! thing work, and since `mooncake` can't access `moon`'s entry points, the
//! behavior is instead modelled as a subcommand of `moon` itself.
//
// MAINTAINERS: Yes, I only use the RR backend for this usage. Legacy backend
// would mean a lot of duplicated code and maintenance burden.
//
// TODO: Check if `moon::cli::build`'s related code are still used. If not,
// they can be deleted for simplification.

use std::path::{Path, PathBuf};

use anyhow::Context;
use moonbuild_rupes_recta::{
    ResolveConfig, discover::DiscoveredPackage, intent::UserIntent, model::PackageId,
};
use moonutil::{
    build_options::RunMode, cli_support::AutoSyncFlags, cli_support::UniversalFlags,
    locks::FileLock, project::PackageDirs, target::TargetBackend, user_log::UserLog,
};

use crate::{
    cli::BuildFlags,
    filter::match_packages_by_name_rr,
    rr_build::{self, BuildConfig, BuildMeta, preconfig_compile},
};

#[derive(clap::Args, Debug)]
#[clap(group = clap::ArgGroup::new("package_selector").multiple(false))]
pub(crate) struct BuildBinaryDepArgs {
    /// The name of the package to build and install, without the module name prefix.
    /// The top-level package can be specified as an empty string.
    #[clap(group = "package_selector")]
    pkg_names: Vec<String>,

    /// Whether to build and install all binary packages in the module.
    #[clap(long, group = "package_selector")]
    all_pkgs: bool,

    /// The parent directory where the binary module is installed to.
    #[clap(long)]
    install_path: PathBuf,
}

pub(crate) fn run_build_binary_dep(
    cli: &UniversalFlags,
    cmd: &BuildBinaryDepArgs,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    let dirs = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .package_dirs()?;
    let PackageDirs {
        target_dir,
        mooncake_bin_dir,
        ..
    } = &dirs;
    if cli.dry_run {
        anyhow::bail!("--dry-run is not supported for `moon tool build-binary-dep`");
    }

    // bin-deps have their build target determined in `moon.pkg.json`, so we
    // must resolve the packages before settling on the build config and then
    // running the build plan.
    // A bin-dep is already being built as a tool in its own isolated project.
    // Its own bin-deps are not transitive inputs to that tool build.
    let resolve_cfg =
        ResolveConfig::new_with_load_defaults(false, false, false, cli.workspace_env.clone())
            .without_bin_deps();
    let synced_env = moonbuild_rupes_recta::sync_dependencies(&resolve_cfg, &dirs)?;
    let resolve_output =
        moonbuild_rupes_recta::resolve_synced_project(&resolve_cfg, synced_env, user_log)?;

    // Note: There's a cyclic dependency!
    //
    // We need to know the target backend in order to find linkable packages,
    // but the preferred target backend for each package is stored in its
    // `bin_target` field, which is only known after resolution.
    //
    // To break the cycle, our strategy is to check if each package is linkable
    // in its own `bin_target`, and if not present, fall back to the main
    // module's preferred target backend (or default backend if not specified).
    let &[main_module_id] = resolve_output.local_modules() else {
        panic!("Expected exactly one main module when building all packages");
    };
    let main_module_ref = resolve_output.module_info(main_module_id);
    let default_backend = main_module_ref.preferred_target.unwrap_or_default();

    // Okay let's filter the packages
    let pkgs = if cmd.all_pkgs {
        let packages = resolve_output
            .pkg_dirs
            .packages_for_module(main_module_id)
            .ok_or_else(|| anyhow::anyhow!("Cannot find the local module!"))?;
        get_linkable_pkgs_for_bin_dep(
            &resolve_output,
            packages.values().cloned(),
            default_backend,
            user_log,
        )
    } else {
        let mut result_pkgs = vec![];
        for pkg_name in cmd.pkg_names.iter() {
            let pkgs = match_packages_by_name_rr(
                &resolve_output,
                resolve_output.local_modules(),
                pkg_name,
                user_log,
            );
            for pkg in pkgs {
                let pkg_ref = resolve_output.pkg_dirs.get_package(pkg);
                let pkg_bin_target = pkg_ref.raw.bin_target.unwrap_or(default_backend);
                add_bin_dep(&mut result_pkgs, pkg, pkg_ref, pkg_bin_target, user_log);
            }
        }
        result_pkgs
    };

    // For each package we need to get its target backend and then we can build it
    for (pkg, target) in pkgs {
        // Get package info
        let package = &*resolve_output.pkg_dirs.get_package(pkg).raw;
        let bin_name = package.bin_name.as_deref();

        let build_flags = BuildFlags {
            release: true,
            ..BuildFlags::default()
        };
        let preconfig = preconfig_compile(
            &AutoSyncFlags { frozen: false },
            cli,
            &build_flags,
            Some(target),
            target_dir,
            RunMode::Build,
        );
        let planning_context = rr_build::prepare_resolved_build(
            &preconfig,
            &cli.unstable_feature,
            target_dir,
            user_log,
            &resolve_output,
        )?;
        let intent = vec![UserIntent::Build(pkg)].into();
        let (build_meta, build_graph) = rr_build::plan_resolved_build_from_intent(
            preconfig,
            &cli.unstable_feature,
            user_log,
            planning_context,
            intent,
            mooncake_bin_dir,
            // FIXME: cloning is not the best way to do this, it takes in this
            // type only to be returned in build meta. We should refactor later.
            resolve_output.clone(),
        )?;

        let _lock = FileLock::lock(target_dir)?;
        // Generate all_pkgs.json for indirect dependency resolution
        rr_build::generate_all_pkgs_json(&build_meta)?;

        let result = rr_build::execute_build(
            &BuildConfig::from_flags(&build_flags, &cli.unstable_feature, cli.verbose),
            build_graph,
            target_dir,
            user_log,
        )?;
        if !result.successful() {
            return Ok(result.return_code_for_success());
        }

        install_build_rr(&build_meta, &cmd.install_path, bin_name)?;
    }

    Ok(0)
}

fn get_linkable_pkgs_for_bin_dep(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    packages: impl Iterator<Item = PackageId>,
    default_backend: TargetBackend,
    user_log: &UserLog,
) -> Vec<(PackageId, TargetBackend)> {
    let mut linkable_pkgs = vec![];
    for pkg_id in packages {
        let pkg = resolve_output.pkg_dirs.get_package(pkg_id);
        let pkg_bin_target = pkg.raw.bin_target.unwrap_or(default_backend);

        add_bin_dep(&mut linkable_pkgs, pkg_id, pkg, pkg_bin_target, user_log);
    }
    linkable_pkgs
}

fn add_bin_dep(
    linkable_pkgs: &mut Vec<(PackageId, TargetBackend)>,
    pkg_id: PackageId,
    pkg: &DiscoveredPackage,
    pkg_bin_target: TargetBackend,
    user_log: &UserLog,
) {
    if pkg.raw.force_link
        || pkg
            .raw
            .link
            .as_ref()
            .is_some_and(|link| link.need_link(pkg_bin_target))
        || pkg.raw.is_main
    {
        linkable_pkgs.push((pkg_id, pkg_bin_target))
    } else if pkg.raw.bin_target.is_some() {
        user_log.warn(format!(
            "Package {} has bin_target set, but cannot be linked; skipping",
            pkg.fqn
        ));
    }
}

/// Install a built binary artifact into the requested output directory.
fn install_build_rr(
    meta: &BuildMeta,
    install_dir: &Path,
    bin_name: Option<&str>,
) -> anyhow::Result<()> {
    // Assume one artifact node and one artifact file
    let (_node, arts) = meta.artifacts.get_index(0).unwrap();
    let artifact = arts
        .artifacts
        .first()
        .context("RR build should yield exactly one artifact file")?;

    // Determine filename
    // Matching legacy, it uses the following fallbacks:
    // - provided bin_name
    // - artifact file stem
    // - "moonbin" (???)
    let name = bin_name
        .or_else(|| artifact.file_stem().and_then(|s| s.to_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "moonbin".to_string());

    // The build target may be temporary. Copy the runnable artifact into the
    // installation tree before creating a launcher that refers to it.
    let artifact_name = artifact
        .file_name()
        .context("binary dependency artifact path has no file name")?;
    let artifact_dir = install_dir.join(".artifacts").join(&name);
    std::fs::create_dir_all(&artifact_dir)?;
    let installed_artifact = crate::cli::install_binary::install_file_atomically(
        artifact,
        &artifact_dir,
        artifact_name,
    )?;

    // Build command using existing runtime mapping, then shlex-join
    let guard = crate::run::command_for(
        meta.target_backend,
        meta.tcc_run.as_ref(),
        &installed_artifact,
        None,
    );
    let parts = std::iter::once(guard.as_std().get_program())
        .chain(guard.as_std().get_args())
        .map(|x| x.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let line = shlex::try_join(parts.iter().map(|s| &**s))
        .expect("unexpected null byte in args when forming exec command");

    // Write a minimal launcher script
    #[cfg(unix)]
    {
        let path = install_dir.join(&name);
        std::fs::create_dir_all(install_dir)?;
        let script = format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nexec {line} \"$@\"\n",
            line = line
        );
        std::fs::write(&path, script)?;
        // chmod 0755
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))?;
    }
    #[cfg(windows)]
    {
        let name_ps1 = if name.to_ascii_lowercase().ends_with(".ps1") {
            name
        } else {
            format!("{name}.ps1")
        };
        let path = install_dir.join(name_ps1);
        std::fs::create_dir_all(install_dir)?;
        let script = format!(
            "$ErrorActionPreference = \"Stop\"\n& {line} $Args\n",
            line = line
        );
        std::fs::write(&path, script)?;
    }
    #[cfg(not(any(unix, windows)))]
    {
        return Err(anyhow!(
            "Installing build artifacts is not supported on this platform"
        ));
    }

    Ok(())
}
