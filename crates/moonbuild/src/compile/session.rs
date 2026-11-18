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

//! Hold the target-directory lock across planning, publication, and execution.

use super::prebuild;
use crate::{
    BuildMeta,
    execution::{self, BuildConfig, BuildInput, BuildStats, JsonBuildOutput, resolve_parallelism},
    metadata,
};
use moonbuild_rupes_recta::{
    CompileConfig, ResolvedProject,
    build_plan::{ArtifactKey, InputDirective},
    intent::UserIntent,
};
use moonutil::{build_options::RunMode, target::TargetBackend, user_log::UserLog};
use std::{
    fs::File,
    path::{Path, PathBuf},
};
use tracing::{info, instrument};

/// Requested artifacts expressed as command intents and input directives.
pub struct BuildRequest {
    /// The list of user intents; will be expanded to requested artifacts later.
    pub intents: Vec<UserIntent>,
    /// The input directive that the user wants to apply to the packages
    pub directive: InputDirective,
}

impl BuildRequest {
    pub fn new(intents: Vec<UserIntent>, directive: InputDirective) -> Self {
        Self { intents, directive }
    }

    fn requested_artifacts(
        &self,
        resolved_project: &ResolvedProject,
        user_log: &UserLog,
        target_backend: TargetBackend,
    ) -> Vec<ArtifactKey> {
        let mut artifacts = Vec::new();
        for intent in &self.intents {
            intent.append_artifacts(
                resolved_project,
                &mut artifacts,
                user_log,
                &self.directive,
                target_backend,
            );
        }
        artifacts
    }
}

impl From<Vec<UserIntent>> for BuildRequest {
    fn from(intents: Vec<UserIntent>) -> Self {
        Self {
            intents,
            directive: InputDirective::default(),
        }
    }
}

impl From<(Vec<UserIntent>, InputDirective)> for BuildRequest {
    fn from((intents, directive): (Vec<UserIntent>, InputDirective)) -> Self {
        Self { intents, directive }
    }
}

