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

//! The formatter's pipeline
//!
//! The formatter only needs a bare minimum project to run, so its pipeline
//! bypasses the regular compilation pipeline of resolving and discovering
//! modules and packages.
//!
//! This pipeline still strives to use as much of the existing infrastructure
//! as possible.
//!
//! # Maintainers
//!
//! If a similar no-resolving, files-only command is needed, refactor this
//! module into a more generic one, probably named "source utility" or similar.

use log::*;
use std::{collections::HashSet, ffi::OsStr, path::Path};

use moonutil::project::ProjectManifest;
use moonutil::resolution::{ModuleSourceKind, ResolvedModule};
use moonutil::toolchain::BINARIES;
use moonutil::{
    cond_expr::OptLevel,
    constants::{MOON_MOD, MOON_MOD_JSON, MOON_PKG, MOON_PKG_JSON, MOON_WORK},
    manifest::validate_module_dsl_deps,
    user_log::UserLog,
};

use crate::{
    discover::{DiscoveredLocalProject, DiscoveredPackage, discover_local_project},
    execution_plan::{
        ExecutionAction, ExecutionPlan, ExecutionPlanBuilder, ExternalInput, LoweredCommand,
    },
    model::PackageId,
    pkg_name::{PackageFQN, PackagePath},
    resolve::ResolveError,
    target_layout::TargetLayout,
};

pub type FmtResolveOutput = DiscoveredLocalProject;

/// Perform a barebones, faked resolving process for `moon fmt`.
///
/// This supports either a single module rooted at `source_dir` or a workspace
/// rooted there via `moon.work`.
pub fn resolve_for_fmt(
    source_dir: &Path,
    project_manifest: &ProjectManifest,
    user_log: &UserLog,
) -> Result<FmtResolveOutput, ResolveError> {
    info!(
        "Resolving formatter environment for {}",
        source_dir.display()
    );
    discover_local_project(source_dir, project_manifest, user_log).map_err(ResolveError::from)
}

pub struct FmtConfig {
    /// Checks the formatting without writing to files
    pub check_only: bool,

    /// Extra arguments to pass to the formatter
    pub extra_args: Vec<String>,

    /// Warn instead of showing differences
    pub warn_only: bool,

    /// Migrate moon.mod.json to moon.mod when only the JSON file exists.
    pub migrate_moon_mod_json: bool,

    /// Migrate moon.pkg.json to moon.pkg when only the JSON file exists.
    pub migrate_moon_pkg_json: bool,
}

/// Generate the executor-neutral actions for the formatter operation.
///
/// If `selected_packages` is non-empty, only the specified packages will be formatted.
/// Otherwise, all packages in the current module or workspace will be formatted.
pub fn build_execution_plan_for_fmt(
    resolved: &FmtResolveOutput,
    cfg: &FmtConfig,
    target_dir: &Path,
    selected_packages: &[PackageId],
    project_manifest: &ProjectManifest,
    user_log: &UserLog,
) -> anyhow::Result<ExecutionPlan> {
    info!(
        "Building format execution plan for {} root modules",
        resolved.root_module_ids.len()
    );

    let layout =
        TargetLayout::from_fmt_resolve_output(target_dir.into(), resolved, OptLevel::Release);

    debug!("Layout built for formatting");

    let mut execution = ExecutionPlanBuilder::default();
    let mut package_count = 0;
    let selected_packages = (!selected_packages.is_empty())
        .then(|| selected_packages.iter().copied().collect::<HashSet<_>>());
    let has_workspace_manifest = selected_packages.is_none()
        && format_workspace_node(&mut execution, cfg, &layout, project_manifest)?;
    let mut has_module_manifest = false;

    // If no path filter is provided, find and format `moon.mod`/`moon.mod.json`.
    if selected_packages.is_none() {
        for &module_id in &resolved.root_module_ids {
            let module = &resolved.root_modules[module_id];
            match module.source().source() {
                ModuleSourceKind::Local(path) | ModuleSourceKind::Stdlib(path) => {
                    has_module_manifest |=
                        format_moon_mod_node(&mut execution, cfg, &layout, module, path, user_log)?
                }
                ModuleSourceKind::Registry
                | ModuleSourceKind::Git(_)
                | ModuleSourceKind::SingleFile(_) => (),
            };
        }
    }

    for &module_id in &resolved.root_module_ids {
        let Some(packages) = resolved.pkg_dirs.packages_for_module(module_id) else {
            continue;
        };

        for &id in packages.values() {
            if let Some(selected_packages) = &selected_packages
                && !selected_packages.contains(&id)
            {
                continue;
            }

            let pkg = resolved.pkg_dirs.get_package(id);
            info!("Processing package {}", pkg.fqn);
            build_for_package(&mut execution, cfg, &layout, pkg, user_log)?;
            package_count += 1;
        }
    }

    if package_count == 0 && !has_workspace_manifest && !has_module_manifest {
        anyhow::bail!("No packages found in workspace to format");
    }

    Ok(execution.finish([]))
}

