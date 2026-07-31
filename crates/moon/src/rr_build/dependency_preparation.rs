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

//! Materialize every dependency product required by a standalone script graph.
//!
//! Cache restoration and n2 execution are implementation choices behind this
//! boundary. Successful completion means the following script graph can consume
//! all lowered dependency paths without knowing how they were produced.

use std::path::Path;

use anyhow::{Context, bail};
use moonbuild_rupes_recta::build_lower::{LoweredAction, lowered_actions_to_n2_graph};
use moonutil::{
    cache::{CacheKind, CacheRoot, initialize_cache_root, resolve_cache_root},
    user_log::UserLog,
};

use super::{
    BuildConfig, BuildInput, CapturedBuildExecution, StandaloneDependencyPreparation,
    action_cache::{ActionCache, CacheAction, RestoreOutcome},
    action_identity::{ActionIdentityContext, compute_action_identities},
    execute_build_capturing, finish_captured_build,
};

pub(super) fn execute(
    cfg: &BuildConfig,
    input: StandaloneDependencyPreparation,
    target_dir: &Path,
    user_log: &UserLog,
) -> anyhow::Result<CapturedBuildExecution> {
    let CacheRoot::Path(cache_root) = resolve_cache_root(CacheKind::BuildArtifacts)? else {
        return execute_actions(cfg, input.actions, &input.fallback_db_path, target_dir);
    };
    initialize_cache_root(CacheKind::BuildArtifacts, &cache_root)?;

    let identity_context = ActionIdentityContext::new(
        std::env::current_dir().context("failed to determine build working directory")?,
        std::env::vars_os().collect(),
    );
    let initial_identities = compute_action_identities(&input.actions, &identity_context)?;
    let cache = ActionCache::new(cache_root);
    let mut misses = Vec::new();
    let mut publish_after_execution = Vec::new();

    for (action, identity) in input.actions.iter().zip(&initial_identities) {
        let cache_action = if identity.is_cacheable() {
            CacheAction::new(
                identity.digest().to_hex(),
                action
                    .outputs()
                    .iter()
                    .flat_map(|product| product.paths().iter().cloned())
                    .collect(),
            )
        } else {
            None
        };
        let restore = match cache_action.as_ref() {
            Some(cache_action) => cache.restore(cache_action)?,
            None => RestoreOutcome::Miss,
        };
        if restore == RestoreOutcome::Miss {
            misses.push(action.clone());
            if let Some(cache_action) = cache_action {
                publish_after_execution.push(cache_action);
            }
        }
    }

    let execution = execute_misses(cfg, misses, &input.fallback_db_path, target_dir)?;
    if !execution.successful() {
        return Ok(execution);
    }
    if publish_after_execution.is_empty() {
        return Ok(execution);
    }

    let final_identities = match compute_action_identities(&input.actions, &identity_context) {
        Ok(identities) => identities,
        Err(error) => {
            finish_captured_build(cfg, &execution, None, user_log);
            return Err(error.context("failed to revalidate dependency action inputs"));
        }
    };
    if final_identities != initial_identities {
        finish_captured_build(cfg, &execution, None, user_log);
        bail!("dependency action inputs changed while their outputs were being prepared");
    }

    for action in &publish_after_execution {
        if let Err(error) = cache.publish(action) {
            finish_captured_build(cfg, &execution, None, user_log);
            return Err(error);
        }
    }
    Ok(execution)
}

fn execute_misses(
    cfg: &BuildConfig,
    actions: Vec<LoweredAction>,
    fallback_db_path: &Path,
    target_dir: &Path,
) -> anyhow::Result<CapturedBuildExecution> {
    if actions.is_empty() {
        return Ok(CapturedBuildExecution {
            n_tasks_executed: Some(0),
            diagnostics: Default::default(),
        });
    }

    let state_parent = fallback_db_path.parent().with_context(|| {
        format!(
            "dependency executor state path has no parent: {}",
            fallback_db_path.display()
        )
    })?;
    std::fs::create_dir_all(state_parent).with_context(|| {
        format!(
            "failed to create dependency executor state directory {}",
            state_parent.display()
        )
    })?;
    let executor_state = tempfile::Builder::new()
        .prefix(".dependency-misses-")
        .tempdir_in(state_parent)
        .context("failed to create dependency miss executor state")?;
    execute_actions(
        cfg,
        actions,
        &executor_state.path().join("dependencies.moon_db"),
        target_dir,
    )
}

fn execute_actions(
    cfg: &BuildConfig,
    actions: Vec<LoweredAction>,
    db_path: &Path,
    target_dir: &Path,
) -> anyhow::Result<CapturedBuildExecution> {
    let (graph, command_args_by_output) = lowered_actions_to_n2_graph(actions)?;
    execute_build_capturing(
        cfg,
        BuildInput {
            graph,
            command_args_by_output,
            db_path: db_path.to_owned(),
        },
        target_dir,
    )
}
