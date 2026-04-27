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

use std::path::Path;

use anyhow::{Context as _, bail};
use moonbuild_rupes_recta::intent::UserIntent;
use moonutil::common::{FileLock, RunMode};
use moonutil::dirs::PackageDirs;
use moonutil::mooncakes::{ModuleId, sync::AutoSyncFlags};
use tracing::instrument;

use super::UniversalFlags;

use crate::cli::BuildFlags;
use crate::rr_build::{self, BuildConfig, preconfig_compile};
use crate::user_diagnostics::UserDiagnostics;

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
        help = "[Deprecated] The symbol to query documentation for. Use `moon ide doc <SYMBOL>` instead."
    )]
    #[deprecated]
    pub symbol: Option<String>,
}

#[instrument(skip_all)]
#[allow(deprecated)]
pub(crate) fn run_doc(cli: UniversalFlags, cmd: DocSubcommand) -> anyhow::Result<i32> {
    let output = UserDiagnostics::from_flags(&cli);
    if let Some(symbol) = cmd.symbol.as_deref() {
        return run_doc_query(symbol, output);
    }

    run_doc_rr(cli, cmd)
}

#[instrument(skip_all)]
fn run_doc_query(symbol: &str, output: UserDiagnostics) -> anyhow::Result<i32> {
    output.warn("`moon doc <SYMBOL>` is deprecated; use `moon ide doc <SYMBOL>` instead.");
    let query_result = std::process::Command::new(&*moonutil::BINARIES.moon_ide)
        .arg("doc")
        .arg(symbol)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .with_context(|| {
            "no such subcommand: `ide`, is `moon-ide` installed with the current MoonBit toolchain or accessible via your `PATH`?"
        })?
        .wait()?;
    if !query_result.success() {
        bail!("failed to query symbol documentation");
    }
    Ok(0)
}

#[instrument(skip_all)]
pub(crate) fn run_doc_rr(cli: UniversalFlags, cmd: DocSubcommand) -> anyhow::Result<i32> {
    let mut query = cli.source_tgt_dir.query()?;
    let project = query.project()?;
    let doc_source_dir = project
        .selected_module()
        .map(|module| module.root.clone())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "`moon doc` cannot infer a target module in workspace `{}`. Run it from a workspace member or use `moon -C <member> doc ...`.",
                project.root().display(),
            )
        })?;
    let PackageDirs {
        source_dir,
        target_dir,
        mooncakes_dir,
        project_manifest_path,
    } = query.package_dirs()?;

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

    let doc_source_dir_for_intent = doc_source_dir.clone();
    let (build_meta, build_graph) = rr_build::plan_build(
        preconfig,
        &cli.unstable_feature,
        &source_dir,
        &target_dir,
        &mooncakes_dir,
        UserDiagnostics::from_flags(&cli),
        project_manifest_path.as_deref(),
        Box::new(move |resolve_output, _| {
            let module_id = selected_doc_module_id(resolve_output, &doc_source_dir_for_intent)?;
            Ok(vec![UserIntent::Doc(module_id)].into())
        }),
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
    rr_build::generate_metadata(
        &doc_source_dir,
        &target_dir,
        &build_meta,
        RunMode::Check,
        None,
    )?;

    // Execute the build
    let cfg = BuildConfig::from_flags(
        &BuildFlags::default(),
        &cli.unstable_feature,
        cli.verbose,
        UserDiagnostics::from_flags(&cli),
    );
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
        let module_id = selected_doc_module_id(&build_meta.resolve_output, &doc_source_dir)?;
        let full_name = build_meta
            .resolve_output
            .module_rel
            .module_source(module_id)
            .name()
            .to_string();
        moonbuild::doc_http::start_server(static_dir, &full_name, cmd.bind, cmd.port)?;
    }

    Ok(0)
}

fn selected_doc_module_id(
    resolve_output: &moonbuild_rupes_recta::ResolveOutput,
    selected_module_dir: &Path,
) -> anyhow::Result<ModuleId> {
    resolve_output
        .local_modules()
        .iter()
        .copied()
        .find(|&module_id| {
            resolve_output
                .module_dirs
                .get(module_id)
                .is_some_and(|module_dir| module_dir == selected_module_dir)
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot find the local module at `{}`",
                selected_module_dir.display()
            )
        })
}
