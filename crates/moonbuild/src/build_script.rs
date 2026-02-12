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
};

use anyhow::{Context, anyhow};
use log::warn;
use moonutil::{
    build_script::{BuildScriptEnvironment, BuildScriptOutput},
    mooncakes::ModuleName,
};
use regex::{Captures, Regex};

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
        let Some(node) = moonutil::BINARIES.node.as_ref() else {
            anyhow::bail!(
                "Running prebuild script for module {} needs `node` executable in PATH",
                m
            )
        };
        let mut cmd = Command::new(node);
        cmd.arg("--").arg(prebuild);
        Ok(cmd)
    } else if prebuild.ends_with(".py") {
        let Some(py) = moonutil::BINARIES.python.as_ref() else {
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
