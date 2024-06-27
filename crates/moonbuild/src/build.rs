use super::gen;
use anyhow::bail;
use moonutil::common::gen::ModuleDB;
use moonutil::common::MoonbuildOpt;
use n2::load::State;
use std::io::BufRead;
use std::path::Path;
use std::process::{Command, Stdio};

use moonutil::common::MooncOpt;

pub fn load_moon_proj(
    module: &ModuleDB,
    moonc_opt: &MooncOpt,
    moonbuild_opt: &MoonbuildOpt,
) -> anyhow::Result<State> {
    let target_dir = &moonbuild_opt.target_dir;

    let mut contain_link_item = false;
    for (_, pkg) in module.packages.iter() {
        if pkg.is_main || pkg.need_link {
            contain_link_item = true;
            break;
        }
    }
    if !contain_link_item {
        anyhow::bail!("no package need to be linked in the project")
    }

    log::debug!("module: {:#?}", module);
    let n2_input = gen::gen_build::gen_build(module, moonc_opt, moonbuild_opt)?;
    log::debug!("n2_input: {:#?}", n2_input);
    gen::gen_build::gen_n2_build_state(&n2_input, target_dir, moonc_opt, moonbuild_opt)
}

pub fn run_wat(path: &Path, args: &[String]) -> anyhow::Result<()> {
    run("moonrun", path, args)
}

pub fn run_js(path: &Path, args: &[String]) -> anyhow::Result<()> {
    if !args.is_empty() {
        bail!(format!(
            "js backend does not support extra args for now {:?}",
            args
        ))
    }
    run("node", path, args)
}

fn run(command: &str, path: &Path, args: &[String]) -> anyhow::Result<()> {
    let mut execution = Command::new(command)
        .arg(path)
        .arg("--")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let child_stdout = execution.stdout.take().unwrap();
    let mut buf = String::new();
    let mut bufread = std::io::BufReader::new(child_stdout);
    while let Ok(n) = bufread.read_line(&mut buf) {
        if n > 0 {
            print!("{}", buf);
            buf.clear()
        } else {
            break;
        }
    }
    let status = execution.wait()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("failed to run")
    }
}
