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

//! The formatter's pipeline
//!
//! Construct formatting actions from [`FmtProject`], produced by
//! [`crate::discover::discover_for_fmt`].

use log::*;
use std::{collections::HashSet, ffi::OsStr, path::Path};

use moonutil::project::ProjectManifest;
use moonutil::resolution::ModuleSourceKind;
use moonutil::toolchain::BINARIES;
use moonutil::{
    build_options::RunMode,
    cond_expr::OptLevel,
    constants::{MOON_MOD, MOON_MOD_JSON, MOON_PKG, MOON_PKG_JSON, MOON_WORK},
    manifest::{ManifestFormat, validate_module_dsl_deps},
    user_log::UserLog,
};

use crate::{
    discover::{FmtModule, FmtProject, PackageInput, resolve_prebuild_path},
    execution_plan::{ExecutionAction, ExecutionPlan, ExecutionPlanBuilder, LoweredCommand},
    model::PackageId,
    pkg_name::{PackageFQN, PackagePath},
    target_layout::{TargetLayout, TargetLayoutMode},
};

pub struct FmtConfig {
    /// Checks the formatting without writing to files
    pub check_only: bool,

    /// Extra arguments to pass to the formatter
    pub extra_args: Vec<String>,

    /// Warn instead of showing differences
    pub warn_only: bool,
}