fn add_format_action<I, O>(
    execution: &mut ExecutionPlanBuilder,
    inputs: I,
    outputs: O,
    args: Vec<String>,
    description: String,
    cache_eligible: bool,
    can_dirty_on_output: bool,
) where
    I: IntoIterator,
    I::Item: Into<std::path::PathBuf>,
    O: IntoIterator,
    O::Item: Into<std::path::PathBuf>,
{
    let command = LoweredCommand::from(args);
    let external_inputs = command
        .executable()
        .map(|path| vec![ExternalInput::File(path.to_path_buf())])
        .unwrap_or_default();
    let action = ExecutionAction::new(
        inputs.into_iter().map(Into::into).collect(),
        outputs.into_iter().map(Into::into).collect(),
        command,
        description.clone(),
        description,
    )
    .with_external_inputs(external_inputs)
    .with_cache_eligible(cache_eligible)
    .with_can_dirty_on_output(can_dirty_on_output);
    execution.add_action(action, []);
}

fn format_moon_mod_node(
    execution: &mut ExecutionPlanBuilder,
    cfg: &FmtConfig,
    layout: &TargetLayout,
    module: &ResolvedModule,
    module_dir: &Path,
    user_log: &UserLog,
) -> anyhow::Result<bool> {
    let moon_mod = module_dir.join(MOON_MOD);
    let moon_mod_json = module_dir.join(MOON_MOD_JSON);

    let has_dsl = moon_mod.exists();
    let has_json = moon_mod_json.exists();
    if !has_dsl && !has_json {
        return Ok(false);
    }

    let target_moon_mod = layout.format_artifact_path(
        &PackageFQN::new(module.source().clone(), PackagePath::empty()),
        OsStr::new(MOON_MOD),
    );

    if has_dsl {
        format_moon_mod_dsl(execution, cfg, &moon_mod, &target_moon_mod, module_dir)?;
    } else if cfg.migrate_moon_mod_json {
        user_log.warn(format!(
            "Migrating to {} at module root '{}', deprecated {} is removed.",
            MOON_MOD,
            module_dir.display(),
            MOON_MOD_JSON
        ));
        format_moon_mod_json_migrate(
            execution,
            cfg,
            &moon_mod_json,
            &target_moon_mod,
            &moon_mod,
            module.module_info(),
            module_dir,
        )?;
    }

    Ok(true)
}

fn format_moon_mod_dsl(
    execution: &mut ExecutionPlanBuilder,
    cfg: &FmtConfig,
    moon_mod: &Path,
    target_moon_mod: &Path,
    module_dir: &Path,
) -> anyhow::Result<()> {
    if cfg.check_only || cfg.warn_only {
        let mut cmd = vec![
            BINARIES.moonbuild.to_string_lossy().into_owned(),
            "tool".into(),
            "format-and-diff".into(),
            "--old".into(),
            moon_mod.to_string_lossy().into_owned(),
            "--new".into(),
            target_moon_mod.to_string_lossy().into_owned(),
        ];
        if cfg.warn_only {
            cmd.push("--warn".into());
        }

        add_format_action(
            execution,
            [moon_mod],
            [target_moon_mod],
            cmd,
            format!("check moon.mod format {}", module_dir.display()),
            true,
            cfg.warn_only,
        );
    } else {
        let fmt_cmd = vec![
            BINARIES.moonfmt.to_string_lossy().into_owned(),
            moon_mod.to_string_lossy().into_owned(),
            "-w".into(),
            "-o".into(),
            target_moon_mod.to_string_lossy().into_owned(),
        ];

        add_format_action(
            execution,
            [moon_mod],
            [target_moon_mod],
            fmt_cmd,
            format!("format moon.mod {}", module_dir.display()),
            false,
            false,
        );
    }

    Ok(())
}

