//! Handles build configuration script running. Runs pre-build configuration
//! scripts and modify the build graph accordingly.

use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::Context;
use moonutil::{
    build_script::{BuildScriptEnvironment, BuildScriptOutput},
    common::{MoonbuildOpt, MooncOpt},
    module::{ModuleDB, MoonMod},
    mooncakes::{result::ResolvedEnv, ModuleId},
};
use regex::{Captures, Regex};

pub fn run_prebuild_config(
    moonc_opt: &MooncOpt,
    dir_sync_result: HashMap<ModuleId, PathBuf>,
    build_opt: &MoonbuildOpt,
    mods: &ResolvedEnv,
    mdb: &mut ModuleDB,
) -> anyhow::Result<()> {
    // This script currently uses the quickest and dirtiest way to
    // achieve the goals.
    // TODO: refactor and make it efficient and cleaner

    let env_vars = std::env::vars().collect::<HashMap<_, _>>();
    let mut pkg_outputs = HashMap::<String, BuildScriptOutput>::new();

    for module in mods.all_packages() {
        let id = mods.id_from_mod_name(module).expect("module not found");
        let def = mods.module_info(id);

        if let Some(prebuild) = &def.__moonbit_unstable_prebuild {
            // just run `node {prebuild.js}` and read the output
            let dir = dir_sync_result.get(&id).expect("module not found");
            let input =
                make_prebuild_input_from_module(moonc_opt, build_opt, &def, &dir, &env_vars);

            let output = run_build_script_for_module(module, dir, input, prebuild)?;
            pkg_outputs.insert(module.to_string(), output);
        }
    }

    let match_regex = Regex::new(r"${build.([a-zA-Z0-9_]+)}").unwrap();

    let pkgs = mdb.get_all_packages_mut();
    // Iterate over all pkgs and apply the vars
    for (_name, pkg) in pkgs.iter_mut() {
        let root = pkg.root.full_name();
        if let Some(output) = pkg_outputs.get(&root) {
            run_replace_in_package(pkg, &output.vars, &match_regex);
        }
    }

    Ok(())
}

fn run_replace_in_package(
    pkg: &mut moonutil::package::Package,
    env_vars: &HashMap<String, String>,
    regex: &Regex,
) {
    pkg.link.as_mut().map(|link| {
        link.native.as_mut().map(|native| {
            if let Some(cc) = native.cc.as_mut() {
                string_match_and_replace(cc, env_vars, regex);
            }
            if let Some(cc_flags) = native.cc_flags.as_mut() {
                string_match_and_replace(cc_flags, env_vars, regex);
            }
            if let Some(cc_link_flags) = native.cc_link_flags.as_mut() {
                string_match_and_replace(cc_link_flags, env_vars, regex);
            }
            if let Some(stub_cc) = native.stub_cc.as_mut() {
                string_match_and_replace(stub_cc, env_vars, regex);
            }
            if let Some(stub_cc_flags) = native.stub_cc_flags.as_mut() {
                string_match_and_replace(stub_cc_flags, env_vars, regex);
            }
            if let Some(stub_cc_link_flags) = native.stub_cc_link_flags.as_mut() {
                string_match_and_replace(stub_cc_link_flags, env_vars, regex);
            }
        });
    });
}

fn string_match_and_replace(s: &mut String, env_vars: &HashMap<String, String>, regex: &Regex) {
    let out = regex.replace_all(s, |cap: &Captures| {
        let name = cap.get(1).expect("failed to get capture group");
        let name = name.as_str();
        let value = env_vars.get(name).expect("failed to get env var"); // TODO: handle error
        value
    });
    match out {
        std::borrow::Cow::Borrowed(_) => {
            // s is not changed
        }
        std::borrow::Cow::Owned(new_s) => {
            *s = new_s;
        }
    }
}

fn run_build_script_for_module(
    module: &moonutil::mooncakes::ModuleSource,
    dir: &Path,
    input: BuildScriptEnvironment,
    prebuild: &String,
) -> Result<BuildScriptOutput, anyhow::Error> {
    let mut cmd = Command::new("node")
        .arg(prebuild)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| {
            format!(
                "failed to spawn prebuild script `{}` for module `{}`",
                prebuild, module
            )
        })?;
    let stdin = cmd.stdin.take().expect("Didn't get stdin");
    let join = std::thread::spawn(move || {
        let mut stdin = stdin;
        let input = serde_json::to_string(&input).expect("failed to serialize input");
        let _ = stdin.write_all(input.as_bytes());
    });
    let output = cmd.wait_with_output().with_context(|| {
        format!(
            "failed to run prebuild script `{}` for module `{}`",
            prebuild, module
        )
    })?;
    join.join().map_err(|_| {
        anyhow::anyhow!(
            "failed to join prebuild script `{}` for module `{}`",
            prebuild,
            module,
        )
    })?;
    if !output.status.success() {
        anyhow::bail!(
            "prebuild script `{}` for module `{}` failed",
            prebuild,
            module
        );
    }
    let output =
        serde_json::from_slice::<BuildScriptOutput>(&output.stdout).with_context(|| {
            format!(
                "failed to deserialize prebuild script `{}` for module `{}`",
                prebuild, module
            )
        })?;

    Ok(output)
}

fn make_prebuild_input_from_module(
    moonc_opt: &MooncOpt,
    build_opt: &MoonbuildOpt,
    m: &MoonMod,
    m_dir: &Path,
    env_vars: &HashMap<String, String>,
) -> BuildScriptEnvironment {
    let _ = m;
    let _ = build_opt;
    let _ = moonc_opt;
    BuildScriptEnvironment {
        // build: BuildInfo { host: TargetInfo },
        env: env_vars.clone(),
        paths: moonutil::build_script::Paths {
            module_root: m_dir.to_string_lossy().to_string(),
            out_dir: "TODO".to_string(),
        },
    }
}
