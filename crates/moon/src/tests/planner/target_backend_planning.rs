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

use super::fixture::{
    PlannedPackageRun, PlanningFixture, parse_bench_command, parse_build_command,
    parse_bundle_command, parse_check_command, parse_run_command, parse_test_command,
    planned_check_package_runs, planned_graph_inputs, planned_root_package_runs,
};
use moonutil::common::{TargetBackend, lower_surface_targets};

// Phase 3: these tests already know the selected backend and only need to
// verify that planning keeps the right packages and commands in the graph.

fn assert_graph_text_contains_and_omits(graph: &str, present: &[&str], absent: &[&str]) {
    for needle in present {
        assert!(
            graph.contains(needle),
            "expected graph to contain `{needle}`, got:\n{graph}"
        );
    }
    for needle in absent {
        assert!(
            !graph.contains(needle),
            "expected graph to not contain `{needle}`, got:\n{graph}"
        );
    }
}

fn assert_graph_inputs(graph: &str, present: &[&str], absent: &[&str]) {
    let inputs = planned_graph_inputs(graph);
    for input in present {
        assert!(
            inputs.contains(*input),
            "expected graph inputs to contain `{input}`, got:\n{inputs:#?}"
        );
    }
    for input in absent {
        assert!(
            !inputs.contains(*input),
            "expected graph inputs to omit `{input}`, got:\n{inputs:#?}"
        );
    }
}

fn assert_root_package_runs(
    runs: Vec<(crate::rr_build::BuildMeta, crate::rr_build::BuildInput)>,
    expected: &[(TargetBackend, &[&str])],
) {
    assert_eq!(
        planned_root_package_runs(runs),
        expected_package_runs(expected)
    );
}

fn assert_target_backend_runs(
    runs: Vec<(crate::rr_build::BuildMeta, crate::rr_build::BuildInput)>,
    expected: &[TargetBackend],
) {
    let actual = runs
        .into_iter()
        .map(|(meta, _)| meta.target_backend.into())
        .collect::<Vec<TargetBackend>>();
    assert_eq!(actual, expected);
}

fn assert_check_package_runs(
    runs: Vec<(crate::rr_build::BuildMeta, crate::rr_build::BuildInput)>,
    expected: &[(TargetBackend, &[&str])],
) {
    assert_eq!(
        planned_check_package_runs(runs),
        expected_package_runs(expected)
    );
}

fn expected_package_runs(expected: &[(TargetBackend, &[&str])]) -> Vec<PlannedPackageRun> {
    expected
        .iter()
        .map(|(backend, packages)| PlannedPackageRun {
            target_backend: *backend,
            packages: packages.iter().map(|pkg| (*pkg).to_string()).collect(),
        })
        .collect()
}

#[test]
fn target_backend_build_planning_respects_default_and_explicit_backend() {
    let fixture = PlanningFixture::new("target_backend").expect("fixture should resolve");

    let (cli, cmd) = parse_build_command(&["build", "--dry-run", "--nostd", "--sort-input"]);
    let default_graph = fixture
        .plan_build_with_cli(&cli, &cmd)
        .expect("default build graph should plan");
    assert_graph_text_contains_and_omits(
        &default_graph,
        &[
            "./_build/wasm-gc/debug/build/main/main.wasm",
            "-target wasm-gc",
        ],
        &["./_build/js/debug/build/main/main.js", "-target js"],
    );

    let (cli, cmd) = parse_build_command(&[
        "build",
        "--dry-run",
        "--target",
        "js",
        "--nostd",
        "--sort-input",
    ]);
    let js_graph = fixture
        .plan_build_with_cli(&cli, &cmd)
        .expect("js build graph should plan");
    assert_graph_text_contains_and_omits(
        &js_graph,
        &["./_build/js/debug/build/main/main.js", "-target js"],
        &[
            "./_build/wasm-gc/debug/build/main/main.wasm",
            "-target wasm-gc",
        ],
    );
}

