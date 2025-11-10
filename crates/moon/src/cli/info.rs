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

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{Context, bail};
use colored::Colorize;
use futures::future::try_join_all;
use moonbuild_rupes_recta::intent::UserIntent;
use mooncake::pkg::sync::auto_sync;
use moonutil::{
    common::{
        DiagnosticLevel, FileLock, MBTI_GENERATED, MOON_MOD_JSON, MOONBITLANG_CORE, MoonbuildOpt,
        MooncOpt, PrePostBuild, RunMode, SurfaceTarget, TargetBackend, lower_surface_targets,
        read_module_desc_file_in_dir,
    },
    cond_expr::OptLevel,
    dirs::{PackageDirs, mk_arch_mode_dir},
    mooncakes::{RegistryConfig, sync::AutoSyncFlags},
    package::Package,
};
use tracing::warn;

use crate::{
    cli::BuildFlags,
    filter::{canonicalize_with_filename, filter_pkg_by_dir, match_packages_by_name_rr},
    rr_build::{self, BuildConfig, BuildMeta, CalcUserIntentOutput},
};

use super::{UniversalFlags, pre_build::scan_with_x_build};

/// Generate public interface (`.mbti`) files for all packages in the module
#[derive(Debug, Clone, clap::Parser)]
pub struct InfoSubcommand {
    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Do not use alias to shorten package names in the output
    ///
    /// Deprecated: this has been created for AI stuff, and doesn't seem to be
    /// used recently.
    #[clap(long, hide = true)]
    pub no_alias: bool,

    #[clap(skip)]
    pub target_backend: Option<TargetBackend>,

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
    pub path: Option<PathBuf>,
}

pub fn run_info(cli: UniversalFlags, cmd: InfoSubcommand) -> anyhow::Result<i32> {
    if cmd.no_alias {
        warn!(
            "`--no-alias` will be removed soon. See: https://github.com/moonbitlang/moon/issues/1092"
        );
    }
    if cli.dry_run {
        bail!("dry-run is not implemented for info")
    }

    if cli.unstable_feature.rupes_recta {
        run_info_rr(cli, cmd)
    } else {
        run_info_legacy(cli, cmd)
    }
}

pub fn run_info_rr(cli: UniversalFlags, cmd: InfoSubcommand) -> anyhow::Result<i32> {
    // Determine which target to use
    let target = &cmd.target;
    let mut lowered_targets = vec![];
    if let Some(tgts) = target {
        lowered_targets.extend(lower_surface_targets(tgts));
    }

    // If there's zero or one target, just run normally and promote the results
    if lowered_targets.len() <= 1 {
        let target_backend = lowered_targets.first().cloned();
        let (success, meta) = run_info_rr_internal(&cli, &cmd, target_backend)?;
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
    let canonical_target = lowered_targets[0]; // we have >1 targets here
    let mut all_meta = vec![];
    for &tgt in &lowered_targets {
        let (success, meta) = run_info_rr_internal(&cli, &cmd, Some(tgt))?;
        if !success {
            bail!("moon info failed for target {:?}", tgt);
        }

        all_meta.push((tgt, meta));
    }

    let identical = imp::compare_info_outputs(all_meta.iter(), canonical_target)?;

    if identical { Ok(0) } else { Ok(1) }
}

/// Run `moon info` for the given target (`None` for default target)
///
/// Returns `(success, build metadata if not dry-run)`.
pub fn run_info_rr_internal(
    cli: &UniversalFlags,
    cmd: &InfoSubcommand,
    target: Option<TargetBackend>,
) -> anyhow::Result<(bool, BuildMeta)> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let mut preconfig = rr_build::preconfig_compile(
        &cmd.auto_sync_flags,
        cli,
        &BuildFlags::default().with_target_backend(target),
        &target_dir,
        OptLevel::Release,
        RunMode::Build,
    );
    preconfig.info_no_alias = cmd.no_alias;
    let package_filter = cmd.package.clone();
    let path_filter = cmd.path.clone();
    let (_build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        &source_dir,
        &target_dir,
        Box::new(move |r, m| {
            calc_user_intent(package_filter.as_deref(), path_filter.as_deref(), r, m)
        }),
    )?;

    // TODO: `moon info` is a wrapper over `moon check`, so should have flags that `moon check` has?
    let result = rr_build::execute_build(&BuildConfig::default(), build_graph, &target_dir)?;
    result.print_info(cli.quiet, "generating mbti files")?;

    Ok((result.successful(), _build_meta))
}

