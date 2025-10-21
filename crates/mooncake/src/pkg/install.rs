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

use crate::{
    dep_dir::DepDir,
    resolver::{resolve_single_root_with_defaults, ResolveConfig},
};

use anyhow::Context;
use moonutil::{
    common::{read_module_desc_file_in_dir, DiagnosticLevel, MOONBITLANG_CORE},
    module::MoonMod,
    mooncakes::{result::ResolvedEnv, ModuleSource},
    scan::scan,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// Install dependencies
#[derive(Debug, clap::Parser)]
pub struct InstallSubcommand {}

pub fn install(
    source_dir: &Path,
    _target_dir: &Path,
    quiet: bool,
    verbose: bool,
) -> anyhow::Result<i32> {
    let m = read_module_desc_file_in_dir(source_dir)?;
    let m = Arc::new(m);
    install_impl(source_dir, m, quiet, verbose, false).map(|_| 0)
}

pub(crate) fn install_impl(
    source_dir: &Path,
    m: Arc<moonutil::module::MoonMod>,
    quiet: bool,
    verbose: bool,
    dont_sync: bool,
) -> anyhow::Result<(ResolvedEnv, DepDir)> {
    let registry = crate::registry::RegistryList::with_default_registry();

    let is_stdlib = m.name == MOONBITLANG_CORE;
    let ms = ModuleSource::from_local_module(&m, source_dir).expect("Malformed module manifest");

    let resolve_config = ResolveConfig {
        registries: registry,
        inject_std: !is_stdlib,
    };

    let res = resolve_single_root_with_defaults(&resolve_config, ms, Arc::clone(&m))?;
    let dep_dir = crate::dep_dir::DepDir::of_source(source_dir);

    crate::dep_dir::sync_deps(&dep_dir, &resolve_config.registries, &res, quiet, dont_sync)
        .context("When installing packages")?;

    install_bin_deps(m, verbose, &res, &dep_dir)?;

    Ok((res, dep_dir))
}

fn install_bin_deps(
    m: Arc<MoonMod>,
    verbose: bool,
    res: &ResolvedEnv,
    dep_dir: &DepDir,
) -> Result<(), anyhow::Error> {
    if let Some(ref bin_deps) = m.bin_deps {
        let moon_path = std::env::current_exe()
            .map_or_else(|_| "moon".into(), |x| x.to_string_lossy().into_owned());

        for (bin_mod_to_install, info) in bin_deps {
            let bin_mod_path = match info.path {
                Some(ref path) => PathBuf::from(path),
                None => dep_dir.path().join(bin_mod_to_install),
            };

            if !bin_mod_path.exists() {
                anyhow::bail!(
                    "binary module `{}` not found in `{}`",
                    bin_mod_to_install,
                    dep_dir.path().display()
                );
            }

            let module_db = get_module_db(&bin_mod_path, res, dep_dir)?;

            if let Some(ref bin_pkg) = info.bin_pkg {
                for pkg_name in bin_pkg {
                    let full_pkg_name = format!("{bin_mod_to_install}/{pkg_name}");

                    let pkg = module_db.get_package_by_name_safe(&full_pkg_name);
                    match pkg {
                        Some(pkg) => {
                            build_and_install_bin_package(
                                &moon_path,
                                &bin_mod_path,
                                &full_pkg_name,
                                &bin_mod_path,
                                pkg.bin_target.to_backend_ext(),
                                verbose,
                            )?;
                        }
                        _ => anyhow::bail!(format!("package `{}` not found", full_pkg_name)),
                    }
                }
            } else {
                for (full_pkg_name, pkg) in module_db
                    .get_all_packages()
                    .iter()
                    .filter(|(_, p)| p.is_main && !p.is_third_party)
                {
                    build_and_install_bin_package(
                        &moon_path,
                        &bin_mod_path,
                        full_pkg_name,
                        &bin_mod_path,
                        pkg.bin_target.to_backend_ext(),
                        verbose,
                    )?;
                }
            }
        }
    }

    Ok(())
}

fn build_and_install_bin_package(
    moon_path: &str,
    bin_mod_path: &Path,
    full_pkg_name: &str,
    install_path: &Path,
    bin_target: impl AsRef<str>,
    verbose: bool,
) -> anyhow::Result<()> {
    let mut build_args = vec![
        "build".to_string(),
        "--source-dir".to_string(),
        bin_mod_path.display().to_string(),
        "--install-path".to_string(),
        install_path.display().to_string(),
        "--target".to_string(),
        bin_target.as_ref().to_string(),
        "--package".to_string(),
        full_pkg_name.to_string(),
    ];

    if !verbose {
        build_args.push("--quiet".to_string());
    }

    if verbose {
        eprintln!("Installing binary package `{full_pkg_name}`");
    }

    std::process::Command::new(moon_path)
        .args(&build_args)
        .spawn()
        .with_context(|| format!("Failed to spawn build process for {full_pkg_name}"))?
        .wait()
        .with_context(|| format!("Failed to wait for build process of {full_pkg_name}"))?;

    Ok(())
}

fn get_module_db(
    source_dir: &Path,
    resolved_env: &ResolvedEnv,
    dep_dir: &DepDir,
) -> anyhow::Result<moonutil::module::ModuleDB> {
    let dir_sync_result = crate::dep_dir::resolve_dep_dirs(dep_dir, resolved_env);
    let moonbuild_opt = moonutil::common::MoonbuildOpt {
        source_dir: source_dir.to_path_buf(),
        raw_target_dir: source_dir.join("target"),
        target_dir: source_dir.join("target"),
        test_opt: None,
        check_opt: None,
        build_opt: None,
        sort_input: false,
        run_mode: moonutil::common::RunMode::Build,
        quiet: true,
        verbose: false,
        no_parallelize: false,
        build_graph: false,
        fmt_opt: None,
        args: vec![],
        output_json: false,
        parallelism: None, // we don't care about parallelism here
        use_tcc_run: false,
        dynamic_stub_libs: None,
        render_no_loc: DiagnosticLevel::default(),
    };
    let module_db = scan(
        false,
        None,
        resolved_env,
        &dir_sync_result,
        &moonutil::common::MooncOpt::default(),
        &moonbuild_opt,
    )?;
    Ok(module_db)
}
