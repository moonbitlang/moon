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

//! Prepare, plan, and execute builds within a target-directory lifecycle.

mod config;
mod prebuild;
mod session;
mod standalone;

pub use config::{prepare_backend, prepare_config};
pub use session::{BuildRequest, BuildSession, MetadataOutput};
pub use standalone::{StandaloneWasm, build_standalone_wasm};

use anyhow::Context;
use moonbuild_rupes_recta::{
    CompileConfig, ResolvedProject,
    build_lower::lower_build_plan,
    build_plan::{self, ArtifactKey, BuildPlan, InputDirective},
    execution_plan::ExecutionPlan,
    model::BackendConfig,
    prebuild::PrebuildOutput,
};
use moonutil::user_log::UserLog;
use tracing::{debug, info, instrument};
use walkdir::WalkDir;

/// The output information of the compilation.
pub(super) struct CompileOutput {
    /// Executor-neutral concrete actions and declared outputs.
    pub(super) execution_plan: ExecutionPlan,

    /// The build plan, but only if we decided to export it.
    build_plan: Option<Box<BuildPlan>>,
}

/// Plan one backend, prepare its external inputs, and lower its complete graph.
#[instrument(skip_all)]
pub(super) fn compile(
    cx: &CompileConfig,
    resolved_project: &ResolvedProject,
    requested_artifacts: &[ArtifactKey],
    input_directive: &InputDirective,
    prebuild_config: Option<&PrebuildOutput>,
    user_log: &UserLog,
) -> anyhow::Result<CompileOutput> {
    info!(
        "Building compilation plan for {} requested artifacts",
        requested_artifacts.len()
    );

    let plan = build_plan::build_plan(
        resolved_project,
        cx,
        requested_artifacts.iter().cloned(),
        input_directive,
        prebuild_config,
        user_log,
    )
    .context("Failed to build a build plan for the modules")?;

    info!("Build plan created successfully");
    debug!("Build plan contains {} actions", plan.action_count());

    // The backend alone does not imply C compilation: native checks and empty
    // plans must not inspect the include tree. Observe it once, after planning
    // has determined which actions actually need it.
    let mut toolchain_include_files = Vec::new();
    if plan.requires_toolchain_headers() {
        let (BackendConfig::Native { compiler_paths, .. }
        | BackendConfig::Llvm { compiler_paths, .. }) = &cx.backend
        else {
            unreachable!("native compilation requires compiler paths");
        };
        let root = &compiler_paths.include_path;
        for entry in WalkDir::new(root).follow_links(true).sort_by_file_name() {
            let entry = entry.with_context(|| {
                format!("failed to inspect MoonBit toolchain include directory {root}")
            })?;
            if entry.file_type().is_file() {
                toolchain_include_files.push(entry.into_path());
            }
        }
        toolchain_include_files.sort();
    }
    let execution_plan = lower_build_plan(resolved_project, &plan, cx, &toolchain_include_files)
        .context("Failed to lower the build plan")?;

    info!("Execution plan lowering completed successfully");

    Ok(CompileOutput {
        execution_plan,
        build_plan: cx.debug_export_build_plan.then(|| Box::new(plan)),
    })
}

#[cfg(test)]
mod tests {
    use moonbuild_rupes_recta::{
        DiscoveryConfig, RunMode,
        build_lower::WarningCondition,
        model::TargetKind,
        prepare_synced_project,
        target_layout::{ArtifactPathResolver, TargetLayout, TargetLayoutMode},
    };
    use moonutil::{
        compiler_flags::{CompilerPaths, NativeAllocator},
        cond_expr::OptLevel,
        manifest::read_module_manifest,
        resolution::{DEFAULT_VERSION, DirSyncResult, ModuleDependencyGraph, ModuleSource},
        target::TargetBackend,
        user_log::user_log_level,
    };

    use super::*;

