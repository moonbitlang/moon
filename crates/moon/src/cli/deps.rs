// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use anyhow::{Context, bail};
use mooncake::{
    pkg::{
        add::AddSubcommand, install::InstallSubcommand, remove::RemoveSubcommand,
        sync::SyncOutputOptions,
    },
    registry::{
        RegistryClient,
        path::{parse_install_package_path, parse_module_path},
    },
};
use moonutil::{
    cli_support::AutoSyncFlags,
    project::{PackageDirs, ProjectContext},
    user_log::UserLog,
};
use std::path::{Path, PathBuf};

use super::UniversalFlags;

pub(crate) fn require_selected_module(
    project: &ProjectContext,
    command: &str,
) -> anyhow::Result<PathBuf> {
    project
        .selected_module()
        .map(|module| module.root.clone())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "`moon {command}` cannot infer a target module in workspace `{}`. Run it from a workspace member or use `moon -C <member> {command} ...`.",
                project.root().display(),
            )
        })
}
use super::install_binary::{
    GitRef, install_binary, install_from_git, install_from_local, is_git_url, is_local_path,
    strip_wildcard_suffix,
};

/// Returns the local filesystem path used for wildcard local install.
fn local_wildcard_path(source: &str) -> Option<PathBuf> {
    let base = strip_wildcard_suffix(source)?;
    if base.is_empty() {
        if source.starts_with('/') {
            Some(PathBuf::from("/"))
        } else {
            Some(PathBuf::from("."))
        }
    } else if base.ends_with(':') && source.ends_with("/...") {
        // `C:/...` should resolve to `C:/` instead of drive-relative `C:`.
        Some(PathBuf::from(format!("{base}/")))
    } else {
        Some(PathBuf::from(base))
    }
}

pub(crate) fn install_cli(
    cli: UniversalFlags,
    cmd: InstallSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    // If no package path and no local path, use legacy behavior
    if cmd.source.is_none() && cmd.path.is_none() {
        user_log.warn(
            "`moon install` without arguments is deprecated and will be removed in a future version. \
             Use `moon install <package>` to install binaries globally, or use `moon build` to build your project.",
        );
        let dirs = cli
            .source_tgt_dir
            .query(cli.workspace_env.clone())?
            .select(user_log)?
            .package_dirs()?;
        mooncake::pkg::sync::auto_sync(
            &dirs,
            &AutoSyncFlags { frozen: false },
            SyncOutputOptions::default(),
            user_log,
            true,
            cli.workspace_env.clone(),
            true,
        )?;
        return Ok(0);
    }

    let install_dir = cmd.bin.unwrap_or_else(|| moonutil::MOON_HOME.bin_dir());
    let has_git_ref = cmd.rev.is_some() || cmd.branch.is_some() || cmd.tag.is_some();

    // Explicit --path takes priority
    if let Some(local_path) = cmd.path {
        let local_path_str = local_path.to_string_lossy();
        if strip_wildcard_suffix(local_path_str.as_ref()).is_some() {
            user_log.warn(format!(
                "`--path` does not support wildcard selectors like `{}`",
                local_path_str
            ));
            anyhow::bail!(
                "Use positional SOURCE for wildcard install: `moon install {}`",
                local_path_str
            );
        }
        return install_from_local(&cli, &local_path, &install_dir, false, user_log);
    }

    let source = cmd.source.ok_or_else(|| {
        anyhow::anyhow!("`moon install` expects a package, local path, or git URL")
    })?;

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
        let (local_path, install_all) = local_wildcard_path(&source)
            .map_or((PathBuf::from(source.as_str()), false), |base| (base, true));
        return install_from_local(
            &cli,
            Path::new(&local_path),
            &install_dir,
            install_all,
            user_log,
        );
    }

    // Git URL install
    if is_git_url(&source) {
        let install_all = cmd
            .path_in_repo
            .as_deref()
            .is_some_and(|s| strip_wildcard_suffix(s).is_some());
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
            user_log,
        );
    }

    // Registry install
    if has_git_ref {
        anyhow::bail!("--rev, --branch, and --tag can only be used with git URLs");
    }
    if cmd.path_in_repo.is_some() {
        anyhow::bail!("Path in repo can only be used with git URLs");
    }
    let (path, install_all) = parse_install_package_path(&source)
        .with_context(|| format!("Invalid package path `{source}`"))?;
    install_binary(&cli, &path, &install_dir, install_all, user_log)
}

pub(crate) fn remove_cli(
    cli: UniversalFlags,
    cmd: RemoveSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    let project = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(user_log)?;
    let module_dir = require_selected_module(project.context(), "remove")?;
    let PackageDirs {
        project_manifest, ..
    } = project.package_dirs()?;
    let path = parse_module_path(&cmd.package_path)?;
    if path.version.is_some() {
        bail!("`moon remove` expects a module name without a version");
    }
    mooncake::pkg::remove::remove(&module_dir, &project_manifest, &path.module, user_log)
}

pub(crate) fn add_cli(
    cli: UniversalFlags,
    cmd: AddSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    let project = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(user_log)?;
    let module_dir = require_selected_module(project.context(), "add")?;
    let dirs = project.package_dirs()?;

    let path = parse_module_path(&cmd.package_path)?;
    let version = path.exact_version()?;
    if cmd.upgrade && cmd.bin {
        bail!("--bin cannot be used with --upgrade");
    }

    // Update registry index by default (issue #963).
    // - `--no-update` keeps the previous behavior.
    // - If an index already exists, update failures are treated as warnings so users can proceed
    //   with the existing local index.
    let registry = RegistryClient::configured();
    let mut index_updated = false;
    if !cmd.no_update && (!cmd.upgrade || version.is_none()) {
        let had_index = registry.has_cached_index();
        match registry.sync(user_log) {
            Ok(()) => {
                index_updated = true;
            }
            Err(e) => {
                if had_index {
                    user_log.warn(format!(
                        "failed to update registry index, continuing with existing index: {e}"
                    ));
                } else {
                    return Err(e);
                }
            }
        }
    }

    if let Some(version) = version {
        mooncake::pkg::add::add(
            &module_dir,
            &dirs,
            &path.module,
            cmd.bin,
            &version,
            cmd.upgrade,
            user_log,
        )
    } else {
        mooncake::pkg::add::add_latest(
            &registry,
            &module_dir,
            &dirs,
            &path.module,
            cmd.bin,
            index_updated,
            cmd.upgrade,
            user_log,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_wildcard_path_unix_root() {
        let got = local_wildcard_path("/...").unwrap();
        assert_eq!(got, PathBuf::from("/"));
    }

    #[test]
    fn test_local_wildcard_path_relative_current() {
        let got = local_wildcard_path("./...").unwrap();
        assert_eq!(got, PathBuf::from("."));
    }

    #[test]
    fn test_local_wildcard_path_windows_drive_root() {
        let got = local_wildcard_path("C:/...").unwrap();
        assert_eq!(got, PathBuf::from("C:/"));
    }
}
