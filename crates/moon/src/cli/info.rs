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

mod imp;

use std::path::PathBuf;

use anyhow::bail;
use moonbuild_rupes_recta::intent::UserIntent;
use moonbuild_rupes_recta::model::PackageId;
use moonutil::{
    common::{RunMode, SurfaceTarget, TargetBackend, lower_surface_targets},
    dirs::PackageDirs,
    mooncakes::ModuleId,
    mooncakes::sync::AutoSyncFlags,
};

use crate::{
    cli::BuildFlags,
    filter::{
        canonicalize_with_filename, filter_pkg_by_dir, format_supported_backends,
        match_packages_with_fuzzy, package_supports_backend, select_packages,
    },
    rr_build::{self, BuildConfig, BuildMeta, CalcUserIntentOutput},
    user_diagnostics::UserDiagnostics,
};

use super::UniversalFlags;

enum InfoScope {
    Modules(Vec<ModuleId>),
    Packages(Vec<PackageId>),
}

struct InfoGroup {
    target_backend: TargetBackend,
    scope: InfoScope,
}

/// Generate public interface (`.mbti`) files for all packages in the module or workspace
#[derive(Debug, clap::Parser)]
pub(crate) struct InfoSubcommand {
    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Do not use alias to shorten package names in the output
    ///
    /// Deprecated: this has been created for AI stuff, and doesn't seem to be
    /// used recently.
    #[clap(long, hide = true)]
    pub no_alias: bool,

    /// Select output target
    #[clap(long, value_delimiter = ',')]
    pub target: Option<Vec<SurfaceTarget>>,

    /// The full or subset of name of the package to emit `mbti` files for
    #[clap(short, long)]
    pub package: Option<String>,

    /// The file-system path to the package or file in package to emit `mbti` files for
    ///
    /// Conflicts with `--package`.
    #[clap(name = "PATH", conflicts_with("package"))]
    pub path: Vec<PathBuf>,
}

pub(crate) fn run_info(cli: UniversalFlags, cmd: InfoSubcommand) -> anyhow::Result<i32> {
    let output = UserDiagnostics::from_flags(&cli);
    if cmd.no_alias {
        output.warn("`--no-alias` will be removed soon. See: https://github.com/moonbitlang/moon/issues/1092");
    }
    if cli.dry_run {
        bail!("dry-run is not supported for info")
    }

    run_info_rr(cli, cmd)
}

pub(crate) fn run_info_rr(cli: UniversalFlags, cmd: InfoSubcommand) -> anyhow::Result<i32> {
    if cmd.target.is_none() {
        return run_info_rr_grouped_default(cli, cmd);
    }

    // Determine which target to use
    let target = &cmd.target;
    let mut lowered_targets = vec![];
    if let Some(tgts) = target {
        lowered_targets.extend(lower_surface_targets(tgts));
    }

    // If there's zero or one target, just run normally and promote the results
    if lowered_targets.len() <= 1 {
        let target_backend = lowered_targets.first().cloned();
        let (success, meta) = run_info_rr_internal(&cli, &cmd, target_backend, None, None)?;
        if success {
            imp::promote_info_results(&meta);
            return Ok(0);
        } else {
            return Ok(1);
        }
    }

    // For multiple targets, we would like to run them one by one, and then
    // check the consistency of generated mbti files.
    lowered_targets.sort();
    // Prefer WasmGC if present; otherwise use the first one after sorting
    let canonical_target = lowered_targets
        .iter()
        .copied()
        .find(|t| *t == TargetBackend::WasmGC)
        .unwrap_or(lowered_targets[0]);

    let mut all_meta = vec![];
    for &tgt in &lowered_targets {
        let (success, meta) = run_info_rr_internal(&cli, &cmd, Some(tgt), None, None)?;
        if !success {
            bail!("moon info failed for target {:?}", tgt);
        }

        all_meta.push((tgt, meta));
    }

    let identical = imp::compare_info_outputs(all_meta.iter(), canonical_target)?;
    if identical {
        imp::promote_info_results_multi_target(all_meta.iter(), canonical_target);
    }

    if identical { Ok(0) } else { Ok(1) }
}

