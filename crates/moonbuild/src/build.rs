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

use super::gen;
use anyhow::{bail, Context};
use moonutil::common::MoonbuildOpt;
use moonutil::module::ModuleDB;
use n2::load::State;
use std::path::Path;
use std::process::{Command, Stdio};

use moonutil::common::MooncOpt;

pub fn load_moon_proj(
    module: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    let target_dir = &moonbuild_opt.target_dir;
    log::debug!("module: {:#?}", module);
    let n2_input = gen::gen_build::gen_build(module, moonc_opt, moonbuild_opt)?;
    log::debug!("n2_input: {:#?}", n2_input);
    gen::gen_build::gen_n2_build_state(&n2_input, target_dir, moonc_opt, moonbuild_opt)
}

pub fn run_wat(path: &Path, args: &[String], verbose: bool) -> anyhow::Result<()> {
    run("moonrun", path, args, verbose)
}

pub fn run_js(path: &Path, args: &[String], verbose: bool) -> anyhow::Result<()> {
    if !args.is_empty() {
        bail!(format!(
            "js backend does not support extra args for now {:?}",
            args
        ))
    }
    run("node", path, args, verbose)
}

pub fn run_native(path: &Path, args: &[String], verbose: bool) -> anyhow::Result<()> {
    if !args.is_empty() {
        bail!(format!(
            "native backend does not support extra args for now {:?}",
            args
        ))
    }
    run(path.to_str().unwrap(), path, args, verbose)
}

fn run(command: &str, path: &Path, args: &[String], verbose: bool) -> anyhow::Result<()> {
    if verbose {
        eprintln!("{} {} {}", command, path.display(), args.join(" "));
    }
    let mut execution = Command::new(command)
        .arg(path)
        .arg("--")
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context(format!(
            "failed to execute: {} {} {}",
            command,
            path.display(),
            if args.is_empty() {
                "".to_string()
            } else {
                format!("-- {}", args.join(" "))
            }
        ))?;
    let status = execution.wait()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("failed to run")
    }
}
