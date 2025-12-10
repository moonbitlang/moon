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

use anyhow::bail;
use moonbuild::dry_run::print_commands;
use moonbuild_rupes_recta::intent::UserIntent;
use mooncake::pkg::sync::auto_sync;
use moonutil::common::{
    CargoPathExt, DiagnosticLevel, FileLock, MOONBITLANG_CORE, MoonbuildOpt, MooncOpt,
    PrePostBuild, RunMode, read_module_desc_file_in_dir,
};
use moonutil::dirs::{PackageDirs, mk_arch_mode_dir};
use moonutil::mooncakes::RegistryConfig;
use moonutil::mooncakes::sync::AutoSyncFlags;
use tracing::instrument;

use super::UniversalFlags;
use super::pre_build::scan_with_x_build;

use crate::cli::BuildFlags;
use crate::rr_build::{self, BuildConfig, preconfig_compile};

/// Generate documentation or searching documentation for a symbol.
#[derive(Debug, clap::Parser)]
pub struct DocSubcommand {
    /// Start a web server to serve the documentation
    #[clap(long)]
    pub serve: bool,

    /// The address of the server
    #[clap(long, short, default_value = "127.0.0.1", requires("serve"))]
    pub bind: String,

    /// The port of the server
    #[clap(long, short, default_value = "3000", requires("serve"))]
    pub port: u16,

    #[clap(flatten)]
    pub auto_sync_flags: AutoSyncFlags,

    #[clap(
        conflicts_with("serve"),
        help = "The symbol to query documentation for, e.g. 'String::from*' or '@list.from*'"
    )]
    pub symbol: Option<String>,
}

#[instrument(skip_all)]
pub fn run_doc(cli: UniversalFlags, cmd: DocSubcommand) -> anyhow::Result<i32> {
    match cmd.symbol {
        None => {
            // generate the docs
            if cli.unstable_feature.rupes_recta {
                run_doc_rr(cli, cmd)
            } else {
                run_doc_legacy(cli, cmd)
            }
        }
        Some(symbol) => {
            // deligate to `moondoc` for querying symbol
            let query_result = std::process::Command::new(&*moonutil::BINARIES.moondoc)
                .arg("-q")
                .arg(symbol)
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .spawn()?
                .wait()?;
            if !query_result.success() {
                bail!("failed to query symbol documentation");
            }
            Ok(0)
        }
    }
}

#[instrument(skip_all)]
pub fn run_doc_rr(cli: UniversalFlags, cmd: DocSubcommand) -> anyhow::Result<i32> {
    let dir = cli.source_tgt_dir.try_into_package_dirs()?;
    let source_dir = dir.source_dir;
    let target_dir = dir.target_dir;

    // FIXME: This is copied from `moon check`'s code. Share code if possible.
    let mut preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        &cli,
        &BuildFlags::default(),
        &target_dir,
        moonutil::cond_expr::OptLevel::Release,
        RunMode::Check,
    );
    preconfig.docs_serve = cmd.serve;

    let (build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        &source_dir,
        &target_dir,
        // Docs are global
        Box::new(|_, _| Ok(vec![UserIntent::Docs].into())),
    )?;

    // Early exit for dry-run
    if cli.dry_run {
        rr_build::print_dry_run(
            &build_graph,
            build_meta.artifacts.values(),
            &source_dir,
            &target_dir,
        );
        return Ok(0);
    }

    let _lock = FileLock::lock(&target_dir)?;
    // Generate the all_pkgs.json for indirect dependency resolution
    // before executing the build
    rr_build::generate_all_pkgs_json(&target_dir, &build_meta, RunMode::Check)?;
    // Generate metadata for `moondoc`
    rr_build::generate_metadata(&source_dir, &target_dir, &build_meta, RunMode::Check, None)?;

    // Execute the build
    let cfg = BuildConfig::from_flags(&BuildFlags::default(), &cli.unstable_feature, cli.verbose);
    let result = rr_build::execute_build(&cfg, build_graph, &target_dir)?;
    result.print_info(cli.quiet, "checking")?;

    if !result.successful() {
        return Ok(result.return_code_for_success());
    }

    // Release lock before serving (no writes beyond this point)
    drop(_lock);
    // Serve
    if cmd.serve {
        let static_dir = target_dir.join("doc");
        if !static_dir.exists() {
            panic!(
                "Documentation directory does not exist: {}; This is a bug",
                static_dir.display()
            );
        }
        let mid = build_meta.resolve_output.local_modules()[0];
        let full_name = build_meta
            .resolve_output
            .module_rel
            .mod_name_from_id(mid)
            .name()
            .to_string();
        moonbuild::doc_http::start_server(static_dir, &full_name, cmd.bind, cmd.port)?;
    }

    Ok(0)
}

#[instrument(skip_all)]
pub fn run_doc_legacy(cli: UniversalFlags, cmd: DocSubcommand) -> anyhow::Result<i32> {
    let PackageDirs {
        source_dir,
        target_dir,
    } = cli.source_tgt_dir.try_into_package_dirs()?;

    let static_dir = target_dir.join("doc");
    if !static_dir.exists() {
        std::fs::create_dir_all(&static_dir)?;
    }
    let _lock = FileLock::lock(&static_dir)?;

    if static_dir.exists() {
        static_dir.rm_rf();
    }
    let serve = cmd.serve;
    let bind = cmd.bind;
    let port = cmd.port;

    let mod_desc = read_module_desc_file_in_dir(&source_dir)?;

    let mut moonc_opt = MooncOpt::default();
    if mod_desc.name == MOONBITLANG_CORE {
        moonc_opt.nostd = true;
    }

    let (resolved_env, dir_sync_result) = auto_sync(
        &source_dir,
        &cmd.auto_sync_flags,
        &RegistryConfig::load(),
        cli.quiet,
        true, // Legacy don't need std injection
    )?;

    let run_mode = RunMode::Check;
    let raw_target_dir = target_dir.to_path_buf();
    let target_dir = mk_arch_mode_dir(&source_dir, &target_dir, &moonc_opt, run_mode)?;
    let moonbuild_opt = MoonbuildOpt {
        source_dir: source_dir.clone(),
        raw_target_dir,
        target_dir,
        sort_input: true,
        run_mode,
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

    let module = scan_with_x_build(
        false,
        &moonc_opt,
        &moonbuild_opt,
        &resolved_env,
        &dir_sync_result,
        &PrePostBuild::PreBuild,
    )?;

    let mut args = vec![
        source_dir.display().to_string(),
        "-o".to_string(),
        static_dir.display().to_string(),
        "-std-path".to_string(),
        moonutil::moon_dir::core_bundle(moonc_opt.link_opt.target_backend)
            .display()
            .to_string(),
        "-packages-json".to_string(),
        moonbuild_opt
            .raw_target_dir
            .join("packages.json")
            .display()
            .to_string(),
    ];
    if serve {
        args.push("-serve-mode".to_string())
    }
    let moondoc = &*moonutil::BINARIES.moondoc;
    if cli.dry_run {
        print_commands(&module, &moonc_opt, &moonbuild_opt)?;
        println!("{moondoc:?} {}", args.join(" "));
        return Ok(0);
    }
    moonbuild::entry::run_check(&moonc_opt, &moonbuild_opt, &module)?;
    let output = std::process::Command::new(moondoc).args(&args).output()?;
    if output.status.code().unwrap() != 0 {
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        bail!("failed to generate documentation");
    }

    if serve {
        moonbuild::doc_http::start_server(static_dir, &mod_desc.name, bind, port)?;
    }
    Ok(0)
}
