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

use anyhow::bail;
use mooncake::registry::RegistryClient;
use moonutil::command_output::CommandOutput;

use super::UniversalFlags;

/// Update the registry index, or refresh project dependency metadata in API mode
///
/// Without MOON_REGISTRY_API=1, this keeps the existing Git-index update behavior.
/// In API mode, run it inside a module or workspace: it refreshes the deprecation
/// status of selected direct/transitive registry dependencies, queries latest,
/// and reports available updates. It does not change dependency declarations,
/// install source archives, or execute package hooks. Deprecation and available
/// updates do not fail the command; incomplete refreshes do, retaining successful
/// cache writes. Ordinary builds reuse exact cached metadata without age-based
/// refresh.
#[derive(Debug, clap::Parser)]
pub(crate) struct UpdateSubcommand {}

pub(crate) fn update_cli(
    cli: UniversalFlags,
    _cmd: UpdateSubcommand,
    output: &CommandOutput,
) -> anyhow::Result<i32> {
    if cli.dry_run {
        bail!("dry-run is not supported for update")
    }
    let user_log = output.user_log();
    let registry = RegistryClient::configured();
    if !registry.uses_metadata_api() {
        registry.sync(user_log)?;
        return Ok(0);
    }
    let dirs = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(user_log)?
        .package_dirs()?;
    let report = mooncake::pkg::update::update(&dirs, &registry, user_log)?;
    output.write_result(|writer| -> anyhow::Result<()> {
        for update in &report.updates {
            writeln!(
                writer,
                "Update available: {} -> {}",
                update.selected, update.latest
            )?;
        }
        Ok(())
    })?;
    if !report.failures.is_empty() {
        bail!(
            "registry metadata refresh incomplete:\n{}",
            report.failures.join("\n")
        );
    }
    Ok(0)
}
