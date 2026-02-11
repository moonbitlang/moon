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
use moonbuild_rupes_recta::intent::UserIntent;
use moonutil::common::{FileLock, RunMode};
use moonutil::mooncakes::sync::AutoSyncFlags;
use tracing::instrument;

use super::UniversalFlags;

use crate::cli::BuildFlags;
use crate::rr_build::{self, BuildConfig, preconfig_compile};

/// Generate documentation or searching documentation for a symbol.
#[derive(Debug, clap::Parser)]
pub(crate) struct DocSubcommand {
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
pub(crate) fn run_doc(cli: UniversalFlags, cmd: DocSubcommand) -> anyhow::Result<i32> {
    match cmd.symbol {
        None => {
            // generate the docs
            run_doc_rr(cli, cmd)
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
pub(crate) fn run_doc_rr(cli: UniversalFlags, cmd: DocSubcommand) -> anyhow::Result<i32> {
    let dir = cli.source_tgt_dir.try_into_package_dirs()?;
    let source_dir = dir.source_dir;
    let target_dir = dir.target_dir;

    // FIXME: This is copied from `moon check`'s code. Share code if possible.
    let mut preconfig = preconfig_compile(
        &cmd.auto_sync_flags,
        &cli,
        &BuildFlags::default(),
        None,
        &target_dir,
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