#[test]
fn many_target_planning_selects_requested_backends() {
    let fixture = PlanningFixture::new("targets/many_targets").expect("fixture should resolve");
    let all_packages = &["username/hello/lib", "username/hello/link"];
    let root_packages = &["username/hello/link"];

    let (cli, cmd) = parse_check_command(&[
        "check",
        "--target",
        "js,wasm",
        "--dry-run",
        "--serial",
        "--nostd",
        "--sort-input",
    ]);
    let runs = lower_surface_targets(&cmd.build_flags.target)
        .into_iter()
        .flat_map(|target| {
            fixture
                .plan_check_all_with_backend(&cli, &cmd, Some(target))
                .expect("multi-target check plans should resolve")
        })
        .collect::<Vec<_>>();
    assert_check_package_runs(
        runs,
        &[
            (TargetBackend::Wasm, all_packages),
            (TargetBackend::Js, all_packages),
        ],
    );

    let (cli, cmd) = parse_build_command(&[
        "build",
        "--target",
        "js,wasm",
        "--dry-run",
        "--serial",
        "--nostd",
        "--sort-input",
    ]);
    let runs = lower_surface_targets(&cmd.build_flags.target)
        .into_iter()
        .flat_map(|target| {
            fixture
                .plan_build_all_with_backend(&cli, &cmd, Some(target))
                .expect("multi-target build plans should resolve")
        })
        .collect::<Vec<_>>();
    assert_root_package_runs(
        runs,
        &[
            (TargetBackend::Wasm, root_packages),
            (TargetBackend::Js, root_packages),
        ],
    );

    let (cli, cmd) = parse_bundle_command(&[
        "bundle",
        "--target",
        "js,wasm",
        "--dry-run",
        "--serial",
        "--nostd",
        "--sort-input",
    ]);
    let runs = lower_surface_targets(&cmd.build_flags.target)
        .into_iter()
        .flat_map(|target| {
            fixture
                .plan_bundle_all_with_backend(&cli, &cmd, Some(target))
                .expect("multi-target bundle plans should resolve")
        })
        .collect::<Vec<_>>();
    assert_target_backend_runs(runs, &[TargetBackend::Wasm, TargetBackend::Js]);

    let (cli, cmd) = parse_test_command(&[
        "test",
        "--target",
        "js,wasm",
        "--dry-run",
        "--serial",
        "--nostd",
        "--sort-input",
    ]);
    let runs = lower_surface_targets(&cmd.build_flags.target)
        .into_iter()
        .flat_map(|target| {
            fixture
                .plan_test_all_with_backend(&cli, &cmd, Some(target))
                .expect("multi-target test plans should resolve")
        })
        .collect::<Vec<_>>();
    assert_root_package_runs(
        runs,
        &[
            (TargetBackend::Wasm, all_packages),
            (TargetBackend::Js, all_packages),
        ],
    );
}

#[test]
fn all_target_test_planning_selects_every_concrete_backend() {
    let fixture = PlanningFixture::new("targets/many_targets").expect("fixture should resolve");
    let packages = &["username/hello/lib", "username/hello/link"];

    let (cli, cmd) = parse_test_command(&[
        "test",
        "--target",
        "all",
        "--dry-run",
        "--serial",
        "--nostd",
        "--sort-input",
    ]);
    let runs = lower_surface_targets(&cmd.build_flags.target)
        .into_iter()
        .flat_map(|target| {
            fixture
                .plan_test_all_with_backend(&cli, &cmd, Some(target))
                .expect("all-target test plans should resolve")
        })
        .collect::<Vec<_>>();
    assert_root_package_runs(
        runs,
        &[
            (TargetBackend::Wasm, packages),
            (TargetBackend::WasmGC, packages),
            (TargetBackend::Js, packages),
            (TargetBackend::Native, packages),
        ],
    );
}