/// Metadata needed by a command in addition to compiler artifact paths.
/// `Packages` writes scoped documents; `Check` also publishes the active
/// selector and backend index for a complete check invocation.
pub enum MetadataOutput<'a> {
    Compiler,
    Packages(&'a Path),
    Check(&'a Path),
}

/// One build's access to a target directory.
///
/// Execution sessions own the directory lock until dropped, including between
/// backend planning passes and while callers consume generated metadata.
/// Dry-run sessions have no persistent lock; a planning pass locks temporarily
/// only when it must execute a prebuild configuration script.
pub struct BuildSession {
    target_dir: PathBuf,
    user_log: UserLog,
    lock: Option<File>,
}

impl BuildSession {
    /// Open a session for an existing target directory. Non-dry-run sessions
    /// wait for exclusive access before returning.
    pub fn new(target_dir: &Path, dry_run: bool, user_log: &UserLog) -> std::io::Result<Self> {
        let lock = if dry_run {
            None
        } else {
            Some(moonutil::locks::lock_directory(target_dir, user_log)?)
        };
        Ok(Self {
            target_dir: target_dir.to_owned(),
            user_log: user_log.clone(),
            lock,
        })
    }

    /// Run applicable prebuild configuration and lower one backend's plan.
    #[instrument(level = tracing::Level::DEBUG, skip_all)]
    pub fn plan(
        &self,
        cx: CompileConfig,
        intent: BuildRequest,
        resolved_project: ResolvedProject,
        jobs: Option<usize>,
        frozen: bool,
    ) -> anyhow::Result<(BuildMeta, BuildInput)> {
        anyhow::ensure!(
            cx.target_dir == self.target_dir,
            "build plan belongs to a different target directory"
        );
        let target_dir = &self.target_dir;
        let user_log = &self.user_log;
        info!("User intent calculated: {:?}", intent.intents);

        // Decide once, after backend selection, whether planning will execute a
        // script. Dry runs that only lower commands must not acquire a write lock.
        let run_prebuild = cx.action != RunMode::Check
            && cx.backend.target_backend().is_native()
            && resolved_project
                .module_graph
                .all_modules_and_id()
                .any(|(m, _)| {
                    resolved_project
                        .module_info(m)
                        .__moonbit_unstable_prebuild
                        .is_some()
                });
        let _dry_run_lock = if self.lock.is_none() && run_prebuild {
            Some(moonutil::locks::lock_directory(target_dir, user_log)?)
        } else {
            None
        };
        let prebuild_config = if run_prebuild {
            info!("Running prebuild configuration");
            Some(prebuild::run_prebuild_config(
                &resolved_project,
                &cx,
                resolve_parallelism(jobs),
                frozen,
            )?)
        } else {
            info!("Skipping prebuild configuration: no applicable scripts");
            None
        };

        info!("Expanding user intents to requested artifacts");
        let requested_artifacts =
            intent.requested_artifacts(&resolved_project, user_log, cx.backend.target_backend());
        info!("Begin lowering to build graph");
        let compile_output = super::compile(
            &cx,
            &resolved_project,
            &requested_artifacts,
            &intent.directive,
            prebuild_config.as_ref(),
            user_log,
        )?;

        if cx.debug_export_build_plan
            && let Some(plan) = compile_output.build_plan
        {
            info!("Exporting build plan DOT file");
            moonbuild_rupes_recta::util::print_build_plan_dot(
                &plan,
                &resolved_project.module_graph,
                &resolved_project.pkg_dirs,
                &mut std::fs::File::create(target_dir.join("build_plan.dot"))?,
            )?;
        }

        let artifacts = compile_output
            .execution_plan
            .requested_artifact_paths()
            .map(|(artifact, paths)| (artifact.clone(), paths.to_vec()))
            .collect();
        let build_meta = BuildMeta {
            resolved_project,
            artifacts,
            backend: cx.backend.clone(),
            opt_level: cx.opt_level,
            artifact_paths: cx.artifact_paths.clone(),
        };

        let input = BuildInput::new(
            compile_output.execution_plan,
            Some(cx.backend.target_backend()),
        );

        info!("Build planning completed successfully");

        Ok((build_meta, input))
    }

    fn publish_metadata(
        &self,
        input: &BuildInput,
        metas: &[&BuildMeta],
        output: MetadataOutput<'_>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.lock.is_some(),
            "cannot execute a dry-run build session"
        );
        anyhow::ensure!(
            metas.iter().all(
                |meta| meta.artifact_paths.target_layout().target_base_dir() == self.target_dir
            ),
            "build metadata belongs to a different target directory"
        );
        match output {
            MetadataOutput::Check(source_dir) => {
                let commands = metadata::collect_check_commands_by_output(input);
                metadata::publish_check_metadata(source_dir, metas, &commands)
            }
            MetadataOutput::Packages(source_dir) => {
                let commands = metadata::collect_check_commands_by_output(input);
                for meta in metas {
                    metadata::generate_all_pkgs_json(meta)?;
                    metadata::generate_metadata(source_dir, meta, &commands)?;
                }
                Ok(())
            }
            MetadataOutput::Compiler => {
                for meta in metas {
                    metadata::generate_all_pkgs_json(meta)?;
                }
                Ok(())
            }
        }
    }

    pub fn execute(
        &self,
        cfg: &BuildConfig,
        input: BuildInput,
        metas: &[&BuildMeta],
        metadata: MetadataOutput<'_>,
    ) -> anyhow::Result<BuildStats> {
        self.publish_metadata(&input, metas, metadata)?;
        execution::execute_build(cfg, input, &self.target_dir, &self.user_log)
    }

    pub fn execute_json(
        &self,
        cfg: &BuildConfig,
        input: BuildInput,
        metas: &[&BuildMeta],
        metadata: MetadataOutput<'_>,
    ) -> anyhow::Result<JsonBuildOutput> {
        self.publish_metadata(&input, metas, metadata)?;
        execution::execute_build_json(cfg, input, &self.target_dir)
    }

    pub fn execute_test(
        &self,
        cfg: &BuildConfig,
        input: BuildInput,
        metas: &[&BuildMeta],
    ) -> anyhow::Result<BuildStats> {
        self.publish_metadata(&input, metas, MetadataOutput::Compiler)?;
        execution::execute_test_build(cfg, input, &self.target_dir, metas, &self.user_log)
    }

    /// Rebuild after promotion without releasing the lock between source edits
    /// and the actions that consume them.
    pub fn rebuild<'a>(
        &self,
        cfg: &BuildConfig,
        input: BuildInput,
        meta: &BuildMeta,
        outputs: impl IntoIterator<Item = &'a Path>,
    ) -> anyhow::Result<BuildStats> {
        anyhow::ensure!(
            self.lock.is_some(),
            "cannot execute a dry-run build session"
        );
        anyhow::ensure!(
            meta.artifact_paths.target_layout().target_base_dir() == self.target_dir,
            "build metadata belongs to a different target directory"
        );
        execution::execute_build_partial(
            cfg,
            input,
            &self.target_dir,
            Some(meta),
            &self.user_log,
            outputs,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonbuild_rupes_recta::execution_plan::ExecutionPlan;
    use moonutil::{constants::MOON_LOCK, user_log::user_log_level};

    #[test]
    fn execution_session_holds_directory_lock_until_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let user_log = UserLog::new(user_log_level(false, true));
        let session = BuildSession::new(dir.path(), false, &user_log).unwrap();
        let competing = File::options()
            .read(true)
            .write(true)
            .open(dir.path().join(MOON_LOCK))
            .unwrap();
        assert!(matches!(
            competing.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        drop(session);
        competing.try_lock().unwrap();
    }

    #[test]
    fn dry_run_session_cannot_publish_or_execute() {
        let dir = tempfile::tempdir().unwrap();
        let user_log = UserLog::new(user_log_level(false, true));
        let session = BuildSession::new(dir.path(), true, &user_log).unwrap();
        let input = BuildInput::new(ExecutionPlan::default(), None);
        let result = session.execute(
            &BuildConfig::default(),
            input,
            &[],
            MetadataOutput::Compiler,
        );
        assert_eq!(
            result.err().unwrap().to_string(),
            "cannot execute a dry-run build session"
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
