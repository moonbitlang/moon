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

use crate::NODE_EXECUTABLE;

use super::gen;
use anyhow::Context;
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
    run(Some("moonrun"), path, args, verbose)
}

pub fn run_js(path: &Path, args: &[String], verbose: bool) -> anyhow::Result<()> {
    let node = NODE_EXECUTABLE.as_deref();
    run(node, path, args, verbose)
}

pub fn run_native(path: &Path, args: &[String], verbose: bool) -> anyhow::Result<()> {
    run(None, path, args, verbose)
}

fn run(runtime: Option<&str>, path: &Path, args: &[String], verbose: bool) -> anyhow::Result<()> {
    if verbose {
        if let Some(runtime) = runtime {
            eprintln!("{} {} {}", runtime, path.display(), args.join(" "));
        } else {
            eprintln!("{} {}", path.display(), args.join(" "));
        }
    }
    let mut subprocess = Command::new(if let Some(runtime) = runtime {
        runtime
    } else {
        path.to_str().unwrap()
    });

    if runtime.is_some() {
        subprocess.arg(path);
    }
    subprocess.args(args);

    let mut execution = subprocess
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context(format!(
            "failed to execute: {} {} {}",
            runtime.unwrap_or(""),
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