    #[test]
    fn compile_observes_headers_only_for_c_compilation() {
        // Planning runtime objects requires an installed MoonBit toolchain and
        // a host C compiler, but this test never executes the planned commands.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("moon.mod"), "name = \"test/headers\"\n").unwrap();
        std::fs::write(dir.path().join("moon.pkg"), "").unwrap();
        std::fs::write(dir.path().join("main.mbt"), "fn main {}\n").unwrap();
        let module = ModuleSource::local_path(
            "test/headers".parse().unwrap(),
            dir.path().to_owned(),
            DEFAULT_VERSION.clone(),
        )
        .unwrap();
        let (_, manifest) = read_module_manifest(dir.path(), false).unwrap();
        let (modules, module_id) = ModuleDependencyGraph::only_one_module(module, manifest);
        let mut dirs = DirSyncResult::default();
        dirs.insert(module_id, dir.path().to_owned());
        let user_log = UserLog::new(user_log_level(false, true));
        let resolved = prepare_synced_project(
            &DiscoveryConfig::default(),
            (modules, dirs),
            &[TargetBackend::Native],
            &user_log,
        )
        .unwrap();
        let package = resolved
            .pkg_dirs
            .get_package_id_by_name("test/headers")
            .unwrap();
        let target_dir = dir.path().join("_build");
        let include_dir = dir.path().join("include");
        let mut config = CompileConfig {
            configuration: moonbuild_rupes_recta::BuildConfiguration::new(
                moonutil::target::TargetBackend::Native,
                OptLevel::Debug,
            ),
            backend: BackendConfig::Native {
                direct_object_candidate: None,
                collect_ref_cycle: false,
                allocator: NativeAllocator::Default,
                os: std::env::consts::OS.parse().unwrap(),
                compiler_paths: CompilerPaths {
                    include_path: include_dir.display().to_string(),
                    lib_path: dir.path().join("lib").display().to_string(),
                },
            },
            action: RunMode::Build,
            debug_info: Default::default(),
            artifact_paths: ArtifactPathResolver::new(
                TargetLayout::new(
                    target_dir,
                    TargetLayoutMode::Workspace,
                    OptLevel::Debug,
                    RunMode::Build,
                ),
                None,
            ),
            debug_export_build_plan: true,
            enable_coverage: false,
            moonc_output_json: false,
            docs_serve: false,
            warning_condition: WarningCondition::Default,
            warn_list: None,
        };
        for requested in [
            vec![],
            vec![ArtifactKey::CheckMi {
                package,
                target_kind: TargetKind::Source,
            }],
            vec![ArtifactKey::CoreIr {
                package,
                target_kind: TargetKind::Source,
            }],
        ] {
            let output = compile(
                &config,
                &resolved,
                &requested,
                &InputDirective::default(),
                None,
                &user_log,
            )
            .expect("plans without C compilation must not read the absent include directory");
            let plan = output
                .build_plan
                .expect("debug export should retain the complete plan");
            assert!(!plan.requires_toolchain_headers());
            assert_eq!(
                plan.action_count(),
                output.execution_plan.action_ids().count()
            );
        }

        // A runtime build does need the tree. A missing tree must be reported,
        // not silently replaced by an empty input list.
        let requested = [ArtifactKey::RuntimeLibrary];
        let error = compile(
            &config,
            &resolved,
            &requested,
            &InputDirective::default(),
            None,
            &user_log,
        )
        .err()
        .expect("C compilation must report an unreadable include directory");
        assert!(
            error
                .to_string()
                .contains("failed to inspect MoonBit toolchain include directory"),
            "{error:#}"
        );

        std::fs::create_dir_all(include_dir.join("internal")).unwrap();
        let mut headers = vec![
            include_dir.join("internal/runtime.h"),
            include_dir.join("moonbit.h"),
        ];
        for header in &headers {
            std::fs::write(header, "/* test header */").unwrap();
        }
        #[cfg(unix)]
        {
            let alias = include_dir.join("runtime-alias.h");
            std::os::unix::fs::symlink(&headers[0], &alias).unwrap();
            headers.push(alias);
        }
        headers.sort();
        config.debug_export_build_plan = false;
        let output = compile(
            &config,
            &resolved,
            &requested,
            &InputDirective::default(),
            None,
            &user_log,
        )
        .unwrap();
        assert!(output.build_plan.is_none());
        let execution = &output.execution_plan;
        let mut runtime_objects = 0;
        for id in execution.action_ids() {
            let action = execution.action(id);
            let is_runtime_object = action.outputs().iter().any(|path| {
                matches!(
                    execution
                        .declared_output(path)
                        .and_then(|output| output.artifact()),
                    Some(ArtifactKey::RuntimeObject { .. })
                )
            });
            let observed_headers = action
                .inputs()
                .iter()
                .filter(|path| path.starts_with(&include_dir))
                .cloned()
                .collect::<Vec<_>>();
            if is_runtime_object {
                runtime_objects += 1;
                assert_eq!(observed_headers, headers);
            } else {
                assert!(
                    observed_headers.is_empty(),
                    "archiving should not consume toolchain headers"
                );
            }
        }
        assert!(runtime_objects > 0);
    }
}