#[test]
fn conflicting_workspace_preferred_targets_build_selection_splits_by_module_backend() {
    let fixture = PlanningFixture::new("workspace_conflicting_preferred_targets.in")
        .expect("fixture should resolve");

    let (cli, cmd) = parse_build_command(&["build", "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_build_all_with_cli(&cli, &cmd)
        .expect("default build plans should resolve");

    assert_root_package_runs(
        runs,
        &[
            (TargetBackend::Js, &["workspace/js_preferred/lib"]),
            (TargetBackend::Native, &["workspace/native_preferred/lib"]),
        ],
    );
}

#[test]
fn conflicting_workspace_preferred_targets_build_path_selection_uses_module_backend() {
    let fixture = PlanningFixture::new("workspace_conflicting_preferred_targets.in")
        .expect("fixture should resolve");

    let js_path = fixture.case_dir().join("js_preferred/src/lib");
    let native_path = fixture.case_dir().join("native_preferred/src/lib");
    let js_path = js_path.to_str().expect("fixture path should be UTF-8");
    let native_path = native_path.to_str().expect("fixture path should be UTF-8");

    let (cli, cmd) =
        parse_build_command(&["build", js_path, native_path, "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_build_all_with_cli(&cli, &cmd)
        .expect("path-selected build plans should resolve");

    assert_root_package_runs(
        runs,
        &[
            (TargetBackend::Js, &["workspace/js_preferred/lib"]),
            (TargetBackend::Native, &["workspace/native_preferred/lib"]),
        ],
    );
}

#[test]
fn explicit_build_target_keeps_single_backend_selection() {
    let fixture = PlanningFixture::new("workspace_conflicting_preferred_targets.in")
        .expect("fixture should resolve");

    let (cli, cmd) = parse_build_command(&["build", "--target", "js", "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_build_all_with_cli(&cli, &cmd)
        .expect("explicit js build plans should resolve");

    assert_root_package_runs(
        runs,
        &[(
            TargetBackend::Js,
            &[
                "workspace/js_preferred/lib",
                "workspace/native_preferred/lib",
            ],
        )],
    );
}

#[test]
fn conflicting_workspace_preferred_targets_test_selection_splits_by_module_backend() {
    let fixture = PlanningFixture::new("workspace_conflicting_preferred_targets.in")
        .expect("fixture should resolve");

    let (cli, cmd) = parse_test_command(&["test", "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_test_all_with_cli(&cli, &cmd)
        .expect("default test plans should resolve");

    assert_root_package_runs(
        runs,
        &[
            (TargetBackend::Js, &["workspace/js_preferred/lib"]),
            (TargetBackend::Native, &["workspace/native_preferred/lib"]),
        ],
    );
}

#[test]
fn conflicting_workspace_preferred_targets_test_path_selection_uses_module_backend() {
    let fixture = PlanningFixture::new("workspace_conflicting_preferred_targets.in")
        .expect("fixture should resolve");

    let js_path = fixture.case_dir().join("js_preferred/src/lib");
    let native_path = fixture.case_dir().join("native_preferred/src/lib");
    let js_path = js_path.to_str().expect("fixture path should be UTF-8");
    let native_path = native_path.to_str().expect("fixture path should be UTF-8");

    let (cli, cmd) =
        parse_test_command(&["test", js_path, native_path, "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_test_all_with_cli(&cli, &cmd)
        .expect("path-selected test plans should resolve");

    assert_root_package_runs(
        runs,
        &[
            (TargetBackend::Js, &["workspace/js_preferred/lib"]),
            (TargetBackend::Native, &["workspace/native_preferred/lib"]),
        ],
    );
}

#[test]
fn explicit_test_target_keeps_single_backend_selection() {
    let fixture = PlanningFixture::new("workspace_conflicting_preferred_targets.in")
        .expect("fixture should resolve");

    let (cli, cmd) = parse_test_command(&["test", "--target", "js", "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_test_all_with_cli(&cli, &cmd)
        .expect("explicit js test plans should resolve");

    assert_root_package_runs(
        runs,
        &[(
            TargetBackend::Js,
            &[
                "workspace/js_preferred/lib",
                "workspace/native_preferred/lib",
            ],
        )],
    );
}

#[test]
fn conflicting_workspace_preferred_targets_bench_selection_splits_by_module_backend() {
    let fixture = PlanningFixture::new("workspace_conflicting_preferred_targets.in")
        .expect("fixture should resolve");

    let (cli, cmd) = parse_bench_command(&["bench", "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_bench_all_with_cli(&cli, &cmd)
        .expect("default bench plans should resolve");

    assert_root_package_runs(
        runs,
        &[
            (TargetBackend::Js, &["workspace/js_preferred/lib"]),
            (TargetBackend::Native, &["workspace/native_preferred/lib"]),
        ],
    );
}

#[test]
fn explicit_bench_target_keeps_single_backend_selection() {
    let fixture = PlanningFixture::new("workspace_conflicting_preferred_targets.in")
        .expect("fixture should resolve");

    let (cli, cmd) = parse_bench_command(&["bench", "--target", "js", "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_bench_all_with_cli(&cli, &cmd)
        .expect("explicit js bench plans should resolve");

    assert_root_package_runs(
        runs,
        &[(
            TargetBackend::Js,
            &[
                "workspace/js_preferred/lib",
                "workspace/native_preferred/lib",
            ],
        )],
    );
}

#[test]
fn mixed_backend_build_and_check_planning_are_target_aware() {
    let fixture =
        PlanningFixture::new("mixed_backend_local_dep.in").expect("fixture should resolve");

    let (cli, cmd) = parse_check_command(&["check", "--target", "js", "--dry-run", "--sort-input"]);
    let check_js = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("js check graph should plan");
    assert_graph_inputs(
        &check_js,
        &[
            "./shared/shared.mbt",
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
        ],
        &[
            "./server/main.mbt",
            "./deps/nativedep/lib/lib.mbt",
            "./deps/unuseddep/lib/lib.mbt",
        ],
    );

    let (cli, cmd) = parse_build_command(&["build", "--target", "js", "--dry-run", "--sort-input"]);
    let build_js = fixture
        .plan_build_with_cli(&cli, &cmd)
        .expect("js build graph should plan");
    assert_graph_inputs(
        &build_js,
        &[
            "./shared/shared.mbt",
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
        ],
        &["./server/main.mbt", "./deps/nativedep/lib/lib.mbt"],
    );

    let (cli, cmd) =
        parse_check_command(&["check", "--target", "native", "--dry-run", "--sort-input"]);
    let check_native = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("native check graph should plan");
    assert_graph_inputs(
        &check_native,
        &[
            "./shared/shared.mbt",
            "./server/main.mbt",
            "./deps/nativedep/lib/lib.mbt",
        ],
        &[
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
            "./deps/unuseddep/lib/lib.mbt",
        ],
    );

    let (cli, cmd) =
        parse_build_command(&["build", "--target", "native", "--dry-run", "--sort-input"]);
    let build_native = fixture
        .plan_build_with_cli(&cli, &cmd)
        .expect("native build graph should plan");
    assert_graph_inputs(
        &build_native,
        &[
            "./shared/shared.mbt",
            "./server/main.mbt",
            "./deps/nativedep/lib/lib.mbt",
        ],
        &["./web/main.mbt", "./deps/jsdep/lib/lib.mbt"],
    );
}

#[test]
fn mixed_backend_run_planning_is_target_aware() {
    let fixture =
        PlanningFixture::new("mixed_backend_local_dep.in").expect("fixture should resolve");

    let (cli, cmd) =
        parse_run_command(&["run", "web", "--target", "js", "--dry-run", "--sort-input"]);
    let run_js = fixture
        .plan_run_with_cli(&cli, &cmd)
        .expect("js run graph should plan");
    assert_graph_inputs(
        &run_js,
        &[
            "./shared/shared.mbt",
            "./web/main.mbt",
            "./deps/jsdep/lib/lib.mbt",
        ],
        &["./server/main.mbt", "./deps/nativedep/lib/lib.mbt"],
    );

    let (cli, cmd) = parse_run_command(&[
        "run",
        "server",
        "--target",
        "native",
        "--dry-run",
        "--sort-input",
    ]);
    let run_native = fixture
        .plan_run_with_cli(&cli, &cmd)
        .expect("native run graph should plan");
    assert_graph_inputs(
        &run_native,
        &[
            "./shared/shared.mbt",
            "./server/main.mbt",
            "./deps/nativedep/lib/lib.mbt",
        ],
        &["./web/main.mbt", "./deps/jsdep/lib/lib.mbt"],
    );
}

#[test]
fn supported_targets_empty_packages_are_skipped_in_check_planning() {
    let fixture =
        PlanningFixture::new("supported_targets_empty.in").expect("fixture should resolve");

    let (cli, cmd) = parse_check_command(&["check", "--target", "js", "--dry-run", "--sort-input"]);
    let check_js = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("js check graph should plan");
    assert_graph_inputs(
        &check_js,
        &["./main/main.mbt", "./lib/lib.mbt"],
        &["./never/never.mbt"],
    );

    let (cli, cmd) =
        parse_check_command(&["check", "--target", "native", "--dry-run", "--sort-input"]);
    let check_native = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("native check graph should plan");
    assert_graph_inputs(
        &check_native,
        &["./main/main.mbt", "./lib/lib.mbt"],
        &["./never/never.mbt"],
    );
}

#[test]
fn conflicting_workspace_preferred_targets_check_selection_splits_by_module_backend() {
    let fixture = PlanningFixture::new("workspace_conflicting_preferred_targets.in")
        .expect("fixture should resolve");

    let (cli, cmd) = parse_check_command(&["check", "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_check_all_with_cli(&cli, &cmd)
        .expect("default check plans should resolve");

    assert_check_package_runs(
        runs,
        &[
            (TargetBackend::Js, &["workspace/js_preferred/lib"]),
            (TargetBackend::Native, &["workspace/native_preferred/lib"]),
        ],
    );
}

#[test]
fn conflicting_workspace_preferred_targets_check_path_selection_uses_module_backend() {
    let fixture = PlanningFixture::new("workspace_conflicting_preferred_targets.in")
        .expect("fixture should resolve");

    let js_path = fixture.case_dir().join("js_preferred/src/lib");
    let native_path = fixture.case_dir().join("native_preferred/src/lib");
    let js_path = js_path.to_str().expect("fixture path should be UTF-8");
    let native_path = native_path.to_str().expect("fixture path should be UTF-8");

    let (cli, cmd) =
        parse_check_command(&["check", js_path, native_path, "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_check_all_with_cli(&cli, &cmd)
        .expect("path-selected check plans should resolve");

    assert_check_package_runs(
        runs,
        &[
            (TargetBackend::Js, &["workspace/js_preferred/lib"]),
            (TargetBackend::Native, &["workspace/native_preferred/lib"]),
        ],
    );
}

#[test]
fn explicit_check_target_keeps_single_backend_selection() {
    let fixture = PlanningFixture::new("workspace_conflicting_preferred_targets.in")
        .expect("fixture should resolve");

    let (cli, cmd) = parse_check_command(&["check", "--target", "js", "--dry-run", "--sort-input"]);
    let runs = fixture
        .plan_check_all_with_cli(&cli, &cmd)
        .expect("explicit js check plans should resolve");

    assert_check_package_runs(
        runs,
        &[(
            TargetBackend::Js,
            &[
                "workspace/js_preferred/lib",
                "workspace/native_preferred/lib",
            ],
        )],
    );
}

#[test]
fn module_supported_targets_intersection_filters_check_planning() {
    let fixture = PlanningFixture::new("supported_targets_module_intersection.in")
        .expect("fixture should resolve");
    let lib_path = fixture.case_dir().join("lib");
    let lib_path = lib_path.to_str().expect("fixture path should be UTF-8");

    let (cli, cmd) = parse_check_command(&[
        "check",
        lib_path,
        "--target",
        "wasm-gc",
        "--dry-run",
        "--sort-input",
    ]);
    let check_wasm_gc = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("wasm-gc check graph should plan");
    assert_graph_inputs(&check_wasm_gc, &["./lib/lib.mbt"], &["./main/main.mbt"]);

    let (cli, cmd) = parse_check_command(&[
        "check",
        lib_path,
        "--target",
        "native",
        "--dry-run",
        "--sort-input",
    ]);
    let check_native = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("native check graph should plan");
    assert_graph_inputs(&check_native, &["./lib/lib.mbt"], &["./main/main.mbt"]);

    let (cli, cmd) = parse_check_command(&[
        "check",
        lib_path,
        "--target",
        "llvm",
        "--dry-run",
        "--sort-input",
    ]);
    let check_llvm = fixture
        .plan_check_with_cli(&cli, &cmd)
        .expect("llvm check graph should plan");
    assert_graph_inputs(&check_llvm, &["./lib/lib.mbt"], &["./main/main.mbt"]);
}
