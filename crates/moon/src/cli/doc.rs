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

use std::path::Path;

use anyhow::{Context as _, bail};
use moonbuild_rupes_recta::intent::UserIntent;
use moonutil::cli_support::AutoSyncFlags;
use moonutil::command_output::CommandOutput;
use moonutil::constants::MOON_MOD_JSON;
use moonutil::project::PackageDirs;
use moonutil::resolution::ModuleId;
use moonutil::{build_options::RunMode, locks::lock_directory, user_log::UserLog};
use tracing::instrument;

use super::UniversalFlags;

use crate::cli::BuildFlags;
use crate::rr_build::{self};

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
pub(crate) fn run_doc(
    cli: UniversalFlags,
    cmd: DocSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    if let Some(symbol) = cmd.symbol.as_deref() {
        return run_doc_query(symbol, output.user_log());
    }

    run_doc_rr(cli, cmd, output)
}

#[instrument(skip_all)]
fn run_doc_query(symbol: &str, user_log: &UserLog) -> anyhow::Result<i32> {
    user_log.warn("`moon doc <SYMBOL>` is deprecated; use `moon ide doc <SYMBOL>` instead.");
    let query_result = std::process::Command::new(&*moonutil::toolchain::BINARIES.moon_ide)
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
pub(crate) fn run_doc_rr(
    cli: UniversalFlags,
    cmd: DocSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    let user_log = output.user_log();
    let project = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(user_log)?;
    let selected_module = project
        .context()
        .selected_module()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "`moon doc` cannot infer a target module in workspace `{}`. Run it from a workspace member or use `moon -C <member> doc ...`.",
                project.context().root().display(),
            )
        })?;
    if selected_module.manifest_path.ends_with(MOON_MOD_JSON) {
        bail!(
            "`moon doc` does not support the deprecated `moon.mod.json` manifest; run `moon fmt` to migrate it to `moon.mod` first"
        );
    }
    let dirs = project.package_dirs()?;
    let PackageDirs {
        source_dir,
        target_dir,
        mooncake_bin_dir,
        ..
    } = &dirs;

    // Discover packages to choose the selected module's documentation backend.
    let build_flags = BuildFlags::default();
    let preparation_config = moonbuild_rupes_recta::ProjectPreparationConfig::new(
        cmd.auto_sync_flags.clone(),
        !build_flags.std(),
        build_flags.enable_coverage,
        cli.workspace_env.clone(),
    );
    let synced_modules =
        moonbuild_rupes_recta::sync_module_dependencies(&preparation_config, &dirs, user_log)?;
    let discovered = moonbuild_rupes_recta::discover_synced_project(
        &preparation_config,
        synced_modules,
        user_log,
    )?;

    let module_id = selected_doc_module_id(&discovered, &selected_module.root)?;
    let target_backend = discovered
        .module_info(module_id)
        .preferred_target
        .unwrap_or_default();

    let resolved_project = discovered.resolve_packages(&[target_backend], user_log)?;

    let mut compile_config = rr_build::prepare_resolved_build(
        &cli,
        &build_flags,
        Some(target_backend),
        target_dir,
        RunMode::Check,
        user_log,
        &resolved_project,
    )?;
    compile_config.docs_serve = cmd.serve;
    let intent = vec![UserIntent::Doc(module_id)].into();
    let (build_meta, build_graph) = rr_build::plan_resolved_build_from_intent(
        compile_config,
        user_log,
        intent,
        mooncake_bin_dir,
        resolved_project,
        build_flags.jobs,
        cmd.auto_sync_flags.frozen,
        cli.dry_run,
    )?;

    // Early exit for dry-run
    if cli.dry_run {
        output.write_result(|writer| rr_build::write_dry_run(writer, &build_graph, source_dir))?;
        return Ok(0);
    }

    let lock = lock_directory(target_dir, user_log)?;
    // Generate the all_pkgs.json for indirect dependency resolution
    // before executing the build
    rr_build::generate_all_pkgs_json(&build_meta)?;
    // Generate metadata for `moondoc`
    rr_build::generate_metadata(source_dir, &build_meta, &build_graph)?;

    // Execute the build
    let cfg = build_flags.execution_config(&cli.unstable_feature, cli.verbose);
    let result = moonbuild::execution::execute_build(&cfg, build_graph, target_dir, user_log)?;
    result.print_info(cli.quiet, "checking")?;

    if !result.successful() {
        return Ok(result.return_code_for_success());
    }

    // Release lock before serving (no writes beyond this point)
    drop(lock);
    // Serve
    if cmd.serve {
        let static_dir = target_dir.join("doc");
        if !static_dir.exists() {
            panic!(
                "Documentation directory does not exist: {}; This is a bug",
                static_dir.display()
            );
        }
        let full_name = build_meta
            .resolved_project
            .module_graph
            .module_source(module_id)
            .name()
            .to_string();
        moonbuild::doc_http::start_server(static_dir, &full_name, cmd.bind, cmd.port)?;
    }

    Ok(0)
}

fn selected_doc_module_id(
    discovered: &moonbuild_rupes_recta::DiscoveredProject,
    selected_module_dir: &Path,
) -> anyhow::Result<ModuleId> {
    discovered
        .local_modules()
        .iter()
        .copied()
        .find(|&module_id| {
            discovered
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
