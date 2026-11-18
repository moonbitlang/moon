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

//! Adapt command flags and intents to Rupes Recta builds.
//!
//! Build lifecycle, prebuild configuration, metadata publication, and execution
//! belong to `moonbuild::compile::BuildSession`.

use std::{collections::BTreeSet, path::Path};

use anyhow::Context;
use moonbuild::{execution::BuildInput, prepare::ProjectPreparationConfig};
use moonbuild_rupes_recta::{
    CompileConfig, DiscoveredProject, ResolvedProject,
    build_lower::WarningCondition,
    build_plan::InputDirective,
    discover::FmtProject,
    fmt::FmtConfig,
    model::{DebugInfoRequest, DebugSymbols, PackageId, TargetKind},
};
use moonutil::{
    build_options::RunMode,
    cli_support::UniversalFlags,
    cond_expr::OptLevel as BuildProfile,
    constants::{BLACKBOX_TEST_PATCH, WHITEBOX_TEST_PATCH},
    package::SupportedTargetsDeclKind,
    project::{PackageDirs, ProjectManifest},
    target::TargetBackend,
    user_log::UserLog,
};
use tracing::{Level, info, instrument};

use crate::build_flags::BuildFlags;

mod dry_run;
mod generated_mbti;
#[cfg(test)]
pub(crate) use dry_run::write_build_graph;
pub(crate) use dry_run::{format_dry_run_command, write_dry_run, write_dry_run_with_normalizer};
pub(crate) use generated_mbti::{generated_mbti_artifacts, generated_mbti_dest};

/// Synchronize module dependencies and discover packages.
/// This step does not acquire the target-directory lock.
pub(crate) fn sync_and_discover_project(
    preparation_config: &ProjectPreparationConfig,
    dirs: &PackageDirs,
    user_log: &UserLog,
) -> anyhow::Result<DiscoveredProject> {
    std::fs::create_dir_all(&dirs.target_dir).with_context(|| {
        format!(
            "Failed to create target directory: '{}'",
            dirs.target_dir.display()
        )
    })?;
    let synced_modules =
        moonbuild::prepare::sync_module_dependencies(preparation_config, dirs, user_log)?;
    Ok(moonbuild_rupes_recta::discover_synced_project(
        &preparation_config.discovery,
        synced_modules,
        user_log,
    )?)
}

fn warn_local_legacy_supported_targets(discovered: &DiscoveredProject, user_log: &UserLog) {
    let mut warned = BTreeSet::new();
    for &module_id in discovered.local_modules() {
        if let Some(pkgs) = discovered.pkg_dirs.packages_for_module(module_id) {
            for &pkg_id in pkgs.values() {
                if !warned.insert(pkg_id) {
                    continue;
                }
                let pkg = discovered.pkg_dirs.get_package(pkg_id);
                if pkg.supported_targets_decl == SupportedTargetsDeclKind::LegacyArray {
                    user_log.warn(format!(
                        "Package `{}` uses legacy array syntax for `supported_targets`; use expression syntax like `<backend>` instead",
                        pkg.fqn
                    ));
                }
            }
        }
    }
}

pub(crate) fn local_packages(
    discovered: &DiscoveredProject,
) -> impl Iterator<Item = PackageId> + '_ {
    discovered.local_modules().iter().flat_map(|&module_id| {
        discovered
            .pkg_dirs
            .packages_for_module(module_id)
            .into_iter()
            .flat_map(|packages| packages.values().copied())
    })
}

/// Select a standalone file's Target Backends before package resolution:
/// explicit targets, then the file's preferred backend, then the project default.
pub(crate) fn single_file_target_backends(
    discovered: &DiscoveredProject,
    selected_target_backends: &[TargetBackend],
    preferred_backend: Option<TargetBackend>,
    user_log: &UserLog,
) -> Vec<TargetBackend> {
    if !selected_target_backends.is_empty() {
        return selected_target_backends.to_vec();
    }
    vec![preferred_backend.unwrap_or_else(|| {
        local_modules_preferred_target(discovered, user_log).unwrap_or_default()
    })]
}

/// The preferred target that all local modules agree on, if any.
pub(crate) fn local_modules_preferred_target(
    discovered: &DiscoveredProject,
    user_log: &UserLog,
) -> Option<TargetBackend> {
    let preferred = discovered
        .local_modules()
        .iter()
        .filter_map(|&module_id| discovered.module_info(module_id).preferred_target)
        .collect::<BTreeSet<_>>();

    if preferred.len() > 1 {
        user_log.warn(
            "Multiple local modules specify different preferred targets; pass `--target` to choose one explicitly",
        );
        None
    } else {
        preferred.into_iter().next()
    }
}