fn run_info_rr_grouped_default(cli: UniversalFlags, cmd: InfoSubcommand) -> anyhow::Result<i32> {
    let package_dirs = cli.source_tgt_dir.try_into_package_dirs()?;
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new_with_load_defaults(
        cmd.auto_sync_flags.frozen,
        false,
        false,
    )
    .with_project_manifest_path(package_dirs.project_manifest_path.as_deref());
    let resolve_output = moonbuild_rupes_recta::resolve(
        &resolve_cfg,
        &package_dirs.source_dir,
        &package_dirs.mooncakes_dir,
    )?;
    let groups = resolve_info_groups(&resolve_output, &cmd, UserDiagnostics::from_flags(&cli))?;

    let mut ok = true;
    for group in groups {
        let module_scope = match &group.scope {
            InfoScope::Modules(modules) => Some(modules.as_slice()),
            InfoScope::Packages(_) => None,
        };
        let selected_packages = match &group.scope {
            InfoScope::Modules(_) => None,
            InfoScope::Packages(packages) => Some(packages.as_slice()),
        };
        let (success, meta) = run_info_rr_internal(
            &cli,
            &cmd,
            Some(group.target_backend),
            module_scope,
            selected_packages,
        )?;
        ok &= success;
        if success {
            imp::promote_info_results(&meta);
        }
    }

    Ok(if ok { 0 } else { 1 })
}

/// Run `moon info` for the given target (`None` for default target)
///
/// Returns `(success, build metadata if not dry-run)`.
pub(crate) fn run_info_rr_internal(
    cli: &UniversalFlags,
    cmd: &InfoSubcommand,
    target: Option<TargetBackend>,
    module_scope: Option<&[ModuleId]>,
    selected_packages: Option<&[PackageId]>,
) -> anyhow::Result<(bool, BuildMeta)> {
    let PackageDirs {
        source_dir,
        target_dir,
        mooncakes_dir,
        project_manifest_path,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let mut preconfig = rr_build::preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &BuildFlags::default(),
        target,
        &target_dir,
        RunMode::Check,
    );
    preconfig.info_no_alias = cmd.no_alias;
    let package_filter = cmd.package.clone();
    let path_filter = cmd.path.clone();
    let output = UserDiagnostics::from_flags(cli);
    let resolve_cfg = moonbuild_rupes_recta::ResolveConfig::new_with_load_defaults(
        cmd.auto_sync_flags.frozen,
        false,
        false,
    )
    .with_project_manifest_path(project_manifest_path.as_deref());
    let resolve_output = moonbuild_rupes_recta::resolve(&resolve_cfg, &source_dir, &mooncakes_dir)?;
    let (build_meta, build_graph) = rr_build::plan_build_from_resolved(
        preconfig,
        &cli.unstable_feature,
        &target_dir,
        output,
        Box::new(move |resolve_output, tb| {
            calc_user_intent(
                package_filter.as_deref(),
                &path_filter,
                module_scope.unwrap_or(resolve_output.local_modules()),
                selected_packages,
                resolve_output,
                tb,
                output,
            )
        }),
        resolve_output,
    )?;
    // Generate the all_pkgs.json for indirect dependency resolution
    // before executing the build
    rr_build::generate_all_pkgs_json(&target_dir, &build_meta, RunMode::Check)?;

    // TODO: UX: Consider mirroring flags from `moon check`?
    let cfg = BuildConfig::from_flags(
        &BuildFlags::default(),
        &cli.unstable_feature,
        cli.verbose,
        output,
    );
    let result = rr_build::execute_build(&cfg, build_graph, &target_dir)?;
    result.print_info(cli.quiet, "generating mbti files")?;

    Ok((result.successful(), build_meta))
}

