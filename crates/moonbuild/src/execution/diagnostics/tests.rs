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

use super::super::{
    ActionFailure, ActionFailureKind, BuildInput, CapturedActionOutput, CapturedBuildExecution,
    collect_captured_build_json,
};
use super::*;
use moonutil::target::TargetBackend;

fn composed_build_input() -> BuildInput {
    use moonbuild_rupes_recta::{
        CompileConfig, DiscoveryConfig, RunMode,
        build_plan::{ArtifactKey, InputDirective},
        fmt::{FmtConfig, build_execution_plan_for_fmt_file},
        model::{BackendConfig, TargetKind},
        target_layout::{ArtifactPathResolver, TargetLayout, TargetLayoutMode},
    };
    use moonutil::{
        cond_expr::OptLevel,
        resolution::{DEFAULT_VERSION, DirSyncResult, ModuleDependencyGraph, ModuleSource},
        user_log::user_log_level,
    };

    // Use real plans so backend attribution is checked after composition
    // remaps each plan's action IDs, rather than duplicating metadata in
    // the captured-output fixture.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("moon.mod"), "name = \"test/capture\"\n").unwrap();
    std::fs::write(dir.path().join("moon.pkg"), "").unwrap();
    let source = dir.path().join("main.mbt");
    std::fs::write(&source, "fn main() {}\n").unwrap();
    let module = ModuleSource::local_path(
        "test/capture".parse().unwrap(),
        dir.path().to_owned(),
        DEFAULT_VERSION.clone(),
    )
    .unwrap();
    let (_, manifest) = moonutil::manifest::read_module_manifest(dir.path(), false).unwrap();
    let (modules, module_id) = ModuleDependencyGraph::only_one_module(module, manifest);
    let mut dirs = DirSyncResult::default();
    dirs.insert(module_id, dir.path().to_owned());
    let user_log = UserLog::new(user_log_level(false, true));
    let resolved = moonbuild_rupes_recta::prepare_synced_project(
        &DiscoveryConfig::default(),
        (modules, dirs),
        &[TargetBackend::WasmGC, TargetBackend::Js],
        &user_log,
    )
    .unwrap();
    let package = resolved
        .pkg_dirs
        .get_package_id_by_name("test/capture")
        .unwrap();
    let target_dir = dir.path().join("_build");
    let mut inputs = [BackendConfig::WasmGc { use_wat: false }, BackendConfig::Js]
        .into_iter()
        .map(|backend| {
            let target = backend.target_backend();
            let cfg = {
                let configuration = moonbuild_rupes_recta::BuildConfiguration::new(
                    backend.target_backend(),
                    OptLevel::Debug,
                );
                let mut config = CompileConfig::new(
                    configuration,
                    backend,
                    RunMode::Check,
                    ArtifactPathResolver::new(
                        TargetLayout::new(
                            target_dir.clone(),
                            TargetLayoutMode::Workspace,
                            configuration,
                            RunMode::Check,
                        ),
                        None,
                    ),
                );
                config.moonc_output_json = true;
                config
            };
            let compiled = crate::compile::compile(
                &cfg,
                &resolved,
                &[ArtifactKey::CheckMi {
                    package,
                    target_kind: TargetKind::Source,
                }],
                &InputDirective::default(),
                None,
                &user_log,
            )
            .unwrap();
            BuildInput::new(compiled.execution_plan, Some(target))
        })
        .collect::<Vec<_>>();
    let tool = build_execution_plan_for_fmt_file(
        &FmtConfig {
            check_only: true,
            extra_args: vec![],
            warn_only: false,
        },
        &source,
        &target_dir,
    )
    .unwrap();
    inputs.insert(1, BuildInput::new(tool, None));
    BuildInput::compose(inputs).unwrap()
}

fn diagnostic(path: &str, level: &str) -> MooncDiagnostic {
    MooncDiagnostic {
        path: path.to_string(),
        loc: Loc {
            start: Position { line: 1, col: 1 },
            end: Position { line: 1, col: 2 },
        },
        level: level.to_string(),
        message: String::new(),
        error_code: 0,
        children: Vec::new(),
    }
}

