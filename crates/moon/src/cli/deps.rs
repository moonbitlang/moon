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
    pkg::{add::AddSubcommand, remove::RemoveSubcommand},
    registry::{
        RegistryClient,
        path::{ModuleNotFound, parse_install_package_path, parse_module_path},
    },
};
use moonutil::{
    child_process::{ChildOutputMode, ManagedChildRunner},
    command_output::CommandOutput,
    constants::MOD_NAME_STDLIB,
    locks::lock_directory,
    manifest::read_module_desc_file_in_dir,
    project::{PackageDirs, ProjectContext},
    user_log::UserLog,
};
use std::path::{Path, PathBuf};

use super::{UniversalFlags, registry_queries};

/// Explicitly run a legacy postadd script (temporary compatibility command)
///
/// Dependencies no longer run postadd automatically. This command runs only
/// the selected module's script, every time it is invoked. It does not download
/// or reinstall sources. Without MODULE, use the current module, including
/// sources selected with `moon -C <directory> run-postadd`.
///
/// Postadd is being retired. If you still need it, please open an issue at
/// https://github.com/moonbitlang/moon/issues describing your use case.
#[derive(Debug, clap::Parser)]
pub(super) struct RunPostaddSubcommand {
    /// Installed dependency module name, without a version (defaults to the current module)
    #[clap(value_name = "MODULE")]
    pub module: Option<String>,
}

pub(super) fn run_postadd_cli(
    cli: UniversalFlags,
    cmd: RunPostaddSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let selected = cmd.module.as_deref().map(parse_module_path).transpose()?;
    if selected.as_ref().is_some_and(|path| path.version.is_some()) {
        bail!("`moon run-postadd` expects an installed module name without a version");
    }
    let project = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(output.user_log())?;
    let (module_dir, _lock) = if let Some(path) = &selected {
        let dirs = project.package_dirs()?;
        let module_dir = dirs
            .mooncakes_dir
            .join(path.module.username.as_str())
            .join(path.module.unqual.replace('/', "+"));
        anyhow::ensure!(
            module_dir.is_dir(),
            "Module `{}` is not installed in this project's .mooncakes directory",
            path.module
        );
        // Keep source synchronization from replacing the installation mid-script.
        let lock = lock_directory(&dirs.mooncakes_dir, output.user_log())?;
        (module_dir, Some(lock))
    } else {
        (
            require_selected_module(project.context(), "run-postadd")?,
            None,
        )
    };
    let module = read_module_desc_file_in_dir(&module_dir)?;
    if let Some(path) = selected {
        anyhow::ensure!(
            module.name == path.module.to_string(),
            "Installed module does not match `{}`",
            path.module
        );
    }
    let postadd = module
        .scripts
        .as_ref()
        .and_then(|scripts| scripts.get("postadd"))
        .with_context(|| format!("Module `{}` does not declare a postadd script", module.name))?;
    if cli.dry_run {
        output.write_result(|writer| {
            writeln!(
                writer,
                "cd {} && {postadd}",
                shlex::try_quote(&module_dir.to_string_lossy())
                    .expect("filesystem paths contain no NUL")
            )
        })?;
    } else {
        let runner = ManagedChildRunner::new(ChildOutputMode::Inherit, output.user_log());
        mooncake::pkg::legacy_postadd::run(&module_dir, postadd, &runner)?;
    }
    Ok(0)
}