fn calc_user_intent(
    package_filter: Option<&str>,
    path_filter: &[PathBuf],
    module_scope: &[ModuleId],
    selected_packages: Option<&[PackageId]>,
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    target_backend: TargetBackend,
    output: UserDiagnostics,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let package_ids: Vec<_> =
        rr_build::local_packages_in_modules(resolve_output, module_scope).collect();

    let intents = if let Some(selected_packages) = selected_packages {
        let filtered = selected_packages
            .iter()
            .copied()
            .filter(|&pkg_id| package_supports_backend(resolve_output, pkg_id, target_backend))
            .collect::<Vec<_>>();
        if filtered.is_empty() && !selected_packages.is_empty() {
            output.warn(format!(
                "No selected package supports target backend `{}` for `moon info`",
                target_backend
            ));
        }
        filtered
            .into_iter()
            .map(UserIntent::Info)
            .collect::<Vec<_>>()
    } else if let [path] = path_filter {
        // Preserve the old single-path behavior exactly.
        let (dir, _) = canonicalize_with_filename(path)?;
        let pkg = filter_pkg_by_dir(resolve_output, &dir)?;
        if package_supports_backend(resolve_output, pkg, target_backend) {
            vec![UserIntent::Info(pkg)]
        } else {
            output.warn(format!(
                "Skipping package `{}` for `moon info`: it does not support target backend `{}`",
                resolve_output.pkg_dirs.get_package(pkg).fqn,
                target_backend
            ));
            Vec::new()
        }
    } else if !path_filter.is_empty() {
        let mut filtered = Vec::new();
        let mut unsupported = Vec::new();

        for (path, pkg_id) in select_packages(path_filter, output, |dir| {
            filter_pkg_by_dir(resolve_output, dir)
        })? {
            if package_supports_backend(resolve_output, pkg_id, target_backend) {
                filtered.push(UserIntent::Info(pkg_id));
            } else {
                unsupported.push((path, pkg_id));
            }
        }

        for (path, pkg_id) in &unsupported {
            let pkg = resolve_output.pkg_dirs.get_package(*pkg_id);
            output.info(format!(
                "skipping path `{}` because package `{}` does not support target backend `{}`. Supported backends: {}",
                path.display(),
                pkg.fqn,
                target_backend,
                format_supported_backends(resolve_output, *pkg_id),
            ));
        }

        if filtered.is_empty() && !unsupported.is_empty() {
            output.warn(format!(
                "No selected package supports target backend `{}` for `moon info`",
                target_backend
            ));
        }

        filtered
    } else if let Some(filter) = package_filter {
        let matches = match_packages_with_fuzzy(
            resolve_output,
            package_ids.iter().copied(),
            std::iter::once(filter),
        );

        if matches.matched.is_empty() {
            bail!(
                "package `{}` not found, make sure you have spelled it correctly, e.g. `moonbitlang/core/hashmap`(exact match) or `hashmap`(fuzzy match)",
                filter
            );
        }
        if !matches.missing.is_empty() {
            for missing in matches.missing {
                output.warn(format!("Input `{}` did not match any package", missing));
            }
        }

        let filtered = matches
            .matched
            .into_iter()
            .filter(|&pkg_id| package_supports_backend(resolve_output, pkg_id, target_backend))
            .collect::<Vec<_>>();
        if filtered.is_empty() {
            output.warn(format!(
                "No selected package supports target backend `{}` for `moon info`",
                target_backend
            ));
        }

        filtered
            .into_iter()
            .map(UserIntent::Info)
            .collect::<Vec<_>>()
    } else {
        package_ids
            .into_iter()
            .filter(|&pkg_id| package_supports_backend(resolve_output, pkg_id, target_backend))
            .map(UserIntent::Info)
            .collect()
    };

    Ok(intents.into())
}

fn resolve_info_groups(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    cmd: &InfoSubcommand,
    output: UserDiagnostics,
) -> anyhow::Result<Vec<InfoGroup>> {
    if !cmd.path.is_empty() {
        let selected = select_packages(&cmd.path, output, |dir| {
            filter_pkg_by_dir(resolve_output, dir)
        })?;
        return Ok(group_info_packages_by_default_target(
            resolve_output,
            selected.into_iter().map(|(_, pkg_id)| pkg_id),
        ));
    }

    if let Some(filter) = cmd.package.as_deref() {
        let package_ids: Vec<_> =
            rr_build::local_packages_in_modules(resolve_output, resolve_output.local_modules())
                .collect();
        let matches = match_packages_with_fuzzy(
            resolve_output,
            package_ids.iter().copied(),
            std::iter::once(filter),
        );
        if matches.matched.is_empty() {
            bail!(
                "package `{}` not found, make sure you have spelled it correctly, e.g. `moonbitlang/core/hashmap`(exact match) or `hashmap`(fuzzy match)",
                filter
            );
        }
        if !matches.missing.is_empty() {
            for missing in matches.missing {
                output.warn(format!("Input `{}` did not match any package", missing));
            }
        }
        return Ok(group_info_packages_by_default_target(
            resolve_output,
            matches.matched,
        ));
    }

    Ok(
        rr_build::group_modules_by_default_target(resolve_output, resolve_output.local_modules())
            .into_iter()
            .map(|(target_backend, modules)| InfoGroup {
                target_backend,
                scope: InfoScope::Modules(modules),
            })
            .collect(),
    )
}

fn group_info_packages_by_default_target(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    packages: impl IntoIterator<Item = PackageId>,
) -> Vec<InfoGroup> {
    let mut grouped = std::collections::BTreeMap::<TargetBackend, Vec<PackageId>>::new();
    for pkg_id in packages {
        let module_id = resolve_output.pkg_dirs.get_package(pkg_id).module;
        grouped
            .entry(rr_build::default_target_for_module(
                resolve_output,
                module_id,
            ))
            .or_default()
            .push(pkg_id);
    }
    grouped
        .into_iter()
        .map(|(target_backend, packages)| InfoGroup {
            target_backend,
            scope: InfoScope::Packages(packages),
        })
        .collect()
}
