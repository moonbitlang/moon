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
use moonbuild_rupes_recta::{
    ResolveConfig, ResolveOutput, intent::UserIntent, model::PackageId, resolve,
};
use moonutil::{
    common::{RunMode, SurfaceTarget, TargetBackend, lower_surface_targets},
    dirs::PackageDirs,
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

/// Generate public interface (`.mbti`) files for all packages in the module or workspace
///
/// By default, `moon info` writes `pkg.generated.mbti` from each selected package's
/// canonical backend: module `preferred-backend`, then workspace
/// `preferred-backend`, then `wasm-gc`.
///
/// `--target` inspects backend-specific interfaces and reports differences, but
/// does not change which backend is written to `pkg.generated.mbti`.
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

    /// Inspect one or more target backends without changing the canonical
    /// `pkg.generated.mbti` output
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

struct PackageSelection {
    mode: SelectionMode,
    package_ids: Vec<PackageId>,
    path_packages: Vec<(PathBuf, PackageId)>,
}

enum SelectionMode {
    SinglePath,
    Paths,
    Packages,
    All,
}

impl PackageSelection {
    fn new(
        cmd: &InfoSubcommand,
        resolve_output: &ResolveOutput,
        output: UserDiagnostics,
    ) -> anyhow::Result<Self> {
        let package_ids: Vec<_> = resolve_output
            .local_modules()
            .iter()
            .flat_map(|&module_id| {
                resolve_output
                    .pkg_dirs
                    .packages_for_module(module_id)
                    .into_iter()
                    .flat_map(|packages| packages.values().copied())
            })
            .collect();

        if let [path] = cmd.path.as_slice() {
            let (dir, _) = canonicalize_with_filename(path)?;
            let pkg = filter_pkg_by_dir(resolve_output, &dir)?;
            return Ok(Self {
                mode: SelectionMode::SinglePath,
                package_ids: vec![pkg],
                path_packages: vec![],
            });
        }

        if !cmd.path.is_empty() {
            let path_packages = select_packages(&cmd.path, output, |dir| {
                filter_pkg_by_dir(resolve_output, dir)
            })?;
            let package_ids = path_packages.iter().map(|(_, pkg_id)| *pkg_id).collect();
            return Ok(Self {
                mode: SelectionMode::Paths,
                package_ids,
                path_packages,
            });
        }

        if let Some(filter) = cmd.package.as_deref() {
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

            for missing in matches.missing {
                output.warn(format!("Input `{}` did not match any package", missing));
            }

            return Ok(Self {
                mode: SelectionMode::Packages,
                package_ids: matches.matched,
                path_packages: vec![],
            });
        }

        Ok(Self {
            mode: SelectionMode::All,
            package_ids,
            path_packages: vec![],
        })
    }
}

struct InfoIntentContext<'a> {
    selection: &'a PackageSelection,
    output_plan: &'a imp::InfoOutputPlan,
    target_kind: imp::TargetKind,
    output: UserDiagnostics,
}