fn format_moon_mod_json_migrate(
    execution: &mut ExecutionPlanBuilder,
    cfg: &FmtConfig,
    moon_mod_json: &Path,
    target_moon_mod: &Path,
    moon_mod: &Path,
    module_info: &moonutil::manifest::MoonMod,
    module_dir: &Path,
) -> anyhow::Result<()> {
    // moon.mod `import` cannot represent local dependencies; those must live in moon.work.
    validate_module_dsl_deps(Some(&module_info.deps))?;

    if cfg.check_only || cfg.warn_only {
        let mut cmd = vec![
            BINARIES.moonbuild.to_string_lossy().into_owned(),
            "tool".into(),
            "format-and-diff".into(),
            "--old".into(),
            moon_mod_json.to_string_lossy().into_owned(),
            "--new".into(),
            target_moon_mod.to_string_lossy().into_owned(),
        ];
        if cfg.warn_only {
            cmd.push("--warn".into());
        }

        add_format_action(
            execution,
            [moon_mod_json],
            [target_moon_mod],
            cmd,
            format!("check moon.mod.json migration {}", module_dir.display()),
            true,
            cfg.warn_only,
        );
    } else {
        let migrate_cmd = vec![
            BINARIES.moonbuild.to_string_lossy().into_owned(),
            "tool".into(),
            "migrate-manifest".into(),
            "--old".into(),
            moon_mod_json.to_string_lossy().into_owned(),
            "--dest".into(),
            moon_mod.to_string_lossy().into_owned(),
        ];
        add_format_action(
            execution,
            [moon_mod_json],
            [moon_mod],
            migrate_cmd,
            format!("migrate moon.mod.json {}", module_dir.display()),
            false,
            false,
        );
    }

    Ok(())
}

fn format_workspace_node(
    execution: &mut ExecutionPlanBuilder,
    cfg: &FmtConfig,
    layout: &TargetLayout,
    project_manifest: &ProjectManifest,
) -> anyhow::Result<bool> {
    let ProjectManifest::Workspace(workspace) = project_manifest else {
        return Ok(false);
    };

    let target_moon_work = layout.format_root_artifact_path(std::ffi::OsStr::new(MOON_WORK));
    format_moon_work_dsl(execution, cfg, workspace.manifest_path(), &target_moon_work)?;
    Ok(true)
}

fn build_for_package(
    execution: &mut ExecutionPlanBuilder,
    cfg: &FmtConfig,
    layout: &TargetLayout,
    pkg: &DiscoveredPackage,
    user_log: &UserLog,
) -> anyhow::Result<()> {
    let ignore_set = &pkg.raw.formatter.ignore;
    let prebuild_outputs = pkg
        .raw
        .pre_build
        .as_ref()
        .iter()
        .flat_map(|prebuild_plans| {
            prebuild_plans
                .iter()
                .flat_map(|plan| plan.output().iter().map(|path| path.as_str()))
        })
        .collect::<HashSet<_>>();

    let mut add_fmt_for_file = |file: &Path| -> anyhow::Result<()> {
        let name = file.file_name().and_then(|name| name.to_str());
        if name.is_some_and(|name| ignore_set.contains(name)) {
            debug!(
                "Skipping formatter input {} due to formatter.ignore",
                file.display()
            );
            return Ok(());
        }
        if name.is_some_and(|name| prebuild_outputs.contains(name)) {
            debug!(
                "Skipping formatter input {} due to pre-build output",
                file.display()
            );
            return Ok(());
        }

        format_node(execution, cfg, layout, pkg, file)?;
        Ok(())
    };

    for file in &pkg.source_files {
        add_fmt_for_file(file)?;
    }
    for file in &pkg.mbt_md_files {
        add_fmt_for_file(file)?;
    }

    // Always format moon.pkg when present; migration from moon.pkg.json is gated.
    format_moon_pkg_node(execution, cfg, layout, pkg, user_log)?;

    Ok(())
}

