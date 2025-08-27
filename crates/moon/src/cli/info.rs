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

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::{bail, Context};
use colored::Colorize;
use futures::future::try_join_all;
use moonbuild_rupes_recta::model::{BuildPlanNode, BuildTarget};
use mooncake::pkg::sync::auto_sync;
use moonutil::{
    common::{
        lower_surface_targets, read_module_desc_file_in_dir, DiagnosticLevel, FileLock,
        MoonbuildOpt, MooncOpt, PrePostBuild, RunMode, SurfaceTarget, TargetBackend,
        MBTI_GENERATED, MOONBITLANG_CORE, MOON_MOD_JSON,
    },
    cond_expr::OptLevel,
    dirs::{mk_arch_mode_dir, PackageDirs},
    mooncakes::{sync::AutoSyncFlags, RegistryConfig},
    package::Package,
};

use crate::{cli::BuildFlags, rr_build};

use super::{pre_build::scan_with_x_build, UniversalFlags};

/// Generate public interface (`.mbti`) files for all packages in the module
#[derive(Debug, Clone, clap::Parser)]
pub struct InfoSubcommand {
    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    /// Do not use alias to shorten package names in the output
    #[clap(long)]
    pub no_alias: bool,

    #[clap(skip)]
    pub target_backend: Option<TargetBackend>,

    /// Select output target
    #[clap(long, value_delimiter = ',')]
    pub target: Option<Vec<SurfaceTarget>>,

    /// only emit mbti files for the specified package
    // (username/hello/lib)
    #[clap(short, long)]
    pub package: Option<String>,
}

pub fn run_info(cli: UniversalFlags, cmd: InfoSubcommand) -> anyhow::Result<i32> {
    if cli.unstable_feature.rupes_recta {
        run_info_rr(cli, cmd)
    } else {
        run_info_legacy(cli, cmd)
    }
}

pub fn run_info_rr(cli: UniversalFlags, cmd: InfoSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let preconfig = rr_build::preconfig_compile(
        &cmd.auto_sync_flags,
        &cli,
        &BuildFlags::default(),
        &target_dir,
        OptLevel::Release,
        RunMode::Build,
    );
    let (_build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        &source_dir,
        &target_dir,
        Box::new(calc_user_intent),
    )?;

    if cli.dry_run {
        rr_build::print_dry_run(
            &build_graph,
            &_build_meta.artifacts,
            &source_dir,
            &target_dir,
        );
        Ok(0)
    } else {
        // TODO: `moon info` is a wrapper over `moon check`, so should have flags that `moon check` has?
        let result = rr_build::execute_build(build_graph, &target_dir, None)?;
        result.print_info();
        Ok(result.return_code_for_success())
    }
}

fn calc_user_intent(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    main_modules: &[moonutil::mooncakes::ModuleId],
) -> Result<Vec<BuildPlanNode>, anyhow::Error> {
    let &[main_module_id] = main_modules else {
        panic!("No multiple main modules are supported");
    };

    let packages = resolve_output
        .pkg_dirs
        .packages_for_module(main_module_id)
        .ok_or_else(|| anyhow::anyhow!("Cannot find the local module!"))?;
    let res = packages
        .values()
        .filter_map(|package_id| {
            let pkg = resolve_output.pkg_dirs.get_package(*package_id);
            if pkg.raw.virtual_pkg.is_some() {
                None
            } else {
                Some(BuildPlanNode::GenerateMbti(BuildTarget {
                    package: *package_id,
                    kind: moonbuild_rupes_recta::model::TargetKind::Source,
                }))
            }
        })
        .collect();
    Ok(res)
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
        // create a map to store the mbti files for each package
        let mut pkg_mbti_files: HashMap<String, HashMap<TargetBackend, PathBuf>> = HashMap::new();
        for (backend, paths) in &mbti_files_for_targets {
            for (pkg_name, mbti_file) in paths {
                pkg_mbti_files
                    .entry(pkg_name.to_string())
                    .or_default()
                    .insert(*backend, mbti_file.clone());
            }
        }

        // compare the mbti files for each package in different backends
        for (pkg_name, backend_files) in pkg_mbti_files {
            let mut backends: Vec<_> = backend_files.keys().collect();
            backends.sort();

            for window in backends.windows(2) {
                let backend1 = window[0];
                let backend2 = window[1];
                let file1 = &backend_files[backend1];
                let file2 = &backend_files[backend2];

                let output = std::process::Command::new("git")
                    .args(["diff", "--no-index", "--exit-code"])
                    .arg(file1)
                    .arg(file2)
                    .output()
                    .context("Failed to run git diff")?;

                if !output.status.success() {
                    // print the diff
                    println!("{}", String::from_utf8_lossy(&output.stdout));
                    bail!(
                        "Package '{}' has different interfaces for backends {:?} and {:?}.\nFiles:\n{}\n{}", 
                        pkg_name,
                        backend1,
                        backend2,
                        file1.display(),
                        file2.display()
                    );
                }
            }
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
    if cli.dry_run {
        bail!("dry-run is not implemented for info")
    }

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
        output_json: false,
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

    let package_filter = if let Some(pkg_name) = &cmd.package {
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
            bail!("package `{}` not found, make sure you have spelled it correctly, e.g. `moonbitlang/core/hashmap`(exact match) or `hashmap`(fuzzy match)", pkg_name);
        }
        Some(move |pkg: &Package| final_set.contains(&pkg.full_name()))
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
