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

//! Handles build configuration script running. Runs pre-build configuration
//! scripts and modify the build graph accordingly.

use std::{
    collections::HashMap,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    str::FromStr,
};

use anyhow::{Context, anyhow};
use log::warn;
use moonutil::{
    build_script::{BuildScriptEnvironment, BuildScriptOutput},
    module::ModuleDB,
    mooncakes::{DirSyncResult, ModuleName, result::ResolvedEnv},
    path::PathComponent,
};
use regex::{Captures, Regex};

use crate::{NODE_EXECUTABLE, PYTHON_EXECUTABLE};

pub fn run_prebuild_config(
    dir_sync_result: &DirSyncResult,
    mods: &ResolvedEnv,
    mdb: &mut ModuleDB,
) -> anyhow::Result<()> {
    // This script currently uses the quickest and dirtiest way to
    // achieve the goals.
    // TODO: refactor and make it efficient and cleaner

    let env_vars: HashMap<String, String> = std::env::vars().collect();
    let mut pkg_outputs = HashMap::<PathComponent, BuildScriptOutput>::new();

    for (id, module) in mods.all_modules_and_id() {
        let def = mods.module_info(id);

        if let Some(prebuild) = &def.__moonbit_unstable_prebuild {
            // just run `node {prebuild.js}` and read the output
            let dir = dir_sync_result.get(id).expect("module not found");
            let input = make_prebuild_input_from_module(dir, &env_vars);

            let output = run_build_script_for_module(module, dir, input, prebuild)?;
            pkg_outputs.insert(
                PathComponent::from_str(&module.name().to_string()).with_context(|| {
                    format!("Name of module `{}` cannot be parsed", &module.name())
                })?,
                output,
            );
        }
    }

    let match_regex = Regex::new(r"\$\{build\.([a-zA-Z0-9_]+)\}").unwrap();

    let pkgs = mdb.get_all_packages_mut();
    // Iterate over all pkgs and apply the vars
    for (_name, pkg) in pkgs.iter_mut() {
        if let Some(output) = pkg_outputs.get(&pkg.root) {
            run_replace_in_package(pkg, &output.vars, &match_regex).with_context(|| {
                format!(
                    "when handling replace in package {} from build script output of {:?}",
                    _name, &pkg.root
                )
            })?;
        }
    }

    // Apply link configs to packages
    for (_mod, output) in pkg_outputs {
        apply_output(output, mdb);
    }

    Ok(())
}

fn apply_output(output: BuildScriptOutput, mdb: &mut ModuleDB) {
    // Set the link flags and stuff
    for link_cfg in output.link_configs {
        // FIXME: We don't check whether the package and outputs match yet. This
        // means a module might be able to modify some other package's link config.
        // This is a bug that needs to be address further down the polish.
        let Some(pkg) = mdb.get_package_by_name_mut_safe(&link_cfg.package) else {
            continue;
        };
        pkg.link_flags = link_cfg.link_flags;
        pkg.link_libs = link_cfg.link_libs;
        pkg.link_search_paths = link_cfg.link_search_paths;
    }
}

fn run_replace_in_package(
    pkg: &mut moonutil::package::Package,
    env_vars: &HashMap<String, String>,
    regex: &Regex,
) -> anyhow::Result<()> {
    if let Some(link) = pkg.link.as_mut() {
        if let Some(native) = link.native.as_mut() {
            if let Some(cc) = native.cc.as_mut() {
                string_match_and_replace(cc, env_vars, regex).context("when replacing cc")?;
            }
            if let Some(cc_flags) = native.cc_flags.as_mut() {
                string_match_and_replace(cc_flags, env_vars, regex)
                    .context("when replacing cc_flags")?;
            }
            if let Some(cc_link_flags) = native.cc_link_flags.as_mut() {
                string_match_and_replace(cc_link_flags, env_vars, regex)
                    .context("when replacing cc_link_flags")?;
            }
            if let Some(stub_cc) = native.stub_cc.as_mut() {
                string_match_and_replace(stub_cc, env_vars, regex)
                    .context("when replacing stub_cc")?;
            }
            if let Some(stub_cc_flags) = native.stub_cc_flags.as_mut() {
                string_match_and_replace(stub_cc_flags, env_vars, regex)
                    .context("when replacing stub_cc_flags")?;
            }
            if let Some(stub_cc_link_flags) = native.stub_cc_link_flags.as_mut() {
                string_match_and_replace(stub_cc_link_flags, env_vars, regex)
                    .context("when replacing stub_cc_link_flags")?;
            }
        }
    }
    Ok(())
}