fn format_node(
    execution: &mut ExecutionPlanBuilder,
    cfg: &FmtConfig,
    layout: &TargetLayout,
    pkg: &DiscoveredPackage,
    file: &Path,
) -> anyhow::Result<()> {
    let out_file = layout
        .format_artifact_path(&pkg.fqn, file.file_name().expect("Should have filename"))
        .to_string_lossy()
        .into_owned();
    let cmd: Vec<String> = if cfg.check_only || cfg.warn_only {
        let mut cmd = vec![
            BINARIES.moonbuild.to_string_lossy().into_owned(),
            "tool".into(),
            "format-and-diff".into(),
            "--old".into(),
            file.to_string_lossy().into_owned(),
            "--new".into(),
            out_file.clone(),
        ];
        if cfg.warn_only {
            cmd.push("--warn".into());
        }
        cmd.extend_from_slice(&cfg.extra_args);
        cmd
    } else {
        let mut cmd = vec![
            BINARIES.moonfmt.to_string_lossy().into_owned(),
            file.to_string_lossy().into_owned(),
            "-w".into(),
            "-o".into(),
            out_file.clone(),
        ];
        cmd.extend_from_slice(&cfg.extra_args);
        cmd
    };

    add_format_action(
        execution,
        [file],
        [out_file],
        cmd,
        format!("format {}", file.display()),
        cfg.check_only || cfg.warn_only,
        cfg.warn_only,
    );
    Ok(())
}

fn format_moon_work_dsl(
    execution: &mut ExecutionPlanBuilder,
    cfg: &FmtConfig,
    moon_work: &std::path::Path,
    target_moon_work: &std::path::Path,
) -> anyhow::Result<()> {
    if cfg.check_only || cfg.warn_only {
        let mut cmd = vec![
            BINARIES.moonbuild.to_string_lossy().into_owned(),
            "tool".into(),
            "format-workspace".into(),
            "--old".into(),
            moon_work.to_string_lossy().into_owned(),
            "--new".into(),
            target_moon_work.to_string_lossy().into_owned(),
            "--check".into(),
        ];
        if cfg.warn_only {
            cmd.pop();
            cmd.push("--warn".into());
        }

        add_format_action(
            execution,
            [moon_work],
            [target_moon_work],
            cmd,
            "check moon.work format".to_string(),
            true,
            cfg.warn_only,
        );
    } else {
        let fmt_cmd: Vec<String> = vec![
            BINARIES.moonbuild.to_string_lossy().into_owned(),
            "tool".into(),
            "format-workspace".into(),
            "--old".into(),
            moon_work.to_string_lossy().into_owned(),
            "--write".into(),
            "--new".into(),
            target_moon_work.to_string_lossy().into_owned(),
        ];

        add_format_action(
            execution,
            [moon_work],
            [target_moon_work],
            fmt_cmd,
            "format moon.work".to_string(),
            false,
            false,
        );
    }

    Ok(())
}

/// Format moon.pkg package configuration files and optionally migrate moon.pkg.json.
///
/// This function handles three scenarios:
/// 1. Both `moon.pkg` and `moon.pkg.json` exist: prefer `moon.pkg`, report error about duplicate
/// 2. Only `moon.pkg.json` exists: migrate to `moon.pkg` format if enabled
/// 3. Only `moon.pkg` exists: format it in place
fn format_moon_pkg_node(
    execution: &mut ExecutionPlanBuilder,
    cfg: &FmtConfig,
    layout: &TargetLayout,
    pkg: &DiscoveredPackage,
    user_log: &UserLog,
) -> anyhow::Result<()> {
    use moonutil::constants::{MOON_PKG, MOON_PKG_JSON};

    let moon_pkg_dsl = pkg.root_path.join(MOON_PKG);
    let moon_pkg_json = pkg.root_path.join(MOON_PKG_JSON);

    let has_dsl = moon_pkg_dsl.exists();
    let has_json = moon_pkg_json.exists();

    if !has_dsl && !has_json {
        debug!(
            "Skipping moon.pkg formatting for {} - no config file exists",
            pkg.fqn
        );
        return Ok(());
    }

    // Output to target directory
    let target_moon_pkg = layout.format_artifact_path(&pkg.fqn, std::ffi::OsStr::new("moon.pkg"));

    if has_dsl {
        // Format moon.pkg (new format)
        format_moon_pkg_dsl(execution, cfg, &moon_pkg_dsl, &target_moon_pkg, pkg)
    } else if cfg.migrate_moon_pkg_json {
        // Only moon.pkg.json exists: migrate to moon.pkg
        format_moon_pkg_json_migrate(
            execution,
            cfg,
            &moon_pkg_json,
            &target_moon_pkg,
            &moon_pkg_dsl,
            pkg,
            user_log,
        )
    } else {
        debug!(
            "Skipping moon.pkg.json migration for {} - feature disabled",
            pkg.fqn
        );
        Ok(())
    }
}

