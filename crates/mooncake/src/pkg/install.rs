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

use crate::{
    dependency_source, registry,
    resolver::{ModuleResolutionConfig, resolve_modules},
};

use super::sync::SyncOutputOptions;
use moonutil::{
    cache::CacheRoot,
    constants::MOONBITLANG_CORE,
    project::PackageDirs,
    resolution::{DirSyncResult, ModuleDependencyGraph, ResolvedRootModules},
    user_log::UserLog,
};
use std::path::PathBuf;

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
pub struct InstallSubcommand {
    #[clap(
        value_name = "SOURCE",
        required_unless_present = "path",
        help = "Package path, local path, or git URL",
        long_help = "Install source.\n\nInterpretation order:\n  1. local path (`./`, `../`, `/`, Windows drive)\n  2. git URL\n  3. registry package path (`user/module/pkg[@version]`)\n\nUse `/...` suffix to install all matching main packages."
    )]
    pub source: Option<String>,

    #[clap(
        value_name = "PATH_IN_REPO",
        help = "Filesystem path inside git repo (git SOURCE only)",
        long_help = "Filesystem path inside the repository source.\nUsed only when SOURCE is a git URL.\n\nUse `/...` suffix to install all matching main packages under this path prefix."
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
    user_log: &UserLog,
    dont_sync: bool,
    no_std: bool,
    source_cache: &CacheRoot,
) -> anyhow::Result<(ModuleDependencyGraph, DirSyncResult)> {
    let includes_core = roots
        .iter()
        .any(|(_, module)| module.module_info().name == MOONBITLANG_CORE);
    if includes_core && roots.len() != 1 {
        anyhow::bail!("workspaces that include `moonbitlang/core` are not supported yet");
    }

    let registry = if dont_sync {
        registry::default_registry(user_log).for_frozen_dependencies()
    } else {
        registry::default_registry(user_log)
    };
    let module_resolution_config = ModuleResolutionConfig {
        registry: &registry,
        inject_std: !includes_core && !no_std,
        warn_deprecated: true,
    };

    let res = resolve_modules(&module_resolution_config, roots, user_log)?;
    let dependency_user_log = if output_options.quiet {
        user_log.with_level(log::LevelFilter::Error)
    } else {
        user_log.clone()
    };
    let dependency_source = dependency_source::select(
        &dirs.mooncakes_dir,
        source_cache,
        &res,
        output_options.child_output,
    )?;
    let dependency_paths =
        dependency_source.ensure(&registry, &res, dont_sync, &dependency_user_log)?;

    Ok((res, dependency_paths))
}
