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
//! bypasses thew regular compilation pipeline of resolving and discovering
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
use std::{collections::HashSet, path::Path};

use moonutil::common::{MOON_PKG, MOON_PKG_JSON, MOON_WORK, MOON_WORK_JSON};
use n2::graph::Build;

use crate::{
    build_lower::{
        artifact::{LegacyLayout, LegacyLayoutBuilder},
        build_ins, build_n2_fileloc, build_outs,
    },
    discover::{DiscoveredLocalProject, DiscoveredPackage, discover_local_project},
    model::PackageId,
    resolve::ResolveError,
};

pub type FmtResolveOutput = DiscoveredLocalProject;

/// Perform a barebones, faked resolving process for `moon fmt`.
///
/// This supports either a single module rooted at `source_dir` or a workspace
/// rooted there via `moon.work` or legacy `moon.work.json`.
pub fn resolve_for_fmt(source_dir: &Path) -> Result<FmtResolveOutput, ResolveError> {
    info!(
        "Resolving formatter environment for {}",
        source_dir.display()
    );
    discover_local_project(source_dir).map_err(ResolveError::from)
}

pub struct FmtConfig {
    /// Enable `///|` block-lines in formatting
    pub block_style: bool,

    /// Checks the formatting without writing to files
    pub check_only: bool,

    /// Extra arguments to pass to the formatter
    pub extra_args: Vec<String>,

    /// Warn instead of showing differences
    pub warn_only: bool,

    /// Migrate moon.pkg.json to moon.pkg when only the JSON file exists.
    pub migrate_moon_pkg_json: bool,

    /// Migrate moon.work.json to moon.work when only the JSON file exists.
    pub migrate_moon_work_json: bool,
}

/// Generate the necessary build graph for the formatter operation.
///
/// If `selected_packages` is non-empty, only the specified packages will be formatted.
/// Otherwise, all packages in the current module or workspace will be formatted.
pub fn build_graph_for_fmt(
    resolved: &FmtResolveOutput,
    cfg: &FmtConfig,
    source_dir: &Path,
    target_dir: &Path,
    selected_packages: &[PackageId],
) -> anyhow::Result<n2::graph::Graph> {
    info!(
        "Building format graph for {} root modules",
        resolved.root_module_ids.len()
    );

    let layout = LegacyLayoutBuilder::default()
        .target_base_dir(target_dir.into())
        .main_module(match resolved.root_module_ids.as_slice() {
            &[module_id] => Some(resolved.root_modules[module_id].source().clone()),
            [_, ..] => None,
            [] => anyhow::bail!("No root modules found to format"),
        })
        .stdlib_dir(None)
        .opt_level(moonutil::cond_expr::OptLevel::Release) // we don't care
        .run_mode(moonutil::common::RunMode::Format) // this too
        .build()
        .expect("Should be valid layout");

    debug!("Layout built for formatting");

    let mut graph = n2::graph::Graph::default();
    let mut package_count = 0;
    let selected_packages = (!selected_packages.is_empty())
        .then(|| selected_packages.iter().copied().collect::<HashSet<_>>());
    let has_workspace_manifest = selected_packages.is_none()
        && format_workspace_node(&mut graph, cfg, &layout, source_dir)?;

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
            build_for_package(&mut graph, cfg, &layout, pkg)?;
            package_count += 1;
        }
    }

    if package_count == 0 && !has_workspace_manifest {
        anyhow::bail!("No packages found in workspace to format");
    }

    Ok(graph)
}