/// Format an existing moon.pkg (DSL format) file.
///
/// - moon_pkg: Path to the source moon.pkg file
/// - target_moon_pkg: Path to the output formatted moon.pkg file
fn format_moon_pkg_dsl(
    execution: &mut ExecutionPlanBuilder,
    cfg: &FmtConfig,
    moon_pkg: &std::path::Path,
    target_moon_pkg: &std::path::Path,
    pkg: &DiscoveredPackage,
) -> anyhow::Result<()> {
    if cfg.check_only || cfg.warn_only {
        // In check/warn mode, use format-and-diff to compare
        let mut cmd = vec![
            BINARIES.moonbuild.to_string_lossy().into_owned(),
            "tool".into(),
            "format-and-diff".into(),
            "--old".into(),
            moon_pkg.to_string_lossy().into_owned(),
            "--new".into(),
            target_moon_pkg.to_string_lossy().into_owned(),
        ];
        if cfg.warn_only {
            cmd.push("--warn".into());
        }

        add_format_action(
            execution,
            [moon_pkg],
            [target_moon_pkg],
            cmd,
            format!("check moon.pkg format {}", pkg.fqn),
            true,
            cfg.warn_only,
        );
    } else {
        // Format moon.pkg - use -w to write back to source and -o to target
        // This is consistent with how .mbt files are formatted
        let fmt_cmd: Vec<String> = vec![
            BINARIES.moonfmt.to_string_lossy().into_owned(),
            moon_pkg.to_string_lossy().into_owned(),
            "-w".into(),
            "-o".into(),
            target_moon_pkg.to_string_lossy().into_owned(),
        ];

        add_format_action(
            execution,
            [moon_pkg],
            [target_moon_pkg],
            fmt_cmd,
            format!("format moon.pkg {}", pkg.fqn),
            false,
            false,
        );
    }

    Ok(())
}

/// Migrate moon.pkg.json to moon.pkg (DSL format).
///
/// This function generates moon.pkg from moon.pkg.json and warns the user
/// to manually remove the deprecated moon.pkg.json file.
///
/// - moon_pkg_json: Path to the source moon.pkg.json file
/// - target_moon_pkg: Path to the output formatted moon.pkg file in the target directory
/// - moon_pkg: Path to the destination moon.pkg file in the source directory
fn format_moon_pkg_json_migrate(
    execution: &mut ExecutionPlanBuilder,
    cfg: &FmtConfig,
    moon_pkg_json: &std::path::Path,
    target_moon_pkg: &std::path::Path,
    moon_pkg: &std::path::Path,
    pkg: &DiscoveredPackage,
    user_log: &UserLog,
) -> anyhow::Result<()> {
    // Warn the user about migration and prompt to remove the old config
    user_log.warn(format!(
        "Migrating to {} in package '{}', deprecated {} is removed.",
        MOON_PKG, pkg.fqn, MOON_PKG_JSON
    ));

    if cfg.check_only || cfg.warn_only {
        // In check/warn mode, use format-and-diff to compare
        let mut cmd = vec![
            BINARIES.moonbuild.to_string_lossy().into_owned(),
            "tool".into(),
            "format-and-diff".into(),
            "--old".into(),
            moon_pkg_json.to_string_lossy().into_owned(),
            "--new".into(),
            target_moon_pkg.to_string_lossy().into_owned(),
        ];
        if cfg.warn_only {
            cmd.push("--warn".into());
        }

        add_format_action(
            execution,
            [moon_pkg_json],
            [target_moon_pkg],
            cmd,
            format!("check moon.pkg.json migration {}", pkg.fqn),
            true,
            cfg.warn_only,
        );
    } else {
        let migrate_cmd = vec![
            BINARIES.moonbuild.to_string_lossy().into_owned(),
            "tool".into(),
            "migrate-manifest".into(),
            "--old".into(),
            moon_pkg_json.to_string_lossy().into_owned(),
            "--dest".into(),
            moon_pkg.to_string_lossy().into_owned(),
        ];
        add_format_action(
            execution,
            [moon_pkg_json],
            [moon_pkg],
            migrate_cmd,
            format!("migrate moon.pkg.json {}", pkg.fqn),
            false,
            false,
        );
    }

    Ok(())
}
