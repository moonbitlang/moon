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

//! Compile and run module-level configuration scripts before native planning.

use std::{
    collections::HashMap,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, anyhow};
use log::warn;
use moonbuild_rupes_recta::{
    ResolveOutput,
    compile::CompileConfig,
    prebuild::{ModulePrebuildOutput, PrebuildOutput},
};
use moonutil::{
    build_script::{BuildScriptEnvironment, BuildScriptOutput, Paths},
    constants::{MOON_MOD, MOON_MOD_JSON},
    manifest::preferred_manifest_in_dir,
    project::SourceTargetDirs,
    resolution::ModuleName,
};
use tracing::instrument;

use crate::{
    cli::run::{build_standalone_wasm, effective_moonrun_policy},
    run::{ExecutionMode, command_for_with_moonrun_policy_source_dir},
};

#[instrument(skip_all)]
pub(super) fn run_prebuild_config(
    resolve_output: &ResolveOutput,
    cx: &CompileConfig,
    jobs: usize,
    frozen: bool,
) -> anyhow::Result<PrebuildOutput> {
    let environment: HashMap<String, String> = std::env::vars().collect();
    let build_dir = cx
        .artifact_paths
        .target_layout()
        .run_mode_dir(cx.backend.target_backend())
        .join("prebuild");
    let mut output = PrebuildOutput::default();
    for (m, ms) in resolve_output.module_rel.all_modules_and_id() {
        let m_info = resolve_output.module_info(m);
        let m_dir = resolve_output.module_dirs.get(m).expect("module not found");
        let Some(prebuild) = &m_info.__moonbit_unstable_prebuild else {
            continue;
        };
        // Keep generated files in caller-owned storage, separated by module and
        // the build configuration selected by the existing target layout.
        let module_hash = blake3::hash(m_dir.as_os_str().as_encoded_bytes()).to_hex();
        let out_dir = build_dir.join(module_hash.as_str());
        std::fs::create_dir_all(&out_dir).with_context(|| {
            format!(
                "failed to create prebuild output directory `{}`",
                out_dir.display()
            )
        })?;
        let input = BuildScriptEnvironment {
            env: environment.clone(),
            paths: Paths {
                module_root: m_dir.to_string_lossy().into_owned(),
                out_dir: out_dir.to_string_lossy().into_owned(),
            },
        };
        let script_output =
            run_build_script_for_module(ms, m_dir, input, prebuild, cx, jobs, frozen)
                .with_context(|| {
                    format!("Failed to run prebuild script for module {}", m_info.name)
                })?;

        // Insert module-level configs
        let module_output = ModulePrebuildOutput {
            rerun_if: script_output.rerun_if,
            vars: script_output.vars,
        };
        output.module_outputs.insert(m, module_output);

        // Insert package-level configs
        let module_name = &m_info.name;
        let packages = resolve_output
            .pkg_dirs
            .packages_for_module(m)
            .expect("module has no packages");
        for cfg in script_output.link_configs {
            // Find the package ID from its name
            let stripped_pkg_name = cfg
                .package
                .strip_prefix(module_name.as_str())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Link config package name {} does not start with module name {}, cannot apply config to an external package",
                        cfg.package,
                        module_name
                    )
                })?
                .trim_start_matches('/');
            let pkg_id = *packages.get(stripped_pkg_name).ok_or_else(|| {
                anyhow::anyhow!(
                    "Link config package name {} does not match any package in module {}",
                    cfg.package,
                    module_name
                )
            })?;
            output.package_configs.insert(pkg_id, cfg);
        }
    }
    Ok(output)
}

fn run_script_cmd(
    prebuild: &str,
    m: &ModuleName,
    dir: &Path,
    target_dir: &Path,
    frozen: bool,
) -> anyhow::Result<Command> {
    if prebuild.ends_with(".js") || prebuild.ends_with(".cjs") || prebuild.ends_with(".mjs") {
        let Some(node) = moonutil::toolchain::BINARIES.node.as_ref() else {
            anyhow::bail!(
                "Running prebuild script for module {} needs `node` executable in PATH",
                m
            )
        };
        let mut cmd = Command::new(node);
        cmd.arg("--").arg(prebuild);
        Ok(cmd)
    } else if prebuild.ends_with(".py") {
        let Some(py) = moonutil::toolchain::BINARIES.python.as_ref() else {
            anyhow::bail!(
                "Running prebuild script for module {} needs `python` or `python3` executable in PATH",
                m
            )
        };
        let mut cmd = Command::new(py);
        cmd.arg("--").arg(prebuild);
        Ok(cmd)
    } else if prebuild.ends_with(".mbtx") {
        let script = dunce::canonicalize(dir.join(prebuild))
            .with_context(|| format!("failed to resolve prebuild script `{prebuild}`"))?;
        // Each script gets stable build storage owned by the caller, even when
        // its source belongs to an immutable, shared registry dependency.
        let script_id = blake3::hash(script.as_os_str().as_encoded_bytes()).to_hex();
        let dirs = SourceTargetDirs {
            cwd: None,
            target_dir: Some(target_dir.join("prebuild").join(script_id.as_str())),
        }
        .single_file_package_dirs(script)?;
        let built = build_standalone_wasm(dirs, frozen, false)?;
        let (policy, policy_source_dir) =
            effective_moonrun_policy(None, built.embedded_mbtx_policy.as_ref());
        Ok(command_for_with_moonrun_policy_source_dir(
            ExecutionMode::MoonRun,
            &built.executable,
            None,
            policy,
            policy_source_dir,
        ))
    } else {
        Err(anyhow!(
            "Unknown extension for build script `{}` of module {}.
                Currently allowed:
                  (running with node) .js, .cjs, .mjs
                  (running with python) .py
                  (compiled to wasm and running with moonrun) .mbtx",
            prebuild,
            m
        ))
    }
}

fn run_build_script_for_module(
    module: &moonutil::resolution::ModuleSource,
    dir: &Path,
    input: BuildScriptEnvironment,
    prebuild: &str,
    cx: &CompileConfig,
    jobs: usize,
    frozen: bool,
) -> Result<BuildScriptOutput, anyhow::Error> {
    // TODO: This executes arbitrary scripts. It's essentially the same as
    // `build.rs` -- the user must check for the safeness of the build script
    // themselves.
    warn!(
        "Running external prebuild config at `{}`. The script can execute arbitrary code.",
        prebuild
    );
    let (manifest_path, _) = preferred_manifest_in_dir(dir, MOON_MOD, MOON_MOD_JSON)
        .with_context(|| format!("failed to locate module manifest for `{module}`"))?;
    let mut cmd = run_script_cmd(prebuild, module.name(), dir, &cx.target_dir, frozen)?
        .current_dir(dir)
        .envs(&input.env)
        .env("MOON_MOD", manifest_path)
        .env("MOON_BUILD_DIR", &input.paths.out_dir)
        .env("MOON_HOST_OS", std::env::consts::OS)
        .env("MOON_HOST_ARCH", std::env::consts::ARCH)
        .env("MOON_BACKEND", cx.backend.target_backend().to_flag())
        .env("MOON_PROFILE", cx.opt_level.as_str())
        .env("MOON_JOBS", jobs.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| {
            format!("failed to spawn prebuild script `{prebuild}` for module `{module}`")
        })?;
    // TODO: Remove this stdin transport once compatibility with scripts reading
    // the legacy JSON input is no longer required.
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