pub(super) fn require_selected_module(
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
    GitRef, install_binary, install_from_git, install_from_github_archive, install_from_local,
    is_git_url, is_local_path, parse_github_permalink, strip_wildcard_suffix,
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

/// Install a binary package globally
#[derive(Debug, clap::Parser)]
#[clap(
    group = clap::ArgGroup::new("git_ref").multiple(false),
    verbatim_doc_comment,
    after_help = "Examples:
  moon install user/module/pkg
  moon install user/module/cmd/...
  moon install ./cmd/tool
  moon install ./cmd/...
  moon install https://github.com/owner/repo/tree/0123456789abcdef0123456789abcdef01234567/cmd/tool
  moon install https://github.com/owner/repo.git cmd/tool --branch main
  moon install https://github.com/owner/repo.git cmd/... --tag v1.0.0

Paste a public GitHub directory permalink containing a full 40-character
commit SHA to install from its source archive. For branches or tags, use the
repository URL, PATH_IN_REPO, and --branch or --tag instead."
)]
pub(super) struct InstallSubcommand {
    #[clap(
        value_name = "SOURCE",
        required_unless_present = "path",
        help = "Package path, local path, or git URL",
        long_help = "Install source.\n\nInterpretation order:\n  1. local path (`./`, `../`, `/`, Windows drive)\n  2. git URL\n  3. registry package path (`user/module/pkg[@version]`)\n\nUse `/...` suffix to install all matching main packages."
    )]
    source: Option<String>,

    #[clap(
        value_name = "PATH_IN_REPO",
        help = "Filesystem path inside git repo (git SOURCE only)",
        long_help = "Filesystem path inside the repository source.\nUsed only when SOURCE is a git URL.\n\nUse `/...` suffix to install all matching main packages under this path prefix."
    )]
    path_in_repo: Option<String>,

    /// Specify installation directory (default: ~/.moon/bin/)
    #[clap(long, value_name = "DIR")]
    bin: Option<PathBuf>,

    /// Install from local path instead of registry
    #[clap(
        long,
        conflicts_with = "source",
        conflicts_with = "git_ref",
        conflicts_with = "path_in_repo"
    )]
    path: Option<PathBuf>,

    /// Git revision to checkout (commit hash, requires git URL)
    #[clap(long, group = "git_ref", requires = "source")]
    rev: Option<String>,

    /// Git branch to checkout (requires git URL)
    #[clap(long, group = "git_ref", requires = "source")]
    branch: Option<String>,

    /// Git tag to checkout (requires git URL)
    #[clap(long, group = "git_ref", requires = "source")]
    tag: Option<String>,
}

pub(super) fn install_cli(
    cli: UniversalFlags,
    cmd: InstallSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
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

    // GitHub browser permalinks select archives; ordinary Git URLs retain the
    // existing repository transport and revision flags.
    if let Some(permalink) = parse_github_permalink(&source)? {
        if has_git_ref {
            anyhow::bail!(
                "--rev, --branch, and --tag must not be used with a tree permalink; the URL already selects a commit"
            );
        }
        if cmd.path_in_repo.is_some() && permalink.path_in_repo.is_some() {
            anyhow::bail!(
                "PATH_IN_REPO must not be used when SOURCE already contains a /tree/... path"
            );
        }
        let path_in_repo = cmd
            .path_in_repo
            .as_deref()
            .or(permalink.path_in_repo.as_deref());
        let install_all = path_in_repo.is_some_and(|s| strip_wildcard_suffix(s).is_some());
        return install_from_github_archive(
            &cli,
            &permalink.archive_url,
            path_in_repo,
            &install_dir,
            install_all,
            user_log,
        );
    }

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

pub(super) fn remove_cli(
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

pub(super) fn add_cli(
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
    if path.module == MOD_NAME_STDLIB {
        user_log.warn(format!("no need to add `{MOD_NAME_STDLIB}` as dependency"));
        return Ok(0);
    }

    let registry = RegistryClient::configured(user_log);
    let allow_remote = !cmd.no_update;
    let module = registry_queries::resolve_module(
        &registry,
        path.module_request()?,
        allow_remote,
        user_log,
    )
    .map_err(|error| {
        if !allow_remote && error.is::<ModuleNotFound>() {
            let hint = if registry.uses_http_metadata() {
                "No cached latest version; retry without `--no-update` to query the registry."
            } else {
                "Please consider running `moon update` to update the index."
            };
            error.context(hint)
        } else {
            error
        }
    })?;
    mooncake::pkg::add::add(&module_dir, &dirs, &module, cmd.upgrade, user_log)
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