fn calc_user_intent(
    package_filter: Option<&str>,
    path_filter: Option<&Path>,
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    main_modules: &[moonutil::mooncakes::ModuleId],
) -> Result<CalcUserIntentOutput, anyhow::Error> {
    let &[main_module_id] = main_modules else {
        panic!("No multiple main modules are supported");
    };

    let intents = if let Some(path) = path_filter {
        // Path filter: resolve a specific file/directory to its containing package
        let (dir, _) = canonicalize_with_filename(path)?;
        let pkg = filter_pkg_by_dir(resolve_output, &dir)?;
        vec![UserIntent::Info(pkg)]
    } else if let Some(filter) = package_filter {
        // Package filter: fuzzy match package names
        let matched = match_packages_by_name_rr(resolve_output, main_modules, filter);
        if matched.is_empty() {
            bail!(
                "package `{}` not found, make sure you have spelled it correctly",
                filter
            );
        }
        matched
            .into_iter()
            .map(UserIntent::Info)
            .collect::<Vec<_>>()
    } else {
        // No filter: generate info for all packages in the module
        resolve_output
            .pkg_dirs
            .packages_for_module(main_module_id)
            .ok_or_else(|| anyhow::anyhow!("Cannot find the local module!"))?
            .values()
            .map(|package_id| UserIntent::Info(*package_id))
            .collect()
    };
    Ok(intents.into())
}

pub fn run_info_legacy(cli: UniversalFlags, cmd: InfoSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let targets = if cmd.target.is_none() {
        let preferred_target = read_module_desc_file_in_dir(&source_dir)
            .with_context(|| {
                format!(
                    "failed to read module description file: {}",
                    source_dir
                        .join(MOON_MOD_JSON)
                        .display()
                        .to_string()
                        .bold()
                        .red()
                )
            })?
            .preferred_target;
        vec![preferred_target.unwrap_or_default()]
    } else {
        lower_surface_targets(&cmd.target.clone().unwrap())
    };

    let mut mbti_files_for_targets = vec![];
    for t in &targets {
        let mut cmd = cmd.clone();
        cmd.target_backend = Some(*t);
        let mut x = run_info_internal(&cli, &cmd, &source_dir, &target_dir)
            .context(format!("failed to run moon info for target {t:?}"))?;
        x.sort_by(|a, b| a.0.cmp(&b.0));
        mbti_files_for_targets.push((*t, x));
    }

    // check consistency if there are multiple targets
    if mbti_files_for_targets.len() > 1 {
        // Sort targets and pick the canonical backend (first after sort), consistent with RR path
        let mut lowered = targets.clone();
        lowered.sort();
        let canonical_target = lowered[0];

        // Diff the files
        let identical =
            imp::compare_info_outputs_from_paths(mbti_files_for_targets.iter(), canonical_target)?;
        if !identical {
            return Ok(1);
        }
    }

    Ok(0)
}

