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
    registry,
    resolver::{ResolveConfig, resolve_with_default_env_and_resolver},
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
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

/// Install a binary package globally or install project dependencies (deprecated without args)
#[derive(Debug, clap::Parser)]
#[clap(
    group = clap::ArgGroup::new("git_ref").multiple(false),
    verbatim_doc_comment
)]
pub struct InstallSubcommand {
    #[clap(
        value_name = "SOURCE",
        help = "Package path, local path, or git URL",
        long_help = "Install source.\n\nInterpretation order:\n  1. local path (`./`, `../`, `/`, Windows drive)\n  2. git URL\n  3. registry package path (`user/module/pkg[@version]`)\n\nUse `/...` suffix to install all matching main packages."
    )]
    pub source: Option<String>,

    #[clap(
        value_name = "PATH_IN_REPO",
        help = "Filesystem path inside git repo (git SOURCE only)",
        long_help = "Filesystem path inside the cloned git repository.\nUsed only when SOURCE is a git URL.\n\nUse `/...` suffix to install all matching main packages under this path prefix."
    )]
    pub path_in_repo: Option<String>,

    /// Specify installation directory (default: ~/.moon/bin/)
    #[clap(long, value_name = "DIR")]
    pub bin: Option<PathBuf>,

    /// Install from local path instead of registry
    #[clap(
        long,
        conflicts_with = "source",
        conflicts_with = "git_ref",
        conflicts_with = "path_in_repo"
    )]
    pub path: Option<PathBuf>,

    /// Git revision to checkout (commit hash, requires git URL)
    #[clap(long, group = "git_ref", requires = "source")]
    pub rev: Option<String>,

    /// Git branch to checkout (requires git URL)
    #[clap(long, group = "git_ref", requires = "source")]
    pub branch: Option<String>,

    /// Git tag to checkout (requires git URL)
    #[clap(long, group = "git_ref", requires = "source")]
    pub tag: Option<String>,
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
    let roots = [(ms, Arc::clone(&m))];
    install_impl(source_dir, &roots, quiet, verbose, false, no_std).map(|_| 0)
}

pub(crate) fn install_impl(
    source_dir: &Path,
    roots: &[(ModuleSource, Arc<MoonMod>)],
    quiet: bool,
    verbose: bool,
    dont_sync: bool,
    no_std: bool,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult)> {
    let includes_core = roots
        .iter()
        .any(|(_, module)| module.name == MOONBITLANG_CORE);
    if includes_core && roots.len() != 1 {
        anyhow::bail!("workspaces that include `moonbitlang/core` are not supported yet");
    }

    let resolve_config = ResolveConfig {
        registry: registry::default_registry(),
        inject_std: !includes_core && !no_std,
    };

    let res = resolve_with_default_env_and_resolver(&resolve_config, roots)?;
    let dep_dir = crate::dep_dir::DepDir::of_source(source_dir);

    crate::dep_dir::sync_deps(
        &dep_dir,
        resolve_config.registry.as_ref(),
        &res,
        quiet,
        dont_sync,
    )
    .context("When installing packages")?;

    let dir_sync_result = resolve_dep_dirs(&dep_dir, &res);

    install_bin_deps(verbose, &res, &dir_sync_result)?;

    Ok((res, dir_sync_result))
}

fn install_bin_deps(
    verbose: bool,
    res: &ResolvedEnv,
    dep_dir: &DirSyncResult,
) -> Result<(), anyhow::Error> {
    for &main_module in res.input_module_ids() {
        let Some(bin_deps) = res.module_info(main_module).bin_deps.as_ref() else {
            continue;
        };
        let moon_path = moonutil::BINARIES.moonbuild.to_string_lossy();

        let bin_deps_iter = res
            .deps_keyed(main_module)
            .filter(|(_, edge)| edge.kind == DependencyKind::Binary);
        for (id, edge) in bin_deps_iter {
            let info = bin_deps
                .get(&edge.name.to_string()) // inefficient but fine for now
                .unwrap();

            let path = dep_dir.get(id).expect("Failed to get dep dir");
            let mut cmd = std::process::Command::new(moon_path.as_ref());
            // root_path
            cmd.arg("-C");
            cmd.arg(path);
            cmd.args(["tool", "build-binary-dep"]);
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