fn format_workspace_node(
    graph: &mut n2::graph::Graph,
    cfg: &FmtConfig,
    layout: &LegacyLayout,
    source_dir: &Path,
) -> anyhow::Result<bool> {
    let moon_work = source_dir.join(MOON_WORK);
    let moon_work_json = source_dir.join(MOON_WORK_JSON);

    let has_dsl = moon_work.exists();
    let has_json = moon_work_json.exists();

    if !has_dsl && !has_json {
        debug!(
            "Skipping moon.work formatting for {} - no workspace file exists",
            source_dir.display()
        );
        return Ok(false);
    }

    let target_moon_work = layout.format_root_artifact_path(std::ffi::OsStr::new(MOON_WORK));

    if has_dsl && has_json {
        warn!(
            "Both {} and {} exist at workspace root '{}', using the new format {}. Please remove the deprecated {}.",
            MOON_WORK_JSON,
            MOON_WORK,
            source_dir.display(),
            MOON_WORK,
            MOON_WORK_JSON
        );
        format_moon_work_dsl(graph, cfg, &moon_work, &target_moon_work)?;
    } else if has_dsl {
        format_moon_work_dsl(graph, cfg, &moon_work, &target_moon_work)?;
    } else if cfg.migrate_moon_work_json {
        format_moon_work_json_migrate(
            graph,
            cfg,
            &moon_work_json,
            &target_moon_work,
            &moon_work,
            source_dir,
        )?;
    } else {
        debug!(
            "Skipping moon.work.json migration for {} - feature disabled",
            source_dir.display()
        );
        return Ok(false);
    }

    Ok(true)
}

fn build_for_package(
    graph: &mut n2::graph::Graph,
    cfg: &FmtConfig,
    layout: &LegacyLayout,
    pkg: &DiscoveredPackage,
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
                .flat_map(|plan| plan.output.iter().map(|path| path.as_str()))
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

        format_node(graph, cfg, layout, pkg, file)?;
        Ok(())
    };

    for file in &pkg.source_files {
        add_fmt_for_file(file)?;
    }
    for file in &pkg.mbt_md_files {
        add_fmt_for_file(file)?;
    }

    // Always format moon.pkg when present; migration from moon.pkg.json is gated.
    format_moon_pkg_node(graph, cfg, layout, pkg)?;

    Ok(())
}

