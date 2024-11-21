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

use crate::{dep_dir::DepDir, resolver::resolve_single_root_with_defaults};

use anyhow::Context;
use moonutil::{
    common::read_module_desc_file_in_dir,
    mooncakes::{result::ResolvedEnv, ModuleSource, RegistryConfig},
};
use std::{path::Path, rc::Rc};

/// Install dependencies
#[derive(Debug, clap::Parser)]
pub struct InstallSubcommand {
    #[clap(long, hide = true)]
    pub bin: Option<String>,

    #[clap(hide = true)]
    pub extra_args: Vec<String>,
}

pub fn install(
    source_dir: &Path,
    _target_dir: &Path,
    registry_config: &RegistryConfig,
    quiet: bool,
    cmd: InstallSubcommand,
) -> anyhow::Result<i32> {
    install_impl(source_dir, registry_config, quiet, false, Some(cmd)).map(|_| 0)
}

pub(crate) fn install_impl(
    source_dir: &Path,
    _registry_config: &RegistryConfig,
    quiet: bool,
    dont_sync: bool,
    cmd: Option<InstallSubcommand>,
) -> anyhow::Result<(ResolvedEnv, DepDir)> {
    let m = read_module_desc_file_in_dir(source_dir)?;
    let m = Rc::new(m);
    let registry = crate::registry::RegistryList::with_default_registry();
    let ms = ModuleSource::from_local_module(&m, source_dir).expect("Malformed module manifest");
    let res = resolve_single_root_with_defaults(&registry, ms, Rc::clone(&m))?;
    let dep_dir = crate::dep_dir::DepDir::of_source(source_dir);
    if !dont_sync {
        crate::dep_dir::sync_deps(&dep_dir, &registry, &res, quiet)
            .context("When installing packages")?;
    }
    if let Some(InstallSubcommand {
        bin: Some(bin_mod_to_install),
        extra_args,
    }) = cmd
    {
        let bin_path = dep_dir.path().join(&bin_mod_to_install);

        if !bin_path.exists() {
            anyhow::bail!(
                "binary module `{}` not found in `{}`",
                bin_mod_to_install,
                dep_dir.path().display()
            );
        }
        let mut cmd = std::process::Command::new(
            std::env::current_exe()
                .map_or_else(|_| "moon".into(), |x| x.to_string_lossy().into_owned()),
        );

        let moon_bin_dir = source_dir
            .join(moonutil::common::DEP_PATH)
            .join(moonutil::common::MOON_BIN_DIR);

        cmd.arg("build")
            .arg("--source-dir")
            .arg(&bin_path)
            .arg("--install-path")
            .arg(&moon_bin_dir)
            .args(extra_args)
            .spawn()?
            .wait()?;

        // remove all files except .exe, .wasm, .js
        if moon_bin_dir.exists() {
            for entry in std::fs::read_dir(&moon_bin_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    let ext = path.extension().and_then(|s| s.to_str());
                    if !matches!(ext, Some("exe" | "wasm" | "js")) {
                        std::fs::remove_file(path)?;
                    }
                }
            }
        }
    }
    Ok((res, dep_dir))
}