pub fn run_info_internal(
    cli: &UniversalFlags,
    cmd: &InfoSubcommand,
    source_dir: &Path,
    target_dir: &Path,
) -> anyhow::Result<Vec<(String, PathBuf)>> {
    let (resolved_env, dir_sync_result) = auto_sync(
        source_dir,
        &cmd.auto_sync_flags,
        &RegistryConfig::load(),
        cli.quiet,
    )?;

    let mod_desc = read_module_desc_file_in_dir(source_dir).with_context(|| {
        format!(
            "failed to read module description file: {}",
            source_dir
                .join(MOON_MOD_JSON)
                .display()
                .to_string()
                .bold()
                .red()
        )
    })?;
    let module_name = &mod_desc.name;
    let mut moonc_opt = MooncOpt::default();
    moonc_opt.link_opt.target_backend = cmd.target_backend.unwrap_or_default();
    moonc_opt.build_opt.target_backend = cmd.target_backend.unwrap_or_default();

    let raw_target_dir = target_dir.to_path_buf();
    let target_dir = mk_arch_mode_dir(source_dir, target_dir, &moonc_opt, RunMode::Check)?;
    let _lock = FileLock::lock(&target_dir)?;

    if module_name == MOONBITLANG_CORE {
        moonc_opt.nostd = true;
    }
    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.to_path_buf(),
        raw_target_dir,
        target_dir: target_dir.clone(),
        sort_input: false,
        run_mode: RunMode::Check,
        test_opt: None,
        check_opt: None,
        build_opt: None,
        fmt_opt: None,
        args: vec![],
        verbose: cli.verbose,
        quiet: cli.quiet,
        no_render_output: false,
        no_parallelize: false,
        build_graph: false,
        parallelism: None,
        use_tcc_run: false,
        dynamic_stub_libs: None,
        render_no_loc: DiagnosticLevel::default(),
    };

    let mdb = scan_with_x_build(
        false,
        &moonc_opt,
        &moonbuild_opt,
        &resolved_env,
        &dir_sync_result,
        &PrePostBuild::PreBuild,
    )?;

    let check_result = moonbuild::entry::run_check(&moonc_opt, &moonbuild_opt, &mdb);
    match check_result {
        Ok(0) => {}
        _ => {
            bail!("moon check failed");
        }
    }

    let runtime = tokio::runtime::Runtime::new()?;
    let mut handlers = vec![];
    let module_source_dir = match &mod_desc.source {
        None => source_dir.to_path_buf(),
        Some(p) => source_dir.join(p),
    };

    type PackageFilter = dyn Fn(&Package) -> bool;
    let package_filter: Option<Box<PackageFilter>> = if let Some(path) = &cmd.path {
        let (path, _filename) =
            canonicalize_with_filename(path).context("Input path is invalid")?;
        Some(Box::new(move |pkg: &Package| pkg.root_path == path))
    } else if let Some(pkg_name) = &cmd.package {
        let all_packages: indexmap::IndexSet<&str> = mdb
            .get_all_packages()
            .iter()
            .map(|pkg| pkg.0.as_str())
            .collect();

        let mut final_set = indexmap::IndexSet::new();
        if all_packages.contains(pkg_name.as_str()) {
            // exact matching
            final_set.insert(pkg_name.to_string());
        } else {
            let xs =
                moonutil::fuzzy_match::fuzzy_match(pkg_name.as_str(), all_packages.iter().copied());
            if let Some(xs) = xs {
                final_set.extend(xs);
            }
        }
        if final_set.is_empty() {
            bail!(
                "package `{}` not found, make sure you have spelled it correctly, e.g. `moonbitlang/core/hashmap`(exact match) or `hashmap`(fuzzy match)",
                pkg_name
            );
        }
        Some(Box::new(move |pkg: &Package| {
            final_set.contains(&pkg.full_name())
        }))
    } else {
        None
    };

    let packages_to_emit_mbti = mdb.get_filtered_packages(package_filter);

    let mbti_files = Arc::new(Mutex::new(vec![]));

    for (name, pkg) in packages_to_emit_mbti {
        // Skip 3-rd party packages and virtual packages(it's .mbti should be written by user)
        if pkg.is_third_party || pkg.virtual_pkg.is_some() {
            continue;
        }

        let mbti_files = Arc::clone(&mbti_files);
        let module_source_dir = std::sync::Arc::new(module_source_dir.clone());
        handlers.push(async move {
            let mi = pkg.artifact.with_extension("mi");
            if !mi.exists() {
                bail!("cannot find mi file for package {}", name);
            }
            let filepath = mi.with_extension("mbti");

            let mut args = vec![
                "-format=text".into(),
                mi.display().to_string(),
                format!("-o={}", filepath.display()),
            ];
            if cmd.no_alias {
                args.push("-no-alias".into());
            }
            let mut mooninfo = tokio::process::Command::new("mooninfo");
            mooninfo.args(&args);
            let out = mooninfo.output().await?;

            if out.status.success() {
                if
                // no target specified, default to wasmgc
                cmd.target.is_none()
                    // specific one target
                    || (cmd.target.as_ref().unwrap().len() == 1
                        && cmd.target.as_ref().unwrap().first().unwrap() != &SurfaceTarget::All)
                    // maybe more than one target, but running for wasmgc target
                    || cmd.target_backend == Some(TargetBackend::WasmGC)
                {
                    tokio::fs::copy(
                        &filepath,
                        &module_source_dir
                            .join(pkg.rel.fs_full_name())
                            .join(MBTI_GENERATED),
                    )
                    .await?;
                }
                mbti_files.lock().unwrap().push((name.clone(), filepath));
            } else {
                eprintln!("{}", String::from_utf8_lossy(&out.stderr));
                bail!("failed to run `mooninfo {}`", args.join(" "));
            }

            Ok(0)
        });
    }

    // `try_join_all` will return immediately if anyone task fail
    runtime.block_on(try_join_all(handlers))?;
    let mbti_files = mbti_files.lock().unwrap().clone();
    Ok(mbti_files)
}