fn format_node(
    graph: &mut n2::graph::Graph,
    cfg: &FmtConfig,
    layout: &LegacyLayout,
    pkg: &DiscoveredPackage,
    file: &Path,
) -> anyhow::Result<()> {
    let out_file = layout
        .format_artifact_path(&pkg.fqn, file.file_name().expect("Should have filename"))
        .to_string_lossy()
        .into_owned();
    let cmd: Vec<String> = if cfg.check_only || cfg.warn_only {
        let mut cmd = vec![
            moonutil::BINARIES.moonbuild.to_string_lossy().into_owned(),
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
        if cfg.block_style {
            cmd.push("--block-style".into());
        }
        cmd.extend_from_slice(&cfg.extra_args);
        cmd
    } else {
        let mut cmd = vec![
            moonutil::BINARIES.moonfmt.to_string_lossy().into_owned(),
            file.to_string_lossy().into_owned(),
            "-w".into(),
            "-o".into(),
            out_file.clone(),
        ];
        cmd.extend_from_slice(&cfg.extra_args);
        if cfg.block_style {
            cmd.push("-block-style".into());
        }
        cmd
    };

    let ins = build_ins(graph, [file]);
    let outs = build_outs(graph, [&out_file]);
    let mut build = Build::new(
        build_n2_fileloc(format!("format {}", file.display())),
        ins,
        outs,
    );
    build.cmdline = Some(moonutil::shlex::join_native(cmd.iter().map(|x| x.as_str())));

    // When `warn_only` is enabled, the artifact is marked as dirty
    // if there are differences, so the command will rerun on the next run.
    if cfg.warn_only {
        build.can_dirty_on_output = true;
    }
    graph.add_build(build)?;
    Ok(())
}

fn format_moon_work_dsl(
    graph: &mut n2::graph::Graph,
    cfg: &FmtConfig,
    moon_work: &std::path::Path,
    target_moon_work: &std::path::Path,
) -> anyhow::Result<()> {
    if cfg.check_only || cfg.warn_only {
        let mut cmd = vec![
            moonutil::BINARIES.moonbuild.to_string_lossy().into_owned(),
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

        let ins = build_ins(graph, [moon_work]);
        let outs = build_outs(graph, [target_moon_work]);
        let mut build = Build::new(build_n2_fileloc("check moon.work format"), ins, outs);
        build.cmdline = Some(moonutil::shlex::join_native(cmd.iter().map(|x| x.as_str())));
        if cfg.warn_only {
            build.can_dirty_on_output = true;
        }
        graph.add_build(build)?;
    } else {
        let fmt_cmd: Vec<String> = vec![
            moonutil::BINARIES.moonbuild.to_string_lossy().into_owned(),
            "tool".into(),
            "format-workspace".into(),
            "--old".into(),
            moon_work.to_string_lossy().into_owned(),
            "--new".into(),
            target_moon_work.to_string_lossy().into_owned(),
        ];

        let ins = build_ins(graph, [moon_work]);
        let outs = build_outs(graph, [target_moon_work.to_string_lossy().into_owned()]);
        let mut build = Build::new(build_n2_fileloc("format moon.work"), ins, outs);
        build.cmdline = Some(moonutil::shlex::join_native(
            fmt_cmd.iter().map(|x| x.as_str()),
        ));
        graph.add_build(build)?;

        let cp_cmd: Vec<String> = if cfg!(windows) {
            vec![
                "cmd".into(),
                "/c".into(),
                "copy".into(),
                target_moon_work.to_string_lossy().into_owned(),
                moon_work.to_string_lossy().into_owned(),
            ]
        } else {
            vec![
                "cp".into(),
                target_moon_work.to_string_lossy().into_owned(),
                moon_work.to_string_lossy().into_owned(),
            ]
        };

        let ins = build_ins(graph, [target_moon_work]);
        let outs = build_outs(graph, [moon_work.to_string_lossy().into_owned()]);
        let mut build = Build::new(build_n2_fileloc("copy moon.work"), ins, outs);
        build.cmdline = Some(moonutil::shlex::join_native(
            cp_cmd.iter().map(|x| x.as_str()),
        ));
        graph.add_build(build)?;
    }

    Ok(())
}

fn format_moon_work_json_migrate(
    graph: &mut n2::graph::Graph,
    cfg: &FmtConfig,
    moon_work_json: &std::path::Path,
    target_moon_work: &std::path::Path,
    moon_work: &std::path::Path,
    source_dir: &Path,
) -> anyhow::Result<()> {
    warn!(
        "Migrating to {} at workspace root '{}', deprecated {} is removed.",
        MOON_WORK,
        source_dir.display(),
        MOON_WORK_JSON
    );

    if cfg.check_only || cfg.warn_only {
        let mut cmd = vec![
            moonutil::BINARIES.moonbuild.to_string_lossy().into_owned(),
            "tool".into(),
            "format-workspace".into(),
            "--old".into(),
            moon_work_json.to_string_lossy().into_owned(),
            "--new".into(),
            target_moon_work.to_string_lossy().into_owned(),
            "--check".into(),
        ];
        if cfg.warn_only {
            cmd.pop();
            cmd.push("--warn".into());
        }

        let ins = build_ins(graph, [moon_work_json]);
        let outs = build_outs(graph, [target_moon_work]);
        let mut build = Build::new(
            build_n2_fileloc("check moon.work.json migration"),
            ins,
            outs,
        );
        build.cmdline = Some(moonutil::shlex::join_native(cmd.iter().map(|x| x.as_str())));
        if cfg.warn_only {
            build.can_dirty_on_output = true;
        }
        graph.add_build(build)?;
    } else {
        let fmt_cmd: Vec<String> = vec![
            moonutil::BINARIES.moonbuild.to_string_lossy().into_owned(),
            "tool".into(),
            "format-workspace".into(),
            "--old".into(),
            moon_work_json.to_string_lossy().into_owned(),
            "--new".into(),
            target_moon_work.to_string_lossy().into_owned(),
        ];

        let ins = build_ins(graph, [moon_work_json]);
        let outs = build_outs(graph, [target_moon_work.to_string_lossy().into_owned()]);
        let mut build = Build::new(build_n2_fileloc("format moon.work.json"), ins, outs);
        build.cmdline = Some(moonutil::shlex::join_native(
            fmt_cmd.iter().map(|x| x.as_str()),
        ));
        graph.add_build(build)?;

        let cp_cmd: Vec<String> = if cfg!(windows) {
            vec![
                "cmd".into(),
                "/c".into(),
                "copy".into(),
                target_moon_work.to_string_lossy().into_owned(),
                moon_work.to_string_lossy().into_owned(),
            ]
        } else {
            vec![
                "cp".into(),
                target_moon_work.to_string_lossy().into_owned(),
                moon_work.to_string_lossy().into_owned(),
            ]
        };

        let ins = build_ins(graph, [target_moon_work]);
        let outs = build_outs(graph, [moon_work.to_string_lossy().into_owned()]);
        let mut build = Build::new(build_n2_fileloc("copy moon.work"), ins, outs);
        build.cmdline = Some(moonutil::shlex::join_native(
            cp_cmd.iter().map(|x| x.as_str()),
        ));
        graph.add_build(build)?;

        let rm_cmd: Vec<String> = if cfg!(windows) {
            vec![
                "cmd".into(),
                "/c".into(),
                "del".into(),
                moon_work_json.to_string_lossy().into_owned(),
            ]
        } else {
            vec!["rm".into(), moon_work_json.to_string_lossy().into_owned()]
        };

        let ins = build_ins(graph, [moon_work]);
        let faked_rm_output = format!("{}.removed", moon_work_json.to_string_lossy());
        let outs = build_outs(graph, [&faked_rm_output]);
        let mut build = Build::new(build_n2_fileloc("remove moon.work.json"), ins, outs);
        build.cmdline = Some(moonutil::shlex::join_native(
            rm_cmd.iter().map(|x| x.as_str()),
        ));
        graph.add_build(build)?;
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
    graph: &mut n2::graph::Graph,
    cfg: &FmtConfig,
    layout: &LegacyLayout,
    pkg: &DiscoveredPackage,
) -> anyhow::Result<()> {
    use moonutil::common::{MOON_PKG, MOON_PKG_JSON};

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

    if has_dsl && has_json {
        // Both files exist: prefer moon.pkg (new format), warn about duplicate
        warn!(
            "Both {} and {} exist in package '{}', using the new format {}. Please remove the deprecated {}.",
            MOON_PKG_JSON, MOON_PKG, pkg.fqn, MOON_PKG, MOON_PKG_JSON
        );
        // Format moon.pkg (new format)
        format_moon_pkg_dsl(graph, cfg, &moon_pkg_dsl, &target_moon_pkg, pkg)
    } else if has_dsl {
        // Only moon.pkg exists: format it
        format_moon_pkg_dsl(graph, cfg, &moon_pkg_dsl, &target_moon_pkg, pkg)
    } else if cfg.migrate_moon_pkg_json {
        // Only moon.pkg.json exists: migrate to moon.pkg
        format_moon_pkg_json_migrate(
            graph,
            cfg,
            &moon_pkg_json,
            &target_moon_pkg,
            &moon_pkg_dsl,
            pkg,
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
    graph: &mut n2::graph::Graph,
    cfg: &FmtConfig,
    moon_pkg: &std::path::Path,
    target_moon_pkg: &std::path::Path,
    pkg: &DiscoveredPackage,
) -> anyhow::Result<()> {
    if cfg.check_only || cfg.warn_only {
        // In check/warn mode, use format-and-diff to compare
        let mut cmd = vec![
            moonutil::BINARIES.moonbuild.to_string_lossy().into_owned(),
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

        let ins = build_ins(graph, [moon_pkg]);
        let outs = build_outs(graph, [target_moon_pkg]);
        let mut build = Build::new(
            build_n2_fileloc(format!("check moon.pkg format {}", pkg.fqn)),
            ins,
            outs,
        );
        build.cmdline = Some(moonutil::shlex::join_native(cmd.iter().map(|x| x.as_str())));
        if cfg.warn_only {
            build.can_dirty_on_output = true;
        }
        graph.add_build(build)?;
    } else {
        // Format moon.pkg - use -w to write back to source and -o to target
        // This is consistent with how .mbt files are formatted
        let fmt_cmd: Vec<String> = vec![
            moonutil::BINARIES.moonfmt.to_string_lossy().into_owned(),
            moon_pkg.to_string_lossy().into_owned(),
            "-w".into(),
            "-o".into(),
            target_moon_pkg.to_string_lossy().into_owned(),
        ];

        let ins = build_ins(graph, [moon_pkg]);
        let outs = build_outs(graph, [target_moon_pkg]);
        let mut build = Build::new(
            build_n2_fileloc(format!("format moon.pkg {}", pkg.fqn)),
            ins,
            outs,
        );
        build.cmdline = Some(moonutil::shlex::join_native(
            fmt_cmd.iter().map(|x| x.as_str()),
        ));
        graph.add_build(build)?;
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
    graph: &mut n2::graph::Graph,
    cfg: &FmtConfig,
    moon_pkg_json: &std::path::Path,
    target_moon_pkg: &std::path::Path,
    moon_pkg: &std::path::Path,
    pkg: &DiscoveredPackage,
) -> anyhow::Result<()> {
    // Warn the user about migration and prompt to remove the old config
    warn!(
        "Migrating to {} in package '{}', deprecated {} is removed.",
        MOON_PKG, pkg.fqn, MOON_PKG_JSON
    );

    if cfg.check_only || cfg.warn_only {
        // In check/warn mode, use format-and-diff to compare
        let mut cmd = vec![
            moonutil::BINARIES.moonbuild.to_string_lossy().into_owned(),
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

        let ins = build_ins(graph, [moon_pkg_json]);
        let outs = build_outs(graph, [target_moon_pkg]);
        let mut build = Build::new(
            build_n2_fileloc(format!("check moon.pkg.json migration {}", pkg.fqn)),
            ins,
            outs,
        );
        build.cmdline = Some(moonutil::shlex::join_native(cmd.iter().map(|x| x.as_str())));
        if cfg.warn_only {
            build.can_dirty_on_output = true;
        }
        graph.add_build(build)?;
    } else {
        // Step 1: Format moon.pkg.json to target directory
        let fmt_cmd: Vec<String> = vec![
            moonutil::BINARIES.moonfmt.to_string_lossy().into_owned(),
            moon_pkg_json.to_string_lossy().into_owned(),
            "-o".into(),
            target_moon_pkg.to_string_lossy().into_owned(),
        ];

        let ins = build_ins(graph, [moon_pkg_json]);
        let outs = build_outs(graph, [target_moon_pkg.to_string_lossy().into_owned()]);
        let mut build = Build::new(
            build_n2_fileloc(format!("format moon.pkg.json {}", pkg.fqn)),
            ins,
            outs,
        );
        build.cmdline = Some(moonutil::shlex::join_native(
            fmt_cmd.iter().map(|x| x.as_str()),
        ));
        graph.add_build(build)?;

        // Step 2: Copy from target to source directory
        let cp_cmd: Vec<String> = if cfg!(windows) {
            vec![
                "cmd".into(),
                "/c".into(),
                "copy".into(),
                target_moon_pkg.to_string_lossy().into_owned(),
                moon_pkg.to_string_lossy().into_owned(),
            ]
        } else {
            vec![
                "cp".into(),
                target_moon_pkg.to_string_lossy().into_owned(),
                moon_pkg.to_string_lossy().into_owned(),
            ]
        };

        let ins = build_ins(graph, [target_moon_pkg]);
        let outs = build_outs(graph, [moon_pkg.to_string_lossy().into_owned()]);
        let mut build = Build::new(
            build_n2_fileloc(format!("copy moon.pkg {}", pkg.fqn)),
            ins,
            outs,
        );
        build.cmdline = Some(moonutil::shlex::join_native(
            cp_cmd.iter().map(|x| x.as_str()),
        ));
        graph.add_build(build)?;

        // Step 3: Remove the original JSON file
        let rm_cmd: Vec<String> = if cfg!(windows) {
            vec![
                "cmd".into(),
                "/c".into(),
                "del".into(),
                moon_pkg_json.to_string_lossy().into_owned(),
            ]
        } else {
            vec!["rm".into(), moon_pkg_json.to_string_lossy().into_owned()]
        };

        let ins = build_ins(graph, [moon_pkg]);
        // The `rm` command does not actually produce an output file, so we fake one to ensure this build task will run in n2.
        // If moon.pkg.json is removed successfully, this branch will not be executed again next time.
        let faked_rm_output = format!("{}.removed", moon_pkg_json.to_string_lossy());
        let outs = build_outs(graph, [&faked_rm_output]);
        let mut build = Build::new(
            build_n2_fileloc(format!("remove moon.pkg.json {}", pkg.fqn)),
            ins,
            outs,
        );
        build.cmdline = Some(moonutil::shlex::join_native(
            rm_cmd.iter().map(|x| x.as_str()),
        ));
        graph.add_build(build)?;
    }

    Ok(())
}
