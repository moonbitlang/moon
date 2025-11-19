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
use moonbuild_rupes_recta::{ResolveConfig, intent::UserIntent};
use moonutil::{
    cli::UniversalFlags,
    common::{FileLock, RunMode},
    cond_expr::OptLevel,
    dirs::PackageDirs,
    mooncakes::sync::AutoSyncFlags,
};

use crate::{
    cli::{BuildFlags, get_linkable_pkgs},
    filter::match_packages_by_name_rr,
    rr_build::{self, BuildConfig, BuildMeta, plan_build_from_resolved, preconfig_compile},
};

#[derive(clap::Args, Debug)]
pub struct BuildBinaryDepArgs {
    /// The name of the package to build and install, without the module name prefix.
    /// The top-level package can be specified as an empty string.
    pkg_names: Vec<String>,

    /// Whether to build and install all binary packages in the module.
    #[clap(long, conflicts_with = "pkg_names")]
    all_pkgs: bool,

    /// The parent directory where the binary module is installed to.
    #[clap(long)]
    install_path: PathBuf,
}

pub fn run_build_binary_dep(cli: &UniversalFlags, cmd: &BuildBinaryDepArgs) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;
    if cli.dry_run {
        anyhow::bail!("--dry-run is not supported for `moon tool build-binary-dep`");
    }

    // bin-deps have their build target determined in `moon.pkg.json`, so we
    // must resolve the packages before settling on the build config and then
    // running the build plan.
    let resolve_cfg = ResolveConfig::new_with_load_defaults(false, false);
    let resolve_output = moonbuild_rupes_recta::resolve(&resolve_cfg, &source_dir)?;

    // Okay let's filter the packages
    let pkgs = if cmd.all_pkgs {
        let &[main_module_id] = resolve_output.local_modules() else {
            panic!("Expected exactly one main module when building all packages");
        };
        let packages = resolve_output
            .pkg_dirs
            .packages_for_module(main_module_id)
            .ok_or_else(|| anyhow::anyhow!("Cannot find the local module!"))?;
        get_linkable_pkgs(
            &resolve_output,
            main_module_id,
            moonutil::common::TargetBackend::Native,
            packages.values().cloned(),
        )?
    } else {
        let mut result_pkgs = vec![];
        for pkg_name in cmd.pkg_names.iter() {
            let pkgs = match_packages_by_name_rr(
                &resolve_output,
                resolve_output.local_modules(),
                pkg_name,
            );
            result_pkgs.extend(pkgs);
        }
        result_pkgs
    };

    // For each package we need to get its target backend and then we can build it
    for pkg in pkgs {
        // Get package info
        let package = &*resolve_output.pkg_dirs.get_package(pkg).raw;
        let backend = package.bin_target;
        let bin_name = package.bin_name.as_deref();

        let preconfig = preconfig_compile(
            &AutoSyncFlags { frozen: false },
            cli,
            &BuildFlags::default().with_target_backend(Some(backend)),
            &target_dir,
            OptLevel::Release,
            RunMode::Build,
        );
        let (build_meta, build_graph) = plan_build_from_resolved(
            preconfig,
            &cli.unstable_feature,
            &target_dir,
            Box::new(|_, _, _| Ok(vec![UserIntent::Build(pkg)].into())),
            // FIXME: cloning is not the best way to do this, it takes in this
            // type only to be returned in build meta. We should refactor later.
            resolve_output.clone(),
        )?;

        let _lock = FileLock::lock(&target_dir)?;

        let result = rr_build::execute_build(&BuildConfig::default(), build_graph, &target_dir)?;
        result.print_info(cli.quiet, "building")?;

        install_build_rr(&build_meta, &cmd.install_path, bin_name)?;
    }

    Ok(0)
}

/// Handle `moon build --install-path`
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

    // Build command using existing runtime mapping, then shlex-join
    let guard = crate::run::command_for(meta.target_backend, artifact, None)?;
    let parts = std::iter::once(guard.command.as_std().get_program())
        .chain(guard.command.as_std().get_args())
        .map(|x| x.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let line = shlex::try_join(parts.iter().map(|s| &**s))
        .expect("unexpected null byte in args when forming exec command");

    // Determine filename
    // Matching legacy, it uses the following fallbacks:
    // - provided bin_name
    // - artifact file stem
    // - "moonbin" (???)
    let name = bin_name
        .or_else(|| artifact.file_stem().and_then(|s| s.to_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| "moonbin".to_string());

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