#[test]
fn action_failures_are_independent_of_diagnostic_limits_and_messages() {
    // The compiler failure needs no extra error message. The tool emits
    // compiler-shaped JSON, and the interrupted action emits no output.
    // None of these presentation differences may erase the task result.
    let input = composed_build_input();
    let action_ids = input.execution_plan.action_ids().collect::<Vec<_>>();
    assert_eq!(action_ids.len(), 3);
    for diagnostic_limit in [None, Some(0)] {
        let cfg = BuildConfig {
            output_style: OutputStyle::Json,
            diagnostic_limit,
            ..Default::default()
        };
        let compiler_error = serde_json::to_string(&diagnostic("main.mbt", "error")).unwrap();
        // Concurrent actions can finish in a different order from the plan.
        let action_outputs = [
            (
                action_ids[1],
                ActionFailureKind::Failed,
                Some(compiler_error.clone()),
            ),
            (
                action_ids[0],
                ActionFailureKind::Failed,
                Some(compiler_error.clone()),
            ),
            (action_ids[2], ActionFailureKind::Interrupted, None),
        ]
        .map(|(action_id, failure, output)| CapturedActionOutput {
            action_id,
            failure: Some(failure),
            content: ResultCatcher {
                content_writer: output.into_iter().collect(),
                ..Default::default()
            },
        });
        let result = collect_captured_build_json(
            &cfg,
            CapturedBuildExecution {
                input: &input,
                n_tasks_executed: None,
                action_outputs: action_outputs.into(),
            },
        );
        assert!(!result.successful());
        assert_eq!(result.n_errors, 1);
        assert_eq!(
            result.hidden_errors,
            usize::from(diagnostic_limit == Some(0))
        );
        assert_eq!(
            result.diagnostics.len(),
            usize::from(diagnostic_limit.is_none())
        );
        assert_eq!(result.action_errors.len(), 2);
        assert!(result.action_errors[0].starts_with("failed to format "));
        assert!(result.action_errors[1].starts_with("Interrupted while attempting to check "));
        assert_eq!(result.failed_commands.len(), 1);
        assert!(result.failed_commands[0].contains("-target wasm-gc"));
        assert_eq!(result.non_diagnostic_output, [compiler_error]);
        assert_eq!(
            result.action_failures,
            [
                ActionFailure {
                    kind: ActionFailureKind::Failed,
                    target_backend: None,
                    output_kind: ActionOutputKind::Tool,
                    has_error_diagnostics: false,
                },
                ActionFailure {
                    kind: ActionFailureKind::Failed,
                    target_backend: Some(TargetBackend::WasmGC),
                    output_kind: ActionOutputKind::Compiler,
                    has_error_diagnostics: true,
                },
                ActionFailure {
                    kind: ActionFailureKind::Interrupted,
                    target_backend: Some(TargetBackend::Js),
                    output_kind: ActionOutputKind::Compiler,
                    has_error_diagnostics: false,
                },
            ]
        );
    }
}

#[test]
fn diagnostic_output_does_not_create_an_action_failure() {
    let input = composed_build_input();
    let cfg = BuildConfig {
        output_style: OutputStyle::Json,
        ..Default::default()
    };
    let result = collect_captured_build_json(
        &cfg,
        CapturedBuildExecution {
            input: &input,
            n_tasks_executed: Some(1),
            action_outputs: vec![CapturedActionOutput {
                action_id: input.execution_plan.action_ids().next().unwrap(),
                content: ResultCatcher {
                    content_writer: vec![
                        serde_json::to_string(&diagnostic("main.mbt", "error")).unwrap(),
                    ],
                    ..Default::default()
                },
                failure: None,
            }],
        },
    );
    assert!(result.successful());
    assert_eq!(result.n_errors, 1);
    assert!(result.action_failures.is_empty());
}

#[test]
fn suppresses_generated_test_driver_warnings_only() {
    assert!(diagnostic_is_generated_test_driver_warning(&diagnostic(
        "./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt",
        "warning"
    )));
    assert!(!diagnostic_is_generated_test_driver_warning(&diagnostic(
        "./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt",
        "error"
    )));
    assert!(!diagnostic_is_generated_test_driver_warning(&diagnostic(
        "./lib/hello.mbt",
        "warning"
    )));
}

