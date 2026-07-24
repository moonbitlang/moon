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
    prepared_source::prepare_cached_deps,
    registry,
    resolver::{ResolveConfig, resolve_with_default_env_and_resolver},
};

use super::sync::SyncOutputOptions;
use anyhow::Context;
use moonutil::{
    constants::MOONBITLANG_CORE,
    project::{DependencySource, PackageDirs},
    resolution::{
        DependencyKind, DirSyncResult, ModuleSourceKind, ResolvedEnv, ResolvedRootModules,
    },
};
use std::path::{Path, PathBuf};

struct PreparedBinDep {
    source_dir: PathBuf,
    target_dir: Option<PathBuf>,
    install_dir: PathBuf,
    _temp_dir: Option<tempfile::TempDir>,
}

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

pub(crate) fn install_impl(
    dirs: &PackageDirs,
    roots: ResolvedRootModules,
    output_options: SyncOutputOptions,
    verbose: bool,
    dont_sync: bool,
    no_std: bool,
) -> anyhow::Result<(ResolvedEnv, DirSyncResult)> {
    let includes_core = roots
        .iter()
        .any(|(_, module)| module.module_info().name == MOONBITLANG_CORE);
    if includes_core && roots.len() != 1 {
        anyhow::bail!("workspaces that include `moonbitlang/core` are not supported yet");
    }

    let resolve_config = ResolveConfig {
        registry: registry::default_registry(),
        inject_std: !includes_core && !no_std,
    };

    let res = resolve_with_default_env_and_resolver(&resolve_config, roots)?;
    let dir_sync_result = match &dirs.dependency_source {
        DependencySource::ProjectLocal => {
            let dep_dir = crate::dep_dir::DepDir::new(dirs.mooncakes_dir.clone());
            crate::dep_dir::sync_deps(
                &dep_dir,
                resolve_config.registry.as_ref(),
                &res,
                output_options.quiet(),
                dont_sync,
                output_options.verbose(),
            )
            .context("When installing packages")?;
            resolve_dep_dirs(&dep_dir, &res)
        }
        DependencySource::SharedCache(cache_root) => prepare_cached_deps(
            cache_root,
            resolve_config.registry.as_ref(),
            &res,
            output_options.quiet(),
            dont_sync,
            output_options.verbose(),
        )
        .context("When preparing cached packages")?,
    };

    install_bin_deps(
        verbose,
        &res,
        &dir_sync_result,
        &dirs.target_dir,
        &dirs.mooncake_bin_dir,
    )?;

    Ok((res, dir_sync_result))
}

fn install_bin_deps(
    verbose: bool,
    res: &ResolvedEnv,
    dep_dir: &DirSyncResult,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
) -> Result<(), anyhow::Error> {
    for &main_module in res.input_module_ids() {
        let Some(bin_deps) = res.module_info(main_module).bin_deps.as_ref() else {
            continue;
        };
        let moon_path = moonutil::toolchain::BINARIES.moonbuild.to_string_lossy();

        let bin_deps_iter = res
            .deps_keyed(main_module)
            .filter(|(_, edge)| edge.kind == DependencyKind::Binary);
        for (id, edge) in bin_deps_iter {
            let info = bin_deps
                .get(&edge.name.to_string()) // inefficient but fine for now
                .unwrap();

            let path = dep_dir.get(id).expect("Failed to get dep dir");
            let module_source = res.module_source(id);
            let prepared =
                prepare_bin_dep(path, target_dir, mooncake_bin_dir, module_source.source())?;
            let mut cmd = std::process::Command::new(moon_path.as_ref());
            cmd.arg("-C");
            cmd.arg(&prepared.source_dir);
            if let Some(target_dir) = &prepared.target_dir {
                cmd.arg("--target-dir");
                cmd.arg(target_dir);
            }
            cmd.args(["tool", "build-binary-dep"]);
            // pkg_names
            if let Some(pkgs) = info.bin_pkg.as_ref() {
                cmd.args(pkgs.iter());
            } else {
                cmd.arg("--all-pkgs");
            }
            // install path
            cmd.arg("--install-path");
            cmd.arg(&prepared.install_dir);

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

fn prepare_bin_dep(
    source_dir: &Path,
    target_dir: &Path,
    mooncake_bin_dir: &Path,
    source_kind: &ModuleSourceKind,
) -> anyhow::Result<PreparedBinDep> {
    match source_kind {
        ModuleSourceKind::Registry => {
            std::fs::create_dir_all(target_dir).with_context(|| {
                format!(
                    "failed to create project target directory `{}`",
                    target_dir.display()
                )
            })?;
            let temp_dir = tempfile::Builder::new()
                .prefix(".bin-dep-")
                .tempdir_in(target_dir)
                .with_context(|| {
                    format!(
                        "failed to create temporary binary dependency directory in `{}`",
                        target_dir.display()
                    )
                })?;
            let temp_source_dir = temp_dir.path().join("source");
            copy_registry_bin_dep_source(source_dir, &temp_source_dir)?;
            Ok(PreparedBinDep {
                source_dir: temp_source_dir,
                target_dir: Some(temp_dir.path().join("target")),
                install_dir: mooncake_bin_dir.to_path_buf(),
                _temp_dir: Some(temp_dir),
            })
        }
        _ => {
            let source_dir = dunce::canonicalize(source_dir).with_context(|| {
                format!(
                    "failed to resolve binary dependency source `{}`",
                    source_dir.display()
                )
            })?;
            Ok(PreparedBinDep {
                install_dir: source_dir.clone(),
                source_dir,
                target_dir: None,
                _temp_dir: None,
            })
        }
    }
}

fn copy_registry_bin_dep_source(source_dir: &Path, destination_dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(destination_dir).with_context(|| {
        format!(
            "failed to create registry binary dependency source directory `{}`",
            destination_dir.display()
        )
    })?;

    // Only root-level build and dependency state are excluded; identically
    // named package assets below the root remain part of the source copy.
    for entry in walkdir::WalkDir::new(source_dir)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() != 1
                || (entry.file_name() != moonutil::constants::BUILD_DIR
                    && entry.file_name() != moonutil::constants::DEP_PATH)
        })
    {
        let entry = entry.with_context(|| {
            format!(
                "failed to read registry binary dependency source `{}`",
                source_dir.display()
            )
        })?;
        let relative = entry.path().strip_prefix(source_dir).with_context(|| {
            format!(
                "registry binary dependency entry `{}` is outside source `{}`",
                entry.path().display(),
                source_dir.display()
            )
        })?;
        let destination = destination_dir.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&destination).with_context(|| {
                format!(
                    "failed to create registry binary dependency directory `{}`",
                    destination.display()
                )
            })?;
        } else {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!(
                        "failed to create registry binary dependency directory `{}`",
                        parent.display()
                    )
                })?;
            }
            std::fs::copy(entry.path(), &destination).with_context(|| {
                format!(
                    "failed to copy registry binary dependency file `{}` to `{}`",
                    entry.path().display(),
                    destination.display()
                )
            })?;
        }
    }
    Ok(())
}
