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
    dep_dir::resolve_dep_dirs,
    resolver::{ResolveConfig, resolve_single_root_with_defaults},
};

use anyhow::Context;
use moonutil::{
    common::{MOONBITLANG_CORE, read_module_desc_file_in_dir},
    module::MoonMod,
    mooncakes::{
        DirSyncResult, ModuleSource,
        result::{DependencyKind, ResolvedEnv},
    },
};
use std::{path::Path, sync::Arc};

/// Install dependencies or install a package executable.
#[derive(Debug, clap::Parser)]
pub struct InstallSubcommand {
    /// Optional package path to install in the form of <author>/<package_name>[@<version>]
    pub package_path: Option<String>,
}

pub fn install(
    source_dir: &Path,
    _target_dir: &Path,
    quiet: bool,
    verbose: bool,
    no_std: bool,
) -> anyhow::Result<i32> {
    let m = read_module_desc_file_in_dir(source_dir)?;
    let m = Arc::new(m);
    let ms = ModuleSource::from_local_module(&m, source_dir).expect("Malformed module manifest");
    install_impl(source_dir, m, ms, quiet, verbose, false, no_std).map(|_| 0)
}

pub(crate) fn install_impl(
    source_dir: &Path,
    m: Arc<moonutil::module::MoonMod>,
    ms: ModuleSource,
    quiet: bool,
    verbose: bool,
    dont_sync: bool,
    no_std: bool,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult)> {
    let registry = crate::registry::RegistryList::with_default_registry();

    let is_stdlib = m.name == MOONBITLANG_CORE;

    let resolve_config = ResolveConfig {
        registries: registry,
        inject_std: !is_stdlib && !no_std,
    };

    let res = resolve_single_root_with_defaults(&resolve_config, ms, Arc::clone(&m))?;
    let dep_dir = crate::dep_dir::DepDir::of_source(source_dir);

    crate::dep_dir::sync_deps(&dep_dir, &resolve_config.registries, &res, quiet, dont_sync)
        .context("When installing packages")?;

    let dir_sync_result = resolve_dep_dirs(&dep_dir, &res);

    install_bin_deps(m, verbose, &res, &dir_sync_result)?;

    Ok((res, dir_sync_result))
}

fn install_bin_deps(
    m: Arc<MoonMod>,
    verbose: bool,
    res: &ResolvedEnv,
    dep_dir: &DirSyncResult,
) -> Result<(), anyhow::Error> {
    if let Some(ref bin_deps) = m.bin_deps {
        let moon_path = moonutil::BINARIES.moonbuild.to_string_lossy();

        let main_module = res.input_module_ids()[0];
        let bin_deps_iter = res
            .deps_keyed(main_module)
            .filter(|(_, edge)| edge.kind == DependencyKind::Binary);
        for (id, edge) in bin_deps_iter {
            let info = bin_deps
                .get(&edge.name.to_string()) // inefficient but fine for now
                .unwrap();

            let path = dep_dir.get(id).expect("Failed to get dep dir");
            let mut cmd = std::process::Command::new(moon_path.as_ref());
            cmd.args(["tool", "build-binary-dep"]);
            // root_path
            cmd.arg("-C");
            cmd.arg(path);
            // pkg_names
            if let Some(pkgs) = info.bin_pkg.as_ref() {
                cmd.args(pkgs.iter());
            } else {
                cmd.arg("--all-pkgs");
            }
            // install path
            cmd.arg("--install-path");
            cmd.arg(path);

            if !verbose {
                cmd.arg("--quiet");
            }

            // Run it
            if verbose {
                eprintln!("Installing binary dependency `{}`", edge.name);
            }
            let status = cmd
                .spawn()
                .with_context(|| {
                    format!(
                        "Failed to spawn build process for binary dep `{}`",
                        edge.name
                    )
                })?
                .wait()
                .with_context(|| {
                    format!(
                        "Failed to wait for build process of binary dep `{}`",
                        edge.name
                    )
                })?;
            if !status.success() {
                return Err(anyhow::anyhow!(
                    "Building binary dependency `{}` failed",
                    edge.name
                ));
            }
        }
    }

    Ok(())
}