#[test]
fn generated_test_driver_errors_are_counted() {
    let generated_driver_path =
        "./_build/wasm-gc/debug/test/lib/__generated_driver_for_internal_test.mbt";
    let warning = diagnostic(generated_driver_path, "warning");
    let error = diagnostic(generated_driver_path, "error");
    let mut catcher = ResultCatcher::default();
    catcher.append_content(serde_json::to_string(&warning).unwrap());
    catcher.append_content(serde_json::to_string(&error).unwrap());

    let cfg = BuildConfig {
        output_style: OutputStyle::Json,
        ..Default::default()
    };
    let processed = process_captured_diagnostics(
        &[CapturedDiagnosticSource {
            diagnostics: &catcher,
            output_kind: ActionOutputKind::Compiler,
            build_succeeded: false,
            build_meta: None,
        }],
        &cfg,
    );

    assert_eq!(processed.n_warnings, 0);
    assert_eq!(processed.n_errors, 1);
}

#[test]
fn diagnostic_limit_is_shared_across_captured_build_errors() {
    let mut dependency_error = diagnostic("./dependency.mbt", "error");
    dependency_error
        .children
        .push(diagnostic("./dependency-detail.mbt", "error"));
    let script_error = diagnostic("./script.mbt", "error");
    let mut dependency = ResultCatcher::default();
    dependency.append_content(serde_json::to_string(&dependency_error).unwrap());
    let mut script = ResultCatcher::default();
    script.append_content(serde_json::to_string(&script_error).unwrap());

    let cfg = BuildConfig {
        output_style: OutputStyle::Json,
        diagnostic_limit: Some(1),
        ..Default::default()
    };
    let processed = process_captured_diagnostics(
        &[
            CapturedDiagnosticSource {
                diagnostics: &dependency,
                output_kind: ActionOutputKind::Compiler,
                build_succeeded: true,
                build_meta: None,
            },
            CapturedDiagnosticSource {
                diagnostics: &script,
                output_kind: ActionOutputKind::Compiler,
                build_succeeded: false,
                build_meta: None,
            },
        ],
        &cfg,
    );

    assert_eq!(processed.n_errors, 2);
    assert_eq!(processed.n_warnings, 0);
    assert_eq!(processed.hidden_errors, 1);
    assert_eq!(processed.hidden_warnings, 0);
}

#[test]
fn tool_output_is_not_parsed_as_compiler_diagnostics() {
    // A tool can print the same JSON as the compiler. Only the compiler
    // source participates in counting, filtering, and diagnostic limits.
    for level in ["error", "warning"] {
        let message = serde_json::to_string(&diagnostic("./script.mbt", level)).unwrap();
        let mut output = ResultCatcher::default();
        output.append_content(&message);
        let cfg = BuildConfig {
            output_style: OutputStyle::Json,
            diagnostic_limit: Some(0),
            ..Default::default()
        };
        let sources = [ActionOutputKind::Compiler, ActionOutputKind::Tool].map(|output_kind| {
            CapturedDiagnosticSource {
                diagnostics: &output,
                output_kind,
                build_succeeded: false,
                build_meta: None,
            }
        });
        let collected = collect_json_diagnostics(&sources, &cfg, true);
        assert!(collected.diagnostics.is_empty());
        assert_eq!(collected.non_diagnostic_output, [message]);
        assert_eq!(collected.processed.n_errors, usize::from(level == "error"));
        assert_eq!(
            collected.processed.n_warnings,
            usize::from(level == "warning")
        );
        assert_eq!(
            collected.processed.hidden_errors,
            usize::from(level == "error")
        );
        assert_eq!(
            collected.processed.hidden_warnings,
            usize::from(level == "warning")
        );
    }
}

#[test]
fn structured_json_retains_successful_output_when_progress_is_suppressed() {
    let mut catcher = ResultCatcher::default();
    catcher.append_content("PREBUILD_SUCCESS");
    let cfg = BuildConfig::default().with_suppressed_progress(true);
    let sources = [CapturedDiagnosticSource {
        diagnostics: &catcher,
        output_kind: ActionOutputKind::Tool,
        build_succeeded: true,
        build_meta: None,
    }];

    let collected = collect_json_diagnostics(&sources, &cfg, true);
    assert_eq!(collected.non_diagnostic_output, ["PREBUILD_SUCCESS"]);

    let rendered = collect_json_diagnostics(&sources, &cfg, false);
    assert!(rendered.non_diagnostic_output.is_empty());
}
