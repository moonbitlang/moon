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

/// Refresh registry metadata and report dependency updates
///
/// With MOON_REGISTRY_API=1, refresh the selected module or workspace's exact
/// dependency versions and query each module's current latest version. Report
/// available updates and deprecation status without changing declarations,
/// downloading sources, or running build hooks. Partial refresh failures return
/// a nonzero exit status; successful refreshes remain cached.
///
/// Otherwise, synchronize the Git registry index and symbols as before. This
/// mode also works outside a project.
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
    let registry = RegistryClient::configured();
    if !registry.uses_http_metadata() {
        registry.sync(output.user_log())?;
        return Ok(0);
    }
    let project = cli
        .source_tgt_dir
        .query(cli.workspace_env.clone())?
        .select(output.user_log())?
        .package_dirs()?;
    let updates =
        mooncake::pkg::update::update(&project.project_manifest, &registry, output.user_log())?;
    let mut failures = Vec::new();
    output.write_result(|writer| -> anyhow::Result<()> {
        if updates.is_empty() {
            writeln!(writer, "No registry dependencies to refresh.")?;
        }
        for update in &updates {
            write!(writer, "{}: ", update.module)?;
            match &update.latest {
                Ok(latest) => {
                    write!(writer, "latest {latest}")?;
                    if latest > update.module.version() {
                        write!(writer, " (update available)")?;
                    }
                }
                Err(error) => {
                    write!(writer, "latest refresh failed")?;
                    failures.push(format!("  {} latest: {error:#}", update.module.name()));
                }
            }
            match &update.status {
                Ok(status) if status.yanked => {
                    write!(writer, "; deprecated")?;
                    let reason =
                        anstream::adapter::strip_str(status.yanked_reason.as_deref().unwrap_or(""))
                            .to_string();
                    if !reason.trim().is_empty() {
                        write!(writer, ": {}", reason.trim())?;
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    write!(writer, "; status refresh failed")?;
                    failures.push(format!("  {}: {error:#}", update.module));
                }
            }
            writeln!(writer)?;
        }
        Ok(())
    })?;
    if !failures.is_empty() {
        bail!("registry refresh incomplete:\n{}", failures.join("\n"));
    }
    Ok(0)
}