/// Generate the executor-neutral actions for the formatter operation.
///
/// If `selected_packages` is non-empty, only the specified packages will be formatted.
/// Otherwise, all packages in the current module or workspace will be formatted.
pub fn build_execution_plan_for_fmt(
    project: &FmtProject,
    cfg: &FmtConfig,
    target_dir: &Path,
    selected_packages: &[PackageId],
    project_manifest: &ProjectManifest,
    user_log: &UserLog,
) -> anyhow::Result<ExecutionPlan> {
    info!(
        "Building format execution plan for {} root modules",
        project.modules.len()
    );

    let mode = match project.modules.as_slice() {
        [module] => TargetLayoutMode::Mono {
            main_module: module.module.source().clone(),
        },
        _ => TargetLayoutMode::Workspace,
    };
    let layout = TargetLayout::new(target_dir.into(), mode, OptLevel::Release, RunMode::Format);

    debug!("Layout built for formatting");

    let mut execution = ExecutionPlanBuilder::default();
    let mut package_count = 0;
    let selected_packages = (!selected_packages.is_empty())
        .then(|| selected_packages.iter().copied().collect::<HashSet<_>>());
    let has_workspace_manifest = selected_packages.is_none()
        && format_workspace_node(&mut execution, cfg, &layout, project_manifest)?;
    let mut has_module_manifest = false;

    if selected_packages.is_none() {
        for module in &project.modules {
            format_moon_mod_node(&mut execution, cfg, &layout, module, user_log)?;
            has_module_manifest = true;
        }
    }
    for module in &project.modules {
        for &id in module.packages.values() {
            if let Some(selected_packages) = &selected_packages
                && !selected_packages.contains(&id)
            {
                continue;
            }
            let pkg = &project.packages[id];
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

/// Format one explicitly selected file without loading a project or its dependencies.
/// `target_dir` is the target directory scoped to the standalone input.
pub fn build_execution_plan_for_fmt_file(
    cfg: &FmtConfig,
    file: &Path,
    target_dir: &Path,
) -> anyhow::Result<ExecutionPlan> {
    let out_file = target_dir
        .join("format")
        .join(file.file_name().expect("Should have filename"));
    let mut execution = ExecutionPlanBuilder::default();
    format_node(&mut execution, cfg, file, &out_file)?;
    Ok(execution.finish([]))
}

fn add_format_action<I, O>(
    execution: &mut ExecutionPlanBuilder,
    external_files: I,
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
    let inputs = external_files
        .into_iter()
        .map(Into::into)
        .chain(command.executable().map(Path::to_path_buf))
        .collect::<Vec<_>>();
    let action = ExecutionAction::new(
        inputs,
        outputs.into_iter().map(Into::into).collect(),
        command,
        description.clone(),
        description,
        crate::execution_plan::ActionOutputKind::Tool,
    )
    .with_cache_eligible(cache_eligible)
    .with_can_dirty_on_output(can_dirty_on_output);
    execution.add_action(action, []);
}

fn format_moon_mod_node(
    execution: &mut ExecutionPlanBuilder,
    cfg: &FmtConfig,
    layout: &TargetLayout,
    input: &FmtModule,
    user_log: &UserLog,
) -> anyhow::Result<()> {
    let module = &input.module;
    let ModuleSourceKind::Local(module_dir) = module.source().source() else {
        unreachable!("formatting only discovers local modules");
    };
    let moon_mod = module_dir.join(MOON_MOD);

    let target_moon_mod = layout.format_artifact_path(
        &PackageFQN::new(module.source().clone(), PackagePath::empty()),
        OsStr::new(MOON_MOD),
    );

    match input.manifest_format {
        ManifestFormat::New => {
            format_moon_mod_dsl(execution, cfg, &moon_mod, &target_moon_mod, module_dir)?;
        }
        ManifestFormat::Legacy => {
            user_log.warn(format!(
                "Migrating to {} at module root '{}', deprecated {} is removed.",
                MOON_MOD,
                module_dir.display(),
                MOON_MOD_JSON
            ));
            format_moon_mod_json_migrate(
                execution,
                cfg,
                &module_dir.join(MOON_MOD_JSON),
                &target_moon_mod,
                &moon_mod,
                module.module_info(),
                module_dir,
            )?;
        }
    }

    Ok(())
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
            dunce::simplified(&BINARIES.moonbuild)
                .to_string_lossy()
                .into_owned(),
            "tool".into(),
            "format-and-diff".into(),
            "--old".into(),
            dunce::simplified(moon_mod).to_string_lossy().into_owned(),
            "--new".into(),
            dunce::simplified(target_moon_mod)
                .to_string_lossy()
                .into_owned(),
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
            dunce::simplified(&BINARIES.moonfmt)
                .to_string_lossy()
                .into_owned(),
            dunce::simplified(moon_mod).to_string_lossy().into_owned(),
            "-w".into(),
            "-o".into(),
            dunce::simplified(target_moon_mod)
                .to_string_lossy()
                .into_owned(),
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
            dunce::simplified(&BINARIES.moonbuild)
                .to_string_lossy()
                .into_owned(),
            "tool".into(),
            "format-and-diff".into(),
            "--old".into(),
            dunce::simplified(moon_mod_json)
                .to_string_lossy()
                .into_owned(),
            "--new".into(),
            dunce::simplified(target_moon_mod)
                .to_string_lossy()
                .into_owned(),
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
            dunce::simplified(&BINARIES.moonbuild)
                .to_string_lossy()
                .into_owned(),
            "tool".into(),
            "migrate-manifest".into(),
            "--old".into(),
            dunce::simplified(moon_mod_json)
                .to_string_lossy()
                .into_owned(),
            "--dest".into(),
            dunce::simplified(moon_mod).to_string_lossy().into_owned(),
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
    pkg: &PackageInput,
    user_log: &UserLog,
) -> anyhow::Result<()> {
    let ignore_set = &pkg.manifest.formatter.ignore;
    let prebuild_outputs = pkg
        .manifest
        .pre_build
        .iter()
        .flatten()
        .flat_map(|plan| plan.output().iter())
        .map(|path| resolve_prebuild_path(&pkg.root_path, path))
        .collect::<HashSet<_>>();

    use moonutil::constants::{PackageSourceFileKind, package_source_file_kind};
    for file in &pkg.files {
        let name = file.file_name().expect("scanned file has a name");
        if !matches!(
            package_source_file_kind(&name.to_string_lossy()),
            Some(PackageSourceFileKind::Mbt | PackageSourceFileKind::MbtMd)
        ) {
            continue;
        }
        if name.to_str().is_some_and(|name| ignore_set.contains(name)) {
            debug!(
                "Skipping formatter input {} due to formatter.ignore",
                file.display()
            );
            continue;
        }
        if prebuild_outputs.contains(file) {
            debug!(
                "Skipping formatter input {} due to pre-build output",
                file.display()
            );
            continue;
        }

        let out_file = layout.format_artifact_path(&pkg.fqn, name);
        format_node(execution, cfg, file, &out_file)?;
    }

    // Always format moon.pkg when present; migration from moon.pkg.json is gated.
    format_moon_pkg_node(execution, cfg, layout, pkg, user_log)?;

    Ok(())
}

fn format_node(
    execution: &mut ExecutionPlanBuilder,
    cfg: &FmtConfig,
    file: &Path,
    out_file: &Path,
) -> anyhow::Result<()> {
    let output_arg = dunce::simplified(out_file).to_string_lossy().into_owned();
    let cmd: Vec<String> = if cfg.check_only || cfg.warn_only {
        let mut cmd = vec![
            dunce::simplified(&BINARIES.moonbuild)
                .to_string_lossy()
                .into_owned(),
            "tool".into(),
            "format-and-diff".into(),
            "--old".into(),
            dunce::simplified(file).to_string_lossy().into_owned(),
            "--new".into(),
            output_arg.clone(),
        ];
        if cfg.warn_only {
            cmd.push("--warn".into());
        }
        if !cfg.extra_args.is_empty() {
            cmd.push("--".into());
        }
        cmd.extend_from_slice(&cfg.extra_args);
        cmd
    } else {
        let mut cmd = vec![
            dunce::simplified(&BINARIES.moonfmt)
                .to_string_lossy()
                .into_owned(),
            dunce::simplified(file).to_string_lossy().into_owned(),
            "-w".into(),
            "-o".into(),
            output_arg.clone(),
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
            dunce::simplified(&BINARIES.moonbuild)
                .to_string_lossy()
                .into_owned(),
            "tool".into(),
            "format-workspace".into(),
            "--old".into(),
            dunce::simplified(moon_work).to_string_lossy().into_owned(),
            "--new".into(),
            dunce::simplified(target_moon_work)
                .to_string_lossy()
                .into_owned(),
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
            dunce::simplified(&BINARIES.moonbuild)
                .to_string_lossy()
                .into_owned(),
            "tool".into(),
            "format-workspace".into(),
            "--old".into(),
            dunce::simplified(moon_work).to_string_lossy().into_owned(),
            "--write".into(),
            "--new".into(),
            dunce::simplified(target_moon_work)
                .to_string_lossy()
                .into_owned(),
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
/// The scanner has already selected the manifest and reported shadowed formats.
/// Planning retains that selection: format DSL directly or migrate legacy JSON.
fn format_moon_pkg_node(
    execution: &mut ExecutionPlanBuilder,
    cfg: &FmtConfig,
    layout: &TargetLayout,
    pkg: &PackageInput,
    user_log: &UserLog,
) -> anyhow::Result<()> {
    let moon_pkg_dsl = pkg.root_path.join(MOON_PKG);

    // Output to target directory
    let target_moon_pkg = layout.format_artifact_path(&pkg.fqn, std::ffi::OsStr::new("moon.pkg"));

    match pkg.manifest_format {
        ManifestFormat::New => {
            format_moon_pkg_dsl(execution, cfg, &moon_pkg_dsl, &target_moon_pkg, pkg)
        }
        ManifestFormat::Legacy => format_moon_pkg_json_migrate(
            execution,
            cfg,
            &pkg.root_path.join(MOON_PKG_JSON),
            &target_moon_pkg,
            &moon_pkg_dsl,
            pkg,
            user_log,
        ),
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
    pkg: &PackageInput,
) -> anyhow::Result<()> {
    if cfg.check_only || cfg.warn_only {
        // In check/warn mode, use format-and-diff to compare
        let mut cmd = vec![
            dunce::simplified(&BINARIES.moonbuild)
                .to_string_lossy()
                .into_owned(),
            "tool".into(),
            "format-and-diff".into(),
            "--old".into(),
            dunce::simplified(moon_pkg).to_string_lossy().into_owned(),
            "--new".into(),
            dunce::simplified(target_moon_pkg)
                .to_string_lossy()
                .into_owned(),
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
            dunce::simplified(&BINARIES.moonfmt)
                .to_string_lossy()
                .into_owned(),
            dunce::simplified(moon_pkg).to_string_lossy().into_owned(),
            "-w".into(),
            "-o".into(),
            dunce::simplified(target_moon_pkg)
                .to_string_lossy()
                .into_owned(),
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
    pkg: &PackageInput,
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
            dunce::simplified(&BINARIES.moonbuild)
                .to_string_lossy()
                .into_owned(),
            "tool".into(),
            "format-and-diff".into(),
            "--old".into(),
            dunce::simplified(moon_pkg_json)
                .to_string_lossy()
                .into_owned(),
            "--new".into(),
            dunce::simplified(target_moon_pkg)
                .to_string_lossy()
                .into_owned(),
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
            dunce::simplified(&BINARIES.moonbuild)
                .to_string_lossy()
                .into_owned(),
            "tool".into(),
            "migrate-manifest".into(),
            "--old".into(),
            dunce::simplified(moon_pkg_json)
                .to_string_lossy()
                .into_owned(),
            "--dest".into(),
            dunce::simplified(moon_pkg).to_string_lossy().into_owned(),
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

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // Mutate a copied fixture between discovery and planning.
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::discover::discover_for_fmt;

    #[test]
    fn formatting_plan_reuses_the_selected_manifests() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = std::fs::canonicalize(dir.path())?;
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fmt_manifest_selection");
        for file in [MOON_MOD_JSON, MOON_PKG_JSON] {
            std::fs::copy(fixture.join("initial").join(file), root.join(file))?;
        }
        let log = UserLog::new(log::LevelFilter::Off);
        let project = discover_for_fmt(&root, &ProjectManifest::None, &log)?;
        // Planning consumes discovery's selection even if another format has
        // appeared in the meantime. It must not select or parse a second time.
        for file in [MOON_MOD, MOON_PKG] {
            std::fs::copy(fixture.join("appeared").join(file), root.join(file))?;
        }
        let plan = build_execution_plan_for_fmt(
            &project,
            &FmtConfig {
                check_only: true,
                warn_only: false,
                extra_args: Vec::new(),
            },
            &root.join("_build"),
            &[],
            &ProjectManifest::None,
            &log,
        )?;
        let mut inputs = plan
            .action_ids()
            .flat_map(|id| plan.action(id).inputs())
            .filter(|path| path.parent() == Some(root.as_path()))
            .cloned()
            .collect::<Vec<_>>();
        inputs.sort();
        assert_eq!(inputs, [root.join(MOON_MOD_JSON), root.join(MOON_PKG_JSON)]);
        Ok(())
    }

    #[test]
    fn formatter_sources_are_file_observations() {
        let source = PathBuf::from("src/main.mbt");
        let executable = PathBuf::from("bin/moonfmt");
        let mut builder = ExecutionPlanBuilder::default();
        add_format_action(
            &mut builder,
            [&source],
            ["_build/main.mbt"],
            vec![
                executable.display().to_string(),
                source.display().to_string(),
            ],
            "format src/main.mbt".to_string(),
            true,
            false,
        );

        let plan = builder.finish([]);
        let action = plan.action(plan.action_ids().next().expect("one formatter action"));
        assert_eq!(action.inputs(), &[executable, source]);
    }
}
