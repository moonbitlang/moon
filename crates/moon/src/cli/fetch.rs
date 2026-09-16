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

use colored::Colorize;
use mooncake::{
    pkg::legacy_postadd,
    registry::{RegistryClient, path::parse_module_path},
};
use moonutil::{
    child_process::{ChildOutputMode, ManagedChildRunner},
    project::PackageDirs,
    user_log::UserLog,
};

use super::UniversalFlags;

#[derive(Debug, clap::Parser)]
#[clap(
    about = "Download a package to .repos directory (unstable)",
    before_help = "Note: This is an unstable command and may change or be removed in future versions."
)]
pub(crate) struct FetchSubcommand {
    /// The registry module name to fetch in the form of <author>/<module_name>[@<version>]
    #[clap(value_name = "MODULE[@VERSION]")]
    pub package_path: String,

    /// Do not update the registry index before fetching
    #[clap(long)]
    pub no_update: bool,
}

pub(crate) fn fetch_cli(
    cli: UniversalFlags,
    cmd: FetchSubcommand,
    user_log: &UserLog,
) -> anyhow::Result<i32> {
    let path = parse_module_path(&cmd.package_path)?;
    let registry = RegistryClient::configured();

    if !cmd.no_update {
        let had_index = registry.has_cached_index();
        match registry.sync(user_log) {
            Ok(()) => {}
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

    let module = path.resolve(&registry)?;
    let pkg_name = module.name();
    let version = module.version();
    if path.version.is_none() && !cli.quiet {
        println!("Latest version of {pkg_name} is {version}");
    }

    // If we are under a project, put it into `.repos` next to `.mooncakes`
    let source_dir = match cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())
        .and_then(|query| query.select(user_log))
        .and_then(|project| project.package_dirs())
    {
        Ok(PackageDirs { source_dir, .. }) => source_dir,
        Err(_) => std::env::current_dir()?,
    };
    let repo_dir = source_dir.join(".repos");
    let pkg_dir = repo_dir
        .join(&*pkg_name.username)
        .join(&*pkg_name.unqual)
        .join(version.to_string());

    if pkg_dir.exists() {
        if !cli.quiet {
            println!(
                "{}: {}@{version} already exists at {}",
                "Info".green().bold(),
                pkg_name,
                pkg_dir.display()
            );
        }
        return Ok(0);
    }

    if !cli.quiet {
        println!("Fetching {}@{version} to {}", pkg_name, pkg_dir.display());
    }

    registry.materialize_source_to(pkg_name, version, &pkg_dir, user_log)?;
    let child = ManagedChildRunner::new(ChildOutputMode::Inherit, user_log);
    legacy_postadd::run(&pkg_dir, &child)?;

    if !cli.quiet {
        println!(
            "{}: Successfully fetched {}@{version} to {}",
            "Success".green().bold(),
            pkg_name,
            pkg_dir.display()
        );
    }

    Ok(0)
}