pub fn string_match_and_replace(
    s: &mut String,
    env_vars: &HashMap<String, String>,
    regex: &Regex,
) -> anyhow::Result<()> {
    let mut err = None;
    let out = regex.replace_all(s, |cap: &Captures| {
        let name = cap.get(1).expect("failed to get capture group");
        let name = name.as_str();
        let Some(value) = env_vars.get(name) else {
            err = Some(anyhow::anyhow!("Unable to find env var `{}`", name));
            return "";
        };
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
    Ok(())
}

fn run_script_cmd(prebuild: &String, m: &ModuleName) -> anyhow::Result<Command> {
    if prebuild.ends_with(".js") || prebuild.ends_with(".cjs") || prebuild.ends_with(".mjs") {
        let Some(node) = NODE_EXECUTABLE.as_ref() else {
            anyhow::bail!(
                "Running prebuild script for module {} needs `node` executable in PATH",
                m
            )
        };
        let mut cmd = Command::new(node);
        cmd.arg("--").arg(prebuild);
        Ok(cmd)
    } else if prebuild.ends_with(".py") {
        let Some(py) = PYTHON_EXECUTABLE.as_ref() else {
            anyhow::bail!(
                "Running prebuild script for module {} needs `python` or `python3` executable in PATH",
                m
            )
        };
        let mut cmd = Command::new(py);
        cmd.arg("--").arg(prebuild);
        Ok(cmd)
    } else {
        Err(anyhow!(
            "Unknown extension for build script `{}` of module {}.
                Currently allowed:
                  (running with node) .js, .cjs, .mjs
                  (running with python) .py",
            prebuild,
            m
        ))
    }
}

pub fn run_build_script_for_module(
    module: &moonutil::mooncakes::ModuleSource,
    dir: &Path,
    input: BuildScriptEnvironment,
    prebuild: &String,
) -> Result<BuildScriptOutput, anyhow::Error> {
    // TODO: This executes arbitrary scripts. It's essentially the same as
    // `build.rs` -- the user must check for the safeness of the build script
    // themselves.
    warn!(
        "Running external prebuild config at `{}`. The script can execute arbitrary code.",
        prebuild
    );
    let mut cmd = run_script_cmd(prebuild, module.name())?
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| {
            format!("failed to spawn prebuild script `{prebuild}` for module `{module}`")
        })?;
    let stdin = cmd.stdin.take().expect("Didn't get stdin");
    let join = std::thread::spawn(move || {
        let mut stdin = stdin;
        let input = serde_json::to_string(&input).expect("failed to serialize input");
        let _ = stdin.write_all(input.as_bytes());
    });
    let output = cmd.wait_with_output().with_context(|| {
        format!("failed to run prebuild script `{prebuild}` for module `{module}`")
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
            format!("failed to deserialize prebuild script `{prebuild}` for module `{module}`")
        })?;

    Ok(output)
}

pub fn make_prebuild_input_from_module(
    m_dir: &Path,
    env_vars: &HashMap<String, String>,
) -> BuildScriptEnvironment {
    BuildScriptEnvironment {
        // build: BuildInfo { host: TargetInfo },
        env: env_vars.clone(),
        paths: moonutil::build_script::Paths {
            module_root: m_dir.to_string_lossy().to_string(),
            out_dir: "TODO".to_string(),
        },
    }
}