/// Convenient function to build a directive based on input kind
pub fn build_patch_directive_for_package(
    pkg: PackageId,
    no_mi: bool,
    value_tracing: Option<PackageId>,
    patch_file: Option<&Path>,
    test_mode: bool,
) -> anyhow::Result<InputDirective> {
    let patch_directive = if let Some(path) = patch_file {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("patch file path is not valid utf-8"))?;
        let kind = if path_str.ends_with(WHITEBOX_TEST_PATCH) {
            TargetKind::WhiteboxTest
        } else if path_str.ends_with(BLACKBOX_TEST_PATCH) {
            TargetKind::BlackboxTest
        } else if test_mode {
            // In tests the patches are applied to tests only
            TargetKind::InlineTest
        } else {
            TargetKind::Source
        };
        Some((pkg.build_target(kind), path.to_path_buf()))
    } else {
        None
    };

    Ok(InputDirective {
        specify_no_mi_for: no_mi.then_some(pkg),
        specify_patch_file: patch_directive,
        value_tracing,
    })
}

/// Construct the final configuration for a resolved project and its selected backend.
///
/// Commands select the Target Backend before package resolution. CLI policy is
/// resolved here while both the original flags and that backend are available;
/// RR receives one completed configuration with no CLI parsing types.
#[instrument(level = Level::DEBUG, skip_all)]
pub(crate) fn prepare_resolved_build(
    cli: &UniversalFlags,
    build_flags: &BuildFlags,
    target_backend: TargetBackend,
    target_dir: &Path,
    action: RunMode,
    user_log: &UserLog,
    resolved_project: &ResolvedProject,
) -> anyhow::Result<CompileConfig> {
    // A couple of debug things:
    if cli.unstable_feature.rr_export_module_graph {
        info!("Exporting module graph DOT file");
        moonbuild_rupes_recta::util::print_module_graph_dot(
            &resolved_project.module_graph,
            &mut std::fs::File::create(target_dir.join("module_graph.dot"))?,
        )?;
    }
    if cli.unstable_feature.rr_export_package_graph {
        info!("Exporting package graph DOT file");
        // Import conditions make each backend's package graph distinct.
        let file_name = format!("package_graph.{}.dot", target_backend.to_dir_name());
        moonbuild_rupes_recta::util::print_package_relations_dot(
            &resolved_project.package_relations[&target_backend],
            &resolved_project.pkg_dirs,
            &mut std::fs::File::create(target_dir.join(file_name))?,
        )?;
    }

    // TODO: remove this once LLVM backend is well supported
    if target_backend == TargetBackend::LLVM {
        user_log.warn(
            "LLVM backend is experimental and only supported on nightly moonbit toolchain for now",
        );
    }
    warn_local_legacy_supported_targets(resolved_project, user_log);

    let opt_level = build_flags.effective_profile(action);
    let strip = build_flags.strip_for(action);
    let debug_info = DebugInfoRequest {
        symbols: if strip {
            DebugSymbols::None
        } else if action == RunMode::Run
            && target_backend == TargetBackend::Native
            && !build_flags.debug
            && !build_flags.no_strip
        {
            // Planning chooses how to retain source backtraces after selecting
            // generated C or direct object output for this Native run.
            DebugSymbols::Backtrace
        } else {
            DebugSymbols::Full
        },
        // Stripping symbols does not disable a debug-profile run's native
        // backtrace machinery. Other commands enable it with debug symbols.
        runtime_backtrace: target_backend.is_native()
            && (!strip || (action == RunMode::Run && opt_level == BuildProfile::Debug)),
    };
    let backend = moonbuild::compile::prepare_backend(
        target_backend,
        build_flags.output_wat,
        cli.unstable_feature.wasi_link,
    )?;
    info!("Final backend configuration: {:?}", backend);
    let mut config = moonbuild::compile::prepare_config(
        resolved_project,
        target_dir,
        backend,
        opt_level,
        action,
        debug_info,
        cli.dry_run,
    )?;
    config.enable_coverage = build_flags.enable_coverage;
    config.debug_export_build_plan = cli.unstable_feature.rr_export_build_plan;
    // In legacy impl, dry run always forces no JSON.
    config.moonc_output_json = !cli.dry_run && build_flags.output_style().needs_moonc_json();
    config.warning_condition = if build_flags.deny_warn {
        WarningCondition::Deny
    } else {
        WarningCondition::Default
    };
    config.warn_list = build_flags.warn_list.clone();
    Ok(config)
}

pub fn plan_fmt(
    resolved: &FmtProject,
    cfg: &FmtConfig,
    target_dir: &Path,
    selected_packages: &[PackageId],
    project_manifest: &ProjectManifest,
    user_log: &UserLog,
) -> anyhow::Result<BuildInput> {
    let execution_plan = moonbuild_rupes_recta::fmt::build_execution_plan_for_fmt(
        resolved,
        cfg,
        target_dir,
        selected_packages,
        project_manifest,
        user_log,
    )?;
    Ok(BuildInput::new(execution_plan, None))
}