fn calc_user_intent_for_info(
    ctx: &InfoIntentContext,
    resolve_output: &ResolveOutput,
    target_backend: TargetBackend,
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let is_canonical_run = matches!(ctx.target_kind, imp::TargetKind::Canonical);

    let intents = match &ctx.selection.mode {
        SelectionMode::SinglePath => {
            let package = ctx.selection.package_ids[0];
            let canonical = ctx.output_plan.canonical_backend_for(&package) == Some(target_backend);
            let supported = package_supports_backend(resolve_output, package, target_backend);

            if (canonical || !is_canonical_run) && supported {
                vec![UserIntent::Info(package)]
            } else {
                ctx.output.warn(format!(
                    "Skipping package `{}` for `moon info`: it does not support target backend `{}`",
                    resolve_output.pkg_dirs.get_package(package).fqn,
                    target_backend
                ));
                Vec::new()
            }
        }
        SelectionMode::Paths => {
            let mut filtered = Vec::new();
            let mut unsupported = Vec::new();

            for (path, pkg_id) in &ctx.selection.path_packages {
                let canonical =
                    ctx.output_plan.canonical_backend_for(pkg_id) == Some(target_backend);
                let supported = package_supports_backend(resolve_output, *pkg_id, target_backend);
                let should_include = (canonical || !is_canonical_run) && supported;

                if should_include {
                    filtered.push(UserIntent::Info(*pkg_id));
                } else {
                    unsupported.push((path.clone(), *pkg_id));
                }
            }

            for (path, pkg_id) in &unsupported {
                let pkg = resolve_output.pkg_dirs.get_package(*pkg_id);
                ctx.output.info(format!(
                    "skipping path `{}` because package `{}` does not support target backend `{}`. Supported backends: {}",
                    path.display(),
                    pkg.fqn,
                    target_backend,
                    format_supported_backends(resolve_output, *pkg_id),
                ));
            }

            if filtered.is_empty() && !unsupported.is_empty() {
                ctx.output.warn(format!(
                    "No selected package supports target backend `{}` for `moon info`",
                    target_backend
                ));
            }

            filtered
        }
        SelectionMode::Packages | SelectionMode::All => {
            let filtered: Vec<_> = ctx
                .selection
                .package_ids
                .iter()
                .copied()
                .filter(|&pkg_id| {
                    let canonical =
                        ctx.output_plan.canonical_backend_for(&pkg_id) == Some(target_backend);
                    let supported =
                        package_supports_backend(resolve_output, pkg_id, target_backend);
                    (canonical || !is_canonical_run) && supported
                })
                .collect();

            if filtered.is_empty() {
                ctx.output.warn(format!(
                    "No selected package supports target backend `{}` for `moon info`",
                    target_backend
                ));
            }

            filtered.into_iter().map(UserIntent::Info).collect()
        }
    };

    Ok(intents.into())
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
    let PackageDirs {
        source_dir,
        target_dir,
        mooncakes_dir,
        project_manifest_path,
    } = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .package_dirs()?;

    let build_flags = BuildFlags::default();
    let resolve_cfg = ResolveConfig::new_with_load_defaults(
        cmd.auto_sync_flags.frozen,
        !build_flags.std(),
        build_flags.enable_coverage,
    )
    .with_workspace_env(cli.workspace_env.clone())
    .with_project_manifest_path(project_manifest_path.as_deref());
    let resolve_output = resolve(&resolve_cfg, &source_dir, &mooncakes_dir)?;
    let output = UserDiagnostics::from_flags(&cli);
    let selection = PackageSelection::new(&cmd, &resolve_output, output)?;

    let requested_targets = cmd
        .target
        .as_deref()
        .map(lower_surface_targets)
        .unwrap_or_default();
    let output_plan =
        imp::plan_info_outputs(&resolve_output, selection.package_ids.iter().copied());
    let execution_targets = output_plan.execution_targets(&requested_targets);

    let mut all_meta = vec![];
    let mut ok = true;
    for (tgt, target_kind) in execution_targets {
        let (success, meta) = run_info_rr_internal(
            &cli,
            &cmd,
            tgt,
            target_kind,
            &target_dir,
            resolve_output.clone(),
            &selection,
            &output_plan,
        )?;
        if !success {
            ok = false;
            continue;
        }
        all_meta.push((tgt, meta));
    }

    if !ok {
        return Ok(1);
    }

    imp::promote_info_results(&output_plan, all_meta.iter());
    imp::report_info_outputs(&output_plan, all_meta.iter(), &requested_targets)?;
    Ok(0)
}

/// Run `moon info` for the given target.
///
/// Returns `(success, build metadata if not dry-run)`.
#[allow(clippy::too_many_arguments)]
fn run_info_rr_internal(
    cli: &UniversalFlags,
    cmd: &InfoSubcommand,
    target: TargetBackend,
    target_kind: imp::TargetKind,
    target_dir: &std::path::Path,
    resolve_output: ResolveOutput,
    selection: &PackageSelection,
    output_plan: &imp::InfoOutputPlan,
) -> anyhow::Result<(bool, BuildMeta)> {
    let mut preconfig = rr_build::preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &BuildFlags::default(),
        Some(target),
        target_dir,
        RunMode::Check,
    );
    preconfig.info_no_alias = cmd.no_alias;
    let output = UserDiagnostics::from_flags(cli);
    let ctx = InfoIntentContext {
        selection,
        output_plan,
        target_kind,
        output,
    };
    let (build_meta, build_graph) = rr_build::plan_build_from_resolved(
        preconfig,
        &cli.unstable_feature,
        target_dir,
        output,
        Box::new(move |resolve_output, tb| calc_user_intent_for_info(&ctx, resolve_output, tb)),
        resolve_output,
    )?;
    // Generate the all_pkgs.json for indirect dependency resolution
    // before executing the build
    rr_build::generate_all_pkgs_json(target_dir, &build_meta, RunMode::Check)?;

    // TODO: UX: Consider mirroring flags from `moon check`?
    let cfg = BuildConfig::from_flags(
        &BuildFlags::default(),
        &cli.unstable_feature,
        cli.verbose,
        output,
    );
    let result = rr_build::execute_build(&cfg, build_graph, target_dir)?;
    let success = result.successful();
    let print_result = result.print_info(cli.quiet, "generating mbti files");
    if success {
        print_result?;
    }

    Ok((success, build_meta))
}
